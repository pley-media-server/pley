//! Where pley listens, which an administrator can change while it runs
//! (Settings → Network): the web app and its API on each address chosen, the
//! Jellyfin-compatible API on its own, and discovery (UDP, on the web port) on
//! the web addresses other devices can reach. An address no longer chosen
//! closes and a new one opens; one pley can't listen on yet (an adapter that
//! comes later: Wi-Fi, a VPN) is tried again until it can. The choice is
//! saved in the config file, but what the command line or the environment
//! sets, which the web app can't change.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::{oneshot, watch};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::Config;
use crate::state::AppState;

/// How soon an address pley can't listen on is tried again the first time:
/// a port one of pley's own listeners is giving up is free a moment later.
const FIRST_RETRY: Duration = Duration::from_millis(50);
/// How long a change waits, at most, for its new listeners to listen.
const CHANGE_WAIT: Duration = Duration::from_secs(3);
/// The most addresses of each kind.
const MAX_ADDRESSES: usize = 32;

/// Where pley listens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listen {
    /// The web app and its API.
    pub web: Vec<SocketAddr>,
    /// The Jellyfin-compatible API on addresses of its own: off when empty,
    /// unless on pley's own port.
    #[serde(default)]
    pub jellyfin: Vec<SocketAddr>,
    /// The Jellyfin-compatible API answers on pley's own port, its clients'
    /// requests told apart from the web app's (see [`crate::jellyfin::on_web_port`]).
    #[serde(default)]
    pub jellyfin_on_web: bool,
    /// Answer the pley apps looking for servers on the local network, and
    /// announce pley there by Bonjour (see [`crate::bonjour`]).
    pub discovery: bool,
}

impl Listen {
    pub fn of(config: &Config) -> Self {
        Self {
            web: std::iter::once(config.bind).chain(config.extra_binds.iter().copied()).collect(),
            jellyfin: config.jellyfin_bind.into_iter().chain(config.jellyfin_extra_binds.iter().copied()).collect(),
            jellyfin_on_web: config.jellyfin_on_web,
            discovery: config.discovery,
        }
    }

    /// Every listener this asks for. Discovery answers on the web addresses
    /// of networks only: other devices can't reach a loopback one.
    fn listeners(&self) -> Vec<(Kind, SocketAddr)> {
        let web = self.web.iter().map(|&addr| (Kind::Web, addr));
        let jellyfin = self.jellyfin.iter().map(|&addr| (Kind::Jellyfin, addr));
        let discovery = self
            .web
            .iter()
            .filter(|addr| self.discovery && !addr.ip().is_loopback())
            .map(|&addr| (Kind::Discovery, addr));
        web.chain(jellyfin).chain(discovery).collect()
    }

    /// Why pley can't listen as this says, if it can't.
    pub fn check(&self) -> Result<(), String> {
        if self.web.is_empty() {
            return Err("Pley must listen on an address at least".into());
        }
        if self.jellyfin_on_web && !self.jellyfin.is_empty() {
            return Err("the Jellyfin-compatible API is on Pley's port or on addresses of its own, not both".into());
        }
        if self.web.len() > MAX_ADDRESSES || self.jellyfin.len() > MAX_ADDRESSES {
            return Err(format!("{MAX_ADDRESSES} addresses at most"));
        }
        let all: Vec<SocketAddr> = self.web.iter().chain(&self.jellyfin).copied().collect();
        for (i, addr) in all.iter().enumerate() {
            if addr.port() == 0 {
                return Err(format!("{addr} has no port"));
            }
            // 0.0.0.0 takes its port on every address: one listener a port.
            let overlaps = |other: &SocketAddr| {
                other.port() == addr.port()
                    && (other.ip() == addr.ip() || other.ip().is_unspecified() || addr.ip().is_unspecified())
            };
            if let Some(other) = all[..i].iter().find(|other| overlaps(other)) {
                return Err(if other == addr {
                    format!("{addr} is there twice")
                } else {
                    format!("{other} and {addr} can't both have port {}", addr.port())
                });
            }
        }
        Ok(())
    }
}

