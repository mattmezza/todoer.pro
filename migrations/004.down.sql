BEGIN transaction;

--lists
DROP INDEX IF EXISTS ix_lists_workspace;
ALTER TABLE lists DROP COLUMN workspace;

--workspaces
DROP TABLE IF EXISTS workspaces;

--remove old tables if needed
DROP TABLE IF EXISTS _sections_20240428;
DROP TABLE IF EXISTS _lists_20240428;

COMMIT;
