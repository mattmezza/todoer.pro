ALTER TABLE sections ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0, 1));
ALTER TABLE tasks ADD COLUMN archive_group TEXT;
CREATE INDEX tasks_archive_group ON tasks(archive_group) WHERE archive_group IS NOT NULL;
