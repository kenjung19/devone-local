//! Fixed-scope elevated helper: no user paths or commands are accepted.
use crate::core::{Result, fail};
use winreg::{RegKey, enums::*};
const BASE: &str = r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters\DnsPolicyConfig";
const POLICY: &str = "{9073AE66-0413-46F2-9F35-D0E001000001}";
const OWNER: &str = "DEVONE Local wildcard DNS";
fn key_path() -> String {
    format!("{BASE}\\{POLICY}")
}
pub fn ready() -> bool {
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(key_path()) else {
        return false;
    };
    k.get_value::<String, _>("DisplayName").ok().as_deref() == Some(OWNER)
        && k.get_value::<Vec<String>, _>("Name").ok() == Some(vec![".test".into()])
        && k.get_value::<String, _>("GenericDNSServers")
            .ok()
            .as_deref()
            == Some("127.0.0.1")
        && k.get_value::<u32, _>("ConfigOptions").ok() == Some(8)
}
fn owned() -> bool {
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(key_path()) else {
        return false;
    };
    k.get_value::<String, _>("DisplayName").ok().as_deref() == Some(OWNER)
        && k.get_value::<Vec<String>, _>("Name")
            .ok()
            .is_none_or(|names| names == vec![".test".to_string()])
}
fn preflight() -> Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for base in [
        BASE,
        r"SOFTWARE\Policies\Microsoft\Windows NT\DNSClient\DnsPolicyConfig",
    ] {
        if let Ok(parent) = hklm.open_subkey(base) {
            for entry in parent.enum_keys() {
                let name = entry?;
                if base == BASE && name == POLICY {
                    continue;
                }
                let k = parent.open_subkey(name)?;
                let names = k.get_value::<Vec<String>, _>("Name").unwrap_or_default();
                if names.iter().any(|n| conflicts(n)) {
                    return fail("Another DNS policy owns .test; resolve the conflict first");
                }
            }
        }
    }
    if hklm.open_subkey(key_path()).is_ok() && !owned() {
        return fail("DNS policy key exists with different ownership/configuration");
    }
    Ok(())
}
pub fn configure(remove: bool) -> Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if remove {
        if owned() {
            hklm.delete_subkey_all(key_path())?;
        } else {
            return fail("Owned DNS policy not found; nothing removed");
        }
    } else {
        preflight()?;
        let (k, _) = hklm.create_subkey(key_path())?;
        k.set_value("DisplayName", &OWNER)?;
        k.set_value("Name", &vec![".test".to_string()])?;
        k.set_value("GenericDNSServers", &"127.0.0.1")?;
        k.set_value("ConfigOptions", &8u32)?;
        k.set_value("Version", &2u32)?;
        k.set_value("DisplayName", &OWNER)?;
        k.set_value("Comment", &OWNER)?;
    }
    Ok(())
}
pub fn elevate(remove: bool) -> Result<()> {
    if !remove {
        preflight()?;
    }
    if !remove && ready() {
        return Ok(());
    }
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, WaitForSingleObject},
        UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
    };
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let exe = wide(&std::env::current_exe()?.to_string_lossy());
    let verb = wide("runas");
    let args = wide(if remove {
        "--devone-remove-dns"
    } else {
        "--devone-setup-dns"
    });
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of_val(&info) as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = exe.as_ptr();
        info.lpParameters = args.as_ptr();
        info.nShow = 0;
        if ShellExecuteExW(&mut info) == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(1223) {
                return fail(
                    "DNS setup was cancelled at the UAC prompt. No policy change was confirmed; retry when ready.",
                );
            }
            if error.raw_os_error() == Some(5) {
                return fail(
                    "Windows denied permission for DNS setup. The application remains a normal user process; retry the helper.",
                );
            }
            return Err(error.into());
        }
        let waited = WaitForSingleObject(info.hProcess, 120_000);
        let mut code = 1;
        let ok = GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        if waited != 0 || ok == 0 || code != 0 {
            return fail("DNS helper failed, timed out, or elevation was cancelled; retry Setup");
        }
    }
    if !remove && !ready() {
        return fail("DNS policy verification failed");
    }
    if remove && owned() {
        return fail("The owned DNS policy is still present after removal; retry");
    }
    crate::process::run_checked(
        &crate::platform::system_executable("ipconfig.exe")?,
        &["/flushdns".into()],
        &std::env::current_dir()?,
        &Default::default(),
        std::time::Duration::from_secs(5),
    )?;
    Ok(())
}
fn conflicts(namespace: &str) -> bool {
    let name = namespace.trim_end_matches('.').to_ascii_lowercase();
    name == "test" || name == ".test" || name.ends_with(".test")
}
#[cfg(test)]
mod tests {
    #[test]
    fn foreign_test_namespaces_are_conflicts_but_other_domains_are_not() {
        for name in [
            "test",
            ".test",
            ".TEST.",
            "legacy.test",
            ".sub.test",
            "*.test",
        ] {
            assert!(super::conflicts(name));
        }
        for name in [".com", "example.com", "contest", ".test.example.com"] {
            assert!(!super::conflicts(name));
        }
    }
}