/// What the command line or the environment sets: the web app can't change it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Fixed {
    /// `--bind`, `PLEY_BIND`.
    pub web: bool,
    /// `--jellyfin-bind`, `PLEY_JELLYFIN_BIND`.
    pub jellyfin: bool,
    /// `--no-discovery`, `PLEY_NO_DISCOVERY`.
    pub discovery: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Web,
    Jellyfin,
    Discovery,
}

impl Kind {
    fn what(self) -> &'static str {
        match self {
            Kind::Web => "Pley",
            Kind::Jellyfin => "the Jellyfin-compatible API",
            Kind::Discovery => "discovery",
        }
    }
}

/// How `kind` at `addr` reads in the log.
fn shown(kind: Kind, addr: SocketAddr) -> String {
    match kind {
        Kind::Discovery => format!("UDP {addr}"),
        Kind::Web | Kind::Jellyfin => format!("http://{addr}"),
    }
}

/// A listener, and whether it listens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listener {
    pub kind: Kind,
    pub address: SocketAddr,
    /// Why pley doesn't listen there (yet).
    pub error: Option<String>,
}

/// Where pley listens and wants to: see the module's documentation.
pub struct Network {
    pub fixed: Fixed,
    /// Where a change is saved.
    pub config_file: PathBuf,
    /// What is wanted, numbered by change.
    wanted: watch::Sender<(u64, Listen)>,
    /// The last change [`run`] made.
    applied: watch::Sender<u64>,
    listeners: parking_lot::RwLock<BTreeMap<(Kind, SocketAddr), Option<String>>>,
    changing: tokio::sync::Mutex<()>,
}

impl Network {
    pub fn new(config: &Config) -> Self {
        Self {
            fixed: config.network_fixed,
            config_file: config.config_file.clone(),
            wanted: watch::Sender::new((0, Listen::of(config))),
            applied: watch::Sender::new(0),
            listeners: parking_lot::RwLock::new(BTreeMap::new()),
            changing: tokio::sync::Mutex::new(()),
        }
    }

    pub fn wanted(&self) -> Listen {
        self.wanted.borrow().1.clone()
    }

    /// Hears each change once [`run`] has made it (see [`crate::bonjour`]).
    pub fn applied(&self) -> watch::Receiver<u64> {
        self.applied.subscribe()
    }

    /// Whether the Jellyfin-compatible API answers on pley's own port now.
    pub fn jellyfin_on_web(&self) -> bool {
        self.wanted.borrow().1.jellyfin_on_web
    }

    /// The listeners, and how each is.
    pub fn listeners(&self) -> Vec<Listener> {
        self.listeners
            .read()
            .iter()
            .map(|(&(kind, address), error)| Listener { kind, address, error: error.clone() })
            .collect()
    }

    /// The web app's address on this computer when it listens on one, else
    /// its first (`None` before it has a port: tests).
    pub fn web_url(&self) -> Option<String> {
        let wanted = self.wanted.borrow();
        let web = wanted.1.web.iter().filter(|addr| addr.port() != 0);
        let mut local = web.clone().filter(|addr| addr.ip().is_loopback() || addr.ip().is_unspecified());
        local.next().or_else(|| web.clone().next()).map(|&addr| crate::desktop::web_url(addr))
    }

    /// Listens as `listen` says from now on, and from the next start on: it
    /// is saved in the config file first. Returns once the new listeners
    /// listen, or have tried for a moment. `listen` must pass
    /// [`Listen::check`].
    pub async fn change(&self, listen: Listen) -> anyhow::Result<()> {
        let _one_at_a_time = self.changing.lock().await;
        crate::config::save_listen(&self.config_file, &listen, self.fixed)?;
        let change = self.wanted.borrow().0 + 1;
        self.wanted.send_replace((change, listen));
        // [`run`] listens, unless the app was built alone (tests).
        if self.wanted.receiver_count() > 0 {
            let mut applied = self.applied.subscribe();
            let _ = tokio::time::timeout(CHANGE_WAIT * 2, applied.wait_for(|&done| done >= change)).await;
        }
        Ok(())
    }

