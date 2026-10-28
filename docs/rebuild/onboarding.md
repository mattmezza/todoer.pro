# Account welcome

New verified accounts receive a short, optional `/welcome` flow after sign-in.
Existing accounts are backfilled as complete and keep their normal destination.
Both Google and local/passkey sign-in use the pending account state. There is no
signup interception before verification or MFA, and skipping remains available.

Step one personalizes the existing default space/list and its color. It does not
create another pair, change visibility, invite people or enable AI/notifications.
Personal, Work and Together are editable examples. A live preview teaches the
space/list/task relationship, while a small capability panel introduces planning,
reminders and connected AI assistance without extra setup questions.

Step two optionally creates one task, with P3 priority and an optional date-only
plan for today in the account timezone. Otherwise it opens the personalized list.
An explicit sign-in return destination takes precedence over the default landing.
No tutorial tasks, permissions prompts or paid conversion step are injected.

Account progress lives in SQLite. Revisions detect stale tabs; authorized normal
mutation methods run inside one atomic write per step. Completion and capture
idempotency protect retries after a lost response. Errors retain the current
form, and offer reload/escape options. A user can return through General settings
and deliberately start another welcome run. Mobile installation is suggested
only after leaving Welcome. Reduced-motion preferences suppress the short finish
animation. Actual operating-system installation and push remain separate tests.

Validation: 243 backend tests passed, including welcome rollback, resume and
idempotency checks. Four Chromium/Firefox browser checks cover desktop/mobile
layout, reload, skipping, installation deferral, and an intentionally lost
completion response followed by retry. The isolated signup/password/MFA/recovery/
WebAuthn acceptance flow passes with the new welcome redirect and skip behavior.
Desktop and mobile screenshots were visually reviewed. Existing local accounts
were verified as complete after migration 37. Physical mobile browser behavior
is not established by viewport emulation.
