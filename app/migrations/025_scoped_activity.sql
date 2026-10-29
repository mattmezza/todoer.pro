CREATE INDEX history_task_sequence ON history(json_extract(event,'$.task.id'),seq DESC);
