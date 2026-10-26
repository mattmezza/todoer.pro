CREATE TABLE agent_bulk_previews (
    id TEXT PRIMARY KEY,
    actor TEXT NOT NULL,
    plan_hash TEXT NOT NULL,
    plan TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX agent_bulk_preview_expiry ON agent_bulk_previews(expires_at);
CREATE TABLE agent_bulk_applies (
    actor TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    preview_id TEXT NOT NULL,
    plan_hash TEXT NOT NULL,
    response TEXT NOT NULL,
    PRIMARY KEY(actor,idempotency_key)
);
CREATE TABLE agent_destination_preferences (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    preference_key TEXT NOT NULL,
    list_id TEXT NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
    PRIMARY KEY(user_id,preference_key)
);
