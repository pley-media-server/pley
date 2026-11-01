//! The TV's system services (luna-service2) from native code, on a thread of
//! their own.
//!
//! One anonymous registration (`LSRegister(NULL)`: a jailed native app can't
//! take its app id's name; what it may call is still its role file's
//! decision), attached to the luna thread's own `GMainContext` and driven by
//! its `GMainLoop`. Other threads post requests to it and get their answers in
//! callbacks run on that thread (or through [`Pending`]). Every call has a
//! timeout that ends in `LSCallCancel`; a subscription's first answer has one
//! too. The interface's thread never waits on the bus.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_ulong, c_void};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::dl::{Lib, fn_table};

/// What went wrong with a call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LunaError {
    /// luna-service2 isn't there (not a TV), or the hub refused the registration.
    Unavailable(String),
    /// No answer in time; the call was cancelled.
    Timeout,
    /// The service answered `returnValue: false`.
    Refused { code: Option<i64>, text: String },
    /// The answer wasn't a JSON object.
    Unreadable,
    /// The bus refused to send the call (its own words).
    Bus(String),
}

impl fmt::Display for LunaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LunaError::Unavailable(why) => write!(f, "luna isn't available: {why}"),
            LunaError::Timeout => f.write_str("no answer in time"),
            LunaError::Refused { code: Some(code), text } => write!(f, "refused ({code}): {text}"),
            LunaError::Refused { code: None, text } => write!(f, "refused: {text}"),
            LunaError::Unreadable => f.write_str("an unreadable answer"),
            LunaError::Bus(why) => write!(f, "the bus refused the call: {why}"),
        }
    }
}

/// A service's answer, or why there is none.
pub type Answer = Result<Value, LunaError>;

/// Reads an answer as `luna.ts` does: JSON, and `returnValue: false` is a refusal
/// (with `errorText`, else `errorCode`, else "refused").
pub fn parse_answer(text: &str) -> Answer {
    let value: Value = serde_json::from_str(text).map_err(|_| LunaError::Unreadable)?;
    if !value.is_object() {
        return Err(LunaError::Unreadable);
    }
    if value.get("returnValue") == Some(&Value::Bool(false)) {
        let code = match value.get("errorCode") {
            Some(Value::Number(n)) => n.as_i64(),
            Some(Value::String(s)) => s.trim().parse().ok(),
            _ => None,
        };
        let text = match (value.get("errorText"), value.get("errorCode")) {
            (Some(Value::String(t)), _) => t.clone(),
            (_, Some(Value::String(c))) => c.clone(),
            (_, Some(Value::Number(c))) => c.to_string(),
            _ => "refused".into(),
        };
        return Err(LunaError::Refused { code, text });
    }
    Ok(value)
}

/// `payload` with `"subscribe": true`, as a subscription is asked for.
pub fn subscribing(payload: &Value) -> Value {
    let mut payload = if payload.is_object() { payload.clone() } else { Value::Object(Default::default()) };
    payload["subscribe"] = Value::Bool(true);
    payload
}

/// The handle to the luna thread: cheap to clone, usable from any thread.
#[derive(Clone)]
pub struct Luna {
    shared: Arc<Shared>,
}

struct Shared {
    tx: Mutex<Sender<Request>>,
    next_id: AtomicU64,
    /// The luna thread's `GMainContext`, once it has one.
    context: AtomicPtr<c_void>,
    /// A drain of the request channel is already on its way.
    scheduled: AtomicBool,
    status: Mutex<Status>,
}

/// How the registration went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Starting,
    Ready,
    /// The hub's own message.
    Failed(String),
}

type Once = Box<dyn FnOnce(Answer) + Send>;
type Many = Box<dyn FnMut(Answer) + Send>;

enum Reply {
    Once(Once),
    Many(Many),
}

enum Request {
    Call { id: u64, uri: String, payload: String, timeout: Option<Duration>, reply: Reply },
    Cancel(u64),
    After { delay: Duration, run: Box<dyn FnOnce() + Send> },
}

/// How much longer than its own timeout [`Pending::wait`] waits: covers a
/// luna thread still registering.
const WAIT_GRACE: Duration = Duration::from_secs(2);

