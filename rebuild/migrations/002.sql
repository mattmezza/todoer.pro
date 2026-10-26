ALTER TABLE tasks ADD COLUMN deleted_at TEXT;
ALTER TABLE tasks ADD COLUMN delete_group TEXT;
CREATE INDEX tasks_deleted ON tasks(deleted_at,delete_group);
