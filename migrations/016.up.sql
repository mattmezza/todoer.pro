CREATE TABLE IF NOT EXISTS user_settings (
    uuid TEXT PRIMARY KEY,
    sid TEXT UNIQUE NOT NULL,
    creator TEXT NOT NULL REFERENCES users(uuid),
    key TEXT NOT NULL,
    value TEXT NOT NULL DEFAULT '',
    created TEXT DEFAULT (datetime('now')),
    updated TEXT,
    UNIQUE(creator, key)
);
