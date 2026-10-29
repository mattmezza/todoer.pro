CREATE TABLE capture_preferences (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    smart_enabled INTEGER NOT NULL DEFAULT 1,
    date_enabled INTEGER NOT NULL DEFAULT 1,
    repeat_capture INTEGER NOT NULL DEFAULT 0,
    ai_instructions TEXT NOT NULL DEFAULT ''
);
