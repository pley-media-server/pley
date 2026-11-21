//! The Service Control Manager: installing, starting, stopping and removing
//! the service, and running as it (`pley service run`).

use std::ffi::c_void;
use std::process::ExitCode;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    ERROR_CALL_NOT_IMPLEMENTED, ERROR_SERVICE_ALREADY_RUNNING, ERROR_SERVICE_DOES_NOT_EXIST,
    ERROR_SERVICE_MARKED_FOR_DELETE, ERROR_SERVICE_NOT_ACTIVE, ERROR_SERVICE_SPECIFIC_ERROR, ERROR_SUCCESS, NO_ERROR,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_MULTI_SZ, RegCloseKey, RegOpenKeyExW, RegSetValueExW,
};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, ControlService, CreateServiceW, DeleteService,
    OpenSCManagerW, OpenServiceW, QueryServiceStatusEx, RegisterServiceCtrlHandlerExW, SC_ACTION, SC_ACTION_RESTART,
    SC_HANDLE, SC_MANAGER_CONNECT, SC_MANAGER_CREATE_SERVICE, SC_STATUS_PROCESS_INFO, SERVICE_ACCEPT_PRESHUTDOWN,
    SERVICE_ACCEPT_STOP, SERVICE_ALL_ACCESS, SERVICE_AUTO_START, SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
    SERVICE_CONFIG_DESCRIPTION, SERVICE_CONFIG_FAILURE_ACTIONS, SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
    SERVICE_CONFIG_PRESHUTDOWN_INFO, SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO, SERVICE_CONFIG_SERVICE_SID_INFO,
    SERVICE_CONTROL_INTERROGATE, SERVICE_CONTROL_PRESHUTDOWN, SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP,
    SERVICE_DELAYED_AUTO_START_INFO, SERVICE_DESCRIPTIONW, SERVICE_ERROR_NORMAL, SERVICE_FAILURE_ACTIONS_FLAG,
    SERVICE_FAILURE_ACTIONSW, SERVICE_PRESHUTDOWN_INFO, SERVICE_QUERY_STATUS, SERVICE_REQUIRED_PRIVILEGES_INFOW,
    SERVICE_RUNNING, SERVICE_SID_INFO, SERVICE_SID_TYPE_UNRESTRICTED, SERVICE_START, SERVICE_START_PENDING,
    SERVICE_STATUS, SERVICE_STATUS_PROCESS, SERVICE_STOP, SERVICE_STOP_PENDING, SERVICE_STOPPED, SERVICE_TABLE_ENTRYW,
    SERVICE_WIN32_OWN_PROCESS, SetServiceStatus, StartServiceCtrlDispatcherW, StartServiceW,
};

use super::NAME;
use crate::config::{Cli, Config};

/// Its writes are checked against its service SID too (`SERVICE_SID_TYPE_RESTRICTED`).
const SERVICE_SID_TYPE_RESTRICTED: u32 = 3;
/// `DELETE`, a standard access right.
const DELETE: u32 = 0x0001_0000;
/// The privileges the service keeps, restricted or not (the restriction is
/// about where it may write): going through folders it may not list, and
/// creating the shared objects GPU drivers use. Not impersonation, which a
/// compromised service could use to become SYSTEM.
const PRIVILEGES: &[&str] = &["SeChangeNotifyPrivilege", "SeCreateGlobalPrivilege"];

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// `strings` as a double NUL-terminated list.
fn multi_sz<S: AsRef<str>>(strings: &[S]) -> Vec<u16> {
    let mut out: Vec<u16> = strings.iter().flat_map(|s| s.as_ref().encode_utf16().chain(Some(0))).collect();
    out.push(0);
    if strings.is_empty() {
        out.push(0);
    }
    out
}

fn last_error(what: &str) -> anyhow::Error {
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(5) {
        anyhow::anyhow!("{what}: access denied (run this as an administrator)")
    } else {
        anyhow::anyhow!("{what}: {err}")
    }
}

/// A Service Control Manager or service handle, closed when dropped.
struct Handle(SC_HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenSCManagerW, OpenServiceW or CreateServiceW.
        unsafe { CloseServiceHandle(self.0) };
    }
}

fn manager(access: u32) -> anyhow::Result<Handle> {
    // SAFETY: no machine or database name: this computer's active database.
    let handle = unsafe { OpenSCManagerW(null(), null(), access) };
    if handle.is_null() {
        return Err(last_error("opening the Service Control Manager"));
    }
    Ok(Handle(handle))
}

