# Loading states

Server-rendered pages (Home, lists, spaces, search results, trash, sharing forms, account, delivery and authentication) arrive with confirmed data. Keep their content visible while navigating or refreshing. Do not replace it with an empty state or a skeleton just because a refresh is running.

Client-loaded content uses `beginLoading` from `assets/loading.js`. The initial skeleton hides unconfirmed controls and makes them inert. Existing children reserve their real layout height; empty containers reserve an approximate height. Rows, forms, calendar columns, security cards, welcome choices, task previews, capture, discussion/files, activity and hierarchy have distinct structures. Skeletons contain no focusable controls, are hidden from accessibility tools and stop animating with reduced motion.

Settings, welcome and notification templates include `data-loading-pending` so unconfirmed content cannot flash before their JavaScript module arrives. CSS provides a fine text-line fallback until the module replaces it with the appropriate structured skeleton.

Call `finish()` in `finally`. For a failed initial load, `fail(message, retry)` removes the animation, keeps unknown values hidden and exposes a retry action. Preserve recoverable drafts instead of hiding them behind a failure. Each container has one current loading owner: a superseded response cannot dismiss the latest placeholder or insert a stale failure.

Use `preserve: true` on refresh after data has loaded. This sets `aria-busy` without removing, hiding or blocking the content. Inbox and Plan also skip unchanged renders and defer changed renders while a pointer interaction or input edit is in progress. Range changes conceal the old range until the new result arrives. Writes retain their existing draft, confirmation and save-feedback behaviour.

Coverage includes Inbox/Plan, notification center and bell, General/AI/Security settings, device status, onboarding, task previews/editor, quick capture, scheduling, metadata and search pickers, discussion/files, sharing/public access, activity, hierarchy, task breakdown and release information. Static screens require no synthetic loading phase.

`loading-states.spec.js` uses delayed and failed requests against an isolated development instance. It checks geometry, hidden defaults, retry, background refresh, dialog cancellation, notifications, search races, task panels, reduced motion and a mobile viewport. Existing settings and welcome tests exercise persistence and interaction after loading.
