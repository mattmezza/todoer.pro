CREATE TABLE workspace_public_ids (
    workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    short_id TEXT NOT NULL UNIQUE CHECK(length(short_id)=7)
);
