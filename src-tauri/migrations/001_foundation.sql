CREATE TABLE sites (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, hostname TEXT NOT NULL COLLATE NOCASE UNIQUE,
 project_path TEXT NOT NULL UNIQUE, project_type TEXT NOT NULL, document_root TEXT NOT NULL,
 present INTEGER NOT NULL DEFAULT 1, issue TEXT, discovered_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE site_runtime_overrides (
 site_id TEXT NOT NULL REFERENCES sites(id), kind TEXT NOT NULL, version TEXT NOT NULL,
 PRIMARY KEY(site_id,kind)
);
CREATE TABLE runtime_catalog (id TEXT PRIMARY KEY, manifest TEXT NOT NULL);
CREATE TABLE runtime_installations (
 id TEXT PRIMARY KEY, kind TEXT NOT NULL, version TEXT NOT NULL, manifest TEXT NOT NULL,
 relative_path TEXT NOT NULL, installed_at INTEGER NOT NULL, UNIQUE(kind,version)
);
CREATE TABLE database_instances (
 runtime_id TEXT PRIMARY KEY REFERENCES runtime_installations(id), data_path TEXT NOT NULL,
 initialized INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL
);
CREATE TABLE port_allocations (owner TEXT PRIMARY KEY, port INTEGER NOT NULL UNIQUE);
CREATE TABLE process_state (
 service_key TEXT PRIMARY KEY, pid INTEGER, status TEXT NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE certificates (id TEXT PRIMARY KEY, path TEXT NOT NULL, trusted INTEGER NOT NULL DEFAULT 0);
CREATE TABLE tools (id TEXT PRIMARY KEY, manifest TEXT NOT NULL);
