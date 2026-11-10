# Todoer application

The current Rust implementation: Axum HTTP server, Askama templates, SQLite,
REST and MCP interfaces, administrative importer, and standalone REST CLI.
Run commands from the monorepo root; see [README](../README.md).

- `src/bin/todoer-server.rs`: web/API/MCP server.
- `src/bin/todoer.rs`: user-facing CLI.
- `src/bin/todoer-migrate.rs`: explicit administrative import/export tool.
- `migrations/`: sequential Rust schema migrations.
- `templates/`, `assets/`: server-rendered UI and frontend assets.
- `browser/`, `tests/`: synthetic browser and integration checks.

The Rust database is independent from the historical Python schema. See the
[migration runbook](../deploy/MIGRATION.md) before importing real data.

## Browser navigation

App pages return full server-rendered HTML for both normal visits and HTMX
navigation. `assets/page-navigation.js` keeps the sidebar and mobile header
mounted, swaps `#main`, updates their contents and follows server redirects and
browser history. Existing task editors, panels and API forms retain their own
page controllers. Authentication, downloads and links marked `data-full-navigation` use
normal browser navigation. Cached list pages also work through HTMX offline.

Task titles open their dedicated page; clicking the rest of a list row opens its
side panel. Panels reuse the freshly rendered task and description, share their
schedule read, and load metadata when the Tags tab opens. Account timezone reads
are shared within the current page scope. Editable task snapshots remain fresh
on each opening, and switching panels cancels abandoned reads.

The list color picker also accepts an optional emoji. It is stored separately
from color, shared with everyone who can access the list, and rendered as escaped
text over its badge. `GET /api/v1/lists/{id}/color` returns `color` and `emoji`;
creation, list updates and revision-checked color updates accept `emoji`. Omit it
to preserve the current marker, or send an empty string to remove it. Native
exports and backups retain it.

Double-click or double-tap a task row or title to complete it. Pointer navigation
waits 300 ms to distinguish a second tap; keyboard activation, modified links,
completion controls and editors keep their normal behavior. The gesture invokes
the existing completion control so subtree confirmations and permissions remain
consistent.

Task and subtask rows show quiet comment and subtask counts when nonzero, before
the row menu where available. The same indicators appear in Inbox, Today,
calendar, search, and subtask rows within task pages and panels. Task detail
headers do not show counts. Comments exclude deleted entries;
subtasks count immediate live children, excluding archives and trash. Counts
arrive with the existing row reads and update through the existing live refresh.
Subtask rows support the same configured mobile swipe actions and Undo as lists. Task and subtask row bodies open the shared details panel; title links open the full task page. The panel owns one browser history entry: Back closes it, Forward restores it, and switching the preview replaces that entry.

New tasks default to P3 (Low), displayed in the theme text color. P4 (Lowest)
is available in capture (`p4` or `!4`), editors, filters and search, and uses the
muted color. Entity add/edit forms share compact bordered fields, growing
textareas, a primary save action and a secondary cancel action. Optional emoji
controls in the list color picker start collapsed.

Local UI state belongs in Alpine components: shared dialogs, menus, search,
filters, color pickers and preference forms. Keep their markup in templates and
their data factories in external modules registered once in `assets/alpine.js`.
The pinned `@alpinejs/csp` runtime is self-hosted by `scripts/prepare-alpine.mjs`;
keep the existing CSP without `unsafe-eval`. Use string values for boolean ARIA
bindings, such as `busy ? 'true' : 'false'`, to retain the false state explicitly.

Alpine observes added and removed HTML, including HTMX fragments. The bootstrap
also marks `#main`, `#navigation` and `[data-app-dialogs]` as initial roots so
removing their subtrees cleans up component listeners and effects. Shared dialogs
are replaced together during navigation. Do not call `Alpine.start()` again from
page controllers. Components that own asynchronous work use `createScope()` from
`assets/page-lifecycle.js` and dispose it in `destroy()`; guard late responses when
their scope has been aborted. Writes that must survive a fragment replacement,
such as section visibility, use the surrounding page scope instead.

Navigation, API helpers, offline storage, gestures and the complex editor
integrations remain plain modules. Their page controllers register with
`onPageLoad` from `assets/page-lifecycle.js`.
Check for the controller's page root before mounting, and use the supplied scope
for listeners, requests, timers and observers. Return or register cleanup for
other resources. The controller runs again after each page swap; module imports
run only once. Shared helpers can use `pageScope()` to register their cleanup.
New top-level page modules belong in the navigation allowlist and the named
entries in `scripts/bundle-app.mjs`. Assets are cached on demand; do not preload
unused feature modules from the service worker.

HTMX and Alpine are pinned and self-hosted. `make assets` copies Alpine's CSP
distribution and applies the documented HTMX browser
compatibility patch in `scripts/prepare-htmx.mjs`; review it when upgrading.
`app/browser/app-navigation.spec.js` and `app/browser/alpine-ui.spec.js` check
navigation, component lifecycles and interaction behavior against a synthetic
server. `make assets` also prepares local Geist/Inter variable fonts, builds
shared minified ES modules with esbuild, and fingerprints the app icons. Run it
before building Rust; Docker uses the same steps. Generated fonts, image copies,
CSS and bundles are ignored. Preserve the upstream font notices when upgrading.
Scripts/styles/fonts use release-specific immutable URLs; icon URLs hash their
own contents. The install manifest revalidates and points at versioned images.
