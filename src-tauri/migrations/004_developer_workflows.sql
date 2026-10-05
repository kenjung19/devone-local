CREATE TABLE managed_databases (
runtime_id TEXT NOT NULL REFERENCES runtime_installations(id), database_name TEXT NOT NULL,
username TEXT NOT NULL, credential_ref TEXT NOT NULL, status TEXT NOT NULL, created_at INTEGER NOT NULL,
PRIMARY KEY(runtime_id,database_name), UNIQUE(runtime_id,username));
CREATE TABLE database_backups (
id TEXT PRIMARY KEY, type TEXT NOT NULL, database_name TEXT NOT NULL, runtime_id TEXT NOT NULL,
path TEXT NOT NULL UNIQUE, size INTEGER NOT NULL, sha256 TEXT NOT NULL, created_at INTEGER NOT NULL, status TEXT NOT NULL);
