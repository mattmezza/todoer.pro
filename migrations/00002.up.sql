BEGIN;
CREATE TABLE IF NOT EXISTS users(
    uuid UUID PRIMARY KEY,
    name TEXT,
    email TEXT NOT NULL,
    password TEXT NOT NULL,
    disabled INTEGER NOT NULL DEFAULT 0,
    confirm_code UUID NOT NULL,
    confirmed_at TIMESTAMP,
    last_login_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP
);
CREATE UNIQUE INDEX IF NOT EXISTS users_email ON users(email);
CREATE INDEX IF NOT EXISTS users_password ON users(password);
CREATE INDEX IF NOT EXISTS users_confirm_code ON users(confirm_code);
CREATE INDEX IF NOT EXISTS users_confirmed_at ON users(confirmed_at);
COMMIT;

