CREATE TABLE task_durations_expanded (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    minutes INTEGER NOT NULL CHECK(minutes BETWEEN 0 AND 5256000)
);
INSERT INTO task_durations_expanded(task_id,minutes) SELECT task_id,minutes FROM task_durations;
DROP TABLE task_durations;
ALTER TABLE task_durations_expanded RENAME TO task_durations;
