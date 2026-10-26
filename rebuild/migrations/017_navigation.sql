CREATE TABLE favorites (
    user_id TEXT NOT NULL REFERENCES users(id),
    target_kind TEXT NOT NULL CHECK(target_kind IN ('workspace','list')),
    target_id TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT(unixepoch()),
    PRIMARY KEY(user_id,target_kind,target_id)
);
ALTER TABLE lists ADD COLUMN color TEXT CHECK(color IN ('blue','green','amber','red','violet','pink','gray'));
