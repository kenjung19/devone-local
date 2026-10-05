use devone::{
    app::{Application, Options},
    catalog,
    config::Home,
    core::{RuntimeRef, RuntimeType},
    ipc::{Action, Shared, execute_shared},
    runtime,
};
use mysql::{Conn, OptsBuilder, prelude::Queryable};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

fn open(home: &Home) -> Shared {
    Arc::new(Mutex::new(
        Application::open_with_options(
            home.clone(),
            Options {
                system_setup: false,
                autostart: false,
            },
        )
        .unwrap(),
    ))
}
fn root(port: u16) -> Conn {
    Conn::new(
        OptsBuilder::new()
            .ip_or_hostname(Some("127.0.0.1"))
            .tcp_port(port)
            .user(Some("root"))
            .pass(Some(""))
            .prefer_socket(false),
    )
    .unwrap()
}
fn database(state: &Shared, version: &str, operation: &str) {
    execute_shared(
        state,
        Action::Database {
            runtime: RuntimeRef {
                kind: RuntimeType::Mysql,
                version: version.into(),
            },
            operation: operation.into(),
        },
    )
    .unwrap();
}

#[test]
#[ignore = "Two exact MySQL catalog downloads or explicit verified catalog fixture directories; native disposable DBs"]
fn catalog_mysql_versions_and_per_site_bindings_preserve_data_and_isolation() {
    let d = tempfile::tempdir().unwrap();
    let home = Home::new(d.path());
    let state = open(&home);
    let versions = ["8.4.11", "9.7.1"];
    for (version, variable) in versions
        .into_iter()
        .zip(["DEVONE_MYSQL_A_SOURCE", "DEVONE_MYSQL_B_SOURCE"])
    {
        let manifest = catalog::available(&state.lock().unwrap().store)
            .unwrap()
            .into_iter()
            .find(|m| m.runtime == RuntimeType::Mysql && m.version == version)
            .unwrap();
        assert!(
            manifest
                .download
                .as_deref()
                .unwrap()
                .starts_with("https://cdn.mysql.com/")
        );
        assert_eq!(manifest.sha256.as_ref().unwrap().len(), 64);
        if let Ok(source) = std::env::var(variable) {
            assert_eq!(
                runtime::inspect_import(RuntimeType::Mysql, Path::new(&source))
                    .unwrap()
                    .version,
                version
            );
            execute_shared(&state, Action::Import { manifest, source }).unwrap();
        } else {
            execute_shared(
                &state,
                Action::Install {
                    runtime: RuntimeRef {
                        kind: RuntimeType::Mysql,
                        version: version.into(),
                    },
                },
            )
            .unwrap();
        }
        execute_shared(
            &state,
            Action::Default {
                runtime: RuntimeRef {
                    kind: RuntimeType::Mysql,
                    version: version.into(),
                },
            },
        )
        .unwrap();
        assert!(
            !state.lock().unwrap().snapshot().unwrap().setup.mysql,
            "Installed binaries alone must not mark MySQL setup ready"
        );
        database(&state, version, "initialize");
        assert!(state.lock().unwrap().snapshot().unwrap().setup.mysql);
        database(&state, version, "start");
    }
    let php_source = std::env::var("DEVONE_ACCEPT_PHP_A").unwrap();
    let php = runtime::inspect_import(RuntimeType::Php, Path::new(&php_source)).unwrap();
    let php_version = php.version.clone();
    execute_shared(
        &state,
        Action::Import {
            manifest: php,
            source: php_source,
        },
    )
    .unwrap();
    execute_shared(
        &state,
        Action::Default {
            runtime: RuntimeRef {
                kind: RuntimeType::Php,
                version: php_version,
            },
        },
    )
    .unwrap();
    for name in ["db-site-a", "db-site-b"] {
        let folder = home.www().join(name);
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(
            folder.join("index.php"),
            "<?php echo 'database acceptance';",
        )
        .unwrap();
        std::fs::write(folder.join(".env"), "USER_FILE=preserve\n").unwrap();
    }
    let snapshot = execute_shared(&state, Action::Scan).unwrap().snapshot;
    let ids: Vec<String> = ["db-site-a", "db-site-b"]
        .into_iter()
        .map(|name| {
            snapshot
                .sites
                .iter()
                .find(|s| s.name == name)
                .unwrap()
                .id
                .clone()
        })
        .collect();
    for (index, id) in ids.iter().enumerate() {
        execute_shared(
            &state,
            Action::Override {
                site_id: id.clone(),
                kind: RuntimeType::Mysql,
                version: Some(versions[index].into()),
            },
        )
        .unwrap();
        execute_shared(
            &state,
            Action::Provision {
                site_id: id.clone(),
                database_name: format!("acceptance_db_{index}"),
            },
        )
        .unwrap();
    }
    execute_shared(
        &state,
        Action::Default {
            runtime: RuntimeRef {
                kind: RuntimeType::Mysql,
                version: versions[0].into(),
            },
        },
    )
    .unwrap();
    execute_shared(
        &state,
        Action::Override {
            site_id: ids[0].clone(),
            kind: RuntimeType::Mysql,
            version: None,
        },
    )
    .unwrap();
    let snapshot = execute_shared(
        &state,
        Action::Default {
            runtime: RuntimeRef {
                kind: RuntimeType::Mysql,
                version: versions[1].into(),
            },
        },
    )
    .unwrap()
    .snapshot;
    let mut ports = vec![];
    let mut data_paths = vec![];
    for (index, version) in versions.iter().enumerate() {
        let runtime_id = format!("mysql:{version}");
        let db = snapshot
            .databases
            .iter()
            .find(|db| db.runtime_id == runtime_id)
            .unwrap();
        ports.push(db.port.unwrap());
        data_paths.push(db.data_path.clone());
        assert!(db.initialized);
        let site = snapshot.sites.iter().find(|s| s.id == ids[index]).unwrap();
        assert_eq!(site.resolved["mysql"], *version);
        let binding = snapshot
            .project_databases
            .iter()
            .find(|b| b.site_id == ids[index])
            .unwrap();
        assert_eq!(binding.runtime_id, runtime_id);
        let mut conn = root(db.port.unwrap());
        let server: String = conn.query_first("SELECT VERSION()").unwrap().unwrap();
        assert!(server.starts_with(version));
        conn.query_drop(format!(
            "CREATE TABLE `{}`.acceptance_marker(value INT)",
            binding.database_name
        ))
        .unwrap();
        conn.query_drop(format!(
            "INSERT INTO `{}`.acceptance_marker VALUES({index})",
            binding.database_name
        ))
        .unwrap();
        let absent: Option<String> = conn
            .exec_first(
                "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME=?",
                (format!("acceptance_db_{}", 1 - index),),
            )
            .unwrap();
        assert!(absent.is_none());
        let secret = devone::database::provision::secret(&home, &binding.credential_ref).unwrap();
        let mut project = Conn::new(
            OptsBuilder::new()
                .ip_or_hostname(Some("127.0.0.1"))
                .tcp_port(db.port.unwrap())
                .user(Some(binding.username.clone()))
                .pass(Some(secret.as_str()))
                .prefer_socket(false),
        )
        .unwrap();
        assert_eq!(
            project
                .query_first::<u32, _>(format!(
                    "SELECT value FROM `{}`.acceptance_marker",
                    binding.database_name
                ))
                .unwrap(),
            Some(index as u32)
        );
        assert!(
            project
                .query_drop("CREATE DATABASE forbidden_global_grant")
                .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(Path::new(&site.project_path).join(".env")).unwrap(),
            "USER_FILE=preserve\n"
        );
    }
    assert_ne!(ports[0], ports[1]);
    assert_ne!(data_paths[0], data_paths[1]);
    let other_pid = snapshot
        .services
        .iter()
        .find(|s| s.key == "mysql:9.7.1")
        .unwrap()
        .pid;
    database(&state, versions[0], "stop");
    assert_eq!(
        state
            .lock()
            .unwrap()
            .snapshot()
            .unwrap()
            .services
            .iter()
            .find(|s| s.key == "mysql:9.7.1")
            .unwrap()
            .pid,
        other_pid
    );
    assert_eq!(
        root(ports[1])
            .query_first::<u32, _>("SELECT value FROM acceptance_db_1.acceptance_marker")
            .unwrap(),
        Some(1)
    );
    database(&state, versions[0], "start");
    assert_eq!(
        root(ports[0])
            .query_first::<u32, _>("SELECT value FROM acceptance_db_0.acceptance_marker")
            .unwrap(),
        Some(0)
    );
    state.lock().unwrap().shutdown().unwrap();
    drop(state);
    let reopened = open(&home);
    for (index, version) in versions.iter().enumerate() {
        database(&reopened, version, "start");
        assert_eq!(
            root(ports[index])
                .query_first::<u32, _>(format!(
                    "SELECT value FROM acceptance_db_{index}.acceptance_marker"
                ))
                .unwrap(),
            Some(index as u32)
        );
        assert_eq!(
            reopened.lock().unwrap().site(&ids[index]).unwrap().resolved["mysql"],
            *version
        );
    }
    assert_eq!(
        reopened.lock().unwrap().snapshot().unwrap().databases.len(),
        2
    );
    reopened.lock().unwrap().shutdown().unwrap();
}
