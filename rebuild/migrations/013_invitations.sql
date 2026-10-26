CREATE TABLE invitations (
 id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE, inviter TEXT NOT NULL REFERENCES users(id),
 target_kind TEXT NOT NULL CHECK(target_kind IN ('workspace','list')), target_id TEXT NOT NULL,
 email TEXT NOT NULL, role TEXT NOT NULL CHECK(role IN ('owner','editor','viewer')),
 created_at INTEGER NOT NULL DEFAULT (unixepoch()), expires_at INTEGER NOT NULL,
 revoked INTEGER NOT NULL DEFAULT 0, claimed_by TEXT REFERENCES users(id),
 idempotency_key TEXT NOT NULL, payload_hash TEXT NOT NULL,
 UNIQUE(inviter,idempotency_key)
);
CREATE INDEX invitations_expiry ON invitations(expires_at,revoked);
CREATE TABLE invitation_outbox (
 id TEXT PRIMARY KEY, invitation_id TEXT NOT NULL UNIQUE REFERENCES invitations(id),
 status TEXT NOT NULL DEFAULT 'pending', attempts INTEGER NOT NULL DEFAULT 0,
 available_at INTEGER NOT NULL, lease_owner TEXT, lease_token TEXT, lease_until INTEGER, last_error TEXT
);
CREATE INDEX invitation_jobs_ready ON invitation_outbox(status,available_at,lease_until);
