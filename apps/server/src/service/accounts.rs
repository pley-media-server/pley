//! Accounts for the service: its SID, the local user it may run as, that
//! user's right to run services and sign-ins to network shares, and who may
//! open the data folder.

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};

use anyhow::Context;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_MEMBER_IN_ALIAS, GetLastError, LocalFree, WAIT_OBJECT_0,
};
use windows_sys::Win32::NetworkManagement::NetManagement::{
    LOCALGROUP_MEMBERS_INFO_0, NERR_Success as NERR_SUCCESS, NERR_UserExists as NERR_USER_EXISTS,
    NERR_UserNotFound as NERR_USER_NOT_FOUND, NetApiBufferFree, NetLocalGroupAddMembers, NetUserAdd, NetUserDel,
    NetUserGetInfo, NetUserSetInfo, UF_DONT_EXPIRE_PASSWD, UF_PASSWD_CANT_CHANGE, UF_SCRIPT, USER_INFO_1,
    USER_INFO_1003, USER_PRIV_USER,
};
use windows_sys::Win32::Security::Authentication::Identity::{
    LSA_HANDLE, LSA_OBJECT_ATTRIBUTES, LSA_UNICODE_STRING, LsaAddAccountRights, LsaClose, LsaNtStatusToWinError,
    LsaOpenPolicy, LsaRemoveAccountRights, POLICY_CREATE_ACCOUNT, POLICY_LOOKUP_NAMES,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW, GetNamedSecurityInfoW,
    SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_DOMAIN_PASSWORD, CREDENTIALW, CredWriteW,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, LookupAccountNameW, LookupAccountSidW, OBJECT_SECURITY_INFORMATION,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
};
use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessWithLogonW, GetExitCodeProcess, LOGON_WITH_PROFILE,
    PROCESS_INFORMATION, STARTUPINFOW, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::DeleteProfileW;

/// The built-in Users group, whatever this Windows calls it.
const USERS: &str = "S-1-5-32-545";
/// What `pley service store-credential` reads, set only for it.
const CREDENTIAL_ENV: [&str; 3] = ["PLEY_CREDENTIAL_TARGET", "PLEY_CREDENTIAL_USER", "PLEY_CREDENTIAL_PASSWORD"];

fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

/// A NUL-terminated wide string at `p`.
///
/// # Safety
/// `p` points to a NUL-terminated UTF-16 string.
unsafe fn from_wide(p: *const u16) -> String {
    let mut len = 0;
    // SAFETY: the caller's promise.
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` characters before the NUL.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
}

/// The SID of `NT SERVICE\<name>`: S-1-5-80 and the SHA-1 of the name in
/// upper case (UTF-16), as five little-endian numbers. Computed, since it
/// is needed before Windows knows the service, and so is the same as `sc
/// showsid` gives.
pub fn service_sid(name: &str) -> String {
    let name: Vec<u8> = name.to_uppercase().encode_utf16().flat_map(u16::to_le_bytes).collect();
    let digest = ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, &name);
    let (parts, _) = digest.as_ref().as_chunks::<4>();
    let parts: Vec<String> = parts.iter().map(|part| u32::from_le_bytes(*part).to_string()).collect();
    format!("S-1-5-80-{}", parts.join("-"))
}

/// A user of this computer, not a domain's of the same name.
fn local(account: &str) -> String {
    match std::env::var("COMPUTERNAME") {
        Ok(computer) if !account.contains('\\') && !account.contains('@') => format!(r"{computer}\{account}"),
        _ => account.to_owned(),
    }
}

/// The binary SID of `account`.
fn lookup(account: &str) -> anyhow::Result<Vec<u8>> {
    let name = wide(account);
    let (mut sid_len, mut domain_len, mut kind) = (0u32, 0u32, 0);
    // SAFETY: sizing call: null buffers with zero lengths.
    unsafe {
        LookupAccountNameW(null(), name.as_ptr(), null_mut(), &mut sid_len, null_mut(), &mut domain_len, &mut kind)
    };
    anyhow::ensure!(sid_len > 0, "there is no account {account} ({})", std::io::Error::last_os_error());
    let mut sid = vec![0u8; sid_len as usize];
    let mut domain = vec![0u16; domain_len as usize];
    // SAFETY: buffers of the sizes the first call asked for.
    let ok = unsafe {
        LookupAccountNameW(
            null(),
            name.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut sid_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut kind,
        )
    };
    anyhow::ensure!(ok != 0, "looking up {account}: {}", std::io::Error::last_os_error());
    Ok(sid)
}

