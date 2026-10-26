# Moving a list between workspaces

Owners can open `/lists/{id}/move` from list settings, choose a destination they also own, review the effective access changes, then submit the move. The native form works without JavaScript. The same operation is available through POST `/api/v1/lists/{id}/workspace-move/preview` and POST `/api/v1/lists/{id}/workspace-move`.

The operation preserves the list ID, task IDs and parent relationships, task positions, sections and section assignments, comments, attachment metadata and object keys, history, and saved reference edges. It does not copy file bytes. Each task, including tasks in trash, receives a new revision and a common history/change event. Both workspace revisions and the list revision advance. Existing task edits consequently fail their old revision checks.

Both workspaces require current owner access. Resource-bounded credentials cannot administer this move. A workspace-less legacy list must first receive a separately implemented adoption flow; this operation rejects it. Moving within the same workspace is not a reorder operation and is rejected.

Direct list grants and the restricted flag remain unchanged. Unrestricted lists inherit destination workspace membership; restricted lists retain direct grants and destination-owner access. Preview compares each user's effective rank before and after the move. Any rank increase, including viewer to editor, requires explicit acknowledgment. Direct guests retain access without being shown hidden workspace IDs. Public task history records the policy hash and acknowledgment, not source or destination workspace identities.

The preview binds the actor, both workspace revisions, list revision and restriction, direct grants, complete effective audiences, task IDs/revisions, section/comment/attachment/upload revisions, and tag mappings into a SHA-256 policy hash. Apply recomputes it under one immediate SQLite transaction, checks the supplied revisions and live ownership, and commits all changes together. New content, independent comment edits, renamed destination tags, membership changes and revoked ownership invalidate approval. A stale native submission returns a review link with no changes committed. A completed move is not idempotent replay: retry requires reading the current location and obtaining a new valid preview.

Task tags move to destination workspace scope by normalized name, reusing an existing destination tag when possible. Mapping history retains source and destination tag IDs. Source tag records remain available to other source content. Existing reference edges retain their original target IDs; they are not silently redirected to a different destination tag. Reference rendering and backlinks continue to check live access.

## Verification

`cargo test --manifest-path rebuild/Cargo.toml workspace::tests --lib` covers preservation, permission expansion acknowledgment, restricted/direct guest access, tag reuse and audit, stale snapshots, independent comment revisions, owner revocation, and bounded credentials. All-target Clippy passes.

`npx playwright test rebuild/browser/list-move.spec.js --project=chromium --project=firefox --workers=1` passed six checks against the running Rust server and a synthetic development database: phone and desktop native move forms, nested task/section preservation, stale task CAS, phone overflow and 44px submit target, and stale-preview recovery. WebKit was not verified because its required host libraries are unavailable. These checks do not establish production object-storage availability or real-account migration completeness.
