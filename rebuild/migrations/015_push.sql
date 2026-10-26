CREATE TABLE push_preferences (
 user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
 opt_in INTEGER NOT NULL DEFAULT 0 CHECK(opt_in IN (0,1))
);
CREATE TABLE push_devices (
 id TEXT PRIMARY KEY,
 user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 session_id TEXT NOT NULL REFERENCES auth_sessions(id) ON DELETE CASCADE,
 endpoint TEXT NOT NULL UNIQUE,
 p256dh TEXT NOT NULL,
 auth_secret TEXT NOT NULL,
 label TEXT NOT NULL,
 revision INTEGER NOT NULL DEFAULT 1,
 active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL
);
CREATE INDEX push_devices_user ON push_devices(user_id,active);
CREATE TABLE push_deliveries (
 reminder_id TEXT NOT NULL REFERENCES reminder_outbox(id) ON DELETE CASCADE,
 device_id TEXT NOT NULL REFERENCES push_devices(id) ON DELETE CASCADE,
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','leased','sent','dead')),
 attempts INTEGER NOT NULL DEFAULT 0,
 available_at INTEGER NOT NULL,
 lease_token TEXT,
 lease_until INTEGER,
 last_error TEXT,
 PRIMARY KEY(reminder_id,device_id)
);
