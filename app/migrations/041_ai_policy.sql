CREATE TABLE ai_account_policy(
 user_id TEXT PRIMARY KEY REFERENCES users(id),
 user_ai_enabled INTEGER NOT NULL DEFAULT 0 CHECK(user_ai_enabled IN(0,1)),
 managed_ai_allowed INTEGER NOT NULL DEFAULT 0 CHECK(managed_ai_allowed IN(0,1)),
 byok_allowed INTEGER NOT NULL DEFAULT 1 CHECK(byok_allowed IN(0,1)),
 funding TEXT NOT NULL DEFAULT 'none' CHECK(funding IN('none','credits','byok')),
 model_id TEXT,
 key_status TEXT NOT NULL DEFAULT 'absent' CHECK(key_status IN('absent','valid','invalid','revoked')),
 daily_limit_micro INTEGER NOT NULL DEFAULT 250000 CHECK(daily_limit_micro BETWEEN 1 AND 1000000000),
 revision INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE ai_provider_keys(
 user_id TEXT PRIMARY KEY REFERENCES users(id),
 ciphertext TEXT NOT NULL,
 key_id TEXT NOT NULL,
 created_at INTEGER NOT NULL DEFAULT(unixepoch()),
 validated_at INTEGER NOT NULL DEFAULT(unixepoch())
);
CREATE TABLE ai_access_requests(
 user_id TEXT PRIMARY KEY REFERENCES users(id),
 state TEXT NOT NULL CHECK(state IN('pending','approved','denied','revoked')),
 requested_at INTEGER NOT NULL DEFAULT(unixepoch()),
 decided_at INTEGER,
 decided_by TEXT REFERENCES users(id)
);
CREATE INDEX ai_access_pending ON ai_access_requests(state,requested_at);
CREATE TABLE ai_admin_audit(
 id INTEGER PRIMARY KEY,
 actor TEXT NOT NULL REFERENCES users(id),
 target TEXT,
 action TEXT NOT NULL,
 reason TEXT NOT NULL,
 before_json TEXT NOT NULL,
 after_json TEXT NOT NULL,
 created_at INTEGER NOT NULL DEFAULT(unixepoch())
);
CREATE TABLE ai_key_checks(
 user_id TEXT PRIMARY KEY REFERENCES users(id),
 window_at INTEGER NOT NULL,
 attempts INTEGER NOT NULL
);
CREATE TABLE ai_account_notices(
 id INTEGER PRIMARY KEY,
 user_id TEXT NOT NULL REFERENCES users(id),
 kind TEXT NOT NULL CHECK(kind='access'),
 state TEXT NOT NULL CHECK(state IN('approved','denied','revoked')),
 event_id INTEGER NOT NULL UNIQUE REFERENCES ai_admin_audit(id),
 created_at INTEGER NOT NULL DEFAULT(unixepoch()),
 read_at INTEGER
);
CREATE INDEX ai_account_notices_user ON ai_account_notices(user_id,id);
