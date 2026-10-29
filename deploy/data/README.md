This folder contains the sqlite databases:

- `todoer_local.db`
- `todoer_dev.db`
- `todoer_prod.db`

The whole folder should be git ignored.

The current Rust deployment uses `/var/lib/todoer` outside the shallow checkout.
This directory is retained for historical/local references only. Never place
production data in Git; see `deploy/README.md` and `deploy/MIGRATION.md`.