fn sid_string(sid: &[u8]) -> anyhow::Result<String> {
    let mut text = null_mut();
    // SAFETY: a valid SID; Windows allocates the string, freed below.
    if unsafe { ConvertSidToStringSidW(sid.as_ptr() as PSID, &mut text) } == 0 {
        anyhow::bail!("reading a SID: {}", std::io::Error::last_os_error());
    }
    // SAFETY: ConvertSidToStringSidW gave a NUL-terminated string.
    let result = unsafe { from_wide(text) };
    // SAFETY: allocated by ConvertSidToStringSidW with LocalAlloc.
    unsafe { LocalFree(text.cast()) };
    Ok(result)
}

/// The SID of `account`, as Windows finds it (this computer's accounts
/// first, then the domain's).
pub fn sid(account: &str) -> anyhow::Result<String> {
    sid_string(&lookup(account)?)
}

/// The SID of the local user `user`, never a domain's of the same name.
pub fn local_user_sid(user: &str) -> anyhow::Result<String> {
    sid_string(&lookup(&local(user))?)
}

/// The name of the account with the SID `sid` (`S-1-…`), in this Windows' language.
fn name_of(sid: &str) -> anyhow::Result<String> {
    let text = wide(sid);
    let mut binary: PSID = null_mut();
    // SAFETY: NUL-terminated string; Windows allocates the SID, freed below.
    if unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut binary) } == 0 {
        anyhow::bail!("reading {sid}: {}", std::io::Error::last_os_error());
    }
    let (mut name_len, mut domain_len, mut kind) = (0u32, 0u32, 0);
    // SAFETY: sizing call.
    unsafe { LookupAccountSidW(null(), binary, null_mut(), &mut name_len, null_mut(), &mut domain_len, &mut kind) };
    let mut name = vec![0u16; name_len.max(1) as usize];
    let mut domain = vec![0u16; domain_len.max(1) as usize];
    // SAFETY: buffers of the sizes asked for.
    let ok = unsafe {
        LookupAccountSidW(
            null(),
            binary,
            name.as_mut_ptr(),
            &mut name_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut kind,
        )
    };
    // SAFETY: allocated by ConvertStringSidToSidW with LocalAlloc.
    unsafe { LocalFree(binary) };
    anyhow::ensure!(ok != 0, "looking up {sid}: {}", std::io::Error::last_os_error());
    Ok(String::from_utf16_lossy(&name[..name_len as usize]))
}

fn net_error(code: u32) -> anyhow::Error {
    match code {
        2245 => anyhow::anyhow!("the password doesn't meet this computer's password rules (length or complexity)"),
        2221 => anyhow::anyhow!("there is no such user"),
        // Most are Windows errors (access denied, say); the others NetAPI's own.
        code if !(2100..3000).contains(&code) => std::io::Error::from_raw_os_error(code as i32).into(),
        code => anyhow::anyhow!("Windows' user accounts said no (NetAPI error {code})"),
    }
}

/// What a user `create_or_update_user` made says about itself.
const COMMENT: &str = "Runs the pley media server";

/// A password nobody needs to know (the service's user's): random, with
/// the kinds of characters password rules ask for.
pub fn random_password() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the system's random numbers");
    let random: String = bytes.iter().map(|b| CHARS[usize::from(*b) % CHARS.len()] as char).collect();
    format!("{random}-Aa1")
}

/// Whether the user `name` is one `create_or_update_user` made; `None`
/// when there is no such user.
fn made_by_pley(name: &str) -> anyhow::Result<Option<bool>> {
    let name_w = wide(name);
    let mut buffer: *mut u8 = null_mut();
    // SAFETY: NetAPI allocates the USER_INFO_1, freed below.
    match unsafe { NetUserGetInfo(null(), name_w.as_ptr(), 1, &mut buffer) } {
        NERR_SUCCESS => {}
        NERR_USER_NOT_FOUND => return Ok(None),
        status => return Err(net_error(status).context(format!("looking up the user {name}"))),
    }
    // SAFETY: level 1 gives a USER_INFO_1 whose comment is NUL-terminated or null.
    let comment = unsafe {
        let info = &*(buffer as *const USER_INFO_1);
        let comment = if info.usri1_comment.is_null() { String::new() } else { from_wide(info.usri1_comment) };
        NetApiBufferFree(buffer.cast());
        comment
    };
    Ok(Some(comment == COMMENT))
}

