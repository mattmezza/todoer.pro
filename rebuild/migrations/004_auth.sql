CREATE TABLE IF NOT EXISTS auth_email_allowlist(email TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS auth_identities(issuer TEXT NOT NULL,subject TEXT NOT NULL,user_id TEXT NOT NULL REFERENCES users(id),email TEXT NOT NULL,PRIMARY KEY(issuer,subject));
CREATE TABLE IF NOT EXISTS auth_sessions(id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),token_hash TEXT NOT NULL UNIQUE,csrf_hash TEXT NOT NULL,expires_at INTEGER NOT NULL,revoked INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS auth_agents(id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),token_hash TEXT NOT NULL UNIQUE,audience TEXT NOT NULL,permissions TEXT NOT NULL,list_ids TEXT,workspace TEXT,expires_at INTEGER NOT NULL,revoked INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS auth_oauth_clients(id TEXT PRIMARY KEY,name TEXT NOT NULL,redirect_uris TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS auth_oauth_requests(id TEXT PRIMARY KEY,request TEXT NOT NULL,expires_at INTEGER NOT NULL,actor TEXT REFERENCES users(id),permissions TEXT,list_ids TEXT,workspace TEXT,code_hash TEXT UNIQUE,approved INTEGER NOT NULL DEFAULT 0,consumed INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS auth_oauth_families(id TEXT PRIMARY KEY,client_id TEXT NOT NULL REFERENCES auth_oauth_clients(id),revoked INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS auth_oauth_refresh(token_hash TEXT PRIMARY KEY,family_id TEXT NOT NULL REFERENCES auth_oauth_families(id),access_id TEXT NOT NULL REFERENCES auth_agents(id),expires_at INTEGER NOT NULL,consumed INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS auth_oauth_code_families(request_id TEXT PRIMARY KEY REFERENCES auth_oauth_requests(id),family_id TEXT NOT NULL REFERENCES auth_oauth_families(id));
