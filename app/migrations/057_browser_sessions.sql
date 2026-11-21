-- Carry over only sessions that were still valid at the time of upgrade.
UPDATE auth_sessions
SET expires_at = expires_at - 43200 + 2592000
WHERE revoked = 0 AND expires_at > unixepoch() AND expires_at <= unixepoch() + 43200;
