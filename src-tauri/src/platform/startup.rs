//! Normal-user startup entry. Environment autostart is a separate SQLite setting.
use crate::{
    config::Home,
    core::{Result, fail},
};
use serde::Serialize;
#[derive(Serialize)]
pub struct State {
    pub supported: bool,
    pub enabled: bool,
    pub conflict: bool,
}
pub fn command(exe: &std::path::Path, home: &Home) -> Result<String> {
    if !exe.is_absolute() || !home.root().is_absolute() {
        return fail("Startup paths must be absolute");
    }
    let exe = exe.to_string_lossy();
    let root = home.root().to_string_lossy();
    if exe.contains(['"', '\r', '\n']) || root.contains(['"', '\r', '\n']) || root.ends_with('\\') {
        return fail("Unsafe Windows startup path");
    }
    Ok(format!("\"{exe}\" --startup --home \"{root}\""))
}
#[cfg(windows)]
const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const NAME: &str = "DEVONE Local";
pub fn state(home: &Home) -> Result<State> {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::*};
        let expected = command(&std::env::current_exe()?, home)?;
        let actual = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(KEY)
            .ok()
            .and_then(|k| k.get_value::<String, _>(NAME).ok());
        Ok(State {
            supported: true,
            enabled: actual.as_deref() == Some(&expected),
            conflict: actual.is_some_and(|s| s != expected),
        })
    }
    #[cfg(not(windows))]
    {
        let _ = home;
        Ok(State {
            supported: false,
            enabled: false,
            conflict: false,
        })
    }
}
pub fn set(home: &Home, enabled: bool) -> Result<()> {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::*};
        if enabled && state(home)?.conflict {
            return fail(
                "The DEVONE startup entry belongs to another executable/Home; it was not overwritten",
            );
        }
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(KEY)?;
        if enabled {
            key.set_value(NAME, &command(&std::env::current_exe()?, home)?)?;
        } else {
            disable(&key)?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (home, enabled);
        fail("Start with Windows is only supported on Windows")
    }
}
#[cfg(windows)]
fn disable(key: &winreg::RegKey) -> Result<()> {
    if let Err(error) = key.delete_value(NAME)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error.into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn disable_removes_conflicting_value_but_preserves_other_apps() {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!("Software\\DevoneAcceptance\\{}", uuid::Uuid::new_v4());
        let (key, _) = root.create_subkey(&path).unwrap();
        key.set_value(NAME, &"foreign.exe --other-home").unwrap();
        key.set_value("Other App", &"keep.exe").unwrap();
        disable(&key).unwrap();
        assert!(key.get_value::<String, _>(NAME).is_err());
        assert_eq!(key.get_value::<String, _>("Other App").unwrap(), "keep.exe");
        disable(&key).unwrap();
        drop(key);
        root.delete_subkey_all(path).unwrap();
    }
    #[test]
    fn startup_is_one_app_command_with_scoped_home_and_safe_quoting() {
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join("App Folder/devone-local.exe");
        let h = Home::new(d.path().join("My Home"));
        assert_eq!(
            command(&exe, &h).unwrap(),
            format!(
                "\"{}\" --startup --home \"{}\"",
                exe.display(),
                h.root().display()
            )
        );
        assert!(command(&exe, &Home::new(d.path().join("bad\"path"))).is_err());
        assert!(command(std::path::Path::new("relative.exe"), &h).is_err());
    }
}
