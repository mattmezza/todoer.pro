CREATE TABLE notification_inbox(
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 user_id TEXT NOT NULL REFERENCES users(id),
 task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('due','reminder','update')),
 event_key TEXT NOT NULL,
 source TEXT NOT NULL,
 created_at INTEGER NOT NULL,
 read_at INTEGER,
 UNIQUE(user_id,event_key)
);
CREATE INDEX notification_inbox_user ON notification_inbox(user_id,id DESC);
CREATE INDEX notification_inbox_unread ON notification_inbox(user_id,id DESC) WHERE read_at IS NULL;
CREATE TABLE notification_push_outbox(id TEXT PRIMARY KEY,notification_id INTEGER NOT NULL UNIQUE REFERENCES notification_inbox(id) ON DELETE CASCADE,user_id TEXT NOT NULL REFERENCES users(id),status TEXT NOT NULL DEFAULT 'pending',available_at INTEGER NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,lease_owner TEXT,lease_token TEXT,lease_until INTEGER,last_error TEXT);
CREATE INDEX notification_push_ready ON notification_push_outbox(status,available_at,lease_until);
CREATE TABLE notification_push_deliveries(reminder_id TEXT NOT NULL REFERENCES notification_push_outbox(id) ON DELETE CASCADE,device_id TEXT NOT NULL REFERENCES push_devices(id),status TEXT NOT NULL DEFAULT 'pending',available_at INTEGER NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,lease_token TEXT,lease_until INTEGER,last_error TEXT,PRIMARY KEY(reminder_id,device_id));