impl Luna {
    /// Opens luna-service2 and GLib and starts the luna thread, which
    /// registers anonymously; `None` off the TV (the libraries aren't there).
    /// Calls made before the registration finishes wait for it.
    pub fn start() -> Option<Luna> {
        let api = api()?;
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            tx: Mutex::new(tx),
            next_id: AtomicU64::new(1),
            context: AtomicPtr::new(ptr::null_mut()),
            scheduled: AtomicBool::new(false),
            status: Mutex::new(Status::Starting),
        });
        let thread_shared = shared.clone();
        std::thread::Builder::new().name("pley-luna".into()).spawn(move || run(api, rx, thread_shared)).ok()?;
        Some(Luna { shared })
    }

    /// How the registration went.
    pub fn status(&self) -> Status {
        self.shared.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Calls `uri` (`luna://service/method`) with `payload`; `done` gets the
    /// answer on the luna thread, so it must be quick (send it on and wake).
    pub fn call_with(&self, uri: &str, payload: &Value, timeout: Duration, done: impl FnOnce(Answer) + Send + 'static) {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        self.post(Request::Call {
            id,
            uri: uri.into(),
            payload: payload.to_string(),
            timeout: Some(timeout),
            reply: Reply::Once(Box::new(done)),
        });
    }

    /// Calls `uri` with `payload`; the answer is waited for or polled through
    /// the result (never wait on the interface's thread).
    pub fn call(&self, uri: &str, payload: &Value, timeout: Duration) -> Pending {
        let (tx, rx) = mpsc::sync_channel(1);
        self.call_with(uri, payload, timeout, move |answer| {
            let _ = tx.try_send(answer);
        });
        Pending { rx, limit: Instant::now() + timeout + WAIT_GRACE }
    }

    /// Subscribes to `uri` (`"subscribe": true` added): every answer goes to
    /// `each` on the luna thread until the [`Subscription`] is dropped. The
    /// first answer must come within `timeout`, or `each` gets
    /// [`LunaError::Timeout`] and the subscription ends.
    pub fn subscribe(
        &self,
        uri: &str,
        payload: &Value,
        timeout: Duration,
        each: impl FnMut(Answer) + Send + 'static,
    ) -> Subscription {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        self.post(Request::Call {
            id,
            uri: uri.into(),
            payload: subscribing(payload).to_string(),
            timeout: Some(timeout),
            reply: Reply::Many(Box::new(each)),
        });
        Subscription { luna: self.clone(), id }
    }

    /// Runs `run` on the luna thread after `delay` (the engine's bounded waits).
    pub fn after(&self, delay: Duration, run: impl FnOnce() + Send + 'static) {
        self.post(Request::After { delay, run: Box::new(run) });
    }

    fn post(&self, request: Request) {
        let sent = self.shared.tx.lock().map(|tx| tx.send(request).is_ok()).unwrap_or(false);
        if sent {
            self.shared.schedule();
        }
    }
}

impl Shared {
    /// Asks the luna thread to drain the request channel, once per batch.
    fn schedule(&self) {
        let context = self.context.load(Ordering::SeqCst);
        if context.is_null() || self.scheduled.swap(true, Ordering::SeqCst) {
            return;
        }
        let Some(api) = api() else { return };
        // SAFETY: the context lives as long as the process (the luna thread never
        // ends); attaching a source is GLib's thread-safe way to wake a context
        // from another thread.
        unsafe {
            let source = (api.glib.g_idle_source_new)();
            (api.glib.g_source_set_callback)(source, on_drain, ptr::null_mut(), None);
            (api.glib.g_source_attach)(source, context);
            (api.glib.g_source_unref)(source);
        }
    }
}

/// An answer on its way.
pub struct Pending {
    rx: mpsc::Receiver<Answer>,
    limit: Instant,
}

impl Pending {
    /// Waits for the answer (the call's own timeout, plus a little in case the
    /// luna thread is still starting).
    pub fn wait(self) -> Answer {
        let left = self.limit.saturating_duration_since(Instant::now());
        self.rx.recv_timeout(left).unwrap_or(Err(LunaError::Timeout))
    }

    /// The answer if it has come.
    pub fn try_take(&mut self) -> Option<Answer> {
        self.rx.try_recv().ok()
    }
}

