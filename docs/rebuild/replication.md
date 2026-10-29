# SQLite replication and isolated recovery

The production configuration is `deploy/litestream.yml`, verified with
Litestream **0.5.17** against its current [configuration reference](https://litestream.io/reference/config/)
and [restore reference](https://litestream.io/reference/restore/). Replica objects
use separate bucket/prefix and credentials from application attachment storage.
The process needs read/write SQLite volume access, including WAL/SHM sidecars.
One application writer instance is supported.

Set `TODOER_RUST_DB`, `TODOER_REPLICA_ENDPOINT`, `TODOER_REPLICA_REGION`,
`TODOER_REPLICA_BUCKET`, `TODOER_REPLICA_PREFIX`,
`TODOER_REPLICA_ACCESS_KEY_ID`, `TODOER_REPLICA_SECRET_ACCESS_KEY` in the replica
container's private environment file. A custom HTTPS endpoint is supported;
loopback HTTP is used only by the isolated fixture. Never disable TLS verification.
Do not reuse attachment credentials. Do not put real values in the repository.

The example syncs every five seconds, retains snapshots for 30 days and fine
L0 restore points for 24 hours. This is an asynchronous recovery window, not zero
possible data loss. Older point-in-time granularity depends on surviving compacted
boundaries. Monitor sync failures and test restores rather than assuming backups
work because the container is running. Shutdown allows a final bounded sync.

## Verification performed

On 2026-10-26 the official Linux x86_64 0.5.17 archive was checked against the
release checksum (`cfb371176d164437ae869f8351cfde49bd1804ae71c61923f75c9cba9c9c006d`).
The ignored local tool replicated a synthetic WAL database to a private bucket
on a loopback Moto S3 service, including a later UPDATE and INSERT, and restored
both latest committed rows into a new database. Litestream's full integrity check,
SQLite `integrity_check` and `foreign_key_check` passed. This exercised real binary
replication/restore traffic, not a mocked Litestream implementation. It does not
claim a production-provider or container-image check.

Repeat using a pinned Litestream executable and an isolated loopback S3 fixture:

```sh
python scripts/rebuild-litestream-check.py --litestream /path/to/litestream
```

The check refuses non-loopback endpoints and never reads the legacy database.
Its generated bucket remains on the isolated fixture for inspection.

## Recovery into a new path

Stop application writes for an eventual cutover; first rehearse in isolation.
Keep the damaged/current DB and its WAL/SHM files intact in a separate safe location.
Use the same replica configuration and credentials to restore into a **new** path:

```sh
litestream restore -config deploy/litestream.yml \
  -o /isolated/restore/todoer.db -integrity-check full "$TODOER_RUST_DB"
```

Add `-timestamp RFC3339` for an available historical recovery boundary. Do not use
`-force`, automatic overwrite, or `-if-replica-exists` to hide missing backups.
Check integrity, foreign keys, expected recent tasks and membership permissions.
Run the rebuilt app against the isolated path with isolated notification/storage
adapters; avoid replaying real reminders during a rehearsal. An authenticated
Google rehearsal needs a separately configured callback origin.

**Database replication alone is not complete recovery.** Attachment objects are
immutable, private, and retained after soft deletion; application GC is disabled.
Configure attachment bucket versioning/retention independently and keep all blobs
referenced by every supported DB recovery point, including archived legacy UUID
objects. Do not add an object lifecycle rule that deletes them after the DB window
without accounting for older snapshots, exports and delayed recovery. Verify
attachment reads/checksums after restoring the DB. Complete checksummed ZIP export
and immutable object restore provide an additional portable path; see
[migration-export.md](migration-export.md).

Schema rollback is not automatic. Keep the prior executable/image digest and a
compatible pre-upgrade DB recovery point plus attachment retention. Never start an
older binary against a schema it does not support. Rehearse a restore rather than
attempting blind reverse migrations after writes by a newer application.
