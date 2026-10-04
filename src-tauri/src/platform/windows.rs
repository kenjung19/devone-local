use super::*;
use std::{
    fs,
    os::windows::{fs::MetadataExt, io::AsRawHandle, process::CommandExt},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    },
};
pub fn default_home() -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| crate::core::Error::Message("LOCALAPPDATA is unavailable".into()))?;
    Ok(PathBuf::from(base).join("Devone"))
}
pub fn is_link(path: &Path) -> Result<bool> {
    Ok(fs::symlink_metadata(path)?.file_attributes() & 0x400 != 0)
}
pub fn configure(command: &mut Command) {
    command.creation_flags(0x08000000);
}
pub fn open(target: &str) -> Result<()> {
    Command::new(system_executable("explorer.exe")?)
        .arg(target)
        .spawn()?;
    Ok(())
}
pub fn terminal(path: &Path, env: &BTreeMap<String, String>) -> Result<()> {
    Command::new(system_executable("cmd.exe")?)
        .arg("/K")
        .current_dir(path)
        .envs(env)
        .creation_flags(0x00000010)
        .spawn()?;
    Ok(())
}
fn hosts_path() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("SystemRoot")
            .ok_or_else(|| crate::core::Error::Message("SystemRoot unavailable".into()))?,
    )
    .join("System32/drivers/etc/hosts"))
}
const BEGIN: &str = "# BEGIN DEVONE LOCAL";
const END: &str = "# END DEVONE LOCAL";
pub fn hosts_ready(hosts: &[String]) -> bool {
    let Ok(path) = hosts_path() else {
        return false;
    };
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    hosts.iter().all(|h| {
        text.lines().any(|line| {
            let mut parts = line.split('#').next().unwrap_or("").split_whitespace();
            parts.next() == Some("127.0.0.1") && parts.any(|v| v == h)
        })
    })
}
pub fn sync_hosts(hosts: &[String]) -> Result<()> {
    for host in hosts {
        super::validate_local_host(host)?;
    }
    let path = hosts_path()?;
    let original = fs::read_to_string(&path)?;
    let mut output = String::new();
    let mut managed = false;
    let mut blocks = 0;
    for line in original.lines() {
        if line == BEGIN {
            if managed {
                return fail("Malformed DEVONE hosts block");
            }
            managed = true;
            blocks += 1;
            continue;
        }
        if line == END {
            if !managed {
                return fail("Malformed DEVONE hosts block");
            }
            managed = false;
            continue;
        }
        if !managed {
            output.push_str(line);
            output.push_str("\r\n");
        }
    }
    if managed || blocks > 1 {
        return fail("Malformed DEVONE hosts block; no changes made");
    }
    // Do not override an unmanaged mapping owned by the user.
    for host in hosts {
        for line in output.lines() {
            let mut parts = line.split('#').next().unwrap_or("").split_whitespace();
            let address = parts.next();
            if parts.any(|part| part == host) && address != Some("127.0.0.1") {
                return fail(format!("Unmanaged hosts entry conflicts with {host}"));
            }
        }
    }
    output.push_str(BEGIN);
    output.push_str("\r\n");
    for host in hosts {
        output.push_str(&format!("127.0.0.1 {host}\r\n"));
    }
    output.push_str(END);
    output.push_str("\r\n");
    if output == original {
        return Ok(());
    }
    // Preserve a recovery copy and write only our managed block. Requires Windows elevation.
    let backup = path.with_extension("devone-backup");
    if !backup.exists() {
        fs::copy(&path, &backup)?;
    }
    fs::write(path, output).map_err(|e| {
        crate::core::Error::Message(format!(
            "Local DNS setup requires an elevated DEVONE session: {e}"
        ))
    })?;
    let mut flush = Command::new(system_executable("ipconfig.exe")?);
    configure(&mut flush);
    let _ = flush.arg("/flushdns").status();
    Ok(())
}
pub fn trust_ca(path: &Path) -> Result<()> {
    let mut cmd = Command::new(system_executable("certutil.exe")?);
    configure(&mut cmd);
    let output = cmd
        .args(["-user", "-addstore", "Root"])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return fail(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}
pub struct Ownership(HANDLE);
unsafe impl Send for Ownership {}
impl Ownership {
    pub fn new() -> Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
            {
                CloseHandle(handle);
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(Self(handle))
        }
    }
    pub fn owns_tcp_listener(&self, port: u16) -> bool {
        listener_owned_by(port, |pid| unsafe {
            use windows_sys::Win32::System::{
                JobObjects::IsProcessInJob,
                Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
            };
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return false;
            }
            let mut member = 0;
            let valid = IsProcessInJob(process, self.0, &mut member) != 0 && member != 0;
            CloseHandle(process);
            valid
        })
    }
    pub fn terminate(&self) -> Result<()> {
        use windows_sys::Win32::System::JobObjects::{
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
            QueryInformationJobObject, TerminateJobObject,
        };
        unsafe {
            if TerminateJobObject(self.0, 1) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
                if QueryInformationJobObject(
                    self.0,
                    JobObjectBasicAccountingInformation,
                    &mut info as *mut _ as *mut _,
                    std::mem::size_of_val(&info) as u32,
                    std::ptr::null_mut(),
                ) == 0
                {
                    return Err(std::io::Error::last_os_error().into());
                }
                if info.ActiveProcesses == 0 {
                    return Ok(());
                }
                if std::time::Instant::now() >= deadline {
                    return fail("Owned process tree did not exit within shutdown timeout");
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    pub fn attach(&self, child: &Child) -> Result<()> {
        if unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}
impl Drop for Ownership {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub fn ca_trusted(path: &Path) -> bool {
    use windows_sys::Win32::Security::Cryptography::*;
    if !path.is_file() {
        return false;
    }
    let wide = path
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        let mut context: *mut std::ffi::c_void = std::ptr::null_mut();
        if CryptQueryObject(
            CERT_QUERY_OBJECT_FILE,
            wide.as_ptr() as *const _,
            CERT_QUERY_CONTENT_FLAG_CERT,
            CERT_QUERY_FORMAT_FLAG_ALL,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut context,
        ) == 0
        {
            return false;
        }
        let name: Vec<u16> = "ROOT".encode_utf16().chain(Some(0)).collect();
        let store = CertOpenStore(
            CERT_STORE_PROV_SYSTEM_W,
            0,
            0,
            CERT_SYSTEM_STORE_CURRENT_USER | CERT_STORE_READONLY_FLAG,
            name.as_ptr() as *const _,
        );
        if store.is_null() {
            CertFreeCertificateContext(context as *const CERT_CONTEXT);
            return false;
        }
        let found = CertFindCertificateInStore(
            store,
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            0,
            CERT_FIND_EXISTING,
            context,
            std::ptr::null(),
        );
        let trusted = !found.is_null();
        if trusted {
            CertFreeCertificateContext(found);
        }
        CertCloseStore(store, 0);
        CertFreeCertificateContext(context as *const CERT_CONTEXT);
        trusted
    }
}

pub fn remove_ca_trust(path: &Path) -> Result<()> {
    use windows_sys::Win32::Security::Cryptography::*;
    let wide: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut context: *mut std::ffi::c_void = std::ptr::null_mut();
        if CryptQueryObject(
            CERT_QUERY_OBJECT_FILE,
            wide.as_ptr() as *const _,
            CERT_QUERY_CONTENT_FLAG_CERT,
            CERT_QUERY_FORMAT_FLAG_ALL,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut context,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let name: Vec<u16> = "ROOT".encode_utf16().chain(Some(0)).collect();
        let store = CertOpenStore(
            CERT_STORE_PROV_SYSTEM_W,
            0,
            0,
            CERT_SYSTEM_STORE_CURRENT_USER,
            name.as_ptr() as *const _,
        );
        if store.is_null() {
            CertFreeCertificateContext(context as *const CERT_CONTEXT);
            return Err(std::io::Error::last_os_error().into());
        }
        let found = CertFindCertificateInStore(
            store,
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            0,
            CERT_FIND_EXISTING,
            context,
            std::ptr::null(),
        );
        // Deletion consumes only the exact matching certificate context.
        let success = found.is_null() || CertDeleteCertificateFromStore(found) != 0;
        CertCloseStore(store, 0);
        CertFreeCertificateContext(context as *const CERT_CONTEXT);
        if !success {
            return fail(
                "Could not remove the exact DEVONE certificate from the current user store",
            );
        }
    }
    Ok(())
}

pub fn crypt_secret(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>> {
    use windows_sys::Win32::{Foundation::LocalFree, Security::Cryptography::*};
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: bytes.len() as u32,
            pbData: bytes.as_ptr() as *mut u8,
        };
        let mut output: CRYPT_INTEGER_BLOB = std::mem::zeroed();
        let ok = if decrypt {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let data = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize).fill(0);
        LocalFree(output.pbData as *mut _);
        Ok(data)
    }
}
pub fn wildcard_system_ready() -> bool {
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};
    static HEALTH: OnceLock<Arc<Mutex<(bool, Instant, bool)>>> = OnceLock::new();
    let cache = HEALTH
        .get_or_init(|| {
            Arc::new(Mutex::new((
                false,
                Instant::now() - Duration::from_secs(30),
                false,
            )))
        })
        .clone();
    let Ok(mut state) = cache.lock() else {
        return false;
    };
    let ready = state.0;
    if !state.2 && state.1.elapsed() > Duration::from_secs(10) {
        state.2 = true;
        let target = cache.clone();
        std::thread::spawn(move || {
            use windows_sys::Win32::NetworkManagement::Dns::*;
            let name = format!("devone-health-{}.test", uuid::Uuid::new_v4().simple())
                .encode_utf16()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let ok = unsafe {
                let mut records = std::ptr::null_mut();
                let result = DnsQuery_W(
                    name.as_ptr(),
                    DNS_TYPE_A,
                    DNS_QUERY_BYPASS_CACHE | DNS_QUERY_NO_HOSTS_FILE,
                    std::ptr::null_mut(),
                    &mut records,
                    std::ptr::null_mut(),
                );
                let mut p = records;
                let mut found = false;
                while !p.is_null() {
                    if (*p).wType == DNS_TYPE_A
                        && (*p).Data.A.IpAddress.to_ne_bytes() == [127, 0, 0, 1]
                    {
                        found = true;
                    }
                    p = (*p).pNext;
                }
                if !records.is_null() {
                    DnsFree(records as *const _, DnsFreeRecordList);
                }
                result == 0 && found
            };
            if let Ok(mut s) = target.lock() {
                *s = (ok, Instant::now(), false);
            }
        });
    }
    ready
}

pub fn system_executable(name: &str) -> Result<PathBuf> {
    use windows_sys::Win32::System::SystemInformation::{
        GetSystemDirectoryW, GetWindowsDirectoryW,
    };
    if !crate::catalog::safe_segment(name) {
        return fail("Invalid system executable name");
    }
    let mut buffer = [0u16; 32768];
    let length = unsafe {
        if name.eq_ignore_ascii_case("explorer.exe") {
            GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32)
        } else {
            GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32)
        }
    } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buffer[..length])).join(name))
}

