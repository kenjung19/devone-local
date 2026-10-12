CREATE TABLE environment_profiles (
 id TEXT PRIMARY KEY,
 name TEXT NOT NULL COLLATE NOCASE UNIQUE,
 description TEXT NOT NULL DEFAULT '',
 bindings TEXT NOT NULL,
 tools TEXT NOT NULL DEFAULT '{}',
 built_in INTEGER NOT NULL DEFAULT 0,
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL
);

ALTER TABLE sites ADD COLUMN last_applied_profile_id TEXT;

CREATE TABLE site_tool_overrides (
 site_id TEXT NOT NULL REFERENCES sites(id),
 tool_id TEXT NOT NULL,
 version TEXT NOT NULL,
 PRIMARY KEY(site_id,tool_id)
);

