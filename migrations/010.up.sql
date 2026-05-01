CREATE TABLE IF NOT EXISTS bookmarks (
    uuid TEXT PRIMARY KEY,
    sid TEXT UNIQUE NOT NULL,
    user TEXT NOT NULL REFERENCES users(uuid),
    entity_type TEXT NOT NULL CHECK(entity_type IN ('space', 'list')),
    entity_uuid TEXT NOT NULL,
    "order" INTEGER NOT NULL DEFAULT 0,
    created TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user, entity_type, entity_uuid)
);
CREATE INDEX IF NOT EXISTS ix_bookmarks_user ON bookmarks(user);
