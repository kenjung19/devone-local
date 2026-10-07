use crate::core::{Result, fail};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
pub fn cancelled(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::SeqCst) {
        fail("Cancelled by user")
    } else {
        Ok(())
    }
}
pub fn download(url: &str, hash: &str, flag: &AtomicBool) -> Result<Vec<u8>> {
    if !url.starts_with("https://")
        || hash.len() != 64
        || !hash.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return fail("Remote source requires HTTPS and exact SHA-256");
    }
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(120))
        .user_agent("DEVONE Local")
        .build()
        .map_err(|e| crate::core::Error::Message(e.to_string()))?;
    let mut response = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| crate::core::Error::Message(e.to_string()))?;
    let mut bytes = vec![];
    let mut chunk = [0; 65536];
    loop {
        cancelled(flag)?;
        crate::operation::check()?;
        let n = response.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() > 256 * 1024 * 1024 {
            return fail("Download exceeds 256 MiB");
        }
    }
    crate::runtime::verify_archive_checksum(&bytes, hash)?;
    Ok(bytes)
}
pub fn extract(bytes: Vec<u8>, root: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| crate::core::Error::Message(e.to_string()))?;
    let mut size = 0;
    for i in 0..archive.len() {
        let mut f = archive
            .by_index(i)
            .map_err(|e| crate::core::Error::Message(e.to_string()))?;
        size += f.size();
        if size > 1024 * 1024 * 1024 || f.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return fail("Unsafe or oversized archive");
        }
        let relative = f
            .enclosed_name()
            .ok_or_else(|| crate::core::Error::Message("Archive traversal rejected".into()))?;
        let name = relative.to_string_lossy().replace('\\', "/");
        if name.is_empty() {
            continue;
        }
        if !crate::catalog::safe_relative(name.trim_end_matches('/')) {
            return fail("Invalid archive path");
        }
        let target = root.join(&relative);
        if f.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            std::fs::create_dir_all(target.parent().unwrap())?;
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)?;
            std::io::copy(&mut f, &mut out)?;
        }
    }
    Ok(())
}
pub fn contained(root: &Path, path: &Path) -> Result<PathBuf> {
    if crate::platform::is_link(root)? {
        return fail("Managed root cannot be a link");
    }
    let canonical = root.canonicalize()?;
    let candidate = path.canonicalize()?;
    if candidate == canonical || !candidate.starts_with(&canonical) {
        return fail("Path escapes managed root");
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| crate::core::Error::Message("Invalid contained path".into()))?;
    let mut current = root.to_path_buf();
    for part in relative.components() {
        current.push(part);
        if crate::platform::is_link(&current)? {
            return fail("Links are not permitted inside managed staging");
        }
    }
    Ok(candidate)
}
pub fn cleanup(root: &Path, path: &Path) -> Result<()> {
    contained(root, path)?;
    fn check_tree(path: &Path) -> Result<()> {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if crate::platform::is_link(&entry.path())? {
                return fail(
                    "Staging contains a link; preserved for explicit recovery instead of recursively deleting it",
                );
            }
            if entry.file_type()?.is_dir() {
                check_tree(&entry.path())?;
            }
        }
        Ok(())
    }
    check_tree(path)?;
    std::fs::remove_dir_all(path)?;
    Ok(())
}
