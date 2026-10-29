-- Removal hides a model from the active catalog without erasing usage history.
ALTER TABLE ai_models ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN(0,1));
