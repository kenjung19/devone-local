ALTER TABLE sites ADD COLUMN metadata TEXT NOT NULL DEFAULT '{}';
CREATE TABLE project_processes(site_id TEXT NOT NULL REFERENCES sites(id), id TEXT NOT NULL, definition TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(site_id,id));
