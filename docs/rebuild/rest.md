# Rebuild REST API

[OpenAPI 3.1 contract](../../app/openapi.json) describes the JSON endpoints under `http://127.0.0.1:8092/api/v1`. This is an isolated development server backed by synthetic data. Its static bearer token and browser cookie adapter identify the synthetic development user; they are not production authentication. The contract does not describe an OAuth deployment.

Send `Authorization: Bearer <TODOER_TOKEN>` for an agent or CLI. Development browser sessions use the `todoer_dev` cookie issued by the development sign-in form; every cookie-authenticated write additionally requires the page's `X-CSRF-Token`. Bearer writes do not require that header. Request bodies use `Content-Type: application/json`; the server caps bodies at 128 KiB.

| Method | Path after `/api/v1` | Result |
| --- | --- | --- |
| GET | `/capabilities` | Runtime version, feature and limit discovery |
| GET | `/lists` | Accessible lists |
| GET | `/lists/{id}/tasks` | Active tasks ordered by position, then ID |
| POST | `/capture` | Atomic batch capture; requires `Idempotency-Key` |
| PATCH | `/tasks/{id}` | Revision-checked update |
| GET | `/tasks/{id}/subtree` | Root and active descendants |
| GET | `/tasks/{id}/move-preview?destination=ID` | Counts of current/destination members and newly added members |
| POST | `/tasks/{id}/move` | Revision-checked subtree move |
| GET | `/search` | Accessible active tasks ordered by task ID |
| GET | `/changes?after=N` | Up to 1000 accessible history events in sequence order |
| GET | `/trash` | Accessible trashed tasks |
| POST | `/tasks/{id}/trash` | Revision-checked atomic subtree trash |
| POST | `/tasks/{id}/restore` | Revision-checked restoration from a trash operation root |
| POST | `/history/{seq}/undo` | Revision-checked compensation for an eligible individual update |

Capabilities currently reports version `0.1.0-dev`, mode `development-only`, REST/CLI/development-MCP interfaces, operation names, limits, prefix-search semantics, and `production_auth:false`. Limits are capture 100 tasks, search 100 results, changes 1000 events, title 500 characters, and description 100000 bytes.

A task response has `id`, `reference`, `list_id`, nullable `parent_id`, `title`, `description`, `complete`, `revision`, and `position`. A list has `id`, `title`, `description`, and `revision`. Use IDs in routes and parent links; do not infer them from titles or display references.

Capture takes `{"list_id":"inbox","tasks":[{"title":"Draft proposal","description":"","parent_id":null}]}`. A batch contains 1–100 tasks; titles must contain non-whitespace text and have at most 500 characters. Descriptions have a 100000-byte limit. A parent must already exist in the destination list. Reuse an idempotency key for an identical retry: it returns the original task array. A reused key with a different payload conflicts. Keys contain 1–200 bytes as checked by the server.

Updates take `revision` plus any of `title`, `description`, `complete`, and `complete_subtree`. Completing a parent with unfinished descendants requires `complete_subtree:true`; reopening a child reopens completed ancestors. Obtain the current revision before each mutation. After a conflict, read the task again and reconcile the change before retrying.

Moves take `{"destination":"groceries","revision":1,"acknowledge_access_change":false}`. The destination is a list ID. Review the preview before explicitly acknowledging an increase in who can access the subtree. The move preserves internal parent links, clears the root's parent, updates descendant list IDs, and increments their revisions atomically.

Search accepts `query` (default empty, at most 4000 UTF-8 bytes), optional `list` ID, optional `complete=true|false`, `limit` (1–100, default 50), and optional `after` task ID. Text terms are treated as prefix matches. Results exclude trash and are ordered by ID; pass the last returned ID as the next `after` cursor with the same filters. URL-encode all query values.

Trash, restore, and undo accept `{"revision":CURRENT_REVISION}`. Restore the original trash operation's root; descendants trashed separately are not restored with it. Undo is limited to the original actor's eligible individual task update and refuses to overwrite later changes. Undo compensates one update event, never an entire cascade. Capture, moves, trash, and restore are not undoable through this endpoint.

History events contain `seq`, `kind`, `actor`, and a `task` snapshot. Visibility depends on membership both in the event's list and the task's current list. The feed is a bounded synchronization mechanism for accessible work, not an unrestricted audit export.

Domain errors use `{"error":{"code":"conflict","message":"stale revision"}}`. Common statuses are 400 for invalid domain input, 401 for missing authentication, 403 for denied access or CSRF, 404 for missing resources, 409 for stale revisions or domain conflicts, and 500 for internal errors. JSON extraction failures use the same error envelope with code `invalid`. Malformed query strings, path parameters or browser form extraction may use plain Axum errors; preserve their status code.


JSON write bodies reject unknown fields, including unsupported scheduling fields,
with a structured `invalid` error (422). Malformed JSON/content-type failures
also use the error envelope. Malformed URL query/path/form extraction may still
use Axum's plain error responses. The browser quick-add form has an authenticated,
CSRF-checked native POST fallback so capturing during script startup cannot
silently turn into an unhandled GET navigation.

Each HTTP response supplies a server-generated `X-Request-Id`, also recorded in
structured request logs and task/discussion activity events created by that
request. Client-supplied request IDs are ignored. Background or direct domain
operations have a null request ID; actor and credential attribution still apply.
Request logs contain method, status and elapsed time, without URLs, bodies,
cookies or authorization headers. Nested blocking operations restore their
request context even after a panic, preventing cross-request attribution.