/// Creates `name`, a standard user whose password doesn't expire, or sets
/// the password of the one an earlier install made. Someone else's account
/// of that name is left alone.
pub fn create_or_update_user(name: &str, password: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        made_by_pley(name)? != Some(false),
        "there is a user named {name} already, not one Pley made: choose another name"
    );
    let mut name_w = wide(name);
    let mut password_w = wide(password);
    let mut comment = wide(COMMENT);
    let info = USER_INFO_1 {
        usri1_name: name_w.as_mut_ptr(),
        usri1_password: password_w.as_mut_ptr(),
        usri1_password_age: 0,
        usri1_priv: USER_PRIV_USER,
        usri1_home_dir: null_mut(),
        usri1_comment: comment.as_mut_ptr(),
        usri1_flags: UF_SCRIPT | UF_DONT_EXPIRE_PASSWD | UF_PASSWD_CANT_CHANGE,
        usri1_script_path: null_mut(),
    };
    let mut bad = 0;
    // SAFETY: a USER_INFO_1 for level 1, its strings alive for the call.
    match unsafe { NetUserAdd(null(), 1, (&raw const info).cast(), &mut bad) } {
        NERR_SUCCESS => {}
        NERR_USER_EXISTS => {
            let info = USER_INFO_1003 { usri1003_password: password_w.as_mut_ptr() };
            // SAFETY: a USER_INFO_1003 for level 1003.
            let status = unsafe { NetUserSetInfo(null(), name_w.as_ptr(), 1003, (&raw const info).cast(), &mut bad) };
            if status != NERR_SUCCESS {
                return Err(net_error(status).context(format!("setting {name}'s password")));
            }
        }
        status => return Err(net_error(status).context(format!("creating the user {name}"))),
    }
    // A member of Users, as a user made in Settings is: what they may read
    // and run, they may.
    let users = name_of(USERS)?;
    let mut sid = lookup(&local(name))?;
    let member = LOCALGROUP_MEMBERS_INFO_0 { lgrmi0_sid: sid.as_mut_ptr().cast() };
    let users_w = wide(&users);
    // SAFETY: one LOCALGROUP_MEMBERS_INFO_0 for level 0.
    match unsafe { NetLocalGroupAddMembers(null(), users_w.as_ptr(), 0, (&raw const member).cast(), 1) } {
        NERR_SUCCESS | ERROR_MEMBER_IN_ALIAS => Ok(()),
        status => Err(net_error(status).context(format!("adding {name} to {users}"))),
    }
}

/// The local security policy, closed when dropped.
struct Policy(LSA_HANDLE);

impl Policy {
    fn open(access: i32) -> anyhow::Result<Self> {
        let attributes = LSA_OBJECT_ATTRIBUTES::default();
        let mut handle: LSA_HANDLE = 0;
        // SAFETY: zeroed attributes, as LsaOpenPolicy wants; this computer's policy.
        let status = unsafe { LsaOpenPolicy(null(), &attributes, access as u32, &mut handle) };
        check(status, "opening the local security policy")?;
        Ok(Self(handle))
    }
}

impl Drop for Policy {
    fn drop(&mut self) {
        // SAFETY: opened by LsaOpenPolicy.
        unsafe { LsaClose(self.0) };
    }
}

fn check(status: i32, what: &str) -> anyhow::Result<()> {
    if status == 0 {
        return Ok(());
    }
    // SAFETY: converts a status code, nothing else.
    let code = unsafe { LsaNtStatusToWinError(status) };
    Err(anyhow::anyhow!("{what}: {}", std::io::Error::from_raw_os_error(code as i32)))
}

/// Gives `account` the user right `right`, such as SeServiceLogonRight.
pub fn grant_right(account: &str, right: &str) -> anyhow::Result<()> {
    let mut sid = lookup(&local(account))?;
    let policy = Policy::open(POLICY_LOOKUP_NAMES | POLICY_CREATE_ACCOUNT)?;
    let mut right_w: Vec<u16> = right.encode_utf16().collect();
    let bytes = (right_w.len() * 2) as u16;
    let right_string = LSA_UNICODE_STRING { Length: bytes, MaximumLength: bytes, Buffer: right_w.as_mut_ptr() };
    // SAFETY: a valid SID and one counted string.
    let status = unsafe { LsaAddAccountRights(policy.0, sid.as_mut_ptr().cast(), &right_string, 1) };
    check(status, &format!("giving {account} {right}"))
}