/// A subscription; dropping it cancels the call.
pub struct Subscription {
    luna: Luna,
    id: u64,
}

impl fmt::Debug for Subscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Subscription({})", self.id)
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.luna.post(Request::Cancel(self.id));
    }
}

/// `LSError`, as luna-service2 declares it.
#[repr(C)]
struct LsError {
    error_code: c_int,
    message: *mut c_char,
    file: *const c_char,
    line: c_int,
    func: *const c_char,
    padding: *mut c_void,
    magic: c_ulong,
}

impl LsError {
    fn new(api: &LunaApi) -> LsError {
        let mut e = LsError {
            error_code: 0,
            message: ptr::null_mut(),
            file: ptr::null(),
            line: 0,
            func: ptr::null(),
            padding: ptr::null_mut(),
            magic: 0,
        };
        // SAFETY: `e` is a valid LSError for the library to initialise.
        unsafe { (api.LSErrorInit)(&mut e) };
        e
    }

    /// The error's message, then frees it.
    fn take(mut self, api: &LunaApi) -> String {
        let text = if self.message.is_null() {
            format!("error {}", self.error_code)
        } else {
            // SAFETY: luna-service2 set `message` to a C string it owns until LSErrorFree.
            unsafe { CStr::from_ptr(self.message) }.to_string_lossy().into_owned()
        };
        // SAFETY: initialised by LSErrorInit and filled by a failed call.
        unsafe { (api.LSErrorFree)(&mut self) };
        format!("{text} ({})", self.error_code)
    }
}

type FilterFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool;
type SourceFn = unsafe extern "C" fn(*mut c_void) -> c_int;
type DestroyFn = unsafe extern "C" fn(*mut c_void);

fn_table! {
    /// luna-service2's functions (`lunaservice.h`).
    struct LunaApi {
        LSErrorInit: unsafe extern "C" fn(*mut LsError) -> bool,
        LSErrorFree: unsafe extern "C" fn(*mut LsError),
        LSRegister: unsafe extern "C" fn(*const c_char, *mut *mut c_void, *mut LsError) -> bool,
        LSGmainContextAttach: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut LsError) -> bool,
        LSCall: unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            *const c_char,
            FilterFn,
            *mut c_void,
            *mut c_ulong,
            *mut LsError,
        ) -> bool,
        LSCallOneReply: unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            *const c_char,
            FilterFn,
            *mut c_void,
            *mut c_ulong,
            *mut LsError,
        ) -> bool,
        LSCallCancel: unsafe extern "C" fn(*mut c_void, c_ulong, *mut LsError) -> bool,
        LSMessageGetPayload: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    }
}

fn_table! {
    /// The few GLib functions the luna thread's loop needs.
    struct GlibApi {
        g_main_context_new: unsafe extern "C" fn() -> *mut c_void,
        g_main_context_push_thread_default: unsafe extern "C" fn(*mut c_void),
        g_main_loop_new: unsafe extern "C" fn(*mut c_void, c_int) -> *mut c_void,
        g_main_loop_run: unsafe extern "C" fn(*mut c_void),
        g_main_context_pending: unsafe extern "C" fn(*mut c_void) -> c_int,
        g_main_context_iteration: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
        g_idle_source_new: unsafe extern "C" fn() -> *mut c_void,
        g_timeout_source_new: unsafe extern "C" fn(c_uint) -> *mut c_void,
        g_source_set_callback: unsafe extern "C" fn(*mut c_void, SourceFn, *mut c_void, Option<DestroyFn>),
        g_source_attach: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_uint,
        g_source_destroy: unsafe extern "C" fn(*mut c_void),
        g_source_unref: unsafe extern "C" fn(*mut c_void),
    }
}

#[derive(Clone, Copy)]
struct Api {
    ls: LunaApi,
    glib: GlibApi,
}

/// Dispatches what is waiting on GLib's default context, at most `max`
/// sources. LG's SDL and ACB deliver their own luna answers there (SAM's
/// relaunch and close among them), and nothing else runs that context, so the
/// interface's thread pumps it between waits.
/// Never blocks: a context another thread holds is left alone.
pub fn pump_default_context(max: usize) {
    let Some(api) = api() else { return };
    for _ in 0..max {
        // SAFETY: GLib's default context (null), without blocking.
        unsafe {
            if (api.glib.g_main_context_pending)(ptr::null_mut()) == 0 {
                break;
            }
            (api.glib.g_main_context_iteration)(ptr::null_mut(), 0);
        }
    }
}

