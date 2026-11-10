# Loading states

Server-rendered pages (Home, lists, spaces, search results, trash, sharing forms, account, delivery and authentication) arrive with confirmed data. Keep confirmed content visible during background refreshes and saves. A new destination immediately hides the old page and shows a spinner, before its HTML arrives.

Client-loaded content uses `beginLoading` from `assets/loading.js`. Spinners are the default whenever content size or shape is unknown: task trees, mixed lists, files, discussion, activity, search results and fetched dialogs. Skeletons are explicit choices for predictable forms, single-line values and calendar grids. Unknown controls stay hidden and inert. Placeholders contain no focusable controls and respect reduced motion.

Task side panels use a small spinner instead of predicting a task layout: descriptions, subtasks and attachments vary. The spinner appears after 150 ms to avoid flashing during fast loads, respects reduced motion, and leaves panel dimensions and the close control stable. The task HTML already contains the authorized subtree, planning, tags and rendered discussion/files. Controllers reuse these snapshots, so showing the panel needs no follow-up data reads. Mount controllers behind one spinner to avoid a second loading phase. Failures reveal the affected section’s retry state. The same loading owner handles cancellation, errors and retry.

Settings, welcome and notification templates include `data-loading-pending` so unconfirmed content cannot flash before their JavaScript module arrives. CSS provides an immediate spinner until the module takes ownership.

Call `finish()` in `finally`. For a failed initial load, `fail(message, retry)` removes the animation, keeps unknown values hidden and exposes a retry action. Preserve recoverable drafts instead of hiding them behind a failure. Each container has one current loading owner: a superseded response cannot dismiss the latest placeholder or insert a stale failure.

Use `preserve: true` on refresh after data has loaded. This sets `aria-busy` without removing, hiding or blocking the content. Inbox and Plan also skip unchanged renders and defer changed renders while a pointer interaction or input edit is in progress. Range changes conceal the old range until the new result arrives. Writes retain their existing draft, confirmation and save-feedback behaviour.

Coverage includes Inbox/Plan, notification center and bell, General/AI/Security settings, device status, onboarding, task previews/editor, quick capture, scheduling, metadata and search pickers, discussion/files, sharing/public access, activity, hierarchy, task breakdown and release information. Static screens require no synthetic loading phase.

`loading-states.spec.js` uses delayed and failed requests against an isolated development instance. It checks geometry, hidden defaults, retry, background refresh, dialog cancellation, notifications, search races, task panels, reduced motion and a mobile viewport. Existing settings and welcome tests exercise persistence and interaction after loading.

List headings reserve the color picker's full hit area. Capture and section controls,
task progress and summaries, discussion controls and the navigation logo render
with their final structure. Custom colors arrive in a render-blocking, immutable
palette stylesheet; navigation constructs validated palette rules synchronously
before swapping. Sidebar favorites, list entries and space cards share these
rules. `stable-render.spec.js` stalls scripts and compares initial/final geometry,
colors and controls. It also checks capture overflow at 320–1280px.