/// Deletes the local user `name`, its user rights and its profile, if it is
/// one `create_or_update_user` made. Nothing to do when it doesn't exist.
pub fn delete_user(name: &str) -> anyhow::Result<()> {
    match made_by_pley(name)? {
        None => return Ok(()),
        Some(false) => anyhow::bail!("{name} isn't a user Pley made: it stays"),
        Some(true) => {}
    }
    let mut sid = lookup(&local(name))?;
    let sid_text = sid_string(&sid)?;
    // Rights stay in the policy after the user is gone, under its SID.
    if let Ok(policy) = Policy::open(POLICY_LOOKUP_NAMES) {
        // SAFETY: a valid SID; all of its rights, so no list.
        unsafe { LsaRemoveAccountRights(policy.0, sid.as_mut_ptr().cast(), true, null(), 0) };
    }
    let name_w = wide(name);
    // SAFETY: NUL-terminated name.
    match unsafe { NetUserDel(null(), name_w.as_ptr()) } {
        NERR_SUCCESS | NERR_USER_NOT_FOUND => {}
        status => return Err(net_error(status).context(format!("deleting the user {name}"))),
    }
    let sid_w = wide(&sid_text);
    // SAFETY: NUL-terminated SID string; the profile's path is looked up.
    if unsafe { DeleteProfileW(sid_w.as_ptr(), null(), null()) } == 0 {
        // SAFETY: reads this thread's last error.
        let err = unsafe { GetLastError() };
        if err != ERROR_FILE_NOT_FOUND {
            println!("(its profile stays: {})", std::io::Error::from_raw_os_error(err as i32));
        }
    }
    Ok(())
}

/// A program in Windows' System32 folder, never one found on PATH.
fn system32(program: &str) -> std::path::PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| OsString::from(r"C:\Windows"));
    Path::new(&root).join("System32").join(program)
}

/// Who owns `path`, as SDDL names them: `BA` (the administrators), `SY`
/// (SYSTEM), or a SID.
pub fn owner(path: &Path) -> anyhow::Result<String> {
    let sddl =
        security(path, OWNER_SECURITY_INFORMATION).with_context(|| format!("reading who owns {}", path.display()))?;
    Ok(sddl.trim_start_matches("O:").to_owned())
}

/// The parts `info` names of `path`'s security descriptor, in SDDL.
fn security(path: &Path, info: OBJECT_SECURITY_INFORMATION) -> anyhow::Result<String> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: a NUL-terminated path; Windows allocates the descriptor, freed below.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            info,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    anyhow::ensure!(status == 0, "{}", std::io::Error::from_raw_os_error(status as i32));
    let mut text = null_mut();
    // SAFETY: the descriptor read above; Windows allocates the string, freed below.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, SDDL_REVISION_1, info, &mut text, null_mut())
    };
    let error = std::io::Error::last_os_error();
    // SAFETY: a NUL-terminated string from Windows, when it gave one.
    let sddl = (ok != 0).then(|| unsafe { from_wide(text) });
    // SAFETY: both allocated by Windows with LocalAlloc (or null).
    unsafe {
        LocalFree(text.cast());
        LocalFree(descriptor);
    }
    sddl.ok_or_else(|| anyhow::Error::from(error))
}

/// Makes the folder `dir` (not its parents) open to SYSTEM and the
/// administrators alone from the moment it exists: nobody can put anything
/// in it before [`protect_folder`] has run.
pub fn create_private_folder(dir: &Path) -> anyhow::Result<()> {
    let sddl: Vec<u16> = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)".encode_utf16().chain(Some(0)).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: a NUL-terminated SDDL string; Windows allocates the descriptor, freed below.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    };
    anyhow::ensure!(ok != 0, "making a security descriptor: {}", std::io::Error::last_os_error());
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: a NUL-terminated path and attributes that outlive the call.
    let made = unsafe { CreateDirectoryW(wide.as_ptr(), &attributes) };
    let error = std::io::Error::last_os_error();
    // SAFETY: allocated by Windows with LocalAlloc.
    unsafe { LocalFree(descriptor) };
    anyhow::ensure!(made != 0, "creating {}: {error}", dir.display());
    Ok(())
}

