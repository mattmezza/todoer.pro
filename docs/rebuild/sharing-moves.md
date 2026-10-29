# Moving a list between workspaces

Owners can open `/lists/{id}/move` from list settings, choose a destination they also own, review the effective access changes, then submit the move. The native form works without JavaScript. The same operation is available through POST `/api/v1/lists/{id}/workspace-move/preview` and POST `/api/v1/lists/{id}/workspace-move`.

The operation preserves the list ID, task IDs and parent relationships, task positions, sections and section assignments, comments, attachment metadata and object keys, history, and saved reference edges. It does not copy file bytes. Each task, including tasks in trash, receives a new revision and a common history/change event. Both workspace revisions and the list revision advance. Existing task edits consequently fail their old revision checks.

Both workspaces require current owner access. Resource-bounded credentials cannot administer this move. A workspace-less legacy list must first receive a separately implemented adoption flow; this operation rejects it. Moving within the same workspace is not a reorder operation and is rejected.

Direct list grants and the restricted flag remain unchanged. Unrestricted lists inherit destination workspace membership; restricted lists retain direct grants and destination-owner access. Preview compares each user's effective rank before and after the move. Any rank increase, including viewer to editor, requires explicit acknowledgment. Direct guests retain access without being shown hidden workspace IDs. Public task history records the policy hash and acknowledgment, not source or destination workspace identities.

The preview binds the actor, both workspace revisions, list revision and restriction, direct grants, complete effective audiences, task IDs/revisions, section/comment/attachment/upload revisions, and tag mappings into a SHA-256 policy hash. Apply recomputes it under one immediate SQLite transaction, checks the supplied revisions and live ownership, and commits all changes together. New content, independent comment edits, renamed destination tags, membership changes and revoked ownership invalidate approval. A stale native submission returns a review link with no changes committed. A completed move is not idempotent replay: retry requires reading the current location and obtaining a new valid preview.

Task tags move to destination workspace scope by normalized name, reusing an existing destination tag when possible. Mapping history retains source and destination tag IDs. Source tag records remain available to other source content. Existing reference edges retain their original target IDs; they are not silently redirected to a different destination tag. Reference rendering and backlinks continue to check live access.

## Verification

`cargo test --manifest-path app/Cargo.toml workspace::tests --lib` covers preservation, permission expansion acknowledgment, restricted/direct guest access, tag reuse and audit, stale snapshots, independent comment revisions, owner revocation, and bounded credentials. All-target Clippy passes.

`npx playwright test app/browser/list-move.spec.js --project=chromium --project=firefox --workers=1` passed six checks against the running Rust server and a synthetic development database: phone and desktop native move forms, nested task/section preservation, stale task CAS, phone overflow and 44px submit target, and stale-preview recovery. WebKit was not verified because its required host libraries are unavailable. These checks do not establish production object-storage availability or real-account migration completeness.

## Section move REST adapter

POST `/api/v1/sections/{id}/move/preview` accepts strict JSON
`{"destination_list_id":"…"}`. POST `/api/v1/sections/{id}/move` accepts the
returned `section_revision`, `source_list_revision`,
`destination_list_revision`, `policy_hash`, the same `destination_list_id`, and
explicit `acknowledge_access_change`. Both routes reuse the shared domain
operation and require authenticated write capability; browser cookies also need
the current CSRF header. Unknown fields fail validation. Authentication failure,
current permission loss and stale/unacknowledged plans return 401, 403 and 409
respectively. No route grants membership or copies file objects.

A section move needs current editor access on both lists (rather than the owner
rights required by a whole-list workspace move). Preview exposes only audience
counts and an expansion flag, not member identities or inaccessible workspace
metadata. The section ID/title/description and assigned task subtrees move
atomically; slug collisions are reallocated in destination scope. See
[section-moves.md](section-moves.md) for the 500-task bound, hidden-task handling,
other-section assignment warnings and transaction/policy fencing. Actual router
tests cover unauthenticated requests, cookie CSRF, strict fields, acknowledged
expansion, stale review and successful preservation.

## Atomic capture into a section

The existing POST `/api/v1/capture` accepts an optional top-level `section_id`
alongside `list_id` and `tasks`. `Store::capture_at` uses the same public
idempotency-key namespace as ordinary capture; changing the section, payload or
switching between sectioned/ordinary capture with the same key conflicts. It
validates current destination write access and section membership in that list,
then captures and assigns all tasks in one transaction. Its private temporary
capture key is collision checked and removed before storing the final response;
no temporary retry row survives success or rollback.

New subtree roots can receive the requested section. A captured child inherits
its existing parent's section: explicitly requesting that same section succeeds
without reparenting or revising siblings; requesting a different section fails
rather than moving an existing parent's checklist implicitly. Section renames do
not invalidate a successful exact-key retry, because the destination is its
stable ID, not its name/revision. A section moved to another list is no longer a
valid destination for the original request. Current list write permission and
every captured task's current list permission are rechecked on replay; revoked
credentials or tasks moved outside their resource scope cannot obtain a stored
response. A replay returns its original response and revisions, so read current
task state before a subsequent CAS edit.

Synthetic tests inject failure during a later task's section assignment after
prior assignment and ancestor reopening. They verify tasks, locations, history,
ancestor revisions/completion and both public/private retry rows all roll back;
removing the fault allows the same public key to succeed exactly once. Additional
tests cover section renames/relocation, permission revocation, scoped credentials,
moved-task replay, inherited parents and shared-key payload mismatches.

`Store::create_list_with_color(actor, CreateList, Option<String>)` likewise wraps
the ordinary list constructor and palette setter in one transaction. Unknown
colors roll back the list, workspace revision change, slug allocation and search
index changes. `None` keeps the ordinary uncolored default. The existing create
list HTTP adapter accepts optional `color` without changing the shared
`CreateList` DTO. This constructor has no idempotency key; clients must not
silently retry an uncertain creation. Permission and resource-scope restrictions
remain those of the shared workspace/list operations.
