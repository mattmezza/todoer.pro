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

## Implemented increments (2026-10-26)

- [x] Read-only legacy copy running; synthetic desktop/phone reference screens in ignored `.local/rebuild/`.
- [x] Shared Rust operations, versioned transactional migrations, independent SQLite WAL database; legacy schema rejected before writes.
- [x] List/task HTML slice, task detail URLs, nested checklists, completion rules, subtree move with access-change acknowledgement.
- [x] REST, Rust CLI, fifteen direct application-layer MCP tools, accurate OpenAPI and capabilities; official Python MCP SDK HTTP check.
- [x] Atomic 1–100 capture, payload-bound retry keys, revision conflicts, current-permission history/search/replay checks.
- [x] FTS5 Unicode token-prefix title/description search, completed filter, ID keyset pagination. No fuzzy matching claim.
- [x] Task trash/restore groups, human/credential-client actor attribution, single-edit compensating undo that refuses newer work.
- [x] Sanitized CommonMark; safe image-free description display.
- [x] SSE invalidation, open draft retention, session draft recovery, optimistic completion rollback, keyboard quick-add.
- [x] Scoped recent-list cache and durable offline capture queue; manifest/icons; explicit worker update lifecycle.
- [x] Development sessions with CSRF, cookie expiry/logout revocation, hashed development bearer credential; security response headers and content-free request logs.
- [x] Local SQLite online backup/isolated restore with integrity/FK checks; synthetic restore integration.
- [x] Private-runner branch/PR checks prepared, fork-job guard; not executed remotely.
- [x] Synthetic 10,000-task performance probe and query-plan-backed subtree fix.

These increments do **not** complete the grouped checklist above. The entries
below record later integration work; remaining product and operational gaps are
explicit. External credentials are not the reason unfinished code remains
unfinished. Do not cut over or deploy this branch yet.

## Identity and sharing increment

Browser Google OIDC, durable revocable sessions, browser-managed scoped REST/MCP
credentials, public-client MCP OAuth/PKCE consent and rotation, workspace roles,
restricted/inherited lists, direct existing-user list grants and section metadata
are now implemented. Common operations intersect live resource permissions with
credential ceilings, including enumeration/search/history. Browser account IDs
scope cached pages and drafts; SSE checks the open list and reports revocation.

Evidence: signed local OIDC provider and key-rotation tests; production-mode
cookie/CSRF/scoped-token/revocation HTTP tests; actual official Python MCP SDK
OAuth flow over an isolated loopback server (including refresh and tool calls).
See `authentication.md`. Google tenant/hosted connector checks remain external.
Invitations, task-to-section assignment, full sharing UX and cross-workspace list
moves remain unfinished; section metadata alone does not complete hierarchy.

Current integrated verification: `make rust-check` passes (78 unit tests, nine
HTTP/backup integration tests; four external-service probes are separately
invoked). Phone/desktop workspace/list/section/direct-sharing browser forms pass
in both Chromium and Firefox. A native-select overflow found on phones was fixed,
and native checkboxes retain their visual size inside wrapping form labels.
Local S3 protocol and Axum upload/download tests use a private Moto fixture;
SMTP transport is exercised against an isolated aiosmtpd capture service.

## Invitations, agent transactions, search and recovery increment

- Email-bound seven-day invitations use a persistent server signing key and hashed
  credentials, current owner checks, explicit access acknowledgement, atomic claim
  and durable SMTP outbox. Google callback verifies the current signed email before
  registration/claim; it never merges identities by email. Owner forms support link
  creation and pending-link revocation. Three domain tests plus a real Axum router
  test exercise retries, payload conflicts, wrong email, expiry, key rotation and
  revocation. These local tests do not constitute a real Google tenant check.
- Bulk previews invoke the same operations inside a rollback transaction; applying
  checks the stored proposal/hash, complete expected state and current permissions.
  Nested operation transactions use SQLite savepoints. Eighteen MCP tools include
  preview/apply and saved destinations. An actual official SDK OAuth session tests
  discovery, consent/PKCE, exchange, refresh, scoped tools and retry semantics.
- Invitations do not replace a new user's private Inbox. The callback creates a
  private destination and saves its explicit `inbox` preference, even if the user
  already received direct access to someone else's list.
- Structured multi-entity search adds authorized comments, tags, references,
  attachment metadata, filters, projections and query-bound cursors. FTS supplies
  candidates; document-local scoring avoids leaking private global index counts
  through bm25 ranking. See `search.md` for exact behavior and measured limits.
- Non-destructive legacy reconciliation is exercised against the ignored actual
  SQLite copy, with source checksum unchanged. Known orphan hierarchy/deleted
  owners require explicit recovery acknowledgement; preserved source archives
  exclude authentication secrets. See `migration-export.md` for tested behavior.
- Separate SMTP and push channel claiming prevents one worker from consuming the
  other channel's reminders. Push encryption/VAPID and partial device retries are
  checked against local real HTTP endpoints; physical devices remain unverified.

Unfinished: all-interface parity and complete browser editors, favorites,
workspace-changing list moves, OpenRouter assistance, full notification/product
polish, production Compose/release and complete recovery runbooks, final broad
browser/performance verification. Required features are not silently deferred.

Latest backend integration: `cargo test --locked --lib` passes 129 active tests;
eight explicit service/performance probes remain separate. `cargo clippy
--all-targets -- -D warnings` passed before the next agent-surface extension.
The recurrence editor passes seven actual-browser scenarios in each of Chromium
and Firefox. A regression test confirms already delivered reminder instants are
not requeued by unrelated schedule edits. Favorites are user-scoped and live
permission-filtered; list color changes use the normal list revision. Account
exports now include visible favorites/default destinations and empty workspaces.
