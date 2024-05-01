BEGIN transaction;

--lists
DROP INDEX IF EXISTS ix_lists_space;
ALTER TABLE lists DROP COLUMN space;

--spaces
DROP TABLE IF EXISTS spaces;

--remove old tables if needed
DROP TABLE IF EXISTS _sections_20240428;
DROP TABLE IF EXISTS _lists_20240428;

COMMIT;
