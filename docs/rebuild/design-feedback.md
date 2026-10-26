# Reference UI correction pass — 2026-10-26

The old Python templates and supplied screenshots remain the visual reference.
The running Rust development server is rebuilt locally; production is untouched.

## Changes

- Shared gray-950 sidebar without a right border, bottom account popover,
  verified-identity Gravatar hashing, workspace caption, and self-hosted Heroicons.
  List navigation is limited to the selected workspace; the workspace overview
  does not enumerate account-wide lists in its sidebar.
- Workspace overview uses 20-row pages without a breadcrumb. Owners can archive
  and restore a workspace. Deleting an archived workspace requires a review step
  and typing its exact name in a second modal step. Revision checks and live ACL
  views protect every mutation; deletion retains the data and revokes list access.
- Today and Upcoming use the shared list layout, with working task URLs and
  responsive content width rather than the old narrow standalone layout.
- List heading/description editing replaces the display, exposes a labeled
  Markdown field, and supports Escape and Cmd/Ctrl+Enter. Add rows belong to their
  respective sections; opening a form hides its trigger. Completed filtering
  stays in the contextual menu. Stale connection/update copy is removed.
- Initial live-feed cursors start at the rendered account snapshot. Expanded
  sections do not count as drafts. Events for unrelated lists advance the cursor
  without replacing the current list. Actual drafts continue to block replacement.
- List settings open in a right-side tabbed panel; phones use a full-width stacked
  panel with browser-back support. Native forms use the existing permission and
  revision checks. Custom native color-picker values update the visible badge.
- Persisted seven-character list references are copy buttons. Collisions retry;
  renames/moves preserve references. Agent destination resolution accepts them,
  and browser reference URLs redirect to canonical UUID URLs after authorization.
- Section progress counts leaves once and inherits the list color. Completion
  uses SVG checks consistently before and after refresh. Dedicated pointer grips
  support mouse/touch ordering, with menu Up/Down alternatives and server CAS.
- Task pages retain raw Markdown editing, replace the saved brief during editing,
  show ancestor/section breadcrumbs, and disclose planning/tags separately.
  Stored source remains Markdown (literal HTML in a fixture remains literal
  source, never executed). Comment composition opens inline and preserves drafts; successful posting
  closes the composer and refreshes the chronological discussion.
- Custom colors/progress are applied through validated DOM properties, preserving
  the existing CSP rather than enabling inline styles/scripts.
- Account export/import preserves owned workspace trash, lifecycle records,
  native colors and short references; another user cannot export that trash.

## Verification

- `cargo test --locked --lib`: 201 passed, eight explicit service/performance
  probes remain ignored in this command.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- 34 distinct Chromium/Firefox scenarios pass across the final focused runs,
  exercising phone/desktop layouts, forms and keyboard
  submission, color saving, settings tabs/back, workspace lifecycle confirmation,
  pagination, long names, pointer ordering, nested task moves, draft conflicts,
  comments, revocation and logout. See `rebuild/browser/design-feedback.spec.js`,
  `reference-layout.spec.js` and `task-page.spec.js`.
- Browser checks use a separate ignored database and port 8094, leaving the
  owner's visible development data intact. Set `TODOER_BROWSER_URL` to target an
  isolated server. Screenshots/logs remain under ignored `.local/rebuild/`.
- All five user-linked pages were checked again on the rebuilt port 8092 in
  Chromium/Firefox at 390px and 1440px: no horizontal overflow or JavaScript
  errors, no sidebar border, and corrected breadcrumb typography. A screenshot
  check confirmed SVG completion marks render above their circular background.
- The visible development database was backed up before applying migration 23.

## Remaining acceptance limits

These checks simulate phone viewports and pointer interactions; they do not
certify physical-device behavior or WebKit on this host. The full original
rebuild acceptance checklist remains open. New lifecycle/short-reference metadata
also needs the final OpenAPI/CLI/MCP parity review. Deleted workspace data is
retained in owner account exports and database recovery; a dedicated in-app
workspace-trash recovery screen remains unfinished. No production cutover,
permanent purge, push, or deployment was performed.