/// The libraries, opened once; `None` off the TV.
fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| {
        let ls = Lib::open(&["libluna-service2.so.3", "libluna-service2.so"])?;
        let glib = Lib::open_loaded(&["libglib-2.0.so.0", "libglib-2.0.so"])?;
        let api = Api { ls: LunaApi::load(&ls).ok()?, glib: GlibApi::load(&glib).ok()? };
        Some(api)
    })
    .as_ref()
}

const G_SOURCE_REMOVE: c_int = 0;

struct Call {
    token: c_ulong,
    reply: Option<Reply>,
    timer: *mut c_void,
}

struct State {
    api: &'static Api,
    handle: *mut c_void,
    failed: Option<String>,
    context: *mut c_void,
    rx: Receiver<Request>,
    shared: Arc<Shared>,
    calls: HashMap<u64, Call>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn run(api: &'static Api, rx: Receiver<Request>, shared: Arc<Shared>) {
    // SAFETY: plain GLib and luna-service2 calls on this thread, with the
    // pointers they return; the context and loop live for the process.
    unsafe {
        let context = (api.glib.g_main_context_new)();
        (api.glib.g_main_context_push_thread_default)(context);
        let main_loop = (api.glib.g_main_loop_new)(context, 0);
        let mut handle = ptr::null_mut();
        let mut error = LsError::new(&api.ls);
        let failed = if !(api.ls.LSRegister)(ptr::null(), &mut handle, &mut error)
            || !(api.ls.LSGmainContextAttach)(handle, context, &mut error)
        {
            Some(error.take(&api.ls))
        } else {
            (api.ls.LSErrorFree)(&mut error);
            None
        };
        *shared.status.lock().unwrap_or_else(|e| e.into_inner()) = match &failed {
            Some(why) => {
                crate::log::line(&format!("luna: the hub refused the anonymous registration: {why}"));
                Status::Failed(why.clone())
            }
            None => Status::Ready,
        };
        STATE.with(|s| {
            *s.borrow_mut() =
                Some(State { api, handle, failed, context, rx, shared: shared.clone(), calls: HashMap::new() });
        });
        // Requests posted before the context was known are drained now; later
        // ones schedule their own drain.
        shared.context.store(context, Ordering::SeqCst);
        shared.schedule();
        (api.glib.g_main_loop_run)(main_loop);
    }
}

/// Runs `f` with the luna thread's state.
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.borrow_mut().as_mut().map(f))
}

/// Gives `answer` to a reply outside the state's borrow, surviving a panic in it.
fn deliver(reply: &mut Reply, answer: Answer) -> Option<Reply> {
    match std::mem::replace(reply, Reply::Many(Box::new(|_| {}))) {
        Reply::Once(f) => {
            if catch_unwind(AssertUnwindSafe(|| f(answer))).is_err() {
                crate::log::line("luna: an answer's handler panicked");
            }
            None
        }
        Reply::Many(mut f) => {
            if catch_unwind(AssertUnwindSafe(|| f(answer))).is_err() {
                crate::log::line("luna: a subscription's handler panicked");
            }
            Some(Reply::Many(f))
        }
    }
}

unsafe extern "C" fn on_drain(_data: *mut c_void) -> c_int {
    let shared = with_state(|s| s.shared.clone());
    if let Some(shared) = shared {
        shared.scheduled.store(false, Ordering::SeqCst);
    }
    loop {
        let Some(request) = with_state(|s| s.rx.try_recv().ok()).flatten() else { break };
        handle_request(request);
    }
    G_SOURCE_REMOVE
}

