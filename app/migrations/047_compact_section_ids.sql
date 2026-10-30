CREATE TABLE section_public_ids (
    section_id TEXT PRIMARY KEY REFERENCES sections(id) ON DELETE CASCADE,
    short_id TEXT NOT NULL UNIQUE CHECK(length(short_id)=7)
);
