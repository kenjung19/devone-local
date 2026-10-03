CREATE TABLE project_databases (
site_id TEXT PRIMARY KEY REFERENCES sites(id),
runtime_id TEXT NOT NULL REFERENCES runtime_installations(id),
database_name TEXT NOT NULL,
username TEXT NOT NULL,
credential_ref TEXT NOT NULL UNIQUE,
status TEXT NOT NULL,
created_at INTEGER NOT NULL,
UNIQUE(runtime_id,database_name),
UNIQUE(runtime_id,username)
);
