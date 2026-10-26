# Weekly review

`GET /review` is a focused review of completed work, current plans, overdue work
and upcoming commitments. `GET /api/v1/review` returns the same structured data.
Both require authentication and return `Cache-Control: no-store, private`.
The page uses a same-origin stylesheet compatible with the strict CSP, an
accessible SVG chart plus a daily-count table, and responsive dark/light styles.
There is no report builder, score, streak, prediction or fabricated metric.

Query parameters:

- `from=YYYY-MM-DD` and `to=YYYY-MM-DD`, inclusive, at most 366 calendar days.
- Optional `list=ID` or `workspace=ID`, never both.
- Defaults: Monday through Sunday in the account's configured IANA timezone.

Timestamped history has a range index; leaf counting uses the recorded snapshot,
not a scan of current descendants.

The domain method is `Store::weekly_review(actor, ReviewRange, now)`. Timezone
interpretation reads only the operational timezone internally. List- and
workspace-scoped credentials can review their current authorized resources;
they do not gain access to the notification-preferences document. Revoked or
expired credentials fail authorization. Explicit inaccessible resource filters
fail before counts or task snippets are returned.

## Counting rules

**Completed work** counts distinct task IDs with a recorded unfinished-to-complete
`update` transition and an operation-time `countable_leaf` snapshot. Containers
are excluded even when their descendants are completed together. Completing,
reopening and completing the same leaf again inside one selected range counts
once, assigned to its first qualifying completion day in that range. Daily bars
sum to the range total. The same task can count again in a different, nonoverlapping
range. Each recurring occurrence has new task IDs, so actually completed leaf
work in separate occurrences counts separately. Skips and compensating undo
events are excluded. Reopening does not erase previously recorded work.

History is authorized against **both** its original list and the task's current
list before grouping. Selected list/workspace filters must allow both. Revoking
access or moving a task outside a credential's scope can remove older work from
its results. Adding children later does not rewrite the leaf classification of
an earlier completion. History lacking timestamps, before images or leaf
snapshots is excluded; older imported work is therefore not claimed complete.

**Planned / completed** compares currently scheduled leaf tasks whose current
planned date falls in the selected range against those tasks' qualifying recorded
completions in that range. This is not a reconstruction of what was planned at
the start of an earlier week, nor a count of tasks still marked complete today.
Historical schedule changes were not fully snapshotted. Trashed tasks, archived
lists and current containers are excluded from current planned work.

**Overdue now** counts current open, live leaf tasks in active lists with a passed
due date/time. A date-only deadline becomes overdue after that local calendar
day ends; it is never shifted through UTC. Timed deadlines use their actual IANA
zoned instant. Planned dates alone are not deadlines. This number is live and
independent of the selected historical range.

**Upcoming commitments** contains current open leaf tasks with a planned date or
unexpired due date/time from today through six days ahead, inclusive. A task
appearing in both categories is listed once under its earliest eligible date.
The response includes the total and at most 20 tasks, ordered by date then ID.
Containers, trash, archived lists and inaccessible tasks are excluded consistently.

Local calendar boundaries resolve midnight DST gaps to the first real instant
and overlaps to the earlier instant. They do not assume that every day is 24
hours. A rare skipped calendar day can have no elapsed time and no completions.

## Actual overdue observations

`Store::record_stats_observations(now)` records the first actual observation for
each readable list, local calendar date and timezone. The standalone worker runs
independently of SMTP, initially and then about once per minute. Each call checks
a fair batch of at most 50 accounts; larger installations may take additional
ticks to establish a first observation. Its shutdown polling is bounded to one
second between ticks. Newly readable lists can be observed later in the same day.

SQLite stores observation coverage/timestamps and overdue task IDs, not task
titles, messages, credentials or only an irreversible aggregate count. A read
reauthorizes both each original observation list and each task's current list.
An account's trend deduplicates tasks observed in two lists on the same day after
a move. Old counts can shrink when access changes; they do not disclose newly
inaccessible work. Observation records survive process restarts.

A real observed zero is shown as zero. Days without observations are absent,
never backfilled with today's state or invented zeros. An observation remains
unchanged when tasks are completed later that day. Trend points aggregate each
list's first capture that day; first/last observation timestamps are retained.
The page shows the latest timestamp in UTC and coverage as observed/currently
readable lists. Partial coverage is marked when some current lists lack a capture.
It is not an exact midnight historical backlog or a claim of complete historical
measurement.

Changing timezone starts a separate observation series. Old local dates are not
reinterpreted in the new timezone; the review displays observations recorded in
the current timezone only. The initial version cannot reconstruct overdue work
before recording began. Database backups include the observation tables; private
attachment bytes remain part of the separate object-storage backup policy.

The older `/api/v1/statistics` compatibility response remains unchanged until its
callers migrate. It counts repeat transitions under its original documented
semantics; `/api/v1/review` provides the distinct-work definitions above.
