# Initial Python → Rust cutover

This is a joint maintenance-window operation, not an automatic deployment step.
The old deployment keeps running until we explicitly perform this procedure.

1. Record the current image/configuration and verify a restorable backup of the
   legacy SQLite DB and its attachment bucket. Use SQLite `.backup` or its backup
   API; a plain copy of a live WAL database is not a consistent backup. Store all
   artifacts privately outside the checkout. Never commit real records or maps.
2. Rehearse from an independent backup into an independent Rust database. Create
   the intended real account in that isolated destination, then map legacy user
   UUIDs to its Rust account ID in a private `identity-map.json`. Email equality
   does not map accounts. Unmapped imported accounts cannot sign in.
3. Use the released image's migration executable (override ENTRYPOINT):

   ```sh
   docker run --rm --user "$(id -u):$(id -g)" \
     --entrypoint todoer-migrate \
     -v /private/migration:/migration \
     ghcr.io/mattmezza/todoer.pro:vX.Y.Z legacy \
     --source /migration/legacy-copy.db \
     --destination /migration/rust.db \
     --identity-map /migration/identity-map.json
   ```

   This defaults to a dry-run. Review counts and unsupported/recovered categories.
   Add `--apply` only for a reviewed import. Do not add `--ack-unsupported` without
   understanding each category. Preserve the original database regardless.
4. Verify lists, sections, todos, subtasks, completed/archived state, comments,
   dates and attachment references. Verify representative attachment downloads
   through a separately configured read-only `TODOER_LEGACY_STORAGE_*` adapter.
   Legacy credentials/passwords are excluded; users must authenticate anew.
   See [full importer semantics](../docs/rebuild/migration-export.md).
5. Freeze writes to the old service for the final snapshot. Repeat the verified
   procedure into a fresh destination using the final snapshot and the reviewed
   identity map. Do not reconcile into a rehearsal database with unrelated edits.
6. Keep both services stopped while placing the verified destination at the new
   `/var/lib/todoer/todoer.db`. Set owner 10001:10001 and mode 600. Preserve a copy
   of that destination before first startup. Start Rust, verify health and login,
   and compare counts before switching nginx to the new service.
7. Switch app.todoer.pro only after acceptance. Keep the frozen Python deployment,
   source database and object storage available for rollback. A rollback after
   new writes needs reconciliation; never overwrite either side's new todos.

No existing production DB is touched by repo reorganization, CI or release jobs.
