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
handlers. Authentication, downloads and links marked `data-full-navigation` use
normal browser navigation. Cached list pages also work through HTMX offline.

Task titles open their dedicated page; clicking the rest of a list row opens its
side panel. Panels reuse the freshly rendered task and description, share their
schedule read, and load metadata when the Tags tab opens. Account timezone reads
are shared within the current page scope. Editable task snapshots remain fresh
on each opening, and switching panels cancels abandoned reads.

Page controllers register with `onPageLoad` from `assets/page-lifecycle.js`.
Check for the controller's page root before mounting, and use the supplied scope
for listeners, requests, timers and observers. Return or register cleanup for
other resources. The controller runs again after each page swap; module imports
run only once. Shared helpers can use `pageScope()` to register their cleanup.
New top-level page modules belong in the navigation module allowlist and new
offline dependencies in the service worker's static asset list.

HTMX is pinned and self-hosted. `make assets` applies the documented browser
compatibility patch in `scripts/prepare-htmx.mjs`; review it when upgrading.
`app/browser/app-navigation.spec.js` checks navigation against a synthetic server.
