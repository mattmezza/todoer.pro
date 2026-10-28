# Notification center

The authenticated shell includes a bell with an unread badge, a five-item recent panel and a full `/notifications` page with All/Unread filters and 25-item cursor pagination. `/settings/notifications` controls categories, muted lists, quiet hours and email summaries; `/settings/devices` enables browser push. Public pages have no bell or private notification API access.

## Identity and delivery

- SQLite migration 036 stores inbox entries and read state per user. Shared task changes are recorded in the task mutation transaction and keyed by history sequence; authors do not notify themselves, including actions through their agent credentials. Existing history is not backfilled.
- Overdue open tasks are keyed by task and actual deadline, independently of priority/planned-date revisions. Date-only deadlines expire at the following local midnight in each recipient's timezone. A changed deadline creates a new alert if overdue. Existing overdue tasks are discovered on the first worker tick.
- Scheduled reminders are keyed by task, recipient and timestamp, merging email/push channel copies. Transport cancellation or disabled push does not remove the in-app reminder. Removing the reminder or completing the task hides it. A delivered reminder retained in a new schedule revision stays delivered and does not replay.
- Inbox rows reference live tasks instead of storing private title snapshots. Reads and badge counts recheck list access, task deletion/completion, list/space archival and current due/reminder schedules. Shared-change entries remain visible after task completion. Revoked access hides entries immediately.
- Shared updates and due alerts enqueue push only when opted in at creation. Explicit reminder pushes keep their existing delivery jobs, avoiding a second push from the inbox. Push reuses session-bound subscriptions, per-device leases, retry limits and live permission/preferences/read-state checks. Marking an entry read cancels unsent inbox push delivery. An already accepted push cannot be recalled.
- Push sends generic text and opens the notification center for due/shared alerts. Stable intent/device tags collapse retry duplicates in the browser; external delivery remains best effort, not exactly once. Email remains reminders/summaries, not a second email for every shared edit.

## Refresh and failure behavior

An independent server worker projects deadlines/reminders every 15 seconds even without SMTP or VAPID. The shell polls every 15 seconds while visible and online, refreshing immediately on reconnect/tab visibility. Failures back off to 30 seconds and preserve the last known badge. Bell and badge animate on a new entry, respecting reduced-motion preferences. Quiet hours defer external delivery, while in-app entries still appear.

Opening the panel does not mark anything read. Individual acknowledgements and “mark all as read” are server-confirmed; the latter includes only IDs through the last fetched watermark, preserving arrivals during an outstanding request. Read state synchronizes on other tabs/devices at their next poll. Notification data is never cached by the service worker.

## Local acceptance

Run `CARGO_INCREMENTAL=0 cargo test --manifest-path rebuild/Cargo.toml --lib` and `npx playwright test rebuild/browser/notifications.spec.js --project=chromium` against a development instance. The browser test checks presentation, arrival animation, pagination, mobile overflow and failed acknowledgements. Rust tests cover user isolation, access revocation, timezone boundaries, schedule identity, channel deduplication and push leases/read-state checks.

For a real push check, use the HTTPS authentication instance, open Settings → Notifications → Manage push devices, enable this browser explicitly, and schedule a push reminder. Verify the bell/inbox with push disabled too. Browser permission and an actual provider delivery still require a manual acceptance test.

The local Google-auth launcher now creates a persistent mode-0600 `.local/rebuild/push-local.key` automatically and enables VAPID for the HTTPS test instance. Explicit `TODOER_PUSH_*` environment values override its defaults. Browser subscription and permission remain opt-in. Keep this local private key out of git, and provision a separate persistent key/contact for production.
