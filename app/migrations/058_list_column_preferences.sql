CREATE TABLE list_column_preferences (
 user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 list_id TEXT NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
 PRIMARY KEY(user_id, list_id)
);
