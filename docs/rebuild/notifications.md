# Planning and notification policy

Today includes incomplete planned or due tasks from the last year through today.
Upcoming covers the next fourteen days. Date-only planning and due values keep
their entered calendar date. Timed due values are projected into the account's
IANA timezone. Planning ranges are bounded to 366 days and return at most 10,000
scheduled tasks, with current list access checked before inclusion.

Weekly statistics count completion transitions of tasks marked as leaves at the
time of the event. Parent containers are excluded. Reopening then completing a
leaf counts another transition. Legacy history without a timestamp or leaf
snapshot is excluded; timestamps are never inferred from current task state.

Notification preferences are account settings. Email delivery defaults off.
Timezone, optional local quiet hours, daily and weekly opt-in, local delivery
clock times and per-list mute IDs are explicit. The browser preference endpoint
requires session authentication, origin checks and CSRF for writes. Scoped agent
credentials cannot inspect or change account-wide preference documents.

The scheduling REST interface uses common task revisions for schedule changes
and separate series revisions for pause/resume/stop. Recurring pages offer these
explicit controls; stopping preserves tasks and occurrence history. SMTP worker
delivery is documented below when configured; no push transport is advertised.

## SMTP worker

Configure `TODOER_SMTP_HOST` and `TODOER_SMTP_FROM` to enable a worker. Optional
`TODOER_SMTP_PORT` defaults to 587; `TODOER_SMTP_USERNAME` and
`TODOER_SMTP_PASSWORD` must be supplied together. STARTTLS is required by default;
set `TODOER_SMTP_TLS=implicit` for implicit TLS, normally port 465. Certificate
verification uses rustls and Mozilla trust roots. SMTP secrets are never stored in
SQLite or included in worker logs.

Production recipients come only from an identity registered with a verified
email, and must opt in. Arbitrary recipient addresses are not accepted from task
content. Per-list mutes, current task schedule revision, completion/trash state,
list archive state and current recipient permissions are checked immediately
before preparing each reminder. Preferences and quiet hours are read again for
send preparation. A permission change concurrent with an external send cannot
recall a message already accepted by SMTP.

Quiet hours defer an intent without consuming its retry allowance. Equal start
and end mute all hours. Overnight intervals work in the selected IANA timezone;
a nonexistent quiet-end time moves forward to the next valid minute. Explicit
email opt-out, missing verified recipient, muted lists, stale schedules and
unavailable push delivery cancel the affected intent. The push transport is not
implemented or presented as an available subscription.

The worker claims one reminder at a time, with a 60-second ownership lease.
Transport operations time out after at most 20 seconds. Failures retry with
bounded exponential backoff and a maximum of five claims, including crashes.
It checks shutdown between jobs. The server owns and awaits the returned worker
handle during graceful shutdown.

Daily and weekly digests are durable jobs with a unique `(user, kind, period)`.
They are created only for the current local delivery day after the selected clock
time. Missed days are not replayed as a backlog. Contents are recomputed at send
time from currently authorized, nonmuted tasks. Emails list at most 100 plaintext
titles and include the total scheduled open count; weekly digests also count leaf
completion transitions for that week. Daily summaries include the previous year
and tomorrow; weekly summaries use a bounded overdue window plus the next week.
Old periods, revoked opt-in and changed delivery weekdays are cancelled.

Each email uses the durable job ID as its stable `Message-ID`. SMTP cannot
promise exactly-once delivery: a network failure after server acceptance, or a
crash before SQLite acknowledgement, may cause a duplicate. `Message-ID` helps
receivers identify retries, but ordinary SMTP does not guarantee deduplication.
The database never marks a delivery successful before SMTP accepts it.

## Local capture check

Run `uv run --no-project --with aiosmtpd python scripts/rebuild-smtp-check.py`.
It starts an isolated local SMTP capture server and runs the ignored Rust worker
integration test. It verifies the envelope recipient, stable message ID format,
UTF-8 plaintext body and absence of replay after acknowledgement. Plain SMTP is
allowed only with `TODOER_SMTP_INSECURE_LOCAL=1` and a loopback host. The explicit
`TODOER_SMTP_TEST_RECIPIENT` adapter additionally requires an `.invalid` recipient;
it cannot redirect a production SMTP delivery.

Reminder intent identity is `(task, recipient, channel, resolved instant)`. Editing
priority/planned date or re-saving unchanged reminders reuses that delivery intent
and its push device ledger. A delivered instant is not sent again on such edits;
a different reminder instant creates a new intent. Exhausted attempts stay dead
until an explicit operational retry policy is used. In-flight SMTP acceptance
still has the documented crash/commit ambiguity; stable Message-ID is assistance,
not an exactly-once guarantee.
