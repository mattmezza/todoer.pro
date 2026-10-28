# Account welcome

New verified accounts receive a short, optional `/welcome` flow after sign-in.
Existing accounts are backfilled as complete and keep their normal destination.
Both Google and local/passkey sign-in use the pending account state. There is no
signup interception before verification or MFA, and skipping remains available.

Step one explains and names a space: an area of life that groups lists.
Step two explains and names its first list: a home for related tasks, with a live
color preview. Names are saved as lowercase, hyphen-separated slugs. Personal,
Work and Together presets suggest different names and colors; Together suggests
home/chores for partners or housemates, without enabling sharing automatically.

The flow reuses account defaults rather than duplicating them. No task creation,
calendar choices, permission prompts or feature grid interrupt the welcome.
Users add tasks in the app afterward. An explicit sign-in return destination
still takes precedence over the personalized list landing.

Account progress lives in SQLite. Revisions detect stale tabs; authorized normal
mutation methods run inside one atomic write per step. Completion replay
protection makes retries safe after a lost response. Errors retain the current
form, and offer reload/escape options. A user can return through General settings
and deliberately start another welcome run. Mobile installation is suggested
only after leaving Welcome. Reduced-motion preferences suppress the short finish
animation. Actual operating-system installation and push remain separate tests.

Validation: 243 backend tests passed (8 ignored), including rollback, resume and
replay checks. Four Chromium/Firefox browser checks cover desktop/mobile layout,
slug input, color changes, reload, skipping, installation deferral, and an
intentionally lost completion response followed by retry. Desktop and mobile
screenshots were visually reviewed. Physical mobile browser behavior is not
established by viewport emulation.