    fn set(&self, key: (Kind, SocketAddr), error: Option<String>) {
        self.listeners.write().insert(key, error);
    }
}

/// Keeps pley's listeners as [`Network`] wants them until pley stops. `first`
/// listens on the first web address already (pley must, to start). The
/// listeners run on `tracker`, which pley waits for as it stops; each lets
/// the requests in progress finish when it closes.
pub async fn run(state: AppState, app: axum::Router, first: TcpListener, retry: Duration, tracker: TaskTracker) {
    let network = &state.network;
    let mut wanted = network.wanted.subscribe();
    let mut first = Some(first);
    let mut running: HashMap<(Kind, SocketAddr), CancellationToken> = HashMap::new();
    loop {
        let (change, listen) = wanted.borrow_and_update().clone();
        let keys = listen.listeners();
        // Those no longer wanted close first, for new ones to take their ports.
        running.retain(|key, stop| {
            if keys.contains(key) {
                return true;
            }
            stop.cancel();
            network.listeners.write().remove(key);
            tracing::info!("no longer listening on {} ({})", shown(key.0, key.1), key.0.what());
            false
        });
        let mut tried = Vec::new();
        for key in keys {
            if running.contains_key(&key) {
                continue;
            }
            let opened = if key == (Kind::Web, state.config.bind) { first.take() } else { None };
            let stop = state.stopping.child_token();
            let (told, tries) = oneshot::channel();
            tracker.spawn(listen_on(state.clone(), app.clone(), key, opened, retry, stop.clone(), told));
            running.insert(key, stop);
            tried.push(tries);
        }
        // Only ever the first time.
        first = None;
        let deadline = tokio::time::Instant::now() + CHANGE_WAIT;
        for tries in tried {
            let _ = tokio::time::timeout_at(deadline, tries).await;
        }
        network.applied.send_replace(change);
        tokio::select! {
            changed = wanted.changed() => if changed.is_err() { return },
            () = state.stopping.cancelled() => return,
        }
    }
}

enum Socket {
    Tcp(TcpListener),
    Udp(UdpSocket),
}

async fn bind(kind: Kind, addr: SocketAddr) -> io::Result<Socket> {
    Ok(match kind {
        Kind::Web | Kind::Jellyfin => Socket::Tcp(TcpListener::bind(addr).await?),
        Kind::Discovery => Socket::Udp(UdpSocket::bind(addr).await?),
    })
}

