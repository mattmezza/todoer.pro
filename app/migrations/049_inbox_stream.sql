-- Inbox membership belongs to the person who captured the task, independently
-- of its destination and schedule. Settling never changes the task itself.
CREATE TABLE inbox_entries (
 user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
 created_at INTEGER NOT NULL DEFAULT (unixepoch()),
 settled_at INTEGER,
 PRIMARY KEY(user_id,task_id)
);
CREATE INDEX inbox_entries_stream ON inbox_entries(user_id,settled_at,created_at DESC,task_id);
-- Preserve the previous inbox for existing accounts; other historic tasks were
-- already filed and should not suddenly appear as new work.
INSERT INTO inbox_entries(user_id,task_id,created_at)
 SELECT i.user_id,t.id,COALESCE(d.created_at,unixepoch())
 FROM user_inboxes i JOIN tasks t ON t.list_id=i.list_id
 LEFT JOIN task_schedule s ON s.task_id=t.id
 LEFT JOIN search_documents d ON d.kind='task' AND d.id=t.id
 WHERE t.parent_id IS NULL AND t.complete=0 AND t.deleted_at IS NULL
   AND json_extract(s.document,'$.planned') IS NULL;