fn handle_request(request: Request) {
    match request {
        Request::Call { id, uri, payload, timeout, mut reply } => {
            let refused = with_state(|s| send_call(s, id, &uri, &payload, timeout, &mut reply)).flatten();
            if let Some(why) = refused {
                deliver(&mut reply, Err(why));
            } else {
                with_state(|s| {
                    if let Some(call) = s.calls.get_mut(&id) {
                        call.reply = Some(reply);
                    }
                });
            }
        }
        Request::Cancel(id) => {
            with_state(|s| {
                if let Some(mut call) = s.calls.remove(&id) {
                    cancel(s, &mut call);
                }
            });
        }
        Request::After { delay, run } => {
            with_state(|s| {
                let data = Box::into_raw(Box::new(Some(run))) as *mut c_void;
                // SAFETY: the source owns `data`: the callback takes the closure out,
                // `free_after` frees the box when the source goes.
                unsafe {
                    let source = (s.api.glib.g_timeout_source_new)(millis(delay));
                    (s.api.glib.g_source_set_callback)(source, on_after, data, Some(free_after));
                    (s.api.glib.g_source_attach)(source, s.context);
                    (s.api.glib.g_source_unref)(source);
                }
            });
        }
    }
}

/// Sends a call; on success it is recorded (its reply put in by the caller).
fn send_call(
    s: &mut State,
    id: u64,
    uri: &str,
    payload: &str,
    timeout: Option<Duration>,
    reply: &mut Reply,
) -> Option<LunaError> {
    if let Some(why) = &s.failed {
        return Some(LunaError::Unavailable(why.clone()));
    }
    let (Ok(c_uri), Ok(c_payload)) = (CString::new(uri), CString::new(payload)) else {
        return Some(LunaError::Bus("a NUL in the call".into()));
    };
    let mut token: c_ulong = 0;
    let mut error = LsError::new(&s.api.ls);
    let call = match reply {
        Reply::Once(_) => s.api.ls.LSCallOneReply,
        Reply::Many(_) => s.api.ls.LSCall,
    };
    // SAFETY: a registered handle, NUL-terminated strings the call copies, and
    // the call's id as the context: a late answer after a cancel finds no call
    // and is dropped, so nothing dangles.
    let sent = unsafe {
        call(
            s.handle,
            c_uri.as_ptr(),
            c_payload.as_ptr(),
            on_answer,
            id as usize as *mut c_void,
            &mut token,
            &mut error,
        )
    };
    if !sent {
        return Some(LunaError::Bus(error.take(&s.api.ls)));
    }
    // SAFETY: as in LsError::new.
    unsafe { (s.api.ls.LSErrorFree)(&mut error) };
    let timer = match timeout {
        // SAFETY: a timeout source on the luna thread's context, kept (one
        // reference) until the call ends.
        Some(t) => unsafe {
            let source = (s.api.glib.g_timeout_source_new)(millis(t));
            (s.api.glib.g_source_set_callback)(source, on_timeout, id as usize as *mut c_void, None);
            (s.api.glib.g_source_attach)(source, s.context);
            source
        },
        None => ptr::null_mut(),
    };
    s.calls.insert(id, Call { token, reply: None, timer });
    None
}

fn millis(d: Duration) -> c_uint {
    d.as_millis().clamp(1, c_uint::MAX as u128) as c_uint
}

/// Stops a call's timer and drops the reference kept to it.
fn stop_timer(s: &State, timer: &mut *mut c_void) {
    if !timer.is_null() {
        // SAFETY: the reference taken when the timer was made.
        unsafe {
            (s.api.glib.g_source_destroy)(*timer);
            (s.api.glib.g_source_unref)(*timer);
        }
        *timer = ptr::null_mut();
    }
}

fn cancel(s: &State, call: &mut Call) {
    stop_timer(s, &mut call.timer);
    let mut error = LsError::new(&s.api.ls);
    // SAFETY: a token the hub gave for this handle. A refused cancel only means
    // the answer may still come; it finds no call and is dropped.
    unsafe {
        if !(s.api.ls.LSCallCancel)(s.handle, call.token, &mut error) {
            crate::log::line(&format!("luna: a cancel was refused: {}", error.take(&s.api.ls)));
        } else {
            (s.api.ls.LSErrorFree)(&mut error);
        }
    }
}