/// One listener, until `stop`: on `opened`, or on a socket bound to `addr`
/// as soon as pley can. `tried` hears once it listens, or has tried for a
/// moment.
async fn listen_on(
    state: AppState,
    app: axum::Router,
    (kind, addr): (Kind, SocketAddr),
    opened: Option<TcpListener>,
    retry: Duration,
    stop: CancellationToken,
    tried: oneshot::Sender<()>,
) {
    let network = &state.network;
    let mut tried = Some(tried);
    let started = opened.is_some();
    let socket = match opened {
        Some(listener) => Socket::Tcp(listener),
        None => {
            let mut wait = FIRST_RETRY;
            let mut told = false;
            loop {
                let err = match bind(kind, addr).await {
                    Ok(socket) => break socket,
                    Err(err) => err,
                };
                if stop.is_cancelled() {
                    return;
                }
                // After a few quick tries: not one of pley's own listeners
                // giving the port up.
                let for_now = wait > FIRST_RETRY * 8;
                network.set((kind, addr), Some(err.to_string()));
                if for_now {
                    tried.take().map(|tried| tried.send(()));
                    if !told {
                        tracing::warn!(
                            "{} can't listen on {} yet ({err}): trying every {}s",
                            kind.what(),
                            shown(kind, addr),
                            retry.as_secs_f32()
                        );
                        told = true;
                    }
                }
                tokio::select! {
                    () = stop.cancelled() => return,
                    () = tokio::time::sleep(wait.min(retry)) => {}
                }
                wait = (wait * 2).min(retry.max(FIRST_RETRY * 16));
            }
        }
    };
    if stop.is_cancelled() {
        return;
    }
    network.set((kind, addr), None);
    tried.take().map(|tried| tried.send(()));
    match socket {
        Socket::Tcp(listener) if kind == Kind::Web => {
            if !started {
                // Not "Pley … listening on", which marks a start in the log.
                tracing::info!("listening on http://{addr} too ({})", kind.what());
            }
            crate::http::serve(listener, app, stop.cancelled_owned()).await;
        }
        Socket::Tcp(listener) => crate::jellyfin::serve(state.clone(), listener, stop.cancelled_owned()).await,
        Socket::Udp(socket) => crate::discovery::serve(state.clone(), socket, addr.port(), stop).await,
    }
}

/// A network adapter of this computer, with its IPv4 addresses.
#[derive(Debug, Clone, Serialize)]
pub struct Adapter {
    /// "Wi-Fi", "Ethernet", "eth0".
    pub name: String,
    /// The hardware, on Windows: "Intel(R) Wi-Fi 6 AX201 160MHz".
    pub description: Option<String>,
    /// Connected.
    pub up: bool,
    pub addresses: Vec<Ipv4Addr>,
    /// The network it is on, as Windows knows it.
    pub network: Option<NetworkProfile>,
}

/// A network as Windows knows it: its name, and the kind of network its
/// firewall takes it for (`public`, `private` or `domain`).
#[derive(Debug, Clone, Serialize)]
pub struct NetworkProfile {
    pub name: String,
    pub kind: &'static str,
}

