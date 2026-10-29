-- Additive only: existing entity IDs and all references remain intact.
CREATE TABLE entity_public_ids (
 kind TEXT NOT NULL CHECK(kind IN ('workspace','list','section','task')),
 entity_id TEXT NOT NULL,
 short_id TEXT NOT NULL UNIQUE,
 PRIMARY KEY(kind,entity_id)
);
CREATE TABLE user_defaults (
 user_id TEXT PRIMARY KEY REFERENCES users(id),
 workspace_id TEXT NOT NULL REFERENCES workspaces(id)
);
CREATE TABLE workspace_defaults (
 workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id),
 list_id TEXT NOT NULL UNIQUE REFERENCES lists(id)
);
CREATE TABLE list_defaults (
 list_id TEXT PRIMARY KEY REFERENCES lists(id),
 section_id TEXT NOT NULL UNIQUE REFERENCES sections(id)
);
