# Rust rebuild delivery ledger

Branch: `rebuild/rust`. The Python application and original databases remain intact.
No production cutover, push, or deployment is authorized.

## Decisions

- Add the Rust application alongside Python, using an independent database. Never run new migrations against the legacy schema.
- Axum HTTP, Askama escaped server templates, rusqlite with bundled SQLite and explicit transactions. One process initially; bounded queries. Dependencies pinned by Cargo.lock.
- Authentication must fail closed; isolated development identity is available only through explicit local configuration. Google subject identity and browser sessions must be separate from scoped agent credentials.
- Parent completion rejects unfinished descendants. Explicit subtree completion is separate. Reopening a child reopens its ancestors.
- Dates without times remain ISO dates. Timed scheduling requires an IANA timezone.
- Stable UUID identity plus independently unique readable references. Moves retain IDs.
- List authorization is the boundary for tasks/discussion/attachments/search/feeds.
- No claim of MCP client compatibility until an actual client has connected. Protocol implementation must target a published revision, not moving draft docs.

## Checklist (unchecked means unfinished, not implicitly deferred)

- [x] Inspect git status/instructions; dedicated branch; read-only SQLite backup for legacy runtime.
- [ ] Legacy design screens and interaction evidence; phone/tablet/desktop comparison.
- [ ] Rust vertical slice: persistent lists/tasks, inline add/edit/complete, batch, conflicts, live updates, draft retention.
- [ ] Google invite-only login; secure revocable sessions; scoped hashed agent credentials.
- [ ] Workspaces/lists/sections, arbitrary hierarchy, ordering, tags/references/backlinks.
- [ ] Sharing and audience-change acknowledgement; isolation/revocation tests.
- [ ] Search, structured filters, cursors, breadcrumbs, selected fields.
- [ ] Activity/trash/restore/authorized compensating undo.
- [ ] REST/OpenAPI, Rust CLI, remote MCP discovery/auth, installable skill; six agent scenarios.
- [ ] Comments, private S3 attachments and lifecycle.
- [ ] Recurrence, DST-safe reminders, durable jobs, SMTP/digests/web push.
- [ ] Optional OpenRouter capture/drafting, guarded previews and budgets.
- [ ] PWA install/update, account-scoped cached reading and durable offline capture.
- [ ] All review/planning screens, favorites/settings, weekly statistics.
- [ ] Non-destructive legacy import, reconciliation, all exports/ZIP/roundtrip.
- [ ] Litestream/complete restore, Compose, release/CI/private runner safeguards.
- [ ] Cross-browser visual/accessibility and integration/performance evidence.

## Initial verification

2026-10-26: clean `main` before branch creation. `uv sync` succeeded.
SQLite online-backup API copied `deploy/data/todoer_local.db` read-only into ignored
`.local/rebuild/legacy.db`. Legacy Uvicorn bound to loopback port 8091 with isolated
placeholder storage credentials; GET `/signin` returned HTTP 200. No original data
was altered. Personal data/screenshots must remain under ignored `.local/`.

## Dependency sources checked

- https://docs.rs/axum/latest/axum/ (0.8.9)
- https://docs.rs/rusqlite/latest/rusqlite/ (0.41.0)
- https://docs.rs/askama/latest/askama/ (0.16.1)
- https://tailwindcss.com/docs/installation/tailwind-cli (compiled CLI)
- https://modelcontextprotocol.io/specification/2025-11-25 (published protocol target; verify SDK support before implementation)
