-- Preserve existing public IDs while adding short, prefix-free task URLs.
CREATE TABLE task_public_ids (
 task_id TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
 short_id TEXT NOT NULL UNIQUE CHECK(length(short_id)=7)
);
