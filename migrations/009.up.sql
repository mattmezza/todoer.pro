CREATE TABLE IF NOT EXISTS task_audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task UUID NOT NULL REFERENCES tasks(uuid) ON DELETE CASCADE,
    action VARCHAR NOT NULL,
    detail TEXT,
    actor UUID NOT NULL REFERENCES users(uuid),
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS ix_task_audit_task ON task_audit_log(task);
CREATE INDEX IF NOT EXISTS ix_task_audit_created ON task_audit_log(created);
