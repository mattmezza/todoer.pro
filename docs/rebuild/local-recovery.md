# Local SQLite recovery

`scripts/rebuild-backup.py` uses Python's standard-library SQLite online backup API. It supports current Rust rebuild databases only: it requires the rebuild tables and a contiguous migration history, and rejects legacy databases identified by `spaces` or `users.uuid`. It does not migrate a legacy database, contact a remote service, or implement Litestream/cloud replication.

Create the destination directory yourself, then choose a **new, nonexistent** filename:

```sh
python3 scripts/rebuild-backup.py backup /path/to/rebuild.sqlite3 /path/to/backups/rebuild-copy.sqlite3
python3 scripts/rebuild-backup.py restore /path/to/backups/rebuild-copy.sqlite3 /path/to/recovery/new-rebuild.sqlite3
```

Both commands perform the same verified database copy. The source is opened with a read-only SQLite URI; the destination is created exclusively with permissions 0600. Existing destinations (including symlinks), the source itself, and missing destination parent directories are refused. The script never overwrites a database. It verifies `PRAGMA integrity_check`, `PRAGMA foreign_key_check`, and rebuild schema identity before copying and again on the resulting file. Success prints only operation, verification status, and migration version numbers as JSON; failure prints a generic JSON error without filenames or database content and exits nonzero. A failed copy removes only its newly created destination file.

The SQLite backup API includes committed WAL content; copying the main `.sqlite3` file with a filesystem copy while the server is running does not provide that guarantee. The script does not checkpoint or write application rows in the source. SQLite may need existing readable WAL/shared-memory sidecars when opening a live database; it can manage shared-memory coordination even for read-only connections. Use a local filesystem supported by SQLite, keep the database and any live sidecars together, and do not pass `immutable=1` or remove WAL files while the source is running. The source and output contain private task data even though command output excludes it: keep their directory access restricted and manage retention explicitly.

Restore into an isolated directory first. Stop the server before changing its configured database path; point a separate local instance at the recovered copy and verify lists, task references, and recent edits before selecting it for ordinary use. Preserve the previous database and its sidecars for rollback. This tool intentionally does not replace the running database or change configuration for you. For future application versions, run recovery validation with a compatible rebuild version; the copy tool detects migration identity but does not interpret future schema semantics.

The synthetic integration test creates a fresh temporary database, leaves its writer open with committed WAL content, captures a task, backs up and restores to new paths, and verifies list count plus task ID/reference/description. It also verifies unchanged source main-file bytes and source task identity, refusal of existing/self/missing-parent destinations, and rejection of legacy/empty databases. All generated files are removed after the successful test. Run:

```sh
cargo test --manifest-path rebuild/Cargo.toml --test backup
```

This is a verified local SQLite copy and isolated restore test, not a complete disaster-recovery system. It does not test power loss, remote retention, encryption at rest, filesystem failure, point-in-time recovery, or periodic restore drills. Once attachments or other external objects are introduced, backing up this database alone will not recover them: object bytes and database/object-key consistency will require a coordinated recovery process.
