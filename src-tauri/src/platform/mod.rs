use crate::core::{Result, fail};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Child, Command},
};
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
use linux as native;
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(windows)]
use windows as native;
pub fn default_home() -> Result<PathBuf> {
    native::default_home()
}
pub fn platform_key() -> String {
    format!(
        "{}-{}",
        std::env::consts::OS,
        match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            other => other,
        }
    )
}
pub fn is_link(path: &Path) -> Result<bool> {
    native::is_link(path)
}
pub fn configure(command: &mut Command) {
    native::configure(command);
}
pub fn open(target: &str) -> Result<()> {
    native::open(target)
}
pub fn terminal(path: &Path, env: &BTreeMap<String, String>) -> Result<()> {
    native::terminal(path, env)
}
pub fn sync_hosts(hosts: &[String]) -> Result<()> {
    native::sync_hosts(hosts)
}
pub fn hosts_ready(hosts: &[String]) -> bool {
    native::hosts_ready(hosts)
}
pub fn trust_ca(path: &Path) -> Result<()> {
    native::trust_ca(path)
}
pub struct Ownership(native::Ownership);
pub fn allow_foreground(pid: u32) {
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(pid);
    }
    #[cfg(not(windows))]
    let _ = pid;
}
pub fn show_startup_error(message: &str) {
    #[cfg(windows)]
    unsafe {
        let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "DEVONE Local".encode_utf16().chain(Some(0)).collect();
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            0x10,
        );
    }
    #[cfg(not(windows))]
    eprintln!("DEVONE: {message}");
}
pub fn remove_ca_trust(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        native::remove_ca_trust(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        fail("Certificate trust management currently supports Windows")
    }
}
pub mod startup;
impl Ownership {
    pub fn new() -> Result<Self> {
        Ok(Self(native::Ownership::new()?))
    }
    pub fn owns_tcp_listener(&self, port: u16) -> bool {
        #[cfg(windows)]
        {
            self.0.owns_tcp_listener(port)
        }
        #[cfg(not(windows))]
        {
            let _ = port;
            false
        }
    }
    pub fn terminate(&self) -> Result<()> {
        #[cfg(windows)]
        {
            self.0.terminate()
        }
        #[cfg(not(windows))]
        {
            Ok(())
        }
    }
    pub fn attach(&self, child: &Child) -> Result<()> {
        self.0.attach(child)
    }
}
pub fn validate_local_host(host: &str) -> Result<()> {
    if !host.ends_with(".test")
        || !host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
    {
        return fail("Invalid managed hostname");
    }
    Ok(())
}

#[cfg(windows)]
mod wildcard;
pub fn wildcard_ready() -> bool {
    #[cfg(windows)]
    {
        wildcard::ready()
    }
    #[cfg(not(windows))]
    {
        false
    }
}
pub fn setup_wildcard(remove: bool) -> Result<()> {
    #[cfg(windows)]
    {
        wildcard::elevate(remove)
    }
    #[cfg(not(windows))]
    {
        let _ = remove;
        fail("Wildcard DNS setup is currently supported on Windows")
    }
}
/// Fixed-scope implementation used by the elevated production helper.
/// This does not elevate or accept arbitrary registry paths/policy identifiers.
#[cfg(windows)]
pub fn configure_wildcard_elevated(remove: bool) -> Result<()> {
    wildcard::configure(remove)?;
    crate::process::run_checked(
        &system_executable("ipconfig.exe")?,
        &["/flushdns".into()],
        &std::env::current_dir()?,
        &BTreeMap::new(),
        std::time::Duration::from_secs(5),
    )?;
    Ok(())
}

pub fn helper_dispatch() -> Option<Result<()>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    #[cfg(windows)]
    {
        wildcard::helper_operation(&args).map(configure_wildcard_elevated)
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        None
    }
}

pub fn ca_trusted(path: &Path) -> bool {
    #[cfg(windows)]
    {
        windows::ca_trusted(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

pub fn protect_secret(bytes: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        windows::crypt_secret(bytes, false)
    }
    #[cfg(not(windows))]
    {
        let _ = bytes;
        fail("Secure credential storage is not implemented on this platform")
    }
}
pub fn unprotect_secret(bytes: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        windows::crypt_secret(bytes, true)
    }
    #[cfg(not(windows))]
    {
        let _ = bytes;
        fail("Secure credential storage is not implemented on this platform")
    }
}

pub fn wildcard_system_ready() -> bool {
    if !wildcard_ready() || crate::dns::server::probe(53).is_err() {
        return false;
    }
    #[cfg(windows)]
    {
        windows::wildcard_system_ready()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn binary_roles(kind: &crate::core::RuntimeType) -> BTreeMap<String, String> {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let pairs: &[(&str, &str)] = match kind {
        crate::core::RuntimeType::Php => &[("cli", "php"), ("fastcgi", "php-cgi")],
        crate::core::RuntimeType::Node => &[("cli", "node")],
        crate::core::RuntimeType::Mysql => &[
            ("server", "bin/mysqld"),
            ("admin", "bin/mysqladmin"),
            ("client", "bin/mysql"),
        ],
        _ => &[("server", "caddy")],
    };
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), format!("{v}{suffix}")))
        .collect()
}

pub fn system_executable(name: &str) -> Result<PathBuf> {
    #[cfg(windows)]
    {
        windows::system_executable(name)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        fail("System executable resolution is not implemented for this platform")
    }
}

pub fn owns_tcp_listener(pid: u32, port: u16) -> bool {
    #[cfg(windows)]
    {
        windows::owns_tcp_listener(pid, port)
    }
    #[cfg(not(windows))]
    {
        let _ = (pid, port);
        false
    }
}

pub fn choose_sql_file() -> Result<Option<PathBuf>> {
    #[cfg(windows)]
    {
        native::choose_sql_file()
    }
    #[cfg(not(windows))]
    {
        fail("SQL file selection currently supports Windows")
    }
}
