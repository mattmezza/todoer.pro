CREATE TABLE comments (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    author_id TEXT NOT NULL REFERENCES users(id),
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=100000),
    revision INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER,
    deleted_at INTEGER
);
CREATE INDEX comments_task_sequence ON comments(task_id,seq);
CREATE TABLE upload_intents (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    comment_id TEXT REFERENCES comments(id),
    creator_id TEXT NOT NULL REFERENCES users(id),
    client_id TEXT,
    filename TEXT NOT NULL,
    size INTEGER NOT NULL CHECK(size>=0 AND size<=26214400),
    sha256 TEXT NOT NULL CHECK(length(sha256)=64),
    staging_key TEXT NOT NULL UNIQUE,
    object_key TEXT NOT NULL UNIQUE,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at INTEGER NOT NULL,
    finalized_at INTEGER
);
CREATE INDEX upload_intents_expiry ON upload_intents(expires_at,finalized_at);
CREATE TABLE attachments (
    id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL UNIQUE REFERENCES upload_intents(id),
    task_id TEXT NOT NULL REFERENCES tasks(id),
    comment_id TEXT REFERENCES comments(id),
    creator_id TEXT NOT NULL REFERENCES users(id),
    filename TEXT NOT NULL,
    size INTEGER NOT NULL CHECK(size>=0 AND size<=26214400),
    sha256 TEXT NOT NULL CHECK(length(sha256)=64),
    object_key TEXT NOT NULL UNIQUE,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    deleted_at INTEGER
);
CREATE INDEX attachments_task_created ON attachments(task_id,created_at,id);