/// This computer's network adapters with an IPv4 address, but loopback, the
/// connected ones first. One that isn't connected doesn't keep the address it
/// gave itself (169.254.x.x): it gets another when it is.
pub fn adapters() -> Vec<Adapter> {
    let mut adapters = platform::adapters();
    for adapter in adapters.iter_mut().filter(|adapter| !adapter.up) {
        adapter.addresses.retain(|ip| !ip.is_link_local());
    }
    adapters.retain(|adapter| !adapter.addresses.is_empty());
    adapters.sort_by(|a, b| b.up.cmp(&a.up).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    adapters
}

#[cfg(windows)]
mod platform {
    use std::collections::HashMap;
    use std::ffi::{CStr, c_void};
    use std::net::Ipv4Addr;
    use std::ptr;

    use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS, SysFreeString};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses,
        IP_ADAPTER_ADDRESSES_LH,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
    use windows_sys::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::core::{BSTR, GUID, HRESULT};

    use super::{Adapter, NetworkProfile};

    /// `IF_TYPE_SOFTWARE_LOOPBACK`.
    const LOOPBACK: u32 = 24;
    /// `IfOperStatusUp`.
    const UP: i32 = 1;

    pub fn adapters() -> Vec<Adapter> {
        // u64s: aligned for the structures written into it.
        let mut buffer: Vec<u64> = vec![0; 2048];
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        loop {
            let mut size = u32::try_from(buffer.len() * 8).unwrap_or(u32::MAX);
            // SAFETY: `buffer` holds `size` bytes.
            let result = unsafe {
                GetAdaptersAddresses(u32::from(AF_INET), flags, ptr::null(), buffer.as_mut_ptr().cast(), &mut size)
            };
            match result {
                ERROR_SUCCESS => break,
                ERROR_BUFFER_OVERFLOW => buffer.resize((size as usize).div_ceil(8), 0),
                _ => return Vec::new(),
            }
        }
        let mut networks = networks();
        let mut adapters = Vec::new();
        let mut next = buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        // SAFETY: the list GetAdaptersAddresses wrote in `buffer`, which
        // outlives the loop: each pointer is null or to one of its entries.
        while let Some(adapter) = unsafe { next.as_ref() } {
            next = adapter.Next;
            if adapter.IfType == LOOPBACK {
                continue;
            }
            let mut addresses = Vec::new();
            let mut unicast = adapter.FirstUnicastAddress;
            // SAFETY: as above.
            while let Some(address) = unsafe { unicast.as_ref() } {
                unicast = address.Next;
                let sockaddr = address.Address.lpSockaddr;
                // SAFETY: an AF_INET address is a SOCKADDR_IN.
                unsafe {
                    if !sockaddr.is_null() && (*sockaddr).sa_family == AF_INET {
                        let ipv4 = &*sockaddr.cast::<SOCKADDR_IN>();
                        addresses.push(Ipv4Addr::from(ipv4.sin_addr.S_un.S_addr.to_ne_bytes()));
                    }
                }
            }
            if addresses.is_empty() {
                continue;
            }
            // SAFETY: strings of the entry, terminated.
            let (id, name, description) = unsafe {
                let id = if adapter.AdapterName.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(adapter.AdapterName.cast()).to_string_lossy().to_uppercase()
                };
                (id, text(adapter.FriendlyName), text(adapter.Description))
            };
            adapters.push(Adapter {
                name,
                description: Some(description).filter(|d| !d.is_empty()),
                up: adapter.OperStatus == UP,
                addresses,
                network: networks.remove(&id),
            });
        }
        adapters
    }

    // Windows' Network List Manager (netlistmgr.h), which tells anyone the
    // networks the adapters are on; its registry is for administrators.
    const NETWORK_LIST_MANAGER: GUID = GUID::from_u128(0xDCB00C01_570F_4A9B_8D69_199FDBA5723B);
    const INETWORK_LIST_MANAGER: GUID = GUID::from_u128(0xDCB00000_570F_4A9B_8D69_199FDBA5723B);
    // Methods by their place in each interface's vtable: IUnknown's three,
    // IDispatch's four, then the interface's own.
    const RELEASE: usize = 2;
    /// `INetworkListManager::GetNetworkConnections`.
    const GET_NETWORK_CONNECTIONS: usize = 9;
    /// `IEnumNetworkConnections::Next`.
    const NEXT: usize = 8;
    /// `INetworkConnection::GetNetwork`.
    const GET_NETWORK: usize = 7;
    /// `INetworkConnection::GetAdapterId`.
    const GET_ADAPTER_ID: usize = 12;
    /// `INetwork::GetName`.
    const GET_NAME: usize = 7;
    /// `INetwork::GetCategory`.
    const GET_CATEGORY: usize = 18;

    type Release = unsafe extern "system" fn(*mut c_void) -> u32;
    type GetObject = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;
    type Next = unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void, *mut u32) -> HRESULT;
    type GetGuid = unsafe extern "system" fn(*mut c_void, *mut GUID) -> HRESULT;
    type GetString = unsafe extern "system" fn(*mut c_void, *mut BSTR) -> HRESULT;
    type GetCategory = unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT;

    /// Method `index` of COM object `object`.
    ///
    /// # Safety
    /// `object` is a live COM object, whose vtable has a method of type `F` at `index`.
    unsafe fn method<F: Copy>(object: *mut c_void, index: usize) -> F {
        // SAFETY: as the caller promises.
        unsafe {
            let vtable = *object.cast::<*const *const c_void>();
            std::mem::transmute_copy(&*vtable.add(index))
        }
    }

    /// # Safety
    /// `object` is a live COM object this code holds a reference to.
    unsafe fn release(object: *mut c_void) {
        // SAFETY: as the caller promises.
        unsafe { method::<Release>(object, RELEASE)(object) };
    }

    /// The networks the connected adapters are on, by adapter id (`{…}`).
    fn networks() -> HashMap<String, NetworkProfile> {
        let mut networks = HashMap::new();
        // SAFETY: COM, on this thread until CoUninitialize; each object is
        // used through its interface's vtable while held, and released once.
        unsafe {
            if CoInitializeEx(ptr::null(), COINIT_MULTITHREADED as u32) < 0 {
                return networks;
            }
            let mut manager = ptr::null_mut();
            if CoCreateInstance(
                &NETWORK_LIST_MANAGER,
                ptr::null_mut(),
                CLSCTX_ALL,
                &INETWORK_LIST_MANAGER,
                &mut manager,
            ) >= 0
            {
                let mut connections = ptr::null_mut();
                if method::<GetObject>(manager, GET_NETWORK_CONNECTIONS)(manager, &mut connections) >= 0
                    && !connections.is_null()
                {
                    loop {
                        let (mut connection, mut fetched) = (ptr::null_mut(), 0u32);
                        let next = method::<Next>(connections, NEXT)(connections, 1, &mut connection, &mut fetched);
                        if next < 0 || fetched == 0 || connection.is_null() {
                            break;
                        }
                        let mut adapter = GUID::from_u128(0);
                        let mut network = ptr::null_mut();
                        if method::<GetGuid>(connection, GET_ADAPTER_ID)(connection, &mut adapter) >= 0
                            && method::<GetObject>(connection, GET_NETWORK)(connection, &mut network) >= 0
                            && !network.is_null()
                        {
                            let mut category = 0;
                            if method::<GetCategory>(network, GET_CATEGORY)(network, &mut category) >= 0 {
                                let mut name: BSTR = ptr::null();
                                let named = method::<GetString>(network, GET_NAME)(network, &mut name) >= 0;
                                let profile = NetworkProfile {
                                    name: if named { text(name) } else { String::new() },
                                    kind: match category {
                                        1 => "private",
                                        2 => "domain",
                                        _ => "public",
                                    },
                                };
                                if !name.is_null() {
                                    SysFreeString(name);
                                }
                                networks.insert(guid_text(&adapter), profile);
                            }
                            release(network);
                        }
                        release(connection);
                    }
                    release(connections);
                }
                release(manager);
            }
            CoUninitialize();
        }
        networks
    }

    /// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`, as adapter ids are written.
    fn guid_text(id: &GUID) -> String {
        let tail: String = id.data4[2..].iter().map(|b| format!("{b:02X}")).collect();
        format!("{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{tail}}}", id.data1, id.data2, id.data3, id.data4[0], id.data4[1])
    }

    /// A terminated UTF-16 string.
    ///
    /// # Safety
    /// `text` is null or points to a terminated string.
    unsafe fn text(text: *const u16) -> String {
        if text.is_null() {
            return String::new();
        }
        let mut len = 0;
        // SAFETY: up to the terminator, which the caller promises.
        unsafe {
            while *text.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
        }
    }
}

