ALTER TABLE upload_intents ADD COLUMN comment_draft INTEGER NOT NULL DEFAULT 0 CHECK(comment_draft IN (0,1));