/// Lets only SYSTEM, the administrators and `allowed` (SIDs, with icacls'
/// `M` or `F`) into `dir` and everything in it. The administrators own it
/// all, and what anyone else was allowed is gone: a folder someone made in
/// ProgramData before pley was installed stays theirs otherwise.
///
/// Unless `every_file`, a folder an earlier install protected for the same
/// accounts is left as it is: going through every file again takes minutes
/// once there are artwork and seek previews for a library. What is in it
/// was made since by those accounts alone, and what they made inherits the
/// folder's permissions, unless one of them changed it. Whether it went
/// through every file.
pub fn protect_folder(dir: &Path, allowed: &[(String, &str)], every_file: bool) -> anyhow::Result<bool> {
    if !every_file {
        let current = security(dir, OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION)
            .with_context(|| format!("reading who may open {}", dir.display()))?;
        // Written as Windows writes it (its names for some accounts, such as
        // LA for the built-in Administrator), or not at all: every file then.
        let wanted = protected_sddl(allowed).and_then(|sddl| canonical(&sddl).ok());
        if wanted.is_some_and(|wanted| same_security(&current, &wanted)) {
            return Ok(false);
        }
    }
    icacls(dir, &["/setowner", "*S-1-5-32-544", "/T", "/Q"])?;
    // Only what the folder inherits, everywhere in it…
    icacls(dir, &["/reset", "/T", "/Q"])?;
    // …and the folder inherits nothing: what it allows is all there is.
    let mut grants = vec!["*S-1-5-18:(OI)(CI)F".to_owned(), "*S-1-5-32-544:(OI)(CI)F".to_owned()];
    grants.extend(allowed.iter().map(|(sid, access)| format!("*{sid}:(OI)(CI){access}")));
    let mut args = vec!["/inheritance:r", "/grant:r"];
    args.extend(grants.iter().map(String::as_str));
    args.push("/Q");
    icacls(dir, &args)?;
    Ok(true)
}

/// The owner and DACL [`protect_folder`] gives the folder for `allowed`, in
/// SDDL: owned by the administrators, inheriting nothing, and open to
/// SYSTEM, the administrators and `allowed` alone, for them and everything
/// in it. None for access other than icacls' `M` and `F`.
pub(super) fn protected_sddl(allowed: &[(String, &str)]) -> Option<String> {
    // One entry per account: icacls replaces an account's earlier grant.
    let mut grants: Vec<(&str, &str)> = vec![("SY", "FA"), ("BA", "FA")];
    for (sid, access) in allowed {
        let rights = match *access {
            "F" => "FA",
            // Read, write, run and delete.
            "M" => "0x1301bf",
            _ => return None,
        };
        grants.retain(|(other, _)| !other.eq_ignore_ascii_case(sid));
        grants.push((sid.as_str(), rights));
    }
    let aces: String = grants.iter().map(|(sid, rights)| format!("(A;OICI;{rights};;;{sid})")).collect();
    Some(format!("O:BAD:P{aces}"))
}

/// `sddl` (an owner and DACL) as Windows writes it back.
pub(super) fn canonical(sddl: &str) -> anyhow::Result<String> {
    let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: a NUL-terminated SDDL string; Windows allocates the descriptor, freed below.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    };
    anyhow::ensure!(ok != 0, "reading {sddl}: {}", std::io::Error::last_os_error());
    let mut back = null_mut();
    // SAFETY: the descriptor made above; Windows allocates the string, freed below.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut back,
            null_mut(),
        )
    };
    let error = std::io::Error::last_os_error();
    // SAFETY: a NUL-terminated string from Windows, when it gave one.
    let result = (ok != 0).then(|| unsafe { from_wide(back) });
    // SAFETY: both allocated by Windows with LocalAlloc (or null).
    unsafe {
        LocalFree(back.cast());
        LocalFree(descriptor);
    }
    result.ok_or_else(|| anyhow::anyhow!("writing {sddl}: {error}"))
}

