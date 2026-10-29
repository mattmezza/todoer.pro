//! Date-only planning, zoned due times, checklist recurrence and durable intents.
//! User-entered ambiguous/nonexistent local times are rejected. Generated dates
//! skip DST gaps and use the earlier instant for overlaps; wall time is retained.
use crate::core::{Error, Store, Task, access_rank, event, raw_task, task};
use chrono::{Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Moment {
    Date { date: String },
    Timed { local: String, timezone: String },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Email,
    Push,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Reminder {
    pub channel: Channel,
    pub at: Moment,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Schedule {
    pub planned: Option<Moment>,
    pub due: Option<Moment>,
    pub priority: Option<u8>,
    #[serde(default)]
    pub reminders: Vec<Reminder>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleState {
    pub task_revision: i64,
    pub schedule_revision: i64,
    pub schedule: Schedule,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Rule {
    Daily { interval_days: u32 },
    Weekdays { days: Vec<u32> },
    CompletionRelative { days: u32 },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurrenceSpec {
    pub rule: Rule,
    pub start_local: String,
    pub timezone: String,
    pub end_date: Option<String>,
    pub count: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SeriesState {
    Active,
    Paused,
    Stopped,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Series {
    pub id: String,
    pub spec: RecurrenceSpec,
    pub state: SeriesState,
    pub revision: i64,
    pub emitted: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Occurrence {
    pub ordinal: i64,
    pub task_id: String,
    pub due_local: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderJob {
    pub id: String,
    pub task_id: String,
    pub schedule_revision: i64,
    pub recipient: String,
    pub channel: Channel,
    pub at: i64,
    pub attempts: i64,
    pub lease_owner: String,
    pub lease_token: String,
    pub lease_until: i64,
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn decode<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, Error> {
    serde_json::from_str(s).map_err(|_| invalid("invalid stored scheduling data"))
}
fn encode<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).expect("serialize scheduling data")
}
fn date(value: &str) -> Result<NaiveDate, Error> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid("date must be YYYY-MM-DD"))
}
fn local(value: &str) -> Result<NaiveDateTime, Error> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .map_err(|_| invalid("local time must be YYYY-MM-DDTHH:MM:SS"))
}
fn zone(value: &str) -> Result<Tz, Error> {
    value.parse().map_err(|_| invalid("unknown IANA timezone"))
}
fn stamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}
fn timed(moment: &Moment) -> Result<i64, Error> {
    match moment {
        Moment::Date { .. } => Err(invalid(
            "reminders require an explicit local time and timezone",
        )),
        Moment::Timed {
            local: value,
            timezone,
        } => match zone(timezone)?.from_local_datetime(&local(value)?) {
            LocalResult::Single(value) => Ok(value.timestamp()),
            _ => Err(invalid(
                "local time is nonexistent or ambiguous; choose an unambiguous time",
            )),
        },
    }
}
fn resolved(moment: &Moment) -> Result<i64, Error> {
    match moment {
        Moment::Timed {
            local: value,
            timezone,
        } => generated_instant(zone(timezone)?, local(value)?)
            .ok_or_else(|| invalid("nonexistent reminder time")),
        _ => Err(invalid("reminder requires timed moment")),
    }
}
fn validate(schedule: &Schedule) -> Result<(), Error> {
    if let Some(planned) = &schedule.planned {
        match planned {
            Moment::Date { date: value } => {
                date(value)?;
            }
            _ => return Err(invalid("planned dates must be date-only")),
        }
    }
    if let Some(due) = &schedule.due {
        match due {
            Moment::Date { date: value } => {
                date(value)?;
            }
            _ => {
                timed(due)?;
            }
        }
    }
    if schedule.priority.is_some_and(|p| !(0..=3).contains(&p)) {
        return Err(invalid("priority must be 0, 1, 2 or 3"));
    }
    if schedule.reminders.len() > 10 {
        return Err(invalid("at most ten reminder intents are supported"));
    }
    for reminder in &schedule.reminders {
        timed(&reminder.at)?;
    }
    Ok(())
}
fn recipient(c: &Connection, actor: &str) -> Result<String, Error> {
    Ok(crate::core::actor_identity(c, actor)?.0)
}

fn state(c: &Connection, t: &Task) -> Result<ScheduleState, Error> {
    let row: Option<(i64, String)> = c
        .query_row(
            "SELECT revision,document FROM task_schedule WHERE task_id=?1",
            [&t.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (revision, schedule) = if let Some((rev, data)) = row {
        (rev, decode(&data)?)
    } else {
        (0, Schedule::default())
    };
    Ok(ScheduleState {
        task_revision: t.revision,
        schedule_revision: revision,
        schedule,
    })
}
fn cancel(c: &Connection, id: &str) -> Result<(), Error> {
    c.execute("UPDATE reminder_outbox SET status='cancelled',lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE task_id=?1 AND status IN ('pending','leased')",[id])?;
    Ok(())
}
fn write_schedule(
    c: &Connection,
    actor: &str,
    t: &Task,
    schedule: &Schedule,
) -> Result<ScheduleState, Error> {
    let revision = state(c, t)?.schedule_revision + 1;
    cancel(c, &t.id)?;
    c.execute("INSERT INTO task_schedule(task_id,revision,document) VALUES(?1,?2,?3) ON CONFLICT(task_id) DO UPDATE SET revision=excluded.revision,document=excluded.document",params![t.id,revision,encode(schedule)])?;
    if !t.complete {
        let user = recipient(c, actor)?;
        for reminder in &schedule.reminders {
            let at = resolved(&reminder.at)?;
            let channel = match reminder.channel {
                Channel::Email => "email",
                Channel::Push => "push",
            };
            // Logical delivery identity survives unrelated schedule edits. Reuse
            // the intent and its device delivery ledger; never replay a delivered
            // timestamp merely because priority/planned date changed.
            let existing:Option<(String,String)>=c.query_row("SELECT id,status FROM reminder_outbox WHERE task_id=?1 AND recipient=?2 AND channel=?3 AND at=?4 ORDER BY (status='delivered') DESC,rowid LIMIT 1",params![t.id,user,channel,at],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((id, status)) = existing {
                if status == "delivered" {
                    c.execute(
                        "UPDATE reminder_outbox SET schedule_revision=?1 WHERE id=?2",
                        params![revision, id],
                    )?;
                } else {
                    c.execute("UPDATE reminder_outbox SET schedule_revision=?1,status=CASE WHEN attempts>=5 THEN 'dead' ELSE 'pending' END,available_at=?2,lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE id=?3",params![revision,at,id])?;
                }
            } else {
                c.execute("INSERT INTO reminder_outbox(id,task_id,schedule_revision,recipient,channel,at,available_at) VALUES(?1,?2,?3,?4,?5,?6,?6)",params![Uuid::new_v4().to_string(),t.id,revision,user,channel,at])?;
            }
        }
    }
    Ok(ScheduleState {
        task_revision: t.revision,
        schedule_revision: revision,
        schedule: schedule.clone(),
    })
}
fn checked(
    c: &Connection,
    actor: &str,
    id: &str,
    rank: i64,
    revision: Option<i64>,
) -> Result<Task, Error> {
    let t = task(c, id)?;
    access_rank(c, actor, &t.list_id, rank)?;
    if revision.is_some_and(|r| r != t.revision) {
        return Err(Error::Conflict("stale task revision".into()));
    }
    Ok(t)
}
fn series_row(c: &Connection, id: &str) -> Result<Series, Error> {
    let (spec, state, revision, emitted): (String, String, i64, i64) = c
        .query_row(
            "SELECT spec,state,revision,emitted FROM recurrence_series WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?
        .ok_or(Error::NotFound)?;
    Ok(Series {
        id: id.into(),
        spec: decode(&spec)?,
        state: decode(&format!("\"{state}\""))?,
        revision,
        emitted,
    })
}
fn series_task(c: &Connection, id: &str) -> Result<Task, Error> {
    let task_id:String=c.query_row("SELECT task_id FROM recurrence_occurrences WHERE series_id=?1 ORDER BY ordinal DESC LIMIT 1",[id],|r|r.get(0)).optional()?.ok_or(Error::NotFound)?;
    raw_task(c, &task_id)
}
fn recurrence_valid(spec: &RecurrenceSpec) -> Result<(), Error> {
    let start = local(&spec.start_local)?;
    timed(&Moment::Timed {
        local: spec.start_local.clone(),
        timezone: spec.timezone.clone(),
    })?;
    match &spec.rule {
        Rule::Daily { interval_days }
        | Rule::CompletionRelative {
            days: interval_days,
        } if !(1..=3650).contains(interval_days) => {
            return Err(invalid("recurrence interval must be 1–3650 days"));
        }
        Rule::Weekdays { days }
            if days.is_empty() || days.len() > 7 || days.iter().any(|d| !(1..=7).contains(d)) =>
        {
            return Err(invalid("weekdays must contain Monday=1 through Sunday=7"));
        }
        _ => {}
    }
    if spec.count.is_some_and(|n| n == 0 || n > 100_000) {
        return Err(invalid("count must be 1–100000 emitted occurrences"));
    }
    if let Some(end) = &spec.end_date
        && date(end)? < start.date()
    {
        return Err(invalid("end date precedes recurrence start"));
    }
    Ok(())
}
fn generated_instant(tz: Tz, value: NaiveDateTime) -> Option<i64> {
    match tz.from_local_datetime(&value) {
        LocalResult::Single(v) => Some(v.timestamp()),
        LocalResult::Ambiguous(a, b) => Some(a.timestamp().min(b.timestamp())),
        LocalResult::None => None,
    }
}
/// Latest missed calendar date, otherwise the next future one; no backlog explosion.
fn next_due(
    spec: &RecurrenceSpec,
    previous: &str,
    now: i64,
) -> Result<Option<NaiveDateTime>, Error> {
    let previous = local(previous)?;
    let anchor = local(&spec.start_local)?;
    let tz = zone(&spec.timezone)?;
    let end = spec.end_date.as_deref().map(date).transpose()?;
    let start = match spec.rule {
        Rule::CompletionRelative { days } => Utc
            .timestamp_opt(now, 0)
            .single()
            .ok_or_else(|| invalid("invalid worker time"))?
            .with_timezone(&tz)
            .date_naive()
            .checked_add_signed(Duration::days(days.into())),
        _ => previous.date().succ_opt(),
    }
    .ok_or_else(|| invalid("recurrence date overflow"))?;
    let mut latest = None;
    for offset in 0..36600 {
        let day = start
            .checked_add_signed(Duration::days(offset))
            .ok_or_else(|| invalid("recurrence date overflow"))?;
        if end.is_some_and(|end| day > end) {
            return Ok(latest);
        }
        let matches = match &spec.rule {
            Rule::Daily { interval_days } => {
                (day - anchor.date())
                    .num_days()
                    .rem_euclid(i64::from(*interval_days))
                    == 0
            }
            Rule::Weekdays { days } => days.contains(&day.weekday().number_from_monday()),
            Rule::CompletionRelative { .. } => true,
        };
        if !matches {
            continue;
        }
        let candidate = day.and_time(anchor.time());
        let Some(at) = generated_instant(tz, candidate) else {
            continue;
        };
        if matches!(spec.rule, Rule::CompletionRelative { .. }) {
            return Ok(Some(candidate));
        }
        if at <= now {
            latest = Some(candidate)
        } else {
            return Ok(latest.or(Some(candidate)));
        }
    }
    Err(invalid(
        "next recurrence lies beyond bounded 100-year horizon",
    ))
}
const MAX_RECURRENCE_TASKS: usize = 500;
/// Parent-first, live-only checklist snapshot, bounded before any generation.
fn checklist(c: &Connection, root: &Task) -> Result<Vec<Task>, Error> {
    let ids = {
        let mut q = c.prepare("WITH RECURSIVE tree(id,depth) AS (SELECT id,0 FROM tasks WHERE id=?1 AND deleted_at IS NULL UNION ALL SELECT child.id,tree.depth+1 FROM tasks child INDEXED BY tasks_parent JOIN tree ON child.parent_id=tree.id WHERE child.deleted_at IS NULL AND tree.depth<500) SELECT tree.id FROM tree JOIN tasks t ON t.id=tree.id ORDER BY depth,t.position,tree.id LIMIT 501")?;
        q.query_map([&root.id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    if ids.len() > MAX_RECURRENCE_TASKS {
        return Err(invalid(
            "recurring checklist exceeds 500 live tasks; split or stop the series",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut tasks = Vec::with_capacity(ids.len());
    for id in ids {
        if !seen.insert(id.clone()) {
            return Err(invalid("recurring checklist contains a cycle"));
        }
        let t = task(c, &id)?;
        if t.list_id != root.list_id {
            return Err(invalid("recurring checklist crosses list boundaries"));
        }
        if id != root.id
            && c.query_row(
                "SELECT EXISTS(SELECT 1 FROM recurrence_occurrences o JOIN recurrence_series s ON s.id=o.series_id WHERE o.task_id=?1 AND s.state!='stopped')",
                [&id],
                |r| r.get::<_, bool>(0),
            )?
        {
            return Err(Error::Conflict(
                "nested recurrence series cannot belong to one checklist occurrence".into(),
            ));
        }
        tasks.push(t);
    }
    Ok(tasks)
}
/// Core invokes this before cascading completion; the explicit flag belongs to
/// the user's command, so it cannot be inferred by the completion event hook.
pub(crate) fn validate_recurrence_completion(
    c: &Connection,
    id: &str,
    complete_subtree: bool,
) -> Result<(), Error> {
    let recurring:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM recurrence_occurrences o JOIN recurrence_series s ON s.id=o.series_id WHERE o.task_id=?1 AND s.state!='stopped')",[id],|r|r.get(0))?;
    if recurring {
        let tree = checklist(c, &task(c, id)?)?;
        if tree.len() > 1 && !complete_subtree {
            return Err(Error::Conflict(
                "completing a recurring checklist requires complete_subtree=true".into(),
            ));
        }
    }
    Ok(())
}
fn fresh_task_identity(c: &Connection) -> Result<(String, String), Error> {
    loop {
        let id = Uuid::new_v4().to_string();
        let reference = format!("T-{}", &id[..8]);
        if !c.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1 OR reference=?2)",
            params![id, reference],
            |r| r.get::<_, bool>(0),
        )? {
            return Ok((id, reference));
        }
    }
}
fn shifted(moment: &Moment, days: i64, seconds: i64) -> Result<Moment, Error> {
    Ok(match moment {
        Moment::Date { date: value } => Moment::Date {
            date: date(value)?
                .checked_add_signed(Duration::days(days))
                .ok_or_else(|| invalid("recurring date overflow"))?
                .format("%Y-%m-%d")
                .to_string(),
        },
        Moment::Timed { timezone, .. } => {
            let at = resolved(moment)?
                .checked_add(seconds)
                .ok_or_else(|| invalid("recurring timestamp overflow"))?;
            let local = Utc
                .timestamp_opt(at, 0)
                .single()
                .ok_or_else(|| invalid("recurring timestamp overflow"))?
                .with_timezone(&zone(timezone)?);
            Moment::Timed {
                local: stamp(local.naive_local()),
                timezone: timezone.clone(),
            }
        }
    })
}
fn clone_checklist(
    c: &Connection,
    actor: &str,
    series: &Series,
    previous: &Task,
    next: NaiveDateTime,
    last_due: &str,
) -> Result<Task, Error> {
    let originals = checklist(c, previous)?;
    let old_anchor = state(c, previous)?
        .schedule
        .due
        .as_ref()
        .and_then(|moment| resolved(moment).ok())
        .unwrap_or(
            generated_instant(zone(&series.spec.timezone)?, local(last_due)?)
                .ok_or_else(|| invalid("invalid old recurrence anchor"))?,
        );
    let new_anchor = generated_instant(zone(&series.spec.timezone)?, next)
        .ok_or_else(|| invalid("invalid recurrence anchor"))?;
    let seconds = new_anchor
        .checked_sub(old_anchor)
        .ok_or_else(|| invalid("recurring offset overflow"))?;
    let days = (next.date() - local(last_due)?.date()).num_days();
    let recipient: String = c.query_row(
        "SELECT actor FROM recurrence_series WHERE id=?1",
        [&series.id],
        |r| r.get(0),
    )?;
    let mut mapped = std::collections::BTreeMap::<String, String>::new();
    let mut generated = Vec::new();
    let mut position: i64 = c.query_row(
        "SELECT COALESCE(MAX(position),0) FROM tasks WHERE list_id=?1",
        [&previous.list_id],
        |r| r.get(0),
    )?;
    for original in &originals {
        let (id, reference) = fresh_task_identity(c)?;
        let parent = if original.id == previous.id {
            None
        } else {
            Some(
                mapped
                    .get(
                        original
                            .parent_id
                            .as_deref()
                            .ok_or_else(|| invalid("checklist child has no parent"))?,
                    )
                    .ok_or_else(|| invalid("checklist parent missing"))?
                    .clone(),
            )
        };
        position = position
            .checked_add(1)
            .ok_or_else(|| invalid("task position overflow"))?;
        c.execute("INSERT INTO tasks(id,reference,list_id,parent_id,title,description,position) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,reference,previous.list_id,parent,original.title,original.description,position])?;
        c.execute("INSERT INTO task_tags(task_id,tag_id) SELECT ?1,tag_id FROM task_tags WHERE task_id=?2",params![id,original.id])?;
        c.execute("INSERT INTO task_locations(task_id,section_id) SELECT ?1,l.section_id FROM task_locations l JOIN sections s ON s.id=l.section_id WHERE l.task_id=?2 AND s.list_id=?3",params![id,original.id,previous.list_id])?;
        let new = task(c, &id)?;
        let schedule = state(c, original)?;
        if original.id == previous.id || schedule.schedule_revision > 0 {
            let mut schedule = schedule.schedule;
            schedule.planned = schedule
                .planned
                .as_ref()
                .map(|m| shifted(m, days, seconds))
                .transpose()?;
            schedule.due = if original.id == previous.id {
                Some(Moment::Timed {
                    local: stamp(next),
                    timezone: series.spec.timezone.clone(),
                })
            } else {
                schedule
                    .due
                    .as_ref()
                    .map(|m| shifted(m, days, seconds))
                    .transpose()?
            };
            for reminder in &mut schedule.reminders {
                reminder.at = shifted(&reminder.at, days, seconds)?;
            }
            write_schedule(c, &recipient, &new, &schedule)?;
        }
        mapped.insert(original.id.clone(), id);
        generated.push(new);
    }
    // All nodes exist before reference indexing. History is new creation history;
    // original comments, attachments and prior edits stay with the old occurrence.
    for new in &generated {
        event(c, new, "recurrence-create", actor, None)?;
    }
    generated
        .into_iter()
        .next()
        .ok_or_else(|| invalid("empty recurring checklist"))
}
fn advance(c: &Connection, actor: &str, id: &str, previous: &Task, now: i64) -> Result<(), Error> {
    let series = series_row(c, id)?;
    if series.state != SeriesState::Active {
        return Ok(());
    }
    let outstanding:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM recurrence_occurrences WHERE series_id=?1 AND status='pending')",[id],|r|r.get(0))?;
    if outstanding {
        return Ok(());
    }
    let last_due:String=c.query_row("SELECT due_local FROM recurrence_occurrences WHERE series_id=?1 ORDER BY ordinal DESC LIMIT 1",[id],|r|r.get(0))?;
    let next = if series
        .spec
        .count
        .is_some_and(|count| series.emitted >= i64::from(count))
    {
        None
    } else {
        next_due(&series.spec, &last_due, now)?
    };
    let Some(next) = next else {
        c.execute(
            "UPDATE recurrence_series SET state='stopped',revision=revision+1 WHERE id=?1",
            [id],
        )?;
        return Ok(());
    };
    access_rank(c, actor, &previous.list_id, 2)?;
    task(c, &previous.id)?;
    let new = clone_checklist(c, actor, &series, previous, next, &last_due)?;
    c.execute("INSERT INTO recurrence_occurrences(series_id,ordinal,task_id,due_local) VALUES(?1,?2,?3,?4)",params![id,series.emitted+1,new.id,stamp(next)])?;
    c.execute(
        "UPDATE recurrence_series SET emitted=emitted+1,revision=revision+1 WHERE id=?1",
        [id],
    )?;
    Ok(())
}
impl Store {
    pub fn get_schedule(&self, actor: &str, id: &str) -> Result<ScheduleState, Error> {
        let t = checked(&self.conn, actor, id, 1, None)?;
        state(&self.conn, &t)
    }
    pub fn set_schedule(
        &mut self,
        actor: &str,
        id: &str,
        revision: i64,
        schedule: Schedule,
    ) -> Result<ScheduleState, Error> {
        validate(&schedule)?;
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let before = checked(&tx, actor, id, 2, Some(revision))?;
        let previous_schedule = state(&tx, &before)?.schedule;
        tx.execute(
            "UPDATE tasks SET revision=revision+1 WHERE id=?1",
            [&before.id],
        )?;
        let after = task(&tx, &before.id)?;
        let state = write_schedule(&tx, actor, &after, &schedule)?;
        event(&tx, &after, "schedule", actor, Some(&before))?;
        let previous = serde_json::to_value(previous_schedule).unwrap();
        let current = serde_json::to_value(&schedule).unwrap();
        let changes: Vec<&str> = [
            ("planned", "planned_changed"),
            ("due", "due_changed"),
            ("priority", "priority_changed"),
            ("reminders", "reminders_changed"),
        ]
        .into_iter()
        .filter_map(|(field, action)| (previous[field] != current[field]).then_some(action))
        .collect();
        tx.execute("UPDATE history SET event=json_set(event,'$.changes',json(?1)) WHERE seq=last_insert_rowid()", [serde_json::to_string(&changes).unwrap()])?;
        tx.commit()?;
        Ok(state)
    }
    pub fn start_series(
        &mut self,
        actor: &str,
        id: &str,
        revision: i64,
        spec: RecurrenceSpec,
    ) -> Result<Series, Error> {
        recurrence_valid(&spec)?;
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let before = checked(&tx, actor, id, 2, Some(revision))?;
        if before.complete || before.parent_id.is_some() {
            return Err(invalid("recurrence requires an unfinished root task"));
        }
        checklist(&tx, &before)?;
        let existing: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM recurrence_occurrences WHERE task_id=?1)",
            [&before.id],
            |r| r.get(0),
        )?;
        if existing {
            return Err(Error::Conflict("task already belongs to a series".into()));
        }
        let series_id = Uuid::new_v4().to_string();
        let user = recipient(&tx, actor)?;
        tx.execute(
            "INSERT INTO recurrence_series(id,actor,spec,state) VALUES(?1,?2,?3,'active')",
            params![series_id, user, encode(&spec)],
        )?;
        tx.execute("INSERT INTO recurrence_occurrences(series_id,ordinal,task_id,due_local) VALUES(?1,1,?2,?3)",params![series_id,before.id,spec.start_local])?;
        let mut schedule = state(&tx, &before)?.schedule;
        schedule.due = Some(Moment::Timed {
            local: spec.start_local.clone(),
            timezone: spec.timezone.clone(),
        });
        tx.execute(
            "UPDATE tasks SET revision=revision+1 WHERE id=?1",
            [&before.id],
        )?;
        let after = task(&tx, &before.id)?;
        write_schedule(&tx, actor, &after, &schedule)?;
        event(&tx, &after, "recurrence-start", actor, Some(&before))?;
        let result = series_row(&tx, &series_id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn series(&self, actor: &str, id: &str) -> Result<Series, Error> {
        let t = series_task(&self.conn, id)?;
        access_rank(&self.conn, actor, &t.list_id, 1)?;
        series_row(&self.conn, id)
    }
    pub fn occurrences(&self, actor: &str, id: &str) -> Result<Vec<Occurrence>, Error> {
        self.series(actor, id)?;
        let mut q=self.conn.prepare("SELECT o.ordinal,o.task_id,o.due_local,o.status FROM recurrence_occurrences o JOIN tasks t ON t.id=o.task_id JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?2 WHERE o.series_id=?1 AND a.rank>=1 ORDER BY o.ordinal LIMIT 1000")?;
        Ok(q.query_map(params![id, actor], |r| {
            Ok(Occurrence {
                ordinal: r.get(0)?,
                task_id: r.get(1)?,
                due_local: r.get(2)?,
                status: r.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?)
    }
    pub fn set_series_state(
        &mut self,
        actor: &str,
        id: &str,
        revision: i64,
        new_state: SeriesState,
        now: i64,
    ) -> Result<Series, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let t = series_task(&tx, id)?;
        access_rank(&tx, actor, &t.list_id, 2)?;
        let series = series_row(&tx, id)?;
        if series.revision != revision {
            return Err(Error::Conflict("stale series revision".into()));
        }
        if series.state == SeriesState::Stopped {
            return Err(Error::Conflict("stopped series cannot resume".into()));
        }
        let value = match new_state {
            SeriesState::Active => "active",
            SeriesState::Paused => "paused",
            SeriesState::Stopped => "stopped",
        };
        tx.execute(
            "UPDATE recurrence_series SET state=?1,revision=revision+1 WHERE id=?2",
            params![value, id],
        )?;
        if new_state == SeriesState::Active {
            advance(&tx, actor, id, &t, now)?
        }
        let result = series_row(&tx, id)?;
        event(&tx, &t, "recurrence-state", actor, None)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn skip_occurrence(
        &mut self,
        actor: &str,
        id: &str,
        revision: i64,
        now: i64,
    ) -> Result<Series, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let before = checked(&tx, actor, id, 2, Some(revision))?;
        let series_id:String=tx.query_row("SELECT series_id FROM recurrence_occurrences WHERE task_id=?1 AND status='pending'",[&before.id],|r|r.get(0)).optional()?.ok_or(Error::NotFound)?;
        tx.execute(
            "UPDATE recurrence_occurrences SET status='skipped' WHERE task_id=?1",
            [&before.id],
        )?;
        for child in checklist(&tx, &before)?
            .into_iter()
            .filter(|t| t.id != before.id && !t.complete)
        {
            tx.execute(
                "UPDATE tasks SET complete=1,revision=revision+1 WHERE id=?1",
                [&child.id],
            )?;
            let after = task(&tx, &child.id)?;
            event(&tx, &after, "recurrence-skip", actor, Some(&child))?;
        }
        tx.execute(
            "UPDATE tasks SET complete=1,revision=revision+1 WHERE id=?1",
            [&before.id],
        )?;
        cancel(&tx, &before.id)?;
        let after = task(&tx, &before.id)?;
        event(&tx, &after, "recurrence-skip", actor, Some(&before))?;
        advance(&tx, actor, &series_id, &after, now)?;
        let series = series_row(&tx, &series_id)?;
        tx.commit()?;
        Ok(series)
    }
}
/// Invoke inside the same transaction for each actual completion state transition.
pub(crate) fn after_task_update(
    c: &Connection,
    actor: &str,
    before: &Task,
    after: &Task,
    now: i64,
) -> Result<(), Error> {
    if before.parent_id != after.parent_id && after.parent_id.is_some() {
        let active:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM recurrence_occurrences o JOIN recurrence_series s ON s.id=o.series_id WHERE o.task_id=?1 AND s.state!='stopped')",[&after.id],|r|r.get(0))?;
        if active {
            return Err(Error::Conflict(
                "recurring occurrences must stay subtree roots; stop the series before reparenting"
                    .into(),
            ));
        }
    }
    if before.complete == after.complete {
        return Ok(());
    }
    if after.complete {
        cancel(c, &after.id)?;
        let series_id:Option<String>=c.query_row("SELECT series_id FROM recurrence_occurrences WHERE task_id=?1 AND status='pending'",[&after.id],|r|r.get(0)).optional()?;
        if let Some(id) = series_id {
            c.execute(
                "UPDATE recurrence_occurrences SET status='completed' WHERE task_id=?1",
                [&after.id],
            )?;
            advance(c, actor, &id, after, now)?;
        } else {
            c.execute("UPDATE recurrence_occurrences SET status='completed' WHERE task_id=?1 AND status='reopened'",[&after.id])?;
        }
    } else {
        let existing: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM recurrence_occurrences WHERE task_id=?1)",
            [&after.id],
            |r| r.get(0),
        )?;
        if existing {
            c.execute(
                "UPDATE recurrence_occurrences SET status='reopened' WHERE task_id=?1",
                [&after.id],
            )?;
            cancel(c, &after.id)?;
            return Ok(());
        }
        let historical_child:bool=c.query_row("WITH RECURSIVE ancestors(id,depth) AS (SELECT parent_id,1 FROM tasks WHERE id=?1 AND parent_id IS NOT NULL UNION ALL SELECT t.parent_id,a.depth+1 FROM tasks t JOIN ancestors a ON t.id=a.id WHERE t.parent_id IS NOT NULL AND a.depth<500) SELECT EXISTS(SELECT 1 FROM ancestors a JOIN recurrence_occurrences o ON o.task_id=a.id WHERE o.status!='pending')",[&after.id],|r|r.get(0))?;
        if historical_child {
            cancel(c, &after.id)?;
        } else {
            let schedule = state(c, after)?.schedule;
            write_schedule(c, actor, after, &schedule)?;
        }
    }
    Ok(())
}
fn job_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ReminderJob> {
    let channel: String = r.get(4)?;
    Ok(ReminderJob {
        id: r.get(0)?,
        task_id: r.get(1)?,
        schedule_revision: r.get(2)?,
        recipient: r.get(3)?,
        channel: if channel == "email" {
            Channel::Email
        } else {
            Channel::Push
        },
        at: r.get(5)?,
        attempts: r.get(6)?,
        lease_owner: r.get(7)?,
        lease_token: r.get(8)?,
        lease_until: r.get(9)?,
    })
}
fn eligible(c: &Connection, id: &str) -> Result<bool, Error> {
    Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM reminder_outbox j JOIN tasks t ON t.id=j.task_id JOIN task_schedule s ON s.task_id=t.id JOIN actor_list_access a ON a.actor_id=j.recipient AND a.list_id=t.list_id JOIN lists l ON l.id=t.list_id WHERE j.id=?1 AND t.complete=0 AND t.deleted_at IS NULL AND s.revision=j.schedule_revision AND a.rank>=1 AND l.archived=0)",[id],|r|r.get(0))?)
}
impl Store {
    pub fn claim_reminders(
        &mut self,
        owner: &str,
        now: i64,
        limit: i64,
    ) -> Result<Vec<ReminderJob>, Error> {
        self.claim_reminders_filtered(owner, now, limit, None)
    }
    pub fn claim_reminders_channel(
        &mut self,
        owner: &str,
        now: i64,
        limit: i64,
        channel: Channel,
    ) -> Result<Vec<ReminderJob>, Error> {
        self.claim_reminders_filtered(owner, now, limit, Some(channel))
    }
    fn claim_reminders_filtered(
        &mut self,
        owner: &str,
        now: i64,
        limit: i64,
        channel: Option<Channel>,
    ) -> Result<Vec<ReminderJob>, Error> {
        let channel = channel.map(|channel| {
            if channel == Channel::Email {
                "email"
            } else {
                "push"
            }
        });
        if owner.is_empty() || owner.len() > 200 || !(1..=100).contains(&limit) {
            return Err(invalid("lease owner and limit 1–100 required"));
        }
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        // Crashes consume an attempt. Exhausted abandoned leases become dead letters.
        tx.execute("UPDATE reminder_outbox SET status='dead',lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE attempts>=5 AND (status='pending' OR (status='leased' AND lease_until<=?1))",[now])?;
        let ids = {
            let mut q=tx.prepare("SELECT id FROM reminder_outbox WHERE attempts<5 AND available_at<=?1 AND (?3 IS NULL OR channel=?3) AND (status='pending' OR (status='leased' AND lease_until<=?1)) ORDER BY available_at,id LIMIT ?2")?;
            q.query_map(params![now, limit, channel], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut jobs = Vec::new();
        for id in ids {
            if !eligible(&tx, &id)? {
                tx.execute("UPDATE reminder_outbox SET status='cancelled',lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE id=?1",[id])?;
                continue;
            }
            let token = Uuid::new_v4().to_string();
            tx.execute("UPDATE reminder_outbox SET status='leased',attempts=attempts+1,lease_owner=?1,lease_token=?2,lease_until=?3 WHERE id=?4",params![owner,token,now+60,id])?;
            jobs.push(tx.query_row("SELECT id,task_id,schedule_revision,recipient,channel,at,attempts,lease_owner,lease_token,lease_until FROM reminder_outbox WHERE id=?1",[id],job_row)?);
        }
        tx.commit()?;
        Ok(jobs)
    }
    pub fn reminder_eligible(
        &self,
        id: &str,
        owner: &str,
        lease_token: &str,
        now: i64,
    ) -> Result<bool, Error> {
        let owned:bool=self.conn.query_row("SELECT EXISTS(SELECT 1 FROM reminder_outbox WHERE id=?1 AND status='leased' AND lease_owner=?2 AND lease_token=?3 AND lease_until>?4)",params![id,owner,lease_token,now],|r|r.get(0))?;
        Ok(owned && eligible(&self.conn, id)?)
    }
    pub fn acknowledge_reminder(
        &mut self,
        id: &str,
        owner: &str,
        lease_token: &str,
        now: i64,
    ) -> Result<(), Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        if !eligible(&tx, id)? {
            return Err(Error::Conflict(
                "reminder is stale or recipient access was revoked".into(),
            ));
        }
        let changed=tx.execute("UPDATE reminder_outbox SET status='delivered',lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE id=?1 AND status='leased' AND lease_owner=?2 AND lease_token=?3 AND lease_until>?4",params![id,owner,lease_token,now])?;
        if changed != 1 {
            return Err(Error::Conflict(
                "reminder lease is expired or no longer owned".into(),
            ));
        }
        tx.commit()?;
        Ok(())
    }
    pub fn fail_reminder(
        &mut self,
        id: &str,
        owner: &str,
        lease_token: &str,
        now: i64,
        message: &str,
    ) -> Result<(), Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let attempts:i64=tx.query_row("SELECT attempts FROM reminder_outbox WHERE id=?1 AND status='leased' AND lease_owner=?2 AND lease_token=?3 AND lease_until>?4",params![id,owner,lease_token,now],|r|r.get(0)).optional()?.ok_or_else(||Error::Conflict("reminder lease is expired or no longer owned".into()))?;
        let delay = 60 * (1_i64 << attempts.min(6));
        tx.execute("UPDATE reminder_outbox SET status=?1,available_at=?2,lease_owner=NULL,lease_token=NULL,lease_until=NULL,last_error=?3 WHERE id=?4",params![if attempts>=5{"dead"}else{"pending"},now+delay.min(3600),message.chars().take(500).collect::<String>(),id])?;
        tx.commit()?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Capture, Change, NewTask};
    fn db() -> Store {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        s
    }
    fn new_task(s: &mut Store) -> Task {
        s.capture(
            "dev",
            &Uuid::new_v4().to_string(),
            Capture {
                list_id: "inbox".into(),
                tasks: vec![NewTask {
                    title: "Recurring".into(),
                    description: String::new(),
                    parent_id: None,
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    fn moment(value: &str) -> Moment {
        Moment::Timed {
            local: value.into(),
            timezone: "Europe/Zurich".into(),
        }
    }
    fn schedule() -> Schedule {
        Schedule {
            due: Some(moment("2026-03-28T09:00:00")),
            reminders: vec![Reminder {
                channel: Channel::Email,
                at: moment("2026-03-28T08:00:00"),
            }],
            ..Default::default()
        }
    }
    fn spec() -> RecurrenceSpec {
        RecurrenceSpec {
            rule: Rule::Daily { interval_days: 1 },
            start_local: "2026-03-28T09:00:00".into(),
            timezone: "Europe/Zurich".into(),
            end_date: None,
            count: Some(3),
        }
    }
    fn unix(value: &str) -> i64 {
        timed(&moment(value)).unwrap()
    }
    fn child(s: &mut Store, parent: &str, title: &str) -> Task {
        s.capture(
            "dev",
            &Uuid::new_v4().to_string(),
            Capture {
                list_id: "inbox".into(),
                tasks: vec![NewTask {
                    title: title.into(),
                    description: format!("{title} description"),
                    parent_id: Some(parent.into()),
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    #[test]
    fn recurring_nested_checklist_preserves_metadata_and_history_across_dst_and_move() {
        let mut s = db();
        let root = new_task(&mut s);
        let first = child(&mut s, &root.id, "First child");
        let grandchild = child(&mut s, &first.id, "Nested child");
        let sibling = child(&mut s, &root.id, "Second child");
        s.conn.execute("INSERT INTO sections(id,list_id,title,position) VALUES('checklist-section','inbox','Checklist section',0)",[]).unwrap();
        let current = task(&s.conn, &root.id).unwrap();
        s.set_section(
            "dev",
            &root.id,
            current.revision,
            Some("checklist-section".into()),
        )
        .unwrap();
        let current = task(&s.conn, &first.id).unwrap();
        s.set_task_tags(
            "dev",
            &first.id,
            current.revision,
            vec![crate::taxonomy::TagSelector::Name("Checklist".into())],
        )
        .unwrap();
        let current = task(&s.conn, &first.id).unwrap();
        s.set_schedule(
            "dev",
            &first.id,
            current.revision,
            Schedule {
                planned: Some(Moment::Date {
                    date: "2026-03-28".into(),
                }),
                due: Some(moment("2026-03-28T08:00:00")),
                priority: Some(1),
                reminders: vec![Reminder {
                    channel: Channel::Email,
                    at: moment("2026-03-28T07:30:00"),
                }],
            },
        )
        .unwrap();
        s.create_comment(
            "dev",
            &first.id,
            "checklist-comment",
            crate::discussion::NewComment {
                body: "Historical discussion".into(),
            },
        )
        .unwrap();
        let current = task(&s.conn, &root.id).unwrap();
        let series = s
            .start_series("dev", &root.id, current.revision, spec())
            .unwrap();
        let paused = s
            .set_series_state(
                "dev",
                &series.id,
                series.revision,
                SeriesState::Paused,
                unix("2026-03-28T10:00:00"),
            )
            .unwrap();
        let current = task(&s.conn, &first.id).unwrap();
        s.update(
            "dev",
            &first.id,
            Change {
                revision: current.revision,
                complete: Some(true),
                complete_subtree: true,
                ..Default::default()
            },
        )
        .unwrap();
        let current = task(&s.conn, &sibling.id).unwrap();
        s.update(
            "dev",
            &sibling.id,
            Change {
                revision: current.revision,
                complete: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let current = task(&s.conn, &root.id).unwrap();
        assert!(
            s.update(
                "dev",
                &root.id,
                Change {
                    revision: current.revision,
                    complete: Some(true),
                    ..Default::default()
                }
            )
            .is_err()
        );
        s.update(
            "dev",
            &root.id,
            Change {
                revision: current.revision,
                complete: Some(true),
                complete_subtree: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s.occurrences("dev", &series.id).unwrap().len(), 1);
        s.set_series_state(
            "dev",
            &series.id,
            paused.revision,
            SeriesState::Active,
            unix("2026-03-28T10:00:00"),
        )
        .unwrap();
        let occurrences = s.occurrences("dev", &series.id).unwrap();
        assert_eq!(occurrences.len(), 2);
        assert_eq!(occurrences[1].due_local, "2026-03-29T09:00:00");
        let next = s.subtree("dev", &occurrences[1].task_id).unwrap();
        assert_eq!(next.len(), 4);
        assert!(next.iter().all(|t| !t.complete));
        let first_copy = next.iter().find(|t| t.title == first.title).unwrap();
        let nested_copy = next.iter().find(|t| t.title == grandchild.title).unwrap();
        assert_eq!(
            first_copy.parent_id.as_deref(),
            Some(occurrences[1].task_id.as_str())
        );
        assert_eq!(
            nested_copy.parent_id.as_deref(),
            Some(first_copy.id.as_str())
        );
        assert_eq!(first_copy.description, first.description);
        for old in [&root, &first, &grandchild, &sibling] {
            assert!(
                next.iter()
                    .all(|new| new.id != old.id && new.reference != old.reference)
            );
        }
        assert!(
            next.iter()
                .all(|t| s.task_section("dev", &t.id).unwrap().as_deref()
                    == Some("checklist-section"))
        );
        assert_eq!(
            s.task_tags("dev", &first_copy.id).unwrap()[0].name,
            "Checklist"
        );
        let shifted = s.get_schedule("dev", &first_copy.id).unwrap().schedule;
        assert_eq!(shifted.priority, Some(1));
        assert_eq!(
            shifted.planned,
            Some(Moment::Date {
                date: "2026-03-29".into()
            })
        );
        assert_eq!(shifted.due, Some(moment("2026-03-29T08:00:00")));
        assert_eq!(shifted.reminders[0].at, moment("2026-03-29T07:30:00"));
        assert_eq!(s.comments("dev", &first.id, 0, 10).unwrap().len(), 1);
        assert!(s.comments("dev", &first_copy.id, 0, 10).unwrap().is_empty());
        assert_eq!(s.conn.query_row("SELECT COUNT(*) FROM history WHERE json_extract(event,'$.kind')='recurrence-create'",[],|r|r.get::<_,i64>(0)).unwrap(),4);
        let old_nested = task(&s.conn, &grandchild.id).unwrap();
        s.update(
            "dev",
            &old_nested.id,
            Change {
                revision: old_nested.revision,
                complete: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s.occurrences("dev", &series.id).unwrap().len(), 2);
        assert_eq!(
            s.conn
                .query_row(
                    "SELECT COUNT(*) FROM reminder_outbox WHERE task_id=?1 AND status='pending'",
                    [&first.id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        let next_root = task(&s.conn, &occurrences[1].task_id).unwrap();
        let moved = s
            .move_subtree("dev", &next_root.id, "groceries", next_root.revision, false)
            .unwrap();
        assert!(
            s.update(
                "other",
                &moved.id,
                Change {
                    revision: moved.revision,
                    complete: Some(true),
                    complete_subtree: true,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            s.update(
                "dev",
                &moved.id,
                Change {
                    revision: next_root.revision,
                    complete: Some(true),
                    complete_subtree: true,
                    ..Default::default()
                }
            )
            .is_err()
        );
        s.update(
            "dev",
            &moved.id,
            Change {
                revision: moved.revision,
                complete: Some(true),
                complete_subtree: true,
                ..Default::default()
            },
        )
        .unwrap();
        let occurrences = s.occurrences("dev", &series.id).unwrap();
        assert_eq!(occurrences.len(), 3);
        let last = s.subtree("dev", &occurrences[2].task_id).unwrap();
        assert_eq!(last.len(), 4);
        assert!(last.iter().all(|t| t.list_id == "groceries" && !t.complete));
        assert!(
            last.iter()
                .all(|t| s.task_section("dev", &t.id).unwrap().is_none())
        );
        assert_eq!(
            s.task_tags(
                "dev",
                &last.iter().find(|t| t.title == first.title).unwrap().id
            )
            .unwrap()[0]
                .name,
            "Checklist"
        );
    }
    #[test]
    fn checklist_skip_is_explicit_and_bounds_roll_back_completion() {
        let mut s = db();
        let root = new_task(&mut s);
        let child = child(&mut s, &root.id, "Skip child");
        let series = s
            .start_series("dev", &root.id, root.revision, spec())
            .unwrap();
        s.skip_occurrence("dev", &root.id, 2, unix("2026-03-28T10:00:00"))
            .unwrap();
        assert!(task(&s.conn, &child.id).unwrap().complete);
        assert_eq!(
            s.occurrences("dev", &series.id).unwrap()[0].status,
            "skipped"
        );
        assert_eq!(s.conn.query_row("SELECT COUNT(*) FROM history WHERE json_extract(event,'$.kind')='recurrence-skip'",[],|r|r.get::<_,i64>(0)).unwrap(),2);
        let mut bounded = db();
        let root = new_task(&mut bounded);
        for n in 0..500 {
            bounded.conn.execute("INSERT INTO tasks(id,reference,list_id,parent_id,title,position) VALUES(?1,?1,'inbox',?2,'Bounded child',?3)",params![format!("bounded-{n}"),root.id,n]).unwrap();
        }
        assert!(bounded.start_series("dev", &root.id, 1, spec()).is_err());
        bounded
            .conn
            .execute("DELETE FROM tasks WHERE id='bounded-499'", [])
            .unwrap();
        let series = bounded.start_series("dev", &root.id, 1, spec()).unwrap();
        bounded.conn.execute("INSERT INTO tasks(id,reference,list_id,parent_id,title,position) VALUES('overflow','overflow','inbox',?1,'Overflow',500)",[&root.id]).unwrap();
        assert!(
            bounded
                .update(
                    "dev",
                    &root.id,
                    Change {
                        revision: 2,
                        complete: Some(true),
                        complete_subtree: true,
                        ..Default::default()
                    }
                )
                .is_err()
        );
        assert!(!task(&bounded.conn, &root.id).unwrap().complete);
        assert!(!task(&bounded.conn, "bounded-0").unwrap().complete);
        assert_eq!(bounded.series("dev", &series.id).unwrap().emitted, 1);
        bounded
            .conn
            .execute("DELETE FROM tasks WHERE id='overflow'", [])
            .unwrap();
        bounded
            .update(
                "dev",
                &root.id,
                Change {
                    revision: 2,
                    complete: Some(true),
                    complete_subtree: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let next = bounded.occurrences("dev", &series.id).unwrap();
        assert_eq!(next.len(), 2);
        assert_eq!(bounded.subtree("dev", &next[1].task_id).unwrap().len(), 500);
    }
    #[test]
    fn occurrence_reparent_requires_stopping_the_series() {
        let mut s = db();
        let root = new_task(&mut s);
        let parent = new_task(&mut s);
        let series = s
            .start_series("dev", &root.id, root.revision, spec())
            .unwrap();
        assert!(
            s.reparent_subtree("dev", &root.id, Some(parent.id.clone()), 2)
                .is_err()
        );
        assert!(task(&s.conn, &root.id).unwrap().parent_id.is_none());
        s.set_series_state(
            "dev",
            &series.id,
            series.revision,
            SeriesState::Stopped,
            unix("2026-03-28T10:00:00"),
        )
        .unwrap();
        s.reparent_subtree("dev", &root.id, Some(parent.id.clone()), 2)
            .unwrap();
        s.start_series("dev", &parent.id, parent.revision, spec())
            .unwrap();
    }
    #[test]
    fn unrelated_schedule_edits_preserve_delivery_intents_and_do_not_replay_sent() {
        let mut s = db();
        let task = new_task(&mut s);
        let value = schedule();
        let first = s
            .set_schedule("dev", &task.id, task.revision, value.clone())
            .unwrap();
        let now = unix("2026-03-29T08:00:00");
        let first_job = s
            .claim_reminders_channel("smtp", now, 1, Channel::Email)
            .unwrap()
            .remove(0);
        // Simulate a provider-accepted delivery committed in the durable ledger.
        s.conn.execute("UPDATE reminder_outbox SET status='delivered',lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE id=?1",[&first_job.id]).unwrap();
        let mut changed = value;
        changed.priority = Some(1);
        let second = s
            .set_schedule("dev", &task.id, first.task_revision, changed)
            .unwrap();
        assert_eq!(second.task_revision, first.task_revision + 1);
        assert!(
            s.claim_reminders_channel("smtp", now, 10, Channel::Email)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            s.conn
                .query_row(
                    "SELECT count(*) FROM reminder_outbox WHERE task_id=?1",
                    [&task.id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }
    #[test]
    fn email_and_push_workers_lease_only_their_own_channel() {
        let mut s = db();
        let task = new_task(&mut s);
        let mut value = schedule();
        value.reminders.push(Reminder {
            channel: Channel::Push,
            at: moment("2026-03-28T08:00:00"),
        });
        s.set_schedule("dev", &task.id, task.revision, value)
            .unwrap();
        let now = unix("2026-03-29T08:00:00");
        let email = s
            .claim_reminders_channel("smtp", now, 100, Channel::Email)
            .unwrap();
        assert_eq!(email.len(), 1);
        assert_eq!(email[0].channel, Channel::Email);
        let push = s
            .claim_reminders_channel("push", now, 100, Channel::Push)
            .unwrap();
        assert_eq!(push.len(), 1);
        assert_eq!(push[0].channel, Channel::Push);
        assert_ne!(email[0].id, push[0].id);
        assert!(
            s.claim_reminders_channel("second-smtp", now, 100, Channel::Email)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn p0_is_persisted_as_highest_priority_and_out_of_range_is_rejected() {
        let mut s = db();
        let t = new_task(&mut s);
        let mut value = schedule();
        value.priority = Some(0);
        let saved = s.set_schedule("dev", &t.id, 1, value.clone()).unwrap();
        assert_eq!(
            s.get_schedule("dev", &t.id).unwrap().schedule.priority,
            Some(0)
        );
        value.priority = Some(4);
        assert!(
            s.set_schedule("dev", &t.id, saved.task_revision, value)
                .is_err()
        );
    }
    #[test]
    fn schedule_validation_permissions_revision_and_stale_intents() {
        let mut s = db();
        let t = new_task(&mut s);
        assert!(matches!(
            s.set_schedule("other", &t.id, 1, schedule()),
            Err(Error::Forbidden)
        ));
        let state = s.set_schedule("dev", &t.id, 1, schedule()).unwrap();
        assert_eq!(state.task_revision, 2);
        assert!(matches!(
            s.set_schedule("dev", &t.id, 1, schedule()),
            Err(Error::Conflict(_))
        ));
        let job = s
            .claim_reminders("worker", unix("2026-03-28T08:00:00"), 10)
            .unwrap()
            .remove(0);
        assert!(
            s.reminder_eligible(&job.id, "worker", &job.lease_token, job.at)
                .unwrap()
        );
        s.set_schedule("dev", &t.id, 2, Schedule::default())
            .unwrap();
        assert!(
            !s.reminder_eligible(&job.id, "worker", &job.lease_token, job.at)
                .unwrap()
        );
        let mut bad = schedule();
        bad.planned = Some(moment("2026-03-28T09:00:00"));
        assert!(s.set_schedule("dev", &t.id, 3, bad).is_err());
        let mut bad = schedule();
        bad.due = Some(moment("2026-03-29T02:30:00"));
        assert!(s.set_schedule("dev", &t.id, 3, bad).is_err());
        let mut bad = schedule();
        bad.due = Some(moment("2026-10-25T02:30:00"));
        assert!(s.set_schedule("dev", &t.id, 3, bad).is_err());
    }
    #[test]
    fn leased_jobs_survive_restart_and_retry_has_ownership() {
        let path = std::env::temp_dir().join(format!("todoer-jobs-{}.db", Uuid::new_v4()));
        let first;
        let at = unix("2026-03-28T08:00:00");
        {
            let mut s = Store::open(path.to_str().unwrap()).unwrap();
            s.seed_dev().unwrap();
            let t = new_task(&mut s);
            s.set_schedule("dev", &t.id, 1, schedule()).unwrap();
            first = s.claim_reminders("first", at, 10).unwrap().remove(0);
        }
        let mut s = Store::open(path.to_str().unwrap()).unwrap();
        assert!(s.claim_reminders("second", at + 59, 10).unwrap().is_empty());
        let second = s.claim_reminders("second", at + 60, 10).unwrap().remove(0);
        assert_eq!(first.id, second.id);
        assert_ne!(first.lease_token, second.lease_token);
        assert!(
            s.acknowledge_reminder(&first.id, "first", &first.lease_token, at + 60)
                .is_err()
        );
        s.fail_reminder(
            &second.id,
            "second",
            &second.lease_token,
            at + 60,
            "temporary",
        )
        .unwrap();
        assert!(
            s.claim_reminders("second", at + 100, 10)
                .unwrap()
                .is_empty()
        );
        let third = s.claim_reminders("third", at + 500, 10).unwrap().remove(0);
        s.acknowledge_reminder(&third.id, "third", &third.lease_token, at + 500)
            .unwrap();
        assert!(s.claim_reminders("last", at + 1000, 10).unwrap().is_empty());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn recurrence_hook_count_pause_skip_and_latest_only() {
        let mut s = db();
        let t = new_task(&mut s);
        let series = s.start_series("dev", &t.id, 1, spec()).unwrap();
        let paused = s
            .set_series_state(
                "dev",
                &series.id,
                1,
                SeriesState::Paused,
                unix("2026-03-28T10:00:00"),
            )
            .unwrap();
        let done = s
            .update(
                "dev",
                &t.id,
                Change {
                    revision: 2,
                    complete: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(done.complete);
        assert_eq!(s.occurrences("dev", &series.id).unwrap().len(), 1);
        let active = s
            .set_series_state(
                "dev",
                &series.id,
                paused.revision,
                SeriesState::Active,
                unix("2026-04-10T10:00:00"),
            )
            .unwrap();
        let occurrences = s.occurrences("dev", &series.id).unwrap();
        assert_eq!(occurrences.len(), 2);
        assert_eq!(occurrences[1].due_local, "2026-04-10T09:00:00");
        let second = task(&s.conn, &occurrences[1].task_id).unwrap();
        s.skip_occurrence("dev", &second.id, 1, unix("2026-04-10T10:00:00"))
            .unwrap();
        let occurrences = s.occurrences("dev", &series.id).unwrap();
        let last = task(&s.conn, &occurrences[2].task_id).unwrap();
        s.skip_occurrence("dev", &last.id, 1, unix("2026-04-11T10:00:00"))
            .unwrap();
        assert_eq!(
            s.series("dev", &series.id).unwrap().state,
            SeriesState::Stopped
        );
        assert!(
            s.set_series_state("dev", &series.id, active.revision, SeriesState::Active, 0)
                .is_err()
        );
    }
    #[test]
    fn recurrence_dst_end_and_completion_relative() {
        let mut r = spec();
        r.start_local = "2026-03-28T02:30:00".into();
        assert_eq!(
            stamp(
                next_due(&r, "2026-03-28T02:30:00", unix("2026-03-28T10:00:00"))
                    .unwrap()
                    .unwrap()
            ),
            "2026-03-30T02:30:00"
        );
        r.end_date = Some("2026-03-29".into());
        assert!(
            next_due(&r, "2026-03-28T02:30:00", unix("2026-03-28T10:00:00"))
                .unwrap()
                .is_none()
        );
        r.end_date = None;
        r.rule = Rule::CompletionRelative { days: 2 };
        assert_eq!(
            stamp(
                next_due(&r, "2026-03-28T02:30:00", unix("2026-04-10T10:00:00"))
                    .unwrap()
                    .unwrap()
            ),
            "2026-04-12T02:30:00"
        );
    }
    #[test]
    fn claim_rechecks_recipient_access_and_completion() {
        let mut s = db();
        let t = new_task(&mut s);
        s.set_schedule("dev", &t.id, 1, schedule()).unwrap();
        s.conn
            .execute(
                "DELETE FROM memberships WHERE user_id='dev' AND list_id='inbox'",
                [],
            )
            .unwrap();
        assert!(
            s.claim_reminders("worker", unix("2026-03-28T08:00:00"), 10)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn retries_are_bounded_and_due_completion_cancels_delivery() {
        let mut s = db();
        let t = new_task(&mut s);
        s.set_schedule("dev", &t.id, 1, schedule()).unwrap();
        let mut at = unix("2026-03-28T08:00:00");
        for attempt in 1..=5 {
            let job = s.claim_reminders("worker", at, 10).unwrap().remove(0);
            assert_eq!(job.attempts, attempt);
            s.fail_reminder(&job.id, "worker", &job.lease_token, at, "transport failed")
                .unwrap();
            at += 4000;
        }
        assert!(s.claim_reminders("worker", at, 10).unwrap().is_empty());
        let t = new_task(&mut s);
        s.set_schedule("dev", &t.id, 1, schedule()).unwrap();
        let job = s.claim_reminders("worker", at, 10).unwrap().remove(0);
        s.update(
            "dev",
            &t.id,
            Change {
                revision: 2,
                complete: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            !s.reminder_eligible(&job.id, "worker", &job.lease_token, at)
                .unwrap()
        );
        assert!(
            s.acknowledge_reminder(&job.id, "worker", &job.lease_token, at)
                .is_err()
        );
    }
    #[test]
    fn reopening_history_does_not_duplicate_the_next_occurrence() {
        let mut s = db();
        let t = new_task(&mut s);
        let series = s.start_series("dev", &t.id, 1, spec()).unwrap();
        let done = s
            .update(
                "dev",
                &t.id,
                Change {
                    revision: 2,
                    complete: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
        let before = s.occurrences("dev", &series.id).unwrap();
        assert_eq!(before.len(), 2);
        let reopened = s
            .update(
                "dev",
                &t.id,
                Change {
                    revision: done.revision,
                    complete: Some(false),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            s.occurrences("dev", &series.id).unwrap()[0].status,
            "reopened"
        );
        s.update(
            "dev",
            &t.id,
            Change {
                revision: reopened.revision,
                complete: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s.occurrences("dev", &series.id).unwrap().len(), 2);
        assert_eq!(
            s.occurrences("dev", &series.id).unwrap()[1].task_id,
            before[1].task_id
        );
    }
    #[test]
    fn concurrent_workers_claim_a_single_lease() {
        let path = std::env::temp_dir().join(format!("todoer-claim-{}.db", Uuid::new_v4()));
        {
            let mut s = Store::open(path.to_str().unwrap()).unwrap();
            s.seed_dev().unwrap();
            let t = new_task(&mut s);
            s.set_schedule("dev", &t.id, 1, schedule()).unwrap();
        }
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles = (0..2)
            .map(|i| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut s = Store::open(path.to_str().unwrap()).unwrap();
                    barrier.wait();
                    s.claim_reminders(&format!("worker-{i}"), unix("2026-03-28T08:00:00"), 10)
                        .unwrap()
                        .len()
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .sum::<usize>(),
            1
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn scoped_agent_reminders_keep_the_human_recipient() {
        let mut s = db();
        let t = new_task(&mut s);
        s.conn.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,list_ids,expires_at) VALUES('scoped','dev','irrelevant','mcp','[\"write\"]','[\"inbox\"]',unixepoch()+3600)",[]).unwrap();
        s.set_schedule("credential:scoped", &t.id, 1, schedule())
            .unwrap();
        let job = s
            .claim_reminders("worker", unix("2026-03-28T08:00:00"), 10)
            .unwrap()
            .remove(0);
        assert_eq!(job.recipient, "dev");
    }
}
