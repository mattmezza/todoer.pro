# Deployment

Todoer runs as a Rust application on a VPS, behind an existing host nginx proxy.
The public website uses Cloudflare Pages; this documentation uses GitHub Pages.
The Python implementation is retained under `legacy/` and is not deployed.

## Application image

An explicit published GitHub release `vX.Y.Z` builds
`ghcr.io/mattmezza/todoer.pro:vX.Y.Z` for Linux amd64. Use a pinned version or digest.
Run `make release name=vX.Y.Z` from a clean, current main checkout. A tag push
alone does not publish anything. Publishing an image never deploys the VPS.

The nonroot container has a read-only filesystem and a persistent `/data` mount.
Development auth is disabled. Host port 8092 binds loopback only for nginx;
the public origin is `https://app.todoer.pro`.

## VPS preparation

Shallow clone the public repository and follow the versioned
[VPS runbook](https://github.com/mattmezza/todoer.pro/blob/main/deploy/README.md).
It includes Compose/environment examples, nginx locations, health polling without
Compose `--wait`, optional separately credentialed Litestream, backups and rollback.
Real configuration belongs in ignored `deploy/.env`, `deploy/app.env` and optional
`deploy/replica.env`; chmod 600 and never paste these into public logs.

## Existing Python data

The [initial cutover runbook](https://github.com/mattmezza/todoer.pro/blob/main/deploy/MIGRATION.md)
keeps the original database untouched. Rehearse from a consistent backup, create
and explicitly map real accounts, review the dry-run, verify counts and attachments,
then freeze old writes for a final snapshot before switching nginx. See
[migration semantics](migration-export.md) for details.

## Backups and upgrades

Back up the SQLite database, attachment objects and stable server key separately.
Rehearse recovery before relying on continuous replication. The app applies schema
migrations at startup; an older image may need its matching pre-upgrade backup.
Never downgrade blindly against a newer schema or delete old data during cutover.