/// The pley service, `None` when it isn't installed.
fn open(manager: &Handle, access: u32) -> anyhow::Result<Option<Handle>> {
    let name = wide(NAME);
    // SAFETY: valid manager handle and NUL-terminated name.
    let handle = unsafe { OpenServiceW(manager.0, name.as_ptr(), access) };
    if handle.is_null() {
        if std::io::Error::last_os_error().raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32) {
            return Ok(None);
        }
        return Err(last_error("opening the Pley service"));
    }
    Ok(Some(Handle(handle)))
}

fn state(service: &Handle) -> anyhow::Result<SERVICE_STATUS_PROCESS> {
    let mut status = SERVICE_STATUS_PROCESS::default();
    let mut needed = 0;
    // SAFETY: the buffer is a SERVICE_STATUS_PROCESS of the size given.
    let ok = unsafe {
        QueryServiceStatusEx(
            service.0,
            SC_STATUS_PROCESS_INFO,
            (&raw mut status).cast(),
            size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(last_error("asking the Pley service how it is"));
    }
    Ok(status)
}

/// Waits until the service is in `wanted` (or stopped, when it can't get
/// there), for at most `timeout`.
fn wait_for(service: &Handle, wanted: u32, timeout: Duration) -> anyhow::Result<SERVICE_STATUS_PROCESS> {
    let deadline = Instant::now() + timeout;
    loop {
        let status = state(service)?;
        if status.dwCurrentState == wanted || status.dwCurrentState == SERVICE_STOPPED {
            return Ok(status);
        }
        anyhow::ensure!(Instant::now() < deadline, "the Pley service took longer than {}s", timeout.as_secs());
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// What `pley service install` sets up.
pub struct Definition {
    pub command_line: String,
    /// `NT SERVICE\pley` or `.\user`.
    pub account: String,
    /// The user's (none for the virtual account).
    pub password: Option<String>,
    pub restricted: bool,
    /// `NAME=value` lines added to the service's environment.
    pub environment: Vec<String>,
}

/// Creates the service, or updates the one there is, as `definition` says.
pub fn install(definition: &Definition) -> anyhow::Result<()> {
    let manager = manager(SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE)?;
    let command_line = wide(&definition.command_line);
    let account = wide(&definition.account);
    let password = definition.password.as_deref().map(wide);
    let password = password.as_ref().map_or(null(), |p| p.as_ptr());
    let display = wide("Pley Media Server");
    let service = match open(&manager, SERVICE_ALL_ACCESS)? {
        Some(service) => {
            // SAFETY: valid handle and NUL-terminated strings; nulls leave the
            // load order group, tag and dependencies as they are.
            let ok = unsafe {
                ChangeServiceConfigW(
                    service.0,
                    SERVICE_WIN32_OWN_PROCESS,
                    SERVICE_AUTO_START,
                    SERVICE_ERROR_NORMAL,
                    command_line.as_ptr(),
                    null(),
                    null_mut(),
                    null(),
                    account.as_ptr(),
                    password,
                    display.as_ptr(),
                )
            };
            if ok == 0 {
                return Err(last_error("updating the Pley service"));
            }
            service
        }
        None => {
            let name = wide(NAME);
            // SAFETY: valid manager handle and NUL-terminated strings.
            let handle = unsafe {
                CreateServiceW(
                    manager.0,
                    name.as_ptr(),
                    display.as_ptr(),
                    SERVICE_ALL_ACCESS,
                    SERVICE_WIN32_OWN_PROCESS,
                    SERVICE_AUTO_START,
                    SERVICE_ERROR_NORMAL,
                    command_line.as_ptr(),
                    null(),
                    null_mut(),
                    null(),
                    account.as_ptr(),
                    password,
                )
            };
            if handle.is_null() {
                if std::io::Error::last_os_error().raw_os_error() == Some(ERROR_SERVICE_MARKED_FOR_DELETE as i32) {
                    anyhow::bail!(
                        "Windows is still removing the previous Pley service: close the Services window and try again"
                    );
                }
                return Err(last_error("creating the Pley service"));
            }
            Handle(handle)
        }
    };

    let mut description =
        wide("Streams your films, shows and music to the Pley apps. Its options are in pley.toml in its data folder.");
    set_config(
        &service,
        SERVICE_CONFIG_DESCRIPTION,
        &SERVICE_DESCRIPTIONW { lpDescription: description.as_mut_ptr() },
    )?;
    // After the services Windows needs at startup, and network shares.
    set_config(
        &service,
        SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
        &SERVICE_DELAYED_AUTO_START_INFO { fDelayedAutostart: 1 },
    )?;
    // Restarted when it stops on its own, crashed or not (it couldn't listen, say).
    let mut actions = [
        SC_ACTION { Type: SC_ACTION_RESTART, Delay: 10_000 },
        SC_ACTION { Type: SC_ACTION_RESTART, Delay: 30_000 },
        SC_ACTION { Type: SC_ACTION_RESTART, Delay: 60_000 },
    ];
    let failure = SERVICE_FAILURE_ACTIONSW {
        dwResetPeriod: 24 * 60 * 60,
        lpRebootMsg: null_mut(),
        lpCommand: null_mut(),
        cActions: actions.len() as u32,
        lpsaActions: actions.as_mut_ptr(),
    };
    set_config(&service, SERVICE_CONFIG_FAILURE_ACTIONS, &failure)?;
    set_config(
        &service,
        SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
        &SERVICE_FAILURE_ACTIONS_FLAG { fFailureActionsOnNonCrashFailures: 1 },
    )?;
    // Time to close the database as Windows shuts down: more than
    // crate::SHUTDOWN_DEADLINE, after which pley ends anyway.
    set_config(&service, SERVICE_CONFIG_PRESHUTDOWN_INFO, &SERVICE_PRESHUTDOWN_INFO { dwPreshutdownTimeout: 50_000 })?;
    let sid_type = if definition.restricted { SERVICE_SID_TYPE_RESTRICTED } else { SERVICE_SID_TYPE_UNRESTRICTED };
    set_config(&service, SERVICE_CONFIG_SERVICE_SID_INFO, &SERVICE_SID_INFO { dwServiceSidType: sid_type })?;
    let mut privileges = multi_sz(PRIVILEGES);
    set_config(
        &service,
        SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO,
        &SERVICE_REQUIRED_PRIVILEGES_INFOW { pmszRequiredPrivileges: privileges.as_mut_ptr() },
    )?;
    ServiceKey::open()?.set_multi_sz("Environment", &definition.environment)
}

fn set_config<T>(service: &Handle, level: u32, info: &T) -> anyhow::Result<()> {
    // SAFETY: `info` is the structure that `level` takes.
    let ok = unsafe { ChangeServiceConfig2W(service.0, level, (info as *const T).cast()) };
    if ok == 0 {
        return Err(last_error("configuring the Pley service"));
    }
    Ok(())
}

/// The service's registry key, for what ChangeServiceConfig2 doesn't set.
struct ServiceKey(HKEY);

impl ServiceKey {
    fn open() -> anyhow::Result<Self> {
        let path = wide(&format!(r"SYSTEM\CurrentControlSet\Services\{NAME}"));
        let mut key: HKEY = null_mut();
        // SAFETY: NUL-terminated path and valid out pointer.
        let status = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, KEY_SET_VALUE, &mut key) };
        if status != ERROR_SUCCESS {
            anyhow::bail!(
                "opening the Pley service's registry key: {}",
                std::io::Error::from_raw_os_error(status as i32)
            );
        }
        Ok(Self(key))
    }

    fn set_multi_sz(&self, name: &str, values: &[String]) -> anyhow::Result<()> {
        let data = multi_sz(values);
        let name = wide(name);
        // SAFETY: valid key; `data` is a double NUL-terminated list, its size in bytes.
        let status = unsafe {
            RegSetValueExW(self.0, name.as_ptr(), 0, REG_MULTI_SZ, data.as_ptr().cast(), (data.len() * 2) as u32)
        };
        if status != ERROR_SUCCESS {
            anyhow::bail!(
                "setting the Pley service's environment: {}",
                std::io::Error::from_raw_os_error(status as i32)
            );
        }
        Ok(())
    }
}

impl Drop for ServiceKey {
    fn drop(&mut self) {
        // SAFETY: the key was opened by `ServiceKey::open`.
        unsafe { RegCloseKey(self.0) };
    }
}

/// Starts the service and waits until it runs.
pub fn start(timeout: Duration) -> anyhow::Result<()> {
    let manager = manager(SC_MANAGER_CONNECT)?;
    let service = open(&manager, SERVICE_START | SERVICE_QUERY_STATUS)?
        .ok_or_else(|| anyhow::anyhow!("the Pley service isn't installed: pley service install"))?;
    // SAFETY: valid handle, no arguments.
    if unsafe { StartServiceW(service.0, 0, null()) } == 0
        && std::io::Error::last_os_error().raw_os_error() != Some(ERROR_SERVICE_ALREADY_RUNNING as i32)
    {
        return Err(last_error("starting the Pley service"));
    }
    let status = wait_for(&service, SERVICE_RUNNING, timeout)?;
    if status.dwCurrentState != SERVICE_RUNNING {
        anyhow::bail!("the Pley service stopped as it started ({}): its log says why", exit_code(&status));
    }
    Ok(())
}

/// Stops the service, if it is installed and runs, and waits until it has
/// stopped. `false` when it isn't installed.
pub fn stop(timeout: Duration) -> anyhow::Result<bool> {
    let manager = manager(SC_MANAGER_CONNECT)?;
    let Some(service) = open(&manager, SERVICE_STOP | SERVICE_QUERY_STATUS)? else { return Ok(false) };
    stop_service(&service, timeout)?;
    Ok(true)
}

fn stop_service(service: &Handle, timeout: Duration) -> anyhow::Result<()> {
    if state(service)?.dwCurrentState == SERVICE_STOPPED {
        return Ok(());
    }
    let mut status = SERVICE_STATUS::default();
    // SAFETY: valid handle and status buffer.
    if unsafe { ControlService(service.0, SERVICE_CONTROL_STOP, &mut status) } == 0
        && std::io::Error::last_os_error().raw_os_error() != Some(ERROR_SERVICE_NOT_ACTIVE as i32)
    {
        return Err(last_error("stopping the Pley service"));
    }
    wait_for(service, SERVICE_STOPPED, timeout)?;
    Ok(())
}

/// Stops and removes the service, if it is installed.
pub fn uninstall() -> anyhow::Result<()> {
    let manager = manager(SC_MANAGER_CONNECT)?;
    let Some(service) = open(&manager, DELETE | SERVICE_STOP | SERVICE_QUERY_STATUS)? else { return Ok(()) };
    stop_service(&service, Duration::from_secs(60))?;
    // SAFETY: valid handle opened with DELETE.
    if unsafe { DeleteService(service.0) } == 0
        && std::io::Error::last_os_error().raw_os_error() != Some(ERROR_SERVICE_MARKED_FOR_DELETE as i32)
    {
        return Err(last_error("removing the Pley service"));
    }
    Ok(())
}

fn exit_code(status: &SERVICE_STATUS_PROCESS) -> String {
    if status.dwWin32ExitCode == ERROR_SERVICE_SPECIFIC_ERROR {
        format!("Pley's exit code {}", status.dwServiceSpecificExitCode)
    } else {
        std::io::Error::from_raw_os_error(status.dwWin32ExitCode as i32).to_string()
    }
}

// ---- running as the service ----

/// The configuration `service_main` serves, loaded before Windows calls it.
static CONFIG: parking_lot::Mutex<Option<Config>> = parking_lot::Mutex::new(None);
static STATUS_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
/// The state last reported, so that a late report never undoes a later one.
static STATE: parking_lot::Mutex<u32> = parking_lot::Mutex::new(SERVICE_STOPPED);
static CHECKPOINT: AtomicU32 = AtomicU32::new(0);
static EXIT_CODE: AtomicU32 = AtomicU32::new(0);
/// How long starting may go without a word, reported each second.
const START_HINT: u32 = 10_000;

/// `pley service run`: hands the main thread to the Service Control Manager,
/// which runs the server through `service_main` until it is told to stop.
pub fn dispatch(cli: &Cli) -> ExitCode {
    // Loaded first, so that whatever goes wrong is in the log (a service has
    // no console).
    let mut config = match Config::load(cli) {
        Ok(config) => config,
        Err(err) => {
            crate::logging::init(&cli.log, Some(&Config::fallback_log_file(cli)));
            tracing::error!("Pley could not start: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    crate::logging::init(&cli.log, Some(&config.log_file()));
    // Tailscale's service may switch to the profile of whichever account
    // connects to it: never this service's (see crate::tailscale).
    config.tailscale_socket = Err(crate::tailscale::NOT_AS_A_SERVICE.into());
    *CONFIG.lock() = Some(config);
    let mut name = wide(NAME);
    let table = [
        SERVICE_TABLE_ENTRYW { lpServiceName: name.as_mut_ptr(), lpServiceProc: Some(service_main) },
        SERVICE_TABLE_ENTRYW::default(),
    ];
    // SAFETY: a table ending with an empty entry; `name` outlives the call,
    // which returns once the service has stopped.
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        let err = std::io::Error::last_os_error();
        tracing::error!("`pley service run` is for Windows to start the service ({err}): use `pley service start`");
        return ExitCode::FAILURE;
    }
    if EXIT_CODE.load(Ordering::SeqCst) == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Called by Windows on a thread of its own: serves until stopped.
unsafe extern "system" fn service_main(_argc: u32, _argv: *mut windows_sys::core::PWSTR) {
    let name = wide(NAME);
    // SAFETY: NUL-terminated name and a handler that lives as long as the process.
    let handle = unsafe { RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control), null()) };
    if handle.is_null() {
        tracing::error!("could not register with the Service Control Manager: {}", std::io::Error::last_os_error());
        return;
    }
    STATUS_HANDLE.store(handle, Ordering::SeqCst);
    report(SERVICE_START_PENDING, 0, START_HINT);
    let Some(config) = CONFIG.lock().take() else {
        report(SERVICE_STOPPED, 1, 0);
        return;
    };
    // Running once pley listens, its database open (an upgrade can take a
    // while); starting until then, unless it stopped or is stopping.
    std::thread::spawn(|| {
        while !crate::listening() {
            if !advance(SERVICE_START_PENDING, SERVICE_START_PENDING, START_HINT) {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        advance(SERVICE_START_PENDING, SERVICE_RUNNING, 0);
    });
    let code = match crate::serve(config, false) {
        Ok(()) => 0,
        Err(err) => {
            tracing::error!("Pley could not start: {err:#}");
            1
        }
    };
    EXIT_CODE.store(code, Ordering::SeqCst);
    report(SERVICE_STOPPED, code, 0);
}

/// Windows asks the service to stop (or the system shuts down).
unsafe extern "system" fn control(control: u32, _event: u32, _data: *mut c_void, _context: *mut c_void) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_PRESHUTDOWN | SERVICE_CONTROL_SHUTDOWN => {
            // Within crate::SHUTDOWN_DEADLINE, which ends it anyway.
            report(SERVICE_STOP_PENDING, 0, 50_000);
            crate::request_shutdown();
            NO_ERROR
        }
        SERVICE_CONTROL_INTERROGATE => NO_ERROR,
        _ => ERROR_CALL_NOT_IMPLEMENTED,
    }
}

/// Tells Windows the service's state.
fn report(state: u32, exit: u32, wait_hint: u32) {
    set_state(None, state, exit, wait_hint);
}

/// Like `report`, if the service is still in state `from`: whether it was.
fn advance(from: u32, state: u32, wait_hint: u32) -> bool {
    set_state(Some(from), state, 0, wait_hint)
}

fn set_state(from: Option<u32>, state: u32, exit: u32, wait_hint: u32) -> bool {
    let mut current = STATE.lock();
    if from.is_some_and(|from| *current != from) {
        return false;
    }
    *current = state;
    let pending = matches!(state, SERVICE_START_PENDING | SERVICE_STOP_PENDING);
    // Stopping works while it starts too: the server stops as soon as it can.
    let accepts = matches!(state, SERVICE_START_PENDING | SERVICE_RUNNING);
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if accepts { SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_PRESHUTDOWN } else { 0 },
        dwWin32ExitCode: if exit == 0 { NO_ERROR } else { ERROR_SERVICE_SPECIFIC_ERROR },
        dwServiceSpecificExitCode: exit,
        dwCheckPoint: if pending { CHECKPOINT.fetch_add(1, Ordering::SeqCst) + 1 } else { 0 },
        dwWaitHint: wait_hint,
    };
    // SAFETY: the handle RegisterServiceCtrlHandlerExW gave, and a valid status.
    unsafe { SetServiceStatus(STATUS_HANDLE.load(Ordering::SeqCst), &status) };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_end_with_two_nuls() {
        assert_eq!(multi_sz(&["a", "b"]), [97, 0, 98, 0, 0]);
        assert_eq!(multi_sz::<&str>(&[]), [0, 0]);
    }
}
