# Network resilience review (2026-10-27)

Reviewed browser request paths for lists, task details, scheduling, metadata,
ordering, discussion/files, Inbox/Plan, search, capture, preferences, sharing,
workspace actions, review and export.

- Module fetch calls use request.js: bounded reads/writes, longer binary upload
  deadline, expired-session redirect detection, explicit unknown-write outcome.
  Writes are never automatically retried here. Existing offline captures retain
  their idempotency keys and replay protocol.
- Existing task, scheduling, metadata and discussion controllers retain their
  lifecycle cancellation, revision checks, saved drafts and mutation locks.
  Ordering waits for canonical server confirmation before moving nodes.
- Inbox/Plan now freeze competing controls in the affected card/form/dialog,
  preserve failed drafts, hide results from a previous calendar range while
  loading, ignore outdated loads, and expose refresh retry.
- Search/destination pickers remove stale choices immediately, including during
  debounce. Search uses contextual informational/empty-state icons.
- Calendar preference fields remain disabled during initial load/save. Calendar
  and animation read/modify/write sequences are serialized within the page, so
  concurrent changes to these forms do not overwrite one another.
- Preview refresh failures are reported rather than silently ignored. A delayed
  initial priority read cannot overwrite the displayed priority after a write.
- Optimistic list completion failure wording distinguishes restoring the display
  from proving a server-side rollback.

Verification: network-resilience.spec.js exercises aborted writes with retained
Inbox drafts, duplicate submits, failed refresh/retry, search invalidation,
uncertain-write messaging, intentional cancellation, and overlapping settings
saves. Passed in Chromium and Firefox. Main-screen navigation smoke check had no
JavaScript errors. WebKit cannot launch here due to missing host libraries.
The older capture/scheduling/metadata/task-page browser suite is not green: it
still expects removed controls, visible legacy capture forms, and uncanonicalized
UUID URLs. It was interrupted after those failures; this review does not claim a
full regression pass.

Limits: preference serialization is per page, not cross-tab compare-and-swap.
Request deadlines and client-side checks cannot prove whether an interrupted
write committed. Non-idempotent writes need explicit saved-state reconciliation
before retry. These tests simulate failures/latency, not production load.

## Mobile navigation and asset caching (2026-11-10)

- A different destination immediately conceals old page content and shows a
  spinner. Background refreshes and saves retain usable content. Aborts and
  failures restore the previous page and its drafts; the newest navigation owns
  loading feedback. Controllers load concurrently after a successful swap.
- Spinners are the default for unknown sizes, including mixed Inbox content,
  search results, task trees, activity, attachments/comments and fetched dialogs.
  Skeletons are opt-in for known form controls, single-line values and calendar
  grids. Initial HTML has a CSS spinner before its JavaScript arrives.
- Page-rendered timezone and animation preferences are reused by controllers.
  They refresh with every navigation and successful local preference save;
  writes still fetch the latest document before merging changes.
- Scripts, styles and their relative module/font dependencies use
  `/static/<content-and-release-hash>/...` URLs, with one-year immutable HTTP
  caching. The hash covers nested asset directories and the release version.
  Shared dependency preloads reduce cold import waterfalls. Gzip compresses
  eligible responses; SSE keeps the compression layer's streaming exclusion.
- The worker serves immutable assets cache-first, validates build headers before
  filling caches, and warms at most three assets concurrently using the existing
  HTTP cache. The previous static build remains available for existing tabs.
  Unversioned install metadata revalidates. `/sw.js` and private pages stay
  `no-store`. Cache writes do not delay page responses.
- Offline list copies are isolated by account and build, bounded to ten pages,
  and cleared on logout/account changes. New-release HTML causes a full
  navigation rather than mounting with old module instances.

Checks: `make setup`, `make check`, `make test`;
`navigation-performance.spec.js`, `app-navigation.spec.js`,
`loading-states.spec.js`, `network-resilience.spec.js`,
`list-task-panel.spec.js` and `planning-priority.spec.js` exercise these paths.
The worker harness includes a stalled-network warm read, cold fill and mismatched
release checks; Rust tests verify cache headers, missing-version errors, HTML
rewrites and response compression. Browser checks use synthetic local data.

## Stable first render and cold-page request budget (2026-11-10)

Task HTML includes authorized subtree/planning/tag snapshots and rendered files
and discussion. The controllers consume snapshots once, then use live reads for
refreshes. List controls and logo markup are server rendered; heading swatches
reserve the final picker footprint. Palette CSS colors custom badges before JS,
including favorites and sidebar lists. Navigation installs validated color rules
synchronously. Emoji overlays share their badge's actual bounds.

The build bundles named page controllers and shares minified modules. Geist and
Inter are local variable fonts with subset ranges, optional display and a preload
for the selected Latin face; the existing numeric face retains swap display. The
worker no longer speculatively downloads unused feature modules. App icons use
content-hashed URLs in HTML, generated JS and install metadata; immutable cache
headers avoid revalidation, while the manifest itself revalidates.

In a fresh synthetic Chromium context, the same space-page setup fell from 182
requests to 32 (Firefox: 34), with 23 script requests and no third-party font
requests. Prior remote fonts attempted 121 requests across weights, subsets and
fallback formats in this environment. The asset-budget browser test checks cold
requests, immutable icon headers and zero icon transfer on reload. These figures
are local synthetic measurements rather than a production traffic benchmark.
The final Chromium measurement transferred approximately 196 KB of response
bodies with compression enabled. Unversioned icon aliases deliberately revalidate;
the application and manifest reference their immutable content-hashed URLs.

Final verification: `make check`, `make test` (360 Rust tests passed, nine
explicitly ignored probes), Clippy with warnings denied, formatting and the
service-worker harness passed. Chromium and Firefox regressions cover stable
first render with scripts held back, 320–1280px capture geometry, cold asset
budgets, zero-transfer icon reloads, immediate navigation feedback and release
handoff, settings, Inbox/task panels, task editing and structured editing.
The final 72-case selection passed after correcting a settings test to wait for
the controller before invoking its settings API (the corrected case was rerun
successfully in both engines). The broader 130-case selection additionally
exercised drafts/conflicts, access revocation, offline navigation and logout;
its earlier failures were covered by that final regression selection.
WebKit still cannot launch because required host shared libraries are missing.
All browser checks used the isolated synthetic database, not the local Google
authentication database or production data.
