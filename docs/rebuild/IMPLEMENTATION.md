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

## Current implementation checklist

Checked means the bounded implementation exists with the evidence below. It does
not mean production acceptance, deployment approval, complete interface parity,
or verification on every browser/device. Unchecked items remain required work.

- [x] Independent Rust database, strict legacy-schema rejection, transactional migrations, persistent list/task operations, retry-safe capture and revision conflicts.
- [x] Native task UI, inline editing, nested checklists, sibling ordering, SSE invalidation, preserved drafts and canonical task links.
- [x] Google OIDC implementation, invite-only registration, durable revocable sessions, scoped hashed REST/MCP credentials and public-client OAuth/PKCE.
- [x] Workspace roles, inherited/restricted/direct list access, invitations, sections, tags, stable references and authorized backlinks.
- [x] Owner-reviewed cross-workspace list moves with audience acknowledgment, policy hash, task revision fencing and preserved sections/direct guests.
- [x] Structured authorized multi-entity search, filters, cursors, projections and breadcrumbs; activity, trash/restore and guarded single-edit undo.
- [x] REST/OpenAPI, Rust CLI, remote MCP, installable skill and actual official-SDK/CLI synthetic scenarios for their documented surfaces.
- [x] Comment retry/CAS/history, private S3 upload intents, checksum finalization, authenticated downloads and soft deletion.
- [x] Scheduling editor, bounded nested recurrence, durable reminder outbox, SMTP reminders/email digests and session-bound encrypted Web Push reminders.
- [ ] Daily/weekly push digests: currently email-only; push reminder delivery does not satisfy this feature.
- [ ] Optional OpenRouter assistance: new domain/HTTP/UI files are in active integration; do not count source presence as passing integration evidence.
- [x] PWA manifest/update lifecycle, account-scoped recent-list cached reading and durable offline capture/replay.
- [x] Today/upcoming/recurring/review screens, favorites/settings and durable observed overdue trends.
- [x] Non-destructive legacy reconciliation, explicit identity mapping, JSON/text/CSV/ZIP export and verified object-byte ZIP restore.
- [x] Local backup/restore, isolated Litestream/S3 recovery, separate Compose image and prepared release/private-runner safeguards.
- [ ] Final interface parity audit: recent workspace-move and weekly-review operations are not yet represented in CLI/MCP and the checked-in OpenAPI document.
- [ ] Final broad visual/accessibility, physical-device/WebKit, concurrent/persistent-storage performance and production-service acceptance.

## Audit snapshot and evidence (2026-10-26)

This is a documentation/code audit after `cd5579d`, `8d6a264`, `21dba9d` and
`786dd12`; active uncommitted work can be newer. No expensive build or external
service check was run for this audit. Earlier test counts below are historical
snapshots, not current totals or evidence for files added afterward.

| Area | Implementation and observed evidence | Remaining verification or limits |
| --- | --- | --- |
| Identity, invitations, sharing | [Authentication](authentication.md), signed local OIDC/key rotation, durable production-mode cookie/CSRF/revocation tests; workspace/native sharing browser checks | Real Google tenant/callback and hosted MCP connector acceptance remain external. Implementation is present; credentials are needed for those checks, not to finish the code. |
| Tasks, metadata, moves | [Move contract](sharing-moves.md); 11 workspace plus 9 taxonomy tests at move integration, followed by a passing same-parent stale-order regression; six move browser checks; twelve metadata browser checks plus two Enter-key checks, Chromium/Firefox | WebKit and physical devices unverified. List move rejects workspace-less legacy lists. It retains object keys rather than copying blobs. |
| Agents and search | [Agent workflow](agent-workflow.md), [search](search.md); actual official Python SDK OAuth/refresh and all 21 registered bearer tools, isolated CLI/Moto checks | New list-workspace move and `/api/v1/review` lack full agent/OpenAPI parity. An official SDK session is not acceptance by every hosted connector. |
| Comments and files | [Attachments](attachments.md); real local HTTP PUT/HEAD/GET, digest verification, retry/finalize and revocation checks against isolated Moto | Production S3 provider/permissions/retention and real legacy object completeness remain external. No automatic blob garbage collection or attachment-backup guarantee from SQLite alone. |
| Scheduling/notifications | [Scheduling](scheduling.md), [notifications](notifications.md), [push](push.md); seven scheduling scenarios per Chromium/Firefox, local SMTP capture and encrypted HTTP push receiver tests | Real SMTP receipt and Apple/Google/Mozilla device delivery unverified. Push digests are unfinished. External-send crash ambiguity remains; no exactly-once delivery claim. |
| PWA | [PWA](pwa.md); emulated phone/tablet/desktop Chromium/Firefox, offline reload, actual browser restart and single logical capture replay | No physical iOS/Home Screen or WebKit acceptance. Closed-app background sync is not promised; disconnected cached copies cannot be remotely erased. |
| Review/statistics | [Statistics](statistics.md), current authorized `/review` and `/api/v1/review`, durable observation worker | No fabricated historical backlog: pre-observation days and older imported completion evidence remain absent. New increment needs its own browser/release acceptance. |
| Migration/recovery/release | [Migration/export](migration-export.md), [replication](replication.md), [deployment](deployment.md); isolated real-copy reconciliation, checksummed ZIP roundtrip, local amd64 Docker/Compose/Litestream recovery | No production cutover, registry push, remote CI run, ARM verification or restore of the user's complete real object corpus. Rebuild/retest the image after new increments. |
| Performance | [Performance probe](performance.md), synthetic in-memory 10,000 tasks and query-plan-backed subtree fix | Whole-list materialization, no virtualized UI, one serialized SQLite connection. No current-feature end-to-end/concurrency/persistent-WAL/mobile memory acceptance. |

### Documentation corrections still needed

Some focused runbooks describe earlier increments: `pwa.md` still says production
authentication and push subscriptions do not exist; `notifications.md` still says
push transport is unimplemented; `performance.md` says comments are not in the
schema; `ci.md` calls production authentication an implementation blocker. Those
claims are superseded by current code and the evidence links above. Their old
measurements remain historical evidence, not measurements of the expanded app.
The CLI scheduling examples also retain an old root-leaf-only recurrence limit;
the common operation now supports bounded nested root checklists. Update those
runbooks with their feature owners before final handoff.

### Changed-file privacy check

A read-only scan of 30 changed/untracked paths at this snapshot found no database,
environment, private-key, identity-map or screenshot filenames. Added text matched
none of the checked private-key delimiters, common provider/AWS key prefixes or
non-example email patterns. No values were printed. This is a narrow heuristic
check of the working diff, not a complete secret/personal-data or git-history
audit; ignored local evidence and original databases were not opened.

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
At this historical increment, invitations, task-to-section assignment and list
moves were unfinished. Later increments below and the current checklist
supersede that status.

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

Historical remaining-work snapshot: later commits supplied browser editors,
favorites, workspace-changing list moves, Compose/release configuration and
recovery runbooks. Current unresolved work is listed at the top, including
interface parity, assistance integration, push digests and final acceptance.

Latest backend integration: `cargo test --locked --lib` passes 129 active tests;
eight explicit service/performance probes remain separate. `cargo clippy
--all-targets -- -D warnings` passed before the next agent-surface extension.
The recurrence editor passes seven actual-browser scenarios in each of Chromium
and Firefox. A regression test confirms already delivered reminder instants are
not requeued by unrelated schedule edits. Favorites are user-scoped and live
permission-filtered; list color changes use the normal list revision. Account
exports now include visible favorites/default destinations and empty workspaces.
