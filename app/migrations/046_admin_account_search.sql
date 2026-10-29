-- Prefix account searches use bounded indexed branches, never an account dump.
CREATE INDEX IF NOT EXISTS users_admin_name_prefix ON users(name COLLATE NOCASE,id);
CREATE INDEX IF NOT EXISTS users_admin_id_prefix ON users(id COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS identities_admin_email_prefix ON auth_identities(email COLLATE NOCASE,user_id);
