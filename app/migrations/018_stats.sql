-- First actual observation for each readable list/local date. No backfill.
CREATE TABLE stats_observation_lists (
    user_id TEXT NOT NULL REFERENCES users(id),
    local_date TEXT NOT NULL,
    timezone TEXT NOT NULL,
    list_id TEXT NOT NULL REFERENCES lists(id),
    observed_at INTEGER NOT NULL,
    PRIMARY KEY(user_id,local_date,timezone,list_id)
);
CREATE TABLE stats_overdue_items (
    user_id TEXT NOT NULL,
    local_date TEXT NOT NULL,
    timezone TEXT NOT NULL,
    list_id TEXT NOT NULL,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    PRIMARY KEY(user_id,local_date,timezone,list_id,task_id),
    FOREIGN KEY(user_id,local_date,timezone,list_id)
      REFERENCES stats_observation_lists(user_id,local_date,timezone,list_id) ON DELETE CASCADE
);
CREATE INDEX stats_observation_range ON stats_observation_lists(user_id,timezone,local_date);
CREATE TABLE stats_observer_checks (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    checked_at INTEGER NOT NULL
);
-- Bound history scans to the selected time window before JSON leaf grouping.
CREATE INDEX history_review_window ON history(occurred_at,list_id);
