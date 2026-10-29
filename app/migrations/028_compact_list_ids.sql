CREATE TABLE list_public_ids (
 list_id TEXT PRIMARY KEY REFERENCES lists(id) ON DELETE CASCADE,
 short_id TEXT NOT NULL UNIQUE CHECK(length(short_id)=7)
);
