//! Fixed-scope elevated helper: no user paths or commands are accepted.
use crate::core::{Result, fail};
use winreg::{RegKey, enums::*};
const BASE: &str = r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters\DnsPolicyConfig";
const POLICY: &str = "{9073AE66-0413-46F2-9F35-D0E001000001}";
const OWNER: &str = "DEVONE Local wildcard DNS";
const SETUP_ARG: &str = "--devone-setup-dns";
const REMOVE_ARG: &str = "--devone-remove-dns";
pub fn helper_argument(remove: bool) -> &'static str {
    if remove { REMOVE_ARG } else { SETUP_ARG }
}
pub fn helper_operation(args: &[String]) -> Option<bool> {
    match args {
        [arg] if arg == SETUP_ARG => Some(false),
        [arg] if arg == REMOVE_ARG => Some(true),
        _ => None,
    }
}
fn policy_conflict(message: &str) -> crate::core::Error {
    crate::core::Error::PolicyConflict(format!(
        "DNS POLICY CONFLICT: {message}. DEVONE did not overwrite it; resolve the conflict and retry Local Domains setup."
    ))
}
fn key_path() -> String {
    format!("{BASE}\\{POLICY}")
}
pub fn ready() -> bool {
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(key_path()) else {
        return false;
    };
    ready_key(&k)
}
fn ready_key(k: &RegKey) -> bool {
    k.get_value::<String, _>("DisplayName").ok().as_deref() == Some(OWNER)
        && k.get_value::<Vec<String>, _>("Name").ok() == Some(vec![".test".into()])
        && k.get_value::<String, _>("GenericDNSServers")
            .ok()
            .as_deref()
            == Some("127.0.0.1")
        && k.get_value::<u32, _>("ConfigOptions").ok() == Some(8)
        && k.get_value::<u32, _>("Version").ok() == Some(2)
}
fn optional_matches<T: winreg::types::FromRegValue + PartialEq>(
    k: &RegKey,
    name: &str,
    expected: T,
) -> bool {
    match k.get_value::<T, _>(name) {
        Ok(value) => value == expected,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}
fn owned_key(k: &RegKey) -> bool {
    k.get_value::<String, _>("DisplayName").ok().as_deref() == Some(OWNER)
        && optional_matches(k, "Name", vec![".test".to_string()])
        && optional_matches(k, "GenericDNSServers", "127.0.0.1".to_string())
        && optional_matches(k, "ConfigOptions", 8u32)
        && optional_matches(k, "Version", 2u32)
}
fn owned() -> bool {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(key_path())
        .is_ok_and(|k| owned_key(&k))
}

fn preflight() -> Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for base in [
        BASE,
        r"SOFTWARE\Policies\Microsoft\Windows NT\DNSClient\DnsPolicyConfig",
    ] {
        match hklm.open_subkey(base) {
            Ok(parent) => check_namespaces(&parent, base == BASE)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(policy_conflict(
                    "Windows DNS policies could not be inspected",
                ));
            }
        }
    }
    if hklm.open_subkey(key_path()).is_ok() && !owned() {
        return Err(policy_conflict(
            "The DEVONE policy identifier has different ownership/configuration",
        ));
    }
    Ok(())
}
fn check_namespaces(parent: &RegKey, skip_owned_identifier: bool) -> Result<()> {
    for entry in parent.enum_keys() {
        let name = entry?;
        if skip_owned_identifier && name == POLICY {
            continue;
        }
        let k = parent.open_subkey(name)?;
        let names = match k.get_value::<Vec<String>, _>("Name") {
            Ok(names) => names,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
            Err(_) => {
                return Err(policy_conflict(
                    "A Windows DNS policy has an unreadable/malformed namespace",
                ));
            }
        };
        if names.iter().any(|n| conflicts(n)) {
            return Err(policy_conflict("Another Windows DNS policy manages .test"));
        }
    }
    Ok(())
}
fn check_existing(k: &RegKey, disposition: RegDisposition) -> Result<()> {
    if disposition == REG_OPENED_EXISTING_KEY && !owned_key(k) {
        return Err(policy_conflict(
            "Policy ownership changed before the helper could write",
        ));
    }
    Ok(())
}
pub fn configure(remove: bool) -> Result<()> {
    if unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() } == 0 {
        return Err(crate::core::Error::NotElevated(
            "DNS HELPER NOT ELEVATED: approve the isolated Windows helper; the main application does not need Administrator rights".into(),
        ));
    }
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if remove {
        if owned() {
            hklm.delete_subkey_all(key_path())?;
        } else {
            return fail("Owned DNS policy not found; nothing removed");
        }
    } else {
        preflight()?;
        let (k, disposition) = hklm.create_subkey(key_path())?;
        check_existing(&k, disposition)?;
        write_policy(&k)?;
    }
    Ok(())
}
fn write_policy(k: &RegKey) -> Result<()> {
    k.set_value("DisplayName", &OWNER)?; // attributable marker precedes payload
    k.set_value("Name", &vec![".test".to_string()])?;
    k.set_value("GenericDNSServers", &"127.0.0.1")?;
    k.set_value("ConfigOptions", &8u32)?;
    k.set_value("Version", &2u32)?;
    k.set_value("Comment", &OWNER)?;
    Ok(())
}
pub fn elevate(remove: bool) -> Result<()> {
    if remove && !owned() {
        return Ok(());
    }
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
    let args = wide(helper_argument(remove));
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
                return Err(uac_launch_error(1223));
            }
            if error.raw_os_error() == Some(5) {
                return fail(
                    "Windows denied permission for DNS setup. The application remains a normal user process; retry the helper.",
                );
            }
            return Err(error.into());
        }
        if info.hProcess.is_null() {
            return fail(
                "DNS helper returned no process handle; inspect Local Domains state and retry",
            );
        }
        let waited = WaitForSingleObject(info.hProcess, 120_000);
        let mut code = 1;
        let ok = GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        helper_wait_result(waited, ok != 0, code)?;
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
fn uac_launch_error(code: i32) -> crate::core::Error {
    if code == 1223 {
        crate::core::Error::Cancelled("Local domain setup was cancelled at the Windows UAC prompt. No helper was started and no system settings were changed. Retry Local Domains setup when ready.".into())
    } else {
        crate::core::Error::Message(format!(
            "DNS helper could not start (Windows error {code}); retry Local Domains setup"
        ))
    }
}
fn helper_wait_result(waited: u32, queried: bool, code: u32) -> Result<()> {
    match waited {
        258 => fail(
            "DNS HELPER TIMEOUT: the helper has not confirmed completion. Do not assume no change; inspect Local Domains state before retrying or removing the exact owned policy.",
        ),
        0 if !queried => {
            fail("DNS helper exit status could not be read; inspect Local Domains state and retry")
        }
        0 if code == 0 => Ok(()),
        0 if code == 2 => Err(policy_conflict("The elevated helper detected a conflicting Windows DNS policy")),
        0 if code == 3 => Err(crate::core::Error::NotElevated("DNS HELPER NOT ELEVATED: Windows did not provide an Administrator token; retry the isolated helper".into())),
        0 => fail(format!(
            "DNS HELPER FAILED (exit {code}). Check policy ownership/conflicts and retry Local Domains setup; any partial policy is retained only with its exact DEVONE marker."
        )),
        _ => fail(format!(
            "DNS helper wait failed (Windows status {waited}); inspect Local Domains state and retry"
        )),
    }
}
fn conflicts(namespace: &str) -> bool {
    let name = namespace.trim_end_matches('.').to_ascii_lowercase();
    name == "test" || name == ".test" || name.ends_with(".test")
}
#[cfg(test)]
mod tests {
    #[test]
    fn foreign_registry_namespaces_and_raced_key_are_rejected_without_writes() {
        use super::*;
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!(r"Software\DevoneAcceptance\{}", uuid::Uuid::new_v4());
        let (parent, _) = root.create_subkey(&path).unwrap();
        let (k, _) = parent.create_subkey("foreign").unwrap();
        k.set_value("Name", &vec![".test".to_string()]).unwrap();
        k.set_value("DisplayName", &"Foreign owner").unwrap();
        assert!(check_namespaces(&parent, true).is_err());
        assert!(check_existing(&k, REG_OPENED_EXISTING_KEY).is_err());
        assert_eq!(
            k.get_value::<String, _>("DisplayName").unwrap(),
            "Foreign owner"
        );
        assert!(k.get_raw_value("GenericDNSServers").is_err());
        k.set_value("Name", &vec![".example.com".to_string()])
            .unwrap();
        assert!(check_namespaces(&parent, true).is_ok());
        k.set_value("Name", &"malformed").unwrap();
        assert!(check_namespaces(&parent, true).is_err());
        drop(k);
        drop(parent);
        root.delete_subkey_all(path).unwrap();
    }
    #[test]
    fn helper_arguments_and_results_do_not_accept_arbitrary_input() {
        use super::*;
        use crate::core::Error;
        for remove in [false, true] {
            assert_eq!(
                helper_operation(&[helper_argument(remove).into()]),
                Some(remove)
            );
        }
        for args in [
            vec![],
            vec!["--devone-setup-dns".into(), "D:\\foreign".into()],
            vec!["--devone-setup-dns & whoami".into()],
            vec!["--home".into()],
        ] {
            assert_eq!(helper_operation(&args), None);
        }
        assert!(matches!(uac_launch_error(1223), Error::Cancelled(_)));
        assert_eq!(
            crate::core::helper_exit_code(&policy_conflict("foreign policy")),
            2
        );
        assert!(matches!(
            helper_wait_result(0, true, 2),
            Err(Error::PolicyConflict(_))
        ));
        assert!(matches!(
            helper_wait_result(0, true, 3),
            Err(Error::NotElevated(_))
        ));
        assert!(helper_wait_result(0, true, 0).is_ok()); // retry after cancel
        for (wait, query, code) in [
            (258, true, 259),
            (u32::MAX, true, 0),
            (0, false, 0),
            (0, true, 1),
        ] {
            assert!(helper_wait_result(wait, query, code).is_err());
        }
    }
    #[test]
    fn every_partial_and_altered_nrpt_payload_has_exact_ownership() {
        use super::*;
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!(r"Software\DevoneAcceptance\{}", uuid::Uuid::new_v4());
        let (k, _) = root.create_subkey(&path).unwrap();
        k.set_value("Name", &vec![".test".to_string()]).unwrap();
        assert!(!owned_key(&k)); // Name alone is not attributable.
        k.delete_value("Name").unwrap();
        k.set_value("DisplayName", &OWNER).unwrap();
        assert!(owned_key(&k));
        assert!(!ready_key(&k));
        write_policy(&k).unwrap();
        assert!(owned_key(&k) && ready_key(&k));
        write_policy(&k).unwrap();
        assert!(ready_key(&k)); // idempotent exact payload
        for (name, value) in [("ConfigOptions", 9u32), ("Version", 3u32)] {
            k.set_value(name, &value).unwrap();
            assert!(!owned_key(&k));
            write_policy(&k).unwrap();
        }
        for (name, value) in [
            ("DisplayName", "foreign owner"),
            ("GenericDNSServers", "203.0.113.53"),
        ] {
            k.set_value(name, &value).unwrap();
            assert!(!owned_key(&k));
            write_policy(&k).unwrap();
        }
        k.set_value("Name", &"wrong registry type").unwrap();
        assert!(!owned_key(&k));
        write_policy(&k).unwrap();
        assert!(ready_key(&k));
        drop(k);
        root.delete_subkey_all(&path).unwrap();
        assert!(root.open_subkey(path).is_err());
    }
    #[test]
    fn ownership_rejects_changed_servers_namespace_and_invalid_value_types() {
        use super::*;
        let path = format!(r"Software\DevoneAcceptance\{}", uuid::Uuid::new_v4());
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        assert!(hkcu.open_subkey(&path).is_err());
        let (k, _) = hkcu.create_subkey(&path).unwrap();
        k.set_value("DisplayName", &OWNER).unwrap();
        assert!(owned_key(&k)); // partial helper write is owned and recoverable
        k.set_value("Name", &vec![".test".to_string()]).unwrap();
        k.set_value("GenericDNSServers", &"127.0.0.1").unwrap();
        k.set_value("ConfigOptions", &8u32).unwrap();
        k.set_value("Version", &2u32).unwrap();
        assert!(owned_key(&k));
        k.set_value("GenericDNSServers", &"203.0.113.53").unwrap();
        assert!(!owned_key(&k));
        k.set_value("GenericDNSServers", &"127.0.0.1").unwrap();
        k.set_value("ConfigOptions", &"invalid type").unwrap();
        assert!(!owned_key(&k));
        k.set_value("ConfigOptions", &8u32).unwrap();
        k.set_value("Name", &vec![".example.com".to_string()])
            .unwrap();
        assert!(!owned_key(&k));
        drop(k);
        hkcu.delete_subkey_all(path).unwrap();
    }
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
