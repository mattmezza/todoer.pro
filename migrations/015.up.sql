CREATE TABLE IF NOT EXISTS notification_reads (
    uuid TEXT PRIMARY KEY,
    creator TEXT NOT NULL REFERENCES users(uuid),
    task_uuid TEXT NOT NULL REFERENCES tasks(uuid),
    kind TEXT NOT NULL,
    created TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(creator, task_uuid, kind)
);