unsafe extern "C" fn on_answer(_handle: *mut c_void, message: *mut c_void, context: *mut c_void) -> bool {
    let id = context as usize as u64;
    let answer = with_state(|s| {
        // SAFETY: a message the hub handed this callback.
        let payload = unsafe { (s.api.ls.LSMessageGetPayload)(message) };
        if payload.is_null() {
            Err(LunaError::Unreadable)
        } else {
            // SAFETY: luna-service2's payload is a C string valid during the callback.
            parse_answer(&unsafe { CStr::from_ptr(payload) }.to_string_lossy())
        }
    });
    let Some(answer) = answer else { return true };
    // A one-reply call ends here; a subscription's first answer ends its timeout.
    let taken = with_state(|s| {
        let call = s.calls.get_mut(&id)?;
        let once = matches!(call.reply, Some(Reply::Once(_)));
        let mut timer = std::mem::replace(&mut call.timer, ptr::null_mut());
        let reply = call.reply.take();
        if once {
            s.calls.remove(&id);
        }
        stop_timer(s, &mut timer);
        reply
    })
    .flatten();
    if let Some(mut reply) = taken
        && let Some(back) = deliver(&mut reply, answer)
    {
        // A subscription goes on, unless it was cancelled meanwhile.
        with_state(|s| {
            if let Some(call) = s.calls.get_mut(&id) {
                call.reply = Some(back);
            }
        });
    }
    true
}

unsafe extern "C" fn on_timeout(context: *mut c_void) -> c_int {
    let id = context as usize as u64;
    let expired = with_state(|s| {
        let mut call = s.calls.remove(&id)?;
        // The source is finishing (this callback returns REMOVE): drop only our reference.
        // SAFETY: the reference taken when the timer was made.
        unsafe { (s.api.glib.g_source_unref)(call.timer) };
        call.timer = ptr::null_mut();
        cancel(s, &mut call);
        call.reply.take()
    })
    .flatten();
    if let Some(mut reply) = expired {
        deliver(&mut reply, Err(LunaError::Timeout));
    }
    G_SOURCE_REMOVE
}

type AfterSlot = Option<Box<dyn FnOnce() + Send>>;

unsafe extern "C" fn on_after(data: *mut c_void) -> c_int {
    // SAFETY: `data` is the slot `handle_request` made; it lives until `free_after`.
    let slot = unsafe { &mut *(data as *mut AfterSlot) };
    if let Some(run) = slot.take()
        && catch_unwind(AssertUnwindSafe(run)).is_err()
    {
        crate::log::line("luna: a timer's handler panicked");
    }
    G_SOURCE_REMOVE
}

unsafe extern "C" fn free_after(data: *mut c_void) {
    // SAFETY: made by Box::into_raw in `handle_request`, freed once, here.
    drop(unsafe { Box::from_raw(data as *mut AfterSlot) });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn answers_read_as_the_web_app_reads_them() {
        assert_eq!(parse_answer(r#"{"returnValue":true,"configs":{"a":1}}"#).unwrap()["configs"]["a"], 1);
        assert_eq!(parse_answer(r#"{"settings":{}}"#).unwrap()["settings"], json!({}));
        assert_eq!(
            parse_answer(r#"{"returnValue":false,"errorCode":-1,"errorText":"Unknown method"}"#),
            Err(LunaError::Refused { code: Some(-1), text: "Unknown method".into() })
        );
        assert_eq!(
            parse_answer(r#"{"returnValue":false,"errorCode":"ERROR_06"}"#),
            Err(LunaError::Refused { code: None, text: "ERROR_06".into() })
        );
        assert_eq!(
            parse_answer(r#"{"returnValue":false}"#),
            Err(LunaError::Refused { code: None, text: "refused".into() })
        );
        assert_eq!(parse_answer("[1]"), Err(LunaError::Unreadable));
        assert_eq!(parse_answer("{"), Err(LunaError::Unreadable));
    }

    #[test]
    fn subscriptions_ask_for_more_answers() {
        assert_eq!(
            subscribing(&json!({ "category": "option", "keys": ["audioGuidance"] })),
            json!({ "category": "option", "keys": ["audioGuidance"], "subscribe": true })
        );
        assert_eq!(subscribing(&Value::Null), json!({ "subscribe": true }));
    }

    #[test]
    fn off_the_tv_there_is_no_luna() {
        if cfg!(not(target_os = "linux")) {
            assert!(Luna::start().is_none());
        }
        assert_eq!(LunaError::Timeout.to_string(), "no answer in time");
    }
}
