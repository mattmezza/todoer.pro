# Bounded agent plans and destinations

Bulk operations use the application's existing mutation methods. A write outside a bulk transaction uses an immediate SQLite transaction; a write inside one uses a savepoint. An outer guard rolls back every uncommitted bulk change, including history, reference indexing and other database side effects. There is no second implementation of task mutation rules.

`POST /api/v1/bulk/preview` accepts an `operations` array. Each operation has `op`, a task ID or full stable task reference, its expected `revision`, and the operation's fields:

| `op` | Fields beyond ID and revision |
| --- | --- |
| `update` | Optional `title`, `description`; at least one is required |
| `move` | `destination` list ID; explicit `acknowledge_access_change` when required |
| `trash`, `restore` | No additional fields |
| `tags` | `tags`, containing existing names or `{ "id": "TAG_UUID" }` selectors |
| `section` | Nullable `section_id` |
| `reparent` | Nullable `parent_id`; same-list rules apply |

Tag names are resolved to existing IDs in their home before preview. A missing tag returns an actionable error; create it with the individual task-tag operation first. Bulk preview never creates tag identities.

Unknown fields are rejected. There are no create operations and no bulk completion operation in this contract. Completion can create recurring occurrences, so its generated resources are not represented by this create-free preview. Use the ordinary task update endpoint for completion.

Preview executes the real operations and rolls them back. It returns an actor-bound `preview_id`, `expected_hash`, `expires_at`, and ordered `steps` with canonical operations and before/after states. Task states include IDs, titles, revisions, placement, completion/deletion flags, tag names, a description excerpt of at most 120 characters, and a full-description hash. Move steps include their audience-change preview. Description hashes detect edits without returning whole descriptions in every step.

Plans expire after ten minutes. A request supports 1–50 operations and at most 500 affected task states across its steps; a stored plan is capped at 1 MiB. At most 100 unexpired plans are retained per actor. Subtree operations review descendants as well as the root; reparenting also reviews new ancestors that the shared rules may reopen.

`POST /api/v1/bulk/apply` accepts only `preview_id`, `expected_hash`, and `idempotency_key`. It executes the server-stored operations. Every step checks the reviewed task states, list revisions, effective permission audience and move preview against current data, then runs the common mutation method with live authorization. A changed descendant or audience invalidates the plan even when the root revision is unchanged. A later failure rolls back earlier successful steps.

A successful apply records its complete result in the same transaction. Retrying the same actor/key and identical preview/hash returns that result without repeating writes. Reusing a key with a different preview or hash conflicts. A retry rechecks current access to the result's historical lists and the tasks' current lists before returning any stored content. A different actor or credential cannot reuse another actor's plan. An expired or stale plan requires a new preview.

Destination endpoints return small authorized results rather than the account's complete contents:

| Method and path | Behavior |
| --- | --- |
| `GET /api/v1/destinations/search?prefix=TEXT&limit=20` | Up to 30 accessible, active list IDs and titles; literal SQL wildcard characters are escaped |
| `GET /api/v1/destinations/resolve?destination=ID_OR_EXACT_TITLE` | Prefer an accessible exact ID, then exact title matches; duplicate titles return `ambiguous` and choices |
| `GET /api/v1/destinations/resolve?default_key=capture&fallback_inbox=true` | Use the saved default; optionally try the saved `inbox` default, then an accessible exact `Inbox` title |
| `GET /api/v1/destinations/preferences` | Only this user's preferences whose destinations are still accessible |
| `POST /api/v1/destinations/preferences` | Save `{ "key": "capture", "list_id": "LIST_ID" }`; requires current write access to that destination |

Resolution returns `status` (`resolved`, `ambiguous`, or `missing`), optional `destination`, bounded `choices`, and `has_more`. Exact-title comparisons preserve case and Unicode spelling. Explicit input never silently falls back to another destination when it is missing or ambiguous. No conversation state or model guess chooses a destination.

Preferences belong to the authenticated user, and scoped credentials can access only defaults pointing to lists currently within their grant. Keys contain 1–40 lowercase ASCII letters, digits, underscores or hyphens; each user has at most 100. Saving a preference updates that key; it does not create or rename a list. Browser bootstrap can save the actual inbox ID instead of relying on a title heuristic.

Domain tests cover nested savepoint rollback, preview rollback, atomic apply failures, stale descendants, hash/key binding, actor binding, expiry, permission loss, exact retries and bounded authorized destination resolution. Run `cargo test --locked agent_ops::tests --lib`.
