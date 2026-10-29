CREATE TABLE user_onboarding (
 user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','complete','skipped')),
 step INTEGER NOT NULL DEFAULT 0,
 revision INTEGER NOT NULL DEFAULT 0,
 run_id TEXT NOT NULL DEFAULT '',
 destination TEXT NOT NULL DEFAULT '/inbox'
);
-- Existing accounts keep their normal sign-in destination.
INSERT INTO user_onboarding(user_id,status) SELECT id,'complete' FROM users;
CREATE TRIGGER new_account_onboarding AFTER INSERT ON users BEGIN
 INSERT INTO user_onboarding(user_id,run_id) VALUES(NEW.id,lower(hex(randomblob(16))));
END;