/// Whether two owners and DACLs in SDDL are the same: the same owner, both
/// protected or not, and the same entries in any order (Windows keeps
/// them in the order they were added).
pub(super) fn same_security(a: &str, b: &str) -> bool {
    fn parts(sddl: &str) -> Option<(&str, bool, Vec<&str>)> {
        let (owner, dacl) = sddl.strip_prefix("O:")?.split_once("D:")?;
        let (flags, aces) = dacl.split_at(dacl.find('(').unwrap_or(dacl.len()));
        let mut aces: Vec<&str> =
            if aces.is_empty() { Vec::new() } else { aces.strip_prefix('(')?.strip_suffix(')')?.split(")(").collect() };
        aces.sort_unstable();
        Some((owner, flags.contains('P'), aces))
    }
    matches!((parts(a), parts(b)), (Some(a), Some(b)) if a == b)
}

fn icacls(dir: &Path, args: &[&str]) -> anyhow::Result<()> {
    let output =
        std::process::Command::new(system32("icacls.exe")).arg(dir).args(args).output().context("running icacls")?;
    anyhow::ensure!(
        output.status.success(),
        "icacls {}: {}{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

/// Saves the sign-in `share_user` / `share_password` to `server` for the
/// local user `user`, by running `pley service store-credential` as them:
/// Windows keeps sign-ins per user, and only theirs are used when the
/// service reaches `\\server`.
pub fn store_credential(
    exe: &Path,
    user: &str,
    password: &str,
    server: &str,
    share_user: &str,
    share_password: &str,
) -> anyhow::Result<()> {
    // This environment, without the passwords given to `install`, and with
    // the sign-in.
    let mut env: Vec<(OsString, OsString)> = std::env::vars_os()
        .filter(|(key, _)| !key.to_string_lossy().to_ascii_uppercase().starts_with("PLEY_"))
        .collect();
    for (key, value) in CREDENTIAL_ENV.iter().zip([server, share_user, share_password]) {
        env.push((key.into(), value.into()));
    }
    env.sort_by_key(|(key, _)| key.to_string_lossy().to_uppercase());
    let mut block: Vec<u16> = Vec::new();
    for (key, value) in &env {
        block.extend(key.encode_wide().chain("=".encode_utf16()).chain(value.encode_wide()).chain(Some(0)));
    }
    block.push(0);

    let user_w = wide(user);
    let domain = wide(".");
    let password_w = wide(password);
    let app = wide(exe);
    let mut command_line =
        wide(format!("{} service store-credential", crate::desktop::autostart::windows_quote(&exe.to_string_lossy())));
    let directory = wide(system32(""));
    let startup = STARTUPINFOW { cb: size_of::<STARTUPINFOW>() as u32, ..Default::default() };
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: NUL-terminated strings, a double NUL-terminated environment
    // block, and startup and process structures of the right sizes.
    let ok = unsafe {
        CreateProcessWithLogonW(
            user_w.as_ptr(),
            domain.as_ptr(),
            password_w.as_ptr(),
            LOGON_WITH_PROFILE,
            app.as_ptr(),
            command_line.as_mut_ptr(),
            CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
            block.as_ptr().cast(),
            directory.as_ptr(),
            &startup,
            &mut process,
        )
    };
    anyhow::ensure!(ok != 0, "signing in as {user}: {}", std::io::Error::last_os_error());
    let mut code = 1;
    // SAFETY: handles CreateProcessWithLogonW gave, closed after.
    unsafe {
        if WaitForSingleObject(process.hProcess, 30_000) == WAIT_OBJECT_0 {
            GetExitCodeProcess(process.hProcess, &mut code);
        }
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
    }
    anyhow::ensure!(code == 0, "Windows didn't save the sign-in for {user} (exit code {code})");
    Ok(())
}

/// `pley service store-credential`: saves the sign-in its environment
/// gives for the account running it, as `cmdkey /add` would.
pub fn store_credential_from_env() -> anyhow::Result<()> {
    let [target, user, password] =
        CREDENTIAL_ENV.map(|key| std::env::var(key).with_context(|| format!("{key} isn't set")));
    let (target, user, password) = (target?, user?, password?);
    let mut target_w = wide(&target);
    let mut user_w = wide(&user);
    let mut blob: Vec<u8> = password.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_DOMAIN_PASSWORD,
        TargetName: target_w.as_mut_ptr(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: user_w.as_mut_ptr(),
        ..Default::default()
    };
    // SAFETY: a CREDENTIALW whose strings and blob outlive the call.
    if unsafe { CredWriteW(&credential, 0) } == 0 {
        anyhow::bail!("saving the sign-in to {target}: {}", std::io::Error::last_os_error());
    }
    Ok(())
}