#[cfg(unix)]
mod platform {
    use std::ffi::CStr;
    use std::net::Ipv4Addr;

    use super::Adapter;

    pub fn adapters() -> Vec<Adapter> {
        let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
        // SAFETY: getifaddrs fills `list`, freed below.
        if unsafe { libc::getifaddrs(&mut list) } != 0 {
            return Vec::new();
        }
        let mut adapters: Vec<Adapter> = Vec::new();
        let mut next = list;
        // SAFETY: the list getifaddrs made, until freeifaddrs: each pointer is
        // null or to one of its entries, whose names are terminated and
        // whose AF_INET addresses are sockaddr_ins.
        unsafe {
            while let Some(entry) = next.as_ref() {
                next = entry.ifa_next;
                let flags = entry.ifa_flags as libc::c_int;
                if flags & libc::IFF_LOOPBACK != 0
                    || entry.ifa_addr.is_null()
                    || libc::c_int::from((*entry.ifa_addr).sa_family) != libc::AF_INET
                {
                    continue;
                }
                let ipv4 = &*entry.ifa_addr.cast::<libc::sockaddr_in>();
                let address = Ipv4Addr::from(u32::from_be(ipv4.sin_addr.s_addr));
                let name = CStr::from_ptr(entry.ifa_name).to_string_lossy().into_owned();
                match adapters.iter_mut().find(|adapter| adapter.name == name) {
                    Some(adapter) => adapter.addresses.push(address),
                    None => adapters.push(Adapter {
                        name,
                        description: None,
                        up: flags & libc::IFF_UP != 0 && flags & libc::IFF_RUNNING != 0,
                        addresses: vec![address],
                        network: None,
                    }),
                }
            }
            libc::freeifaddrs(list);
        }
        adapters
    }
}

