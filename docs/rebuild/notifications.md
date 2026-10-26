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
