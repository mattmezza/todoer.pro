CREATE TABLE ai_usage (
    uuid TEXT PRIMARY KEY,
    sid TEXT NOT NULL UNIQUE,
    user TEXT NOT NULL REFERENCES users(uuid),
    prompt_length INTEGER NOT NULL,
    tasks_generated INTEGER NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_ai_usage_user ON ai_usage(user);
CREATE INDEX idx_ai_usage_created_at ON ai_usage(created_at);
