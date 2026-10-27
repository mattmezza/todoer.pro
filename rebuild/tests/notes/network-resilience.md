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
