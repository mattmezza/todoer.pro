CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL,
    name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    UNIQUE(scope,normalized_name)
);
CREATE TABLE task_tags (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY(task_id,tag_id)
);
CREATE INDEX task_tags_tag ON task_tags(tag_id,task_id);
CREATE TABLE task_locations (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    section_id TEXT NOT NULL REFERENCES sections(id) ON DELETE CASCADE
);
CREATE INDEX task_locations_section ON task_locations(section_id,task_id);
CREATE TABLE tag_move_audit (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    source_tag_id TEXT NOT NULL REFERENCES tags(id),
    destination_tag_id TEXT NOT NULL REFERENCES tags(id),
    actor TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE TABLE reference_sources (
    source_kind TEXT NOT NULL CHECK(source_kind IN ('task','comment')),
    source_id TEXT NOT NULL,
    raw_hash TEXT NOT NULL,
    PRIMARY KEY(source_kind,source_id)
);
CREATE TABLE reference_edges (
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    occurrence INTEGER NOT NULL,
    target_kind TEXT NOT NULL CHECK(target_kind IN ('workspace','list','section','task','tag')),
    target_id TEXT NOT NULL,
    original_token TEXT NOT NULL,
    PRIMARY KEY(source_kind,source_id,occurrence),
    FOREIGN KEY(source_kind,source_id) REFERENCES reference_sources(source_kind,source_id) ON DELETE CASCADE
);
CREATE INDEX reference_edges_target ON reference_edges(target_kind,target_id,source_kind,source_id);