#[cfg(not(any(windows, unix)))]
mod platform {
    pub fn adapters() -> Vec<super::Adapter> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listen(web: &[&str], jellyfin: &[&str], discovery: bool) -> Listen {
        Listen {
            web: web.iter().map(|a| a.parse().unwrap()).collect(),
            jellyfin: jellyfin.iter().map(|a| a.parse().unwrap()).collect(),
            jellyfin_on_web: false,
            discovery,
        }
    }

    #[test]
    fn discovery_answers_on_networks_only() {
        let both = listen(&["127.0.0.1:7539", "192.168.1.10:7539"], &["127.0.0.1:8096"], true);
        assert_eq!(
            both.listeners(),
            [
                (Kind::Web, "127.0.0.1:7539".parse().unwrap()),
                (Kind::Web, "192.168.1.10:7539".parse().unwrap()),
                (Kind::Jellyfin, "127.0.0.1:8096".parse().unwrap()),
                (Kind::Discovery, "192.168.1.10:7539".parse().unwrap()),
            ]
        );
        assert!(!listen(&["127.0.0.1:7539"], &[], true).listeners().iter().any(|(kind, _)| *kind == Kind::Discovery));
        assert_eq!(listen(&["0.0.0.0:7539"], &[], false).listeners().len(), 1, "discovery off");
    }

    #[test]
    fn what_pley_can_listen_on() {
        assert!(listen(&["127.0.0.1:7539", "192.168.1.10:7539"], &["127.0.0.1:8096"], true).check().is_ok());
        assert!(listen(&["0.0.0.0:7539"], &["0.0.0.0:8096"], true).check().is_ok());
        assert!(listen(&[], &["0.0.0.0:8096"], true).check().is_err(), "nowhere");
        assert!(listen(&["127.0.0.1:0"], &[], true).check().is_err(), "no port");
        let twice = listen(&["127.0.0.1:7539", "127.0.0.1:7539"], &[], true).check().unwrap_err();
        assert_eq!(twice, "127.0.0.1:7539 is there twice");
        let covered = listen(&["0.0.0.0:7539", "192.168.1.10:7539"], &[], true).check().unwrap_err();
        assert_eq!(covered, "0.0.0.0:7539 and 192.168.1.10:7539 can't both have port 7539");
        assert!(listen(&["127.0.0.1:7539"], &["0.0.0.0:7539"], true).check().is_err(), "one port for both APIs");
        assert!(listen(&["127.0.0.1:7539"], &["192.168.1.10:7539"], true).check().is_ok(), "other addresses");
    }

    #[test]
    fn adapters_have_addresses_and_no_loopback() {
        for adapter in adapters() {
            assert!(!adapter.addresses.is_empty(), "{adapter:?}");
            assert!(!adapter.addresses.iter().any(Ipv4Addr::is_loopback), "{adapter:?}");
        }
    }
}
