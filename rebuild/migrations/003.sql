ALTER TABLE history ADD COLUMN actor TEXT NOT NULL DEFAULT 'legacy-unknown';
ALTER TABLE history ADD COLUMN before_event TEXT;
