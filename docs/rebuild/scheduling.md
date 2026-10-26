# Scheduling domain

`Store::set_schedule(actor, task_id, expected_task_revision, Schedule)` writes
metadata, bumps the common task revision, appends history and replaces reminder
intents in one transaction. Planning is a date without a time or zone. Due values
are either a date, or an explicit local datetime and IANA timezone. Priorities are
1–3. Reminders require an explicit timed value and email or push channel. User
input in a DST gap or overlap is rejected, so it cannot silently shift the time.
Dates use `YYYY-MM-DD`, local datetimes `YYYY-MM-DDTHH:MM:SS`.

## Recurrence

`start_series` supports unfinished root tasks without children. It binds the first
occurrence to the existing task. `Daily { interval_days }`, `Weekdays { days }`
(Monday=1), and `CompletionRelative { days }` retain the chosen local wall time.
Completion relative means calendar days in the selected timezone, not elapsed
24-hour periods. End dates are inclusive. Counts include emitted occurrences,
including explicit skips; calendar dates missed while a task remains outstanding
do not consume the count. Rules are bounded to a 100-year next-date search.

There is at most one outstanding occurrence per series, enforced by SQLite.
Completing or skipping it creates one successor. Calendar catchup selects the
latest eligible past date, otherwise the next future date. It does not create a
backlog. Generated recurrence dates skip nonexistent DST times; overlaps select
the earlier instant. The local time and timezone remain stored explicitly.

Pause prevents successors; completing the outstanding task while paused records
its completion. Resume emits one successor if none is outstanding. Stop is final
and leaves the outstanding task as an ordinary task to finish. History remains
attached to stable series IDs and ordinal occurrence IDs. Reopening a historical occurrence records its status as reopened while preserving
the successor. Completing it again never creates another successor, and does not
replay reminders unless its schedule is explicitly edited.
Adding children after a series starts requires stopping the series before it can
advance. Recurrence currently clones the task title, description and scheduling
metadata, not descendants. The next task retains reminder offsets relative to
the previous due time.

Core completion transitions must call `after_task_update` inside their existing
transaction, including cascades. Duplicate hook calls cannot advance a completed
occurrence twice. Direct SQL completion outside this hook is unsupported.

## Reminder outbox

Reminder intent uniqueness is `(task, schedule revision, channel, instant)`.
Schedule changes cancel pending and leased intents. Claims use an immediate
transaction, unique lease token, owner and a 60-second lease. A crashed worker's
lease can be reclaimed. Acknowledgements and retries require the current token
and unexpired lease. Attempts are counted at claim, including crashes, and stop
after five attempts; retry delays increase up to one hour.

Workers must call `reminder_eligible` immediately before transport delivery. This
checks the current schedule revision, incomplete/untrashed task, active list and
recipient's current access. Expired or stale intents are never newly claimed.
This module stores channel and recipient identity, not delivery destinations or
provider secrets. No email or push transport is implemented here. External sends
cannot be made exactly once by SQLite alone; transports need an idempotency key
(the reminder job ID), and delivery may repeat if the process crashes after the
provider accepts a message but before local acknowledgement.