pub fn owns_tcp_listener(pid: u32, port: u16) -> bool {
    listener_owned_by(port, |owner| owner == pid)
}
fn listener_owned_by(port: u16, owns: impl Fn(u32) -> bool) -> bool {
    use windows_sys::Win32::NetworkManagement::IpHelper::*;
    unsafe {
        let mut size = 0u32;
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            2,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
        if !(4..=16 * 1024 * 1024).contains(&size) {
            return false;
        }
        let mut buffer = vec![0u32; (size as usize).div_ceil(4)];
        if GetExtendedTcpTable(
            buffer.as_mut_ptr() as *mut _,
            &mut size,
            0,
            2,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        ) != 0
        {
            return false;
        }
        let count = buffer[0] as usize;
        let stride = std::mem::size_of::<MIB_TCPROW_OWNER_PID>();
        if 4 + count * stride > size as usize {
            return false;
        }
        let rows = buffer.as_ptr().add(1) as *const MIB_TCPROW_OWNER_PID;
        (0..count).any(|i| {
            let row = &*rows.add(i);
            owns(row.dwOwningPid)
                && u16::from_be(row.dwLocalPort as u16) == port
                && (row.dwLocalAddr == 0 || row.dwLocalAddr.to_ne_bytes() == [127, 0, 0, 1])
        })
    }
}
#[cfg(test)]
mod secret_tests {
    #[test]
    fn listener_ownership() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(super::owns_tcp_listener(std::process::id(), port));
        assert!(!super::owns_tcp_listener(u32::MAX, port));
    }
    #[test]
    fn dpapi_roundtrip() {
        let plain = b"development-secret";
        let encrypted = super::crypt_secret(plain, false).unwrap();
        assert!(!encrypted.windows(plain.len()).any(|b| b == plain));
        assert_eq!(super::crypt_secret(&encrypted, true).unwrap(), plain);
    }
}
