use crate::{
    core::{Result, fail},
    storage::Store,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Editor {
    pub id: String,
    pub name: String,
    pub executable: String,
    pub args: Vec<String>,
    pub category: String,
}
pub trait EditorProvider {
    fn detect(&self) -> Vec<Editor>;
    fn open_project(&self, editor: &Editor, path: &Path) -> Result<()>;
}
pub struct WindowsEditors;
fn candidate(id: &str, name: &str, category: &str, path: PathBuf) -> Option<Editor> {
    path.is_file().then(|| Editor {
        id: id.into(),
        name: name.into(),
        category: category.into(),
        executable: path.to_string_lossy().into(),
        args: vec!["{project}".into()],
    })
}
impl EditorProvider for WindowsEditors {
    fn detect(&self) -> Vec<Editor> {
        if !cfg!(windows) {
            return vec![];
        }
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_default();
        let mut result = vec![];
        for (id, name, relative) in [
            (
                "vscode",
                "Visual Studio Code",
                "Programs/Microsoft VS Code/Code.exe",
            ),
            ("cursor", "Cursor", "Programs/cursor/Cursor.exe"),
        ] {
            if let Some(e) = candidate(id, name, "editor", local.join(relative)) {
                result.push(e);
            }
        }
        for variable in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            let Some(root) = std::env::var_os(variable).map(PathBuf::from) else {
                continue;
            };
            for (id, name, relative, category) in [
                (
                    "vscode",
                    "Visual Studio Code",
                    "Microsoft VS Code/Code.exe",
                    "editor",
                ),
                (
                    "dbeaver",
                    "DBeaver",
                    "DBeaver/dbeaver.exe",
                    "database_client",
                ),
                (
                    "tableplus",
                    "TablePlus",
                    "TablePlus/TablePlus.exe",
                    "database_client",
                ),
                (
                    "workbench",
                    "MySQL Workbench",
                    "MySQL/MySQL Workbench 8.0 CE/MySQLWorkbench.exe",
                    "database_client",
                ),
            ] {
                if !result.iter().any(|e| e.id == id)
                    && let Some(e) = candidate(id, name, category, root.join(relative))
                {
                    result.push(e);
                }
            }
            for base in [
                root.join("JetBrains"),
                root.join("Programs/JetBrains"),
                root.join("MySQL"),
            ] {
                if let Ok(entries) = std::fs::read_dir(base) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_lowercase();
                        for (id, title, prefix, exe, category) in [
                            (
                                "phpstorm",
                                "PhpStorm",
                                "phpstorm",
                                "bin/phpstorm64.exe",
                                "editor",
                            ),
                            (
                                "webstorm",
                                "WebStorm",
                                "webstorm",
                                "bin/webstorm64.exe",
                                "editor",
                            ),
                            (
                                "workbench",
                                "MySQL Workbench",
                                "mysql workbench",
                                "MySQLWorkbench.exe",
                                "database_client",
                            ),
                        ] {
                            if name.starts_with(prefix)
                                && !result.iter().any(|e| e.id == id)
                                && let Some(e) =
                                    candidate(id, title, category, entry.path().join(exe))
                            {
                                result.push(e);
                            }
                        }
                    }
                }
            }
        }
        result
    }
    fn open_project(&self, e: &Editor, path: &Path) -> Result<()> {
        validate(e)?;
        if !path.is_dir() {
            return fail("Project directory does not exist");
        }
        let mut cmd = Command::new(&e.executable);
        crate::platform::configure(&mut cmd);
        let args = e
            .args
            .iter()
            .map(|a| a.replace("{project}", &path.to_string_lossy()))
            .collect::<Vec<_>>();
        cmd.args(args).current_dir(path).spawn()?;
        Ok(())
    }
}
pub fn validate(e: &Editor) -> Result<()> {
    if !crate::catalog::safe_segment(&e.id)
        || e.id.is_empty()
        || !Path::new(&e.executable).is_absolute()
        || !Path::new(&e.executable).is_file()
    {
        return fail("Choose an existing absolute editor executable path");
    }
    if e.name.is_empty()
        || e.name.len() > 100
        || e.args.len() > 32
        || e.args.iter().any(|a| {
            a.contains(['\0', '\n', '\r'])
                || a.len() > 4096
                || a.replace("{project}", "").contains(['{', '}'])
        })
    {
        return fail(
            "Arguments must be a structured array; only {project} is a supported placeholder",
        );
    }
    if !["editor", "database_client"].contains(&e.category.as_str()) {
        return fail("Unknown integration category");
    }
    Ok(())
}
pub fn list(store: &Store) -> Result<Vec<Editor>> {
    let mut all = WindowsEditors.detect();
    if let Some(value) = store.setting("editors.custom")? {
        for e in serde_json::from_str::<Vec<Editor>>(&value)? {
            if validate(&e).is_ok() {
                all.retain(|v| v.id != e.id);
                all.push(e);
            }
        }
    }
    Ok(all)
}
pub fn save(store: &Store, e: Editor) -> Result<()> {
    validate(&e)?;
    if !e.id.starts_with("custom-") {
        return fail("Custom integration ID must start with custom-");
    }
    let mut all: Vec<Editor> = serde_json::from_str(
        &store
            .setting("editors.custom")?
            .unwrap_or_else(|| "[]".into()),
    )?;
    all.retain(|v| v.id != e.id);
    all.push(e);
    store.set_setting("editors.custom", &serde_json::to_string(&all)?)
}
pub fn set_default(store: &Store, id: &str) -> Result<()> {
    if !list(store)?
        .iter()
        .any(|e| e.id == id && e.category == "editor")
    {
        return fail("Editor is not installed");
    }
    store.set_setting("editor.default", id)
}
pub fn open(store: &Store, id: Option<&str>, path: &Path) -> Result<()> {
    let selected = id
        .map(str::to_string)
        .or(store.setting("editor.default")?)
        .ok_or_else(|| {
            crate::core::Error::Message("Set a default editor in Settings first".into())
        })?;
    let e = list(store)?
        .into_iter()
        .find(|e| e.id == selected)
        .ok_or_else(|| {
            crate::core::Error::Message("Selected editor is no longer installed".into())
        })?;
    if e.category == "database_client" {
        validate(&e)?;
        let mut c = Command::new(e.executable);
        crate::platform::configure(&mut c);
        c.spawn()?;
        Ok(())
    } else {
        WindowsEditors.open_project(&e, path)
    }
}
