ALTER TABLE history ADD COLUMN occurred_at INTEGER;
CREATE TRIGGER history_timestamp AFTER INSERT ON history WHEN new.occurred_at IS NULL BEGIN UPDATE history SET occurred_at=unixepoch() WHERE seq=new.seq; END;
CREATE TABLE notification_preferences(user_id TEXT PRIMARY KEY REFERENCES users(id),document TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 1);
CREATE TABLE digest_outbox(id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),kind TEXT NOT NULL CHECK(kind IN ('daily','weekly')),period TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'pending',available_at INTEGER NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,lease_owner TEXT,lease_token TEXT,lease_until INTEGER,last_error TEXT,UNIQUE(user_id,kind,period));
CREATE INDEX digest_ready ON digest_outbox(status,available_at,lease_until);
