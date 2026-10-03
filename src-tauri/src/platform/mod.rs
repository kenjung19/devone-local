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
impl Ownership {
    pub fn new() -> Result<Self> {
        Ok(Self(native::Ownership::new()?))
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
