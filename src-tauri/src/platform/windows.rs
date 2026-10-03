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
    Command::new("explorer.exe").arg(target).spawn()?;
    Ok(())
}
pub fn terminal(path: &Path, env: &BTreeMap<String, String>) -> Result<()> {
    Command::new("cmd.exe")
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
    let mut flush = Command::new("ipconfig.exe");
    configure(&mut flush);
    let _ = flush.arg("/flushdns").status();
    Ok(())
}
pub fn trust_ca(path: &Path) -> Result<()> {
    let mut cmd = Command::new("certutil.exe");
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
