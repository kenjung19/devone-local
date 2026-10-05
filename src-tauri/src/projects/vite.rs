use crate::{
    config::Home,
    core::{Result, fail},
    sites::Site,
};
use std::path::{Path, PathBuf};
#[derive(serde::Serialize, serde::Deserialize)]
struct Marker {
    expected: String,
}
fn hot(site: &Site) -> Result<PathBuf> {
    let root = Path::new(&site.project_path).canonicalize()?;
    let public = root.join("public").canonicalize()?;
    if !public.starts_with(&root) || crate::platform::is_link(&public)? {
        return fail("Laravel public directory must remain inside project");
    }
    let target = public.join("hot");
    if std::fs::symlink_metadata(&target).is_ok() && crate::platform::is_link(&target)? {
        return fail("Laravel hot file must not be a link");
    }
    Ok(target)
}
fn marker(home: &Home, site: &Site) -> PathBuf {
    home.path("config")
        .join(format!("vite-hot-{}.json", site.id))
}
pub fn cleanup(home: &Home, site: &Site) -> Result<()> {
    let marker = marker(home, site);
    if !marker.exists() {
        return Ok(());
    }
    let state: Marker = serde_json::from_slice(&std::fs::read(&marker)?)?;
    if !Path::new(&site.project_path).join("public").exists() {
        std::fs::remove_file(marker)?;
        return Ok(());
    }
    let hot = hot(site)?;
    if std::fs::read_to_string(&hot).ok().as_deref() == Some(&state.expected) {
        std::fs::remove_file(hot)?;
    }
    std::fs::remove_file(marker)?;
    Ok(())
}
pub fn prepare(home: &Home, site: &Site) -> Result<()> {
    cleanup(home, site)?;
    if hot(site)?.exists() {
        return fail(
            "public/hot belongs to the user or another dev server. Stop that server and remove its stale hot file before starting DEVONE Vite",
        );
    }
    let state = Marker {
        expected: format!("https://{}/__devone_vite", site.hostname),
    };
    std::fs::write(marker(home, site), serde_json::to_vec(&state)?)?;
    Ok(())
}
pub fn validate(home: &Home, site: &Site) -> Result<()> {
    let state: Marker = serde_json::from_slice(&std::fs::read(marker(home, site))?)?;
    if std::fs::read_to_string(hot(site)?).ok().as_deref() != Some(&state.expected) {
        return fail(
            "Laravel Vite did not produce the expected managed public/hot URL. Custom hotFile configurations require an explicit project integration",
        );
    }
    Ok(())
}
/// Runtime config lives in DEVONE_HOME. Loading project config happens only on explicit Start.
pub fn config(home: &Home, site: &Site, port: u16) -> Result<PathBuf> {
    let project = Path::new(&site.project_path);
    let vite = project.join("node_modules/vite/dist/node/index.js");
    if !vite.is_file() {
        return fail("Project Vite module is missing; install dependencies first");
    }
    let path = home.path("config").join(format!("vite-{}.mjs", site.id));
    let origin = format!("https://{}", site.hostname);
    let base = if site.project_type == "laravel" {
        "/__devone_vite/"
    } else {
        "/"
    };
    let json = |s: &str| serde_json::to_string(s).unwrap();
    std::fs::write(
        &path,
        format!(
            r#"// DEVONE generated development config
import {{pathToFileURL}} from 'node:url';
const {{loadConfigFromFile, mergeConfig}} = await import(pathToFileURL({vite}).href);
export default async (env) => {{
  const loaded = await loadConfigFromFile(env, undefined, {root});
  const config = mergeConfig(loaded?.config ?? {{}}, {{
    root: {root}, base: {base}, server: {{host:'127.0.0.1',port:{port},strictPort:true,https:false,
    origin:{origin},allowedHosts:[{host}],cors:{{origin:{origin}}},hmr:{{protocol:'wss',host:{host},clientPort:443}}}}
  }});
  // Managed listener and browser endpoint take precedence over project dev-server settings.
  config.server = {{...config.server, host:'127.0.0.1', port:{port}, strictPort:true, https:false,
    origin:{origin}, allowedHosts:[{host}], cors:{{origin:{origin}}},
    hmr:{{...((typeof config.server?.hmr === 'object') ? config.server.hmr : {{}}), protocol:'wss',host:{host},clientPort:443}}}};
  return config;
}};
"#,
            vite = json(&vite.to_string_lossy()),
            root = json(&site.project_path),
            base = json(base),
            origin = json(&origin),
            host = json(&site.hostname)
        ),
    )?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_only_cleans_matching_content() {
        let t = tempfile::tempdir().unwrap();
        let home = Home::new(t.path());
        home.ensure().unwrap();
        let root = t.path().join("www/laravel");
        std::fs::create_dir_all(root.join("public")).unwrap();
        let mut a = crate::app::Application::open_with_options(
            home.clone(),
            crate::app::Options {
                system_setup: false,
                autostart: false,
            },
        )
        .unwrap();
        std::fs::write(root.join("artisan"), "").unwrap();
        std::fs::write(root.join("public/index.php"), "").unwrap();
        a.scan().unwrap();
        let s = a.sites().unwrap().remove(0);
        std::fs::write(root.join("public/hot"), "user-owned").unwrap();
        assert!(prepare(&home, &s).is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("public/hot")).unwrap(),
            "user-owned"
        );
        std::fs::remove_file(root.join("public/hot")).unwrap();
        prepare(&home, &s).unwrap();
        std::fs::write(
            root.join("public/hot"),
            format!("https://{}/__devone_vite", s.hostname),
        )
        .unwrap();
        validate(&home, &s).unwrap();
        cleanup(&home, &s).unwrap();
        assert!(!root.join("public/hot").exists());
        prepare(&home, &s).unwrap();
        std::fs::write(root.join("public/hot"), "changed-by-user").unwrap();
        cleanup(&home, &s).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("public/hot")).unwrap(),
            "changed-by-user"
        );
    }
}
