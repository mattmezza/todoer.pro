-- Original captures survive provider errors. Reservations are never refunded:
-- a timed out request may still have incurred provider usage.
CREATE TABLE assist_captures (
 actor TEXT NOT NULL, idempotency_key TEXT NOT NULL, input_hash TEXT NOT NULL,
 raw_text TEXT NOT NULL, task_id TEXT NOT NULL REFERENCES tasks(id),
 route_source TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT(unixepoch()),
 PRIMARY KEY(actor,idempotency_key)
);
CREATE TABLE assist_requests (
 id TEXT PRIMARY KEY, actor TEXT NOT NULL, user_id TEXT NOT NULL REFERENCES users(id),
 kind TEXT NOT NULL, idempotency_key TEXT NOT NULL, input_hash TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'reserved', response TEXT,
 reserved_tokens INTEGER NOT NULL, reserved_micro_usd INTEGER NOT NULL,
 created_at INTEGER NOT NULL DEFAULT(unixepoch()),
 UNIQUE(actor,kind,idempotency_key)
);
CREATE INDEX assist_budget_user ON assist_requests(user_id,created_at);
CREATE INDEX assist_budget_deployment ON assist_requests(created_at);
CREATE TABLE assist_drafts (
 id TEXT PRIMARY KEY, actor TEXT NOT NULL, brief TEXT NOT NULL,
 revision INTEGER NOT NULL DEFAULT 1, payload TEXT NOT NULL,
 expected_hash TEXT NOT NULL, policy_hash TEXT NOT NULL,
 expires_at INTEGER NOT NULL, applied INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE assist_applies (
 actor TEXT NOT NULL, idempotency_key TEXT NOT NULL, preview_id TEXT NOT NULL,
 expected_hash TEXT NOT NULL, response TEXT NOT NULL,
 PRIMARY KEY(actor,idempotency_key)
);
CREATE TABLE assist_rules (
 id TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 prefix TEXT NOT NULL, list_id TEXT NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
 UNIQUE(user_id,prefix)
);
