-- Separate channel outboxes prevent SMTP and push from consuming each other's work.
CREATE TABLE push_digest_outbox (
 id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),
 kind TEXT NOT NULL CHECK(kind IN ('daily','weekly')),period TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'pending',available_at INTEGER NOT NULL,
 attempts INTEGER NOT NULL DEFAULT 0,lease_owner TEXT,lease_token TEXT,lease_until INTEGER,
 last_error TEXT,UNIQUE(user_id,kind,period)
);
CREATE INDEX push_digest_ready ON push_digest_outbox(status,available_at,lease_until);
CREATE TABLE push_digest_deliveries (
 reminder_id TEXT NOT NULL REFERENCES push_digest_outbox(id) ON DELETE CASCADE,
 device_id TEXT NOT NULL REFERENCES push_devices(id) ON DELETE CASCADE,
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','leased','sent','dead')),
 attempts INTEGER NOT NULL DEFAULT 0,available_at INTEGER NOT NULL,
 lease_token TEXT,lease_until INTEGER,last_error TEXT,
 PRIMARY KEY(reminder_id,device_id)
);
