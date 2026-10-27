//! Planning projections, user notification policy and transport-independent data.
use crate::core::{Error, Store, Task, actor_user, task};
use crate::scheduling::{Moment, Schedule, ScheduleState, Series};
use chrono::{Duration, NaiveDate, NaiveTime, TimeZone};
use chrono_tz::Tz;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub timezone: String,
    pub week_start: u32,
    pub fun_animations: bool,
    pub fun_animation: String,
    pub email_opt_in: bool,
    pub quiet_start: Option<String>,
    pub quiet_end: Option<String>,
    pub daily_opt_in: bool,
    pub push_daily_opt_in: bool,
    pub push_weekly_opt_in: bool,
    pub daily_time: String,
    pub weekly_opt_in: bool,
    pub weekly_day: u32,
    pub weekly_time: String,
    pub list_opt_out: Vec<String>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            timezone: "UTC".into(),
            week_start: 1,
            fun_animations: true,
            fun_animation: "tasks".into(),
            email_opt_in: false,
            quiet_start: None,
            quiet_end: None,
            daily_opt_in: false,
            push_daily_opt_in: false,
            push_weekly_opt_in: false,
            daily_time: "08:00".into(),
            weekly_opt_in: false,
            weekly_day: 1,
            weekly_time: "08:00".into(),
            list_opt_out: Vec::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedTask {
    pub task: Task,
    pub schedule: ScheduleState,
    pub dates: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeeklyStatistics {
    pub from: String,
    pub to: String,
    pub timezone: String,
    pub completed_transitions: i64,
    pub scheduled_open: usize,
    pub history_timestamp_note: String,
}
fn invalid(value: &str) -> Error {
    Error::Invalid(value.into())
}
fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Error> {
    serde_json::from_str(value).map_err(|_| invalid("invalid notification data"))
}
fn time(value: &str) -> Result<NaiveTime, Error> {
    NaiveTime::parse_from_str(value, "%H:%M")
        .map_err(|_| invalid("notification time must be HH:MM"))
}
fn zone(value: &str) -> Result<Tz, Error> {
    value.parse().map_err(|_| invalid("unknown IANA timezone"))
}
fn day(value: &str) -> Result<NaiveDate, Error> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid("date must be YYYY-MM-DD"))
}
fn dates(schedule: &Schedule, tz: Tz) -> Result<Vec<String>, Error> {
    let mut dates = Vec::new();
    for moment in [&schedule.planned, &schedule.due].into_iter().flatten() {
        let value = match moment {
            Moment::Date { date } => date.clone(),
            Moment::Timed { local, timezone } => {
                let value = chrono::NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M:%S")
                    .map_err(|_| invalid("invalid stored local time"))?;
                let aware = zone(timezone)?
                    .from_local_datetime(&value)
                    .earliest()
                    .ok_or_else(|| invalid("stored schedule is in a DST gap"))?;
                aware.with_timezone(&tz).date_naive().to_string()
            }
        };
        if !dates.contains(&value) {
            dates.push(value)
        }
    }
    dates.sort();
    Ok(dates)
}
impl Store {
    pub fn notification_preferences(&self, actor: &str) -> Result<Preferences, Error> {
        let user = actor_user(&self.conn, actor, 1)?;
        let encoded: Option<String> = self
            .conn
            .query_row(
                "SELECT document FROM notification_preferences WHERE user_id=?1",
                [user],
                |r| r.get(0),
            )
            .optional()?;
        encoded
            .map(|v| decode(&v))
            .unwrap_or(Ok(Preferences::default()))
    }
    pub fn set_notification_preferences(
        &mut self,
        actor: &str,
        value: Preferences,
    ) -> Result<Preferences, Error> {
        let user = actor_user(&self.conn, actor, 3)?;
        if !matches!(value.fun_animation.as_str(), "cards" | "tasks") {
            return Err(invalid("Unknown playful animation"));
        }
        zone(&value.timezone)?;
        time(&value.daily_time)?;
        time(&value.weekly_time)?;
        if !(1..=7).contains(&value.week_start)
            || !(1..=7).contains(&value.weekly_day)
            || value.list_opt_out.len() > 1000
        {
            return Err(invalid("weekly day must be 1–7 and list overrides bounded"));
        }
        match (&value.quiet_start, &value.quiet_end) {
            (Some(a), Some(b)) => {
                time(a)?;
                time(b)?;
            }
            (None, None) => {}
            _ => return Err(invalid("quiet hours require both start and end")),
        }
        for list in &value.list_opt_out {
            if self.list_permission(actor, list)? < 1 {
                return Err(Error::Forbidden);
            }
        }
        self.conn.execute("INSERT INTO notification_preferences(user_id,document) VALUES(?1,?2) ON CONFLICT(user_id) DO UPDATE SET document=excluded.document,revision=revision+1",params![user,serde_json::to_string(&value).unwrap()])?;
        Ok(value)
    }
    // Internal calendar interpretation reveals no account-wide preference
    // document to a scoped credential; resource rows still use live actor views.
    fn planning_timezone(&self, actor: &str) -> Result<String, Error> {
        let user = crate::core::actor_identity(&self.conn, actor)?.0;
        let timezone:Option<String>=self.conn.query_row("SELECT json_extract(document,'$.timezone') FROM notification_preferences WHERE user_id=?1",[user],|r|r.get(0)).optional()?.flatten();
        Ok(timezone.unwrap_or_else(|| "UTC".into()))
    }
    pub fn planning_tasks(
        &self,
        actor: &str,
        from: &str,
        to: &str,
    ) -> Result<Vec<PlannedTask>, Error> {
        let start = day(from)?;
        let end = day(to)?;
        if end < start || (end - start).num_days() > 366 {
            return Err(invalid(
                "planning range must be ordered and at most 366 days",
            ));
        }
        self.planning_rows(actor, from, to, false)
    }
    pub(crate) fn planning_timeline(
        &self,
        actor: &str,
        from: &str,
        to: &str,
    ) -> Result<Vec<PlannedTask>, Error> {
        day(from)?;
        day(to)?;
        self.planning_rows(actor, from, to, true)
    }
    fn planning_rows(
        &self,
        actor: &str,
        from: &str,
        to: &str,
        timeline: bool,
    ) -> Result<Vec<PlannedTask>, Error> {
        let tz = zone(&self.planning_timezone(actor)?)?;
        let mut q=self.conn.prepare("SELECT t.id,s.revision,s.document FROM tasks t JOIN task_schedule s ON s.task_id=t.id JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?1 JOIN lists l ON l.id=t.list_id WHERE t.deleted_at IS NULL AND (?2 OR t.complete=0) AND l.archived=0 AND a.rank>=1 ORDER BY t.id")?;
        let rows = q.query_map(params![actor, timeline], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, revision, document) = row?;
            let schedule: Schedule = decode(&document)?;
            let dates = if timeline {
                match &schedule.planned {
                    Some(Moment::Date { date }) => vec![date.clone()],
                    _ => Vec::new(),
                }
            } else {
                dates(&schedule, tz)?
            };
            if dates
                .iter()
                .any(|value| value.as_str() >= from && value.as_str() <= to)
            {
                let task = task(&self.conn, &id)?;
                let task_revision = task.revision;
                result.push(PlannedTask {
                    task,
                    schedule: ScheduleState {
                        task_revision,
                        schedule_revision: revision,
                        schedule,
                    },
                    dates,
                });
            }
        }
        result.sort_by(|a, b| {
            a.dates
                .cmp(&b.dates)
                .then_with(|| a.task.id.cmp(&b.task.id))
        });
        Ok(result)
    }
    pub fn recurring_series(&self, actor: &str) -> Result<Vec<Series>, Error> {
        let mut q=self.conn.prepare("SELECT DISTINCT o.series_id FROM recurrence_occurrences o JOIN tasks t ON t.id=o.task_id JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?1 WHERE t.deleted_at IS NULL AND a.rank>=1 ORDER BY o.series_id LIMIT 1000")?;
        let ids = q
            .query_map([actor], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut result = Vec::new();
        for id in ids {
            match self.series(actor, &id) {
                Ok(value) => result.push(value),
                Err(Error::Forbidden) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(result)
    }
    pub fn weekly_statistics(&self, actor: &str, from: &str) -> Result<WeeklyStatistics, Error> {
        let start = day(from)?;
        let end = start
            .checked_add_signed(Duration::days(6))
            .ok_or_else(|| invalid("week date overflow"))?;
        let timezone = self.planning_timezone(actor)?;
        let tz = zone(&timezone)?;
        let start_time = tz
            .from_local_datetime(&start.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .ok_or_else(|| invalid("week starts in a timezone gap"))?
            .timestamp();
        let end_time = tz
            .from_local_datetime(
                &end.succ_opt()
                    .ok_or_else(|| invalid("week date overflow"))?
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            )
            .earliest()
            .ok_or_else(|| invalid("week ends in a timezone gap"))?
            .timestamp();
        let count=self.conn.query_row("SELECT COUNT(*) FROM history h JOIN tasks t ON t.id=json_extract(h.event,'$.task.id') JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?1 WHERE h.occurred_at>=?2 AND h.occurred_at<?3 AND json_extract(h.event,'$.task.complete')=1 AND json_extract(h.before_event,'$.complete')=0 AND h.before_event IS NOT NULL AND json_extract(h.event,'$.countable_leaf')=1 AND a.rank>=1",params![actor,start_time,end_time],|r|r.get(0))?;
        Ok(WeeklyStatistics{from:start.to_string(),to:end.to_string(),timezone,completed_transitions:count,scheduled_open:self.planning_tasks(actor,&start.to_string(),&end.to_string())?.len(),history_timestamp_note:"History without timestamps or leaf snapshots is excluded; repeat leaf completion transitions are counted.".into()})
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
    #[test]
    fn playful_preferences_preserve_defaults_validate_and_persist() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let old: Preferences = serde_json::from_str("{}").unwrap();
        assert!(old.fun_animations);
        assert_eq!(old.fun_animation, "tasks");
        let mut value = db.notification_preferences("dev").unwrap();
        value.fun_animation = "tasks".into();
        value.fun_animations = false;
        db.set_notification_preferences("dev", value.clone())
            .unwrap();
        let saved = db.notification_preferences("dev").unwrap();
        assert!(!saved.fun_animations);
        assert_eq!(saved.fun_animation, "tasks");
        value.fun_animation = "unknown".into();
        assert!(db.set_notification_preferences("dev", value).is_err());
    }

    #[test]
    fn planning_dates_preserve_date_only_and_apply_user_timezone() {
        let mut s = db();
        let t = s
            .capture(
                "dev",
                "schedule",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![NewTask {
                        title: "Plan".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        s.set_notification_preferences(
            "dev",
            Preferences {
                timezone: "America/Los_Angeles".into(),
                ..Default::default()
            },
        )
        .unwrap();
        s.set_schedule(
            "dev",
            &t.id,
            1,
            Schedule {
                planned: Some(Moment::Date {
                    date: "2026-04-10".into(),
                }),
                due: Some(Moment::Timed {
                    local: "2026-04-10T01:00:00".into(),
                    timezone: "Europe/Zurich".into(),
                }),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            s.planning_tasks("dev", "2026-04-09", "2026-04-09")
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            s.planning_tasks("dev", "2026-04-10", "2026-04-10")
                .unwrap()
                .len(),
            1
        );
        assert!(
            s.planning_tasks("other", "2026-04-09", "2026-04-10")
                .unwrap()
                .is_empty()
        );
        assert!(s.planning_tasks("dev", "2026-04-10", "2028-04-10").is_err());
    }
    #[test]
    fn preferences_validate_and_statistics_use_real_timestamps() {
        let mut s = db();
        assert!(!s.notification_preferences("dev").unwrap().email_opt_in);
        assert!(
            s.set_notification_preferences(
                "dev",
                Preferences {
                    timezone: "not/zone".into(),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            s.set_notification_preferences(
                "dev",
                Preferences {
                    quiet_start: Some("22:00".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
        let t = s
            .capture(
                "dev",
                "stats",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![NewTask {
                        title: "Done".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        s.update(
            "dev",
            &t.id,
            Change {
                revision: 1,
                complete: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let today = chrono::Utc::now().date_naive().to_string();
        assert_eq!(
            s.weekly_statistics("dev", &today)
                .unwrap()
                .completed_transitions,
            1
        );
        assert_eq!(
            s.weekly_statistics("other", &today)
                .unwrap()
                .completed_transitions,
            0
        );
    }
}

use crate::scheduling::{Channel, ReminderJob};
use chrono::{Datelike, Utc};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};
use std::sync::{Arc, Mutex};
#[derive(Clone)]
pub struct Notifications {
    smtp: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    test_recipient: Option<String>,
}
impl Notifications {
    pub fn from_env() -> Result<Option<Self>, Error> {
        let Ok(host) = std::env::var("TODOER_SMTP_HOST") else {
            return Ok(None);
        };
        let from = std::env::var("TODOER_SMTP_FROM")
            .map_err(|_| invalid("TODOER_SMTP_FROM is required"))?
            .parse()
            .map_err(|_| invalid("SMTP from address is invalid"))?;
        let port = std::env::var("TODOER_SMTP_PORT")
            .unwrap_or_else(|_| "587".into())
            .parse::<u16>()
            .map_err(|_| invalid("SMTP port is invalid"))?;
        let local = matches!(host.as_str(), "127.0.0.1" | "::1" | "localhost")
            && std::env::var("TODOER_SMTP_INSECURE_LOCAL").as_deref() == Ok("1");
        let builder = if local {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
        } else if std::env::var("TODOER_SMTP_TLS").as_deref() == Ok("implicit") {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
                .map_err(|_| invalid("SMTP TLS configuration is invalid"))?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
                .map_err(|_| invalid("SMTP STARTTLS configuration is invalid"))?
        };
        let mut builder = builder
            .port(port)
            .timeout(Some(std::time::Duration::from_secs(15)));
        match (
            std::env::var("TODOER_SMTP_USERNAME").ok(),
            std::env::var("TODOER_SMTP_PASSWORD").ok(),
        ) {
            (Some(user), Some(password)) => {
                builder = builder.credentials(Credentials::new(user, password));
            }
            (None, None) => {}
            _ => {
                return Err(invalid(
                    "SMTP username and password must both be configured",
                ));
            }
        }
        let test_recipient = std::env::var("TODOER_SMTP_TEST_RECIPIENT").ok();
        if let Some(address) = &test_recipient
            && (!local || !address.ends_with(".invalid") || address.parse::<Mailbox>().is_err())
        {
            return Err(invalid(
                "test recipients require explicit local SMTP and an .invalid address",
            ));
        }
        Ok(Some(Self {
            smtp: builder.build(),
            from,
            test_recipient,
        }))
    }
    pub(crate) async fn send_invitation(
        &self,
        id: &str,
        email: &str,
        link: &str,
    ) -> Result<(), Error> {
        self.send(id,Delivery{to:self.test_recipient.as_deref().unwrap_or(email).to_owned(),subject:"Invitation to Todoer".into(),body:format!("You have been invited to a private Todoer workspace or list.\nSign in with the invited Google email to review your access.\n\n{link}\n\nThis invitation expires after seven days. If you did not expect it, you can ignore this message.")}).await
    }
    async fn send(&self, id: &str, delivery: Delivery) -> Result<(), Error> {
        let message = Message::builder()
            .from(self.from.clone())
            .to(delivery
                .to
                .parse()
                .map_err(|_| invalid("verified recipient address is invalid"))?)
            .message_id(Some(format!("<{id}@todoer.invalid>")))
            .subject(delivery.subject)
            .header(lettre::message::header::ContentType::TEXT_PLAIN)
            .body(delivery.body)
            .map_err(|_| invalid("could not build notification message"))?;
        tokio::time::timeout(std::time::Duration::from_secs(20), self.smtp.send(message))
            .await
            .map_err(|_| invalid("SMTP operation timed out; delivery may have been accepted"))?
            .map_err(|_| invalid("SMTP operation failed; delivery may have been accepted"))?;
        Ok(())
    }
}
#[derive(Debug, Default, Serialize)]
pub struct WorkerReport {
    pub sent: usize,
    pub deferred: usize,
    pub cancelled: usize,
    pub failed: usize,
    pub digests_queued: usize,
}
struct Delivery {
    to: String,
    subject: String,
    body: String,
}
enum Decision {
    Send(Delivery),
    Defer(i64),
    Cancel,
}
pub(crate) fn quiet_until(prefs: &Preferences, now: i64) -> Result<Option<i64>, Error> {
    let tz = zone(&prefs.timezone)?;
    let instant = Utc
        .timestamp_opt(now, 0)
        .single()
        .ok_or_else(|| invalid("invalid notification time"))?
        .with_timezone(&tz);
    let (Some(start), Some(end)) = (&prefs.quiet_start, &prefs.quiet_end) else {
        return Ok(None);
    };
    let start = time(start)?;
    let end = time(end)?;
    let clock = instant.time();
    let inside = if start == end {
        true
    } else if start < end {
        clock >= start && clock < end
    } else {
        clock >= start || clock < end
    };
    if !inside {
        return Ok(None);
    }
    let date = if start == end || start > end && clock >= start {
        instant
            .date_naive()
            .succ_opt()
            .ok_or_else(|| invalid("quiet hour date overflow"))?
    } else {
        instant.date_naive()
    };
    let candidate = date.and_time(end);
    for minutes in 0..=180 {
        if let Some(value) = tz
            .from_local_datetime(&(candidate + Duration::minutes(minutes)))
            .earliest()
            && value.timestamp() > now
        {
            return Ok(Some(value.timestamp()));
        }
    }
    Ok(Some(now + 86_400))
}
fn destination(
    c: &rusqlite::Connection,
    user: &str,
    test: Option<&str>,
) -> Result<Option<String>, Error> {
    if let Some(test) = test {
        return Ok(Some(test.into()));
    }
    Ok(c.query_row(
        "SELECT email FROM auth_identities WHERE user_id=?1 ORDER BY issuer,subject LIMIT 1",
        [user],
        |r| r.get(0),
    )
    .optional()?)
}
#[derive(Debug, PartialEq)]
pub(crate) enum DigestTiming {
    Ready,
    Defer(i64),
    Cancel,
}
pub(crate) fn digest_timing(
    prefs: &Preferences,
    kind: &str,
    period: &str,
    now: i64,
) -> Result<DigestTiming, Error> {
    if let Some(at) = quiet_until(prefs, now)? {
        return Ok(DigestTiming::Defer(at));
    }
    let tz = zone(&prefs.timezone)?;
    let today = Utc
        .timestamp_opt(now, 0)
        .single()
        .ok_or_else(|| invalid("invalid worker time"))?
        .with_timezone(&tz)
        .date_naive();
    let local_now = Utc
        .timestamp_opt(now, 0)
        .single()
        .ok_or_else(|| invalid("invalid worker time"))?
        .with_timezone(&tz);
    if kind == "weekly" && local_now.weekday().number_from_monday() != prefs.weekly_day {
        return Ok(DigestTiming::Cancel);
    }
    let clock = time(if kind == "weekly" {
        &prefs.weekly_time
    } else {
        &prefs.daily_time
    })?;
    if local_now.time() < clock
        && let Some(next) = tz.from_local_datetime(&today.and_time(clock)).earliest()
    {
        return Ok(DigestTiming::Defer(next.timestamp()));
    }
    let period = day(period)?;
    if (kind == "daily" && period != today)
        || (kind == "weekly" && (period > today || (today - period).num_days() >= 7))
    {
        return Ok(DigestTiming::Cancel);
    }
    Ok(DigestTiming::Ready)
}

fn policy(prefs: &Preferences, now: i64) -> Result<Option<Decision>, Error> {
    if !prefs.email_opt_in {
        return Ok(Some(Decision::Cancel));
    }
    Ok(quiet_until(prefs, now)?.map(Decision::Defer))
}
impl Store {
    fn prepare_reminder(
        &self,
        job: &ReminderJob,
        now: i64,
        test: Option<&str>,
    ) -> Result<Decision, Error> {
        let tx = self.conn.unchecked_transaction()?;
        if !self.reminder_eligible(&job.id, &job.lease_owner, &job.lease_token, now)?
            || job.channel != Channel::Email
        {
            return Ok(Decision::Cancel);
        }
        let prefs = self.notification_preferences(&job.recipient)?;
        if let Some(decision) = policy(&prefs, now)? {
            return Ok(decision);
        }
        let task = task(&self.conn, &job.task_id)?;
        if prefs.list_opt_out.contains(&task.list_id) {
            return Ok(Decision::Cancel);
        }
        let Some(to) = destination(&self.conn, &job.recipient, test)? else {
            return Ok(Decision::Cancel);
        };
        let schedule = self.get_schedule(&job.recipient, &task.id)?;
        let due = schedule
            .schedule
            .due
            .map(|m| match m {
                Moment::Date { date } => date,
                Moment::Timed { local, timezone } => format!("{local} {timezone}"),
            })
            .unwrap_or_else(|| "No due date".into());
        let result = Decision::Send(Delivery {
            to,
            subject: "Todoer task reminder".into(),
            body: format!(
                "{}\n\nDue: {}\nReference: {}\n\nYou enabled email reminders in Todoer. Change notification preferences to unsubscribe.\n",
                task.title, due, task.reference
            ),
        });
        tx.commit()?;
        Ok(result)
    }
    fn disposition_reminder(
        &mut self,
        job: &ReminderJob,
        decision: Option<i64>,
        now: i64,
    ) -> Result<(), Error> {
        self.conn.execute("UPDATE reminder_outbox SET status=?1,available_at=?2,attempts=MAX(0,attempts-1),lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE id=?3 AND status='leased' AND lease_owner=?4 AND lease_token=?5 AND lease_until>?6",params![if decision.is_some(){"pending"}else{"cancelled"},decision.unwrap_or(now),job.id,job.lease_owner,job.lease_token,now])?;
        Ok(())
    }
    pub fn queue_digests(&mut self, now: i64) -> Result<usize, Error> {
        self.queue_digest_channel(now, false)
    }
    pub(crate) fn queue_push_digests(&mut self, now: i64) -> Result<usize, Error> {
        self.queue_digest_channel(now, true)
    }
    fn queue_digest_channel(&mut self, now: i64, push: bool) -> Result<usize, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let users = {
            let mut q = tx.prepare(
                "SELECT user_id,document FROM notification_preferences ORDER BY user_id LIMIT 1000",
            )?;
            q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut count = 0;
        for (user, data) in users {
            let prefs: Preferences = decode(&data)?;
            if !push && !prefs.email_opt_in {
                continue;
            }
            let tz = zone(&prefs.timezone)?;
            let local = Utc
                .timestamp_opt(now, 0)
                .single()
                .ok_or_else(|| invalid("invalid worker time"))?
                .with_timezone(&tz);
            for (kind, enabled, clock) in [
                (
                    "daily",
                    if push {
                        prefs.push_daily_opt_in
                    } else {
                        prefs.daily_opt_in
                    },
                    &prefs.daily_time,
                ),
                (
                    "weekly",
                    (if push {
                        prefs.push_weekly_opt_in
                    } else {
                        prefs.weekly_opt_in
                    }) && local.weekday().number_from_monday() == prefs.weekly_day,
                    &prefs.weekly_time,
                ),
            ] {
                if !enabled || local.time() < time(clock)? {
                    continue;
                }
                let period = if kind == "daily" {
                    local.date_naive()
                } else {
                    local.date_naive()
                        - Duration::days(local.weekday().num_days_from_monday().into())
                }
                .to_string();
                let table = if push {
                    "push_digest_outbox"
                } else {
                    "digest_outbox"
                };
                count+=tx.execute(&format!("INSERT OR IGNORE INTO {table}(id,user_id,kind,period,available_at) VALUES(?1,?2,?3,?4,?5)"),params![if push {format!("push-digest:{}",uuid::Uuid::new_v4())} else {uuid::Uuid::new_v4().to_string()},user,kind,period,now])?;
            }
        }
        tx.commit()?;
        Ok(count)
    }
}
#[derive(Debug, Clone)]
pub(crate) struct DigestJob {
    pub(crate) id: String,
    pub(crate) user: String,
    pub(crate) kind: String,
    pub(crate) period: String,
    pub(crate) owner: String,
    pub(crate) token: String,
}
impl Store {
    fn claim_digest(&mut self, owner: &str, now: i64) -> Result<Option<DigestJob>, Error> {
        self.claim_digest_channel(owner, now, false)
    }
    pub(crate) fn claim_digest_channel(
        &mut self,
        owner: &str,
        now: i64,
        push: bool,
    ) -> Result<Option<DigestJob>, Error> {
        let table = if push {
            "push_digest_outbox"
        } else {
            "digest_outbox"
        };
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        tx.execute(&format!("UPDATE {table} SET status='dead',lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE attempts>=5 AND (status='pending' OR status='leased' AND lease_until<=?1)"),[now])?;
        let job:Option<(String,String,String,String)>=tx.query_row(&format!("SELECT id,user_id,kind,period FROM {table} WHERE attempts<5 AND available_at<=?1 AND (status='pending' OR status='leased' AND lease_until<=?1) ORDER BY available_at,id LIMIT 1"),[now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let Some((id, user, kind, period)) = job else {
            tx.commit()?;
            return Ok(None);
        };
        let token = uuid::Uuid::new_v4().to_string();
        tx.execute(&format!("UPDATE {table} SET status='leased',attempts=attempts+1,lease_owner=?1,lease_token=?2,lease_until=?3 WHERE id=?4"),params![owner,token,now+60,id])?;
        tx.commit()?;
        Ok(Some(DigestJob {
            id,
            user,
            kind,
            period,
            owner: owner.into(),
            token,
        }))
    }
    fn prepare_digest(
        &self,
        job: &DigestJob,
        now: i64,
        test: Option<&str>,
    ) -> Result<Decision, Error> {
        let tx = self.conn.unchecked_transaction()?;
        let owned:bool=self.conn.query_row("SELECT EXISTS(SELECT 1 FROM digest_outbox WHERE id=?1 AND status='leased' AND lease_owner=?2 AND lease_token=?3 AND lease_until>?4)",params![job.id,job.owner,job.token,now],|r|r.get(0))?;
        if !owned {
            return Ok(Decision::Cancel);
        }
        let prefs = self.notification_preferences(&job.user)?;
        if job.kind == "daily" && !prefs.daily_opt_in
            || job.kind == "weekly" && !prefs.weekly_opt_in
        {
            return Ok(Decision::Cancel);
        }
        if let Some(decision) = policy(&prefs, now)? {
            return Ok(decision);
        }
        match digest_timing(&prefs, &job.kind, &job.period, now)? {
            DigestTiming::Ready => {}
            DigestTiming::Defer(at) => return Ok(Decision::Defer(at)),
            DigestTiming::Cancel => return Ok(Decision::Cancel),
        }
        let tz = zone(&prefs.timezone)?;
        let today = Utc
            .timestamp_opt(now, 0)
            .single()
            .ok_or_else(|| invalid("invalid worker time"))?
            .with_timezone(&tz)
            .date_naive();
        let period = day(&job.period)?;
        let Some(to) = destination(&self.conn, &job.user, test)? else {
            return Ok(Decision::Cancel);
        };
        let from = today - Duration::days(365);
        let to_date = today + Duration::days(1);
        let end = if job.kind == "weekly" {
            today + Duration::days(7)
        } else {
            to_date
        }; // Range stays <=366 below by using a bounded overdue window.
        let start = if job.kind == "weekly" {
            today - Duration::days(359)
        } else {
            from
        };
        let rows = self
            .planning_tasks(&job.user, &start.to_string(), &end.to_string())?
            .into_iter()
            .filter(|r| !prefs.list_opt_out.contains(&r.task.list_id))
            .collect::<Vec<_>>();
        let mut body = format!(
            "Todoer {} summary for {} ({})\n\n{} scheduled open tasks in the last year and upcoming period.\n",
            job.kind,
            job.period,
            prefs.timezone,
            rows.len()
        );
        if job.kind == "weekly" {
            let from = tz
                .from_local_datetime(&period.and_hms_opt(0, 0, 0).unwrap())
                .earliest()
                .ok_or_else(|| invalid("weekly summary starts in a DST gap"))?
                .timestamp();
            let end_day = period
                .checked_add_signed(Duration::days(7))
                .ok_or_else(|| invalid("weekly summary date overflow"))?;
            let to = tz
                .from_local_datetime(&end_day.and_hms_opt(0, 0, 0).unwrap())
                .earliest()
                .ok_or_else(|| invalid("weekly summary ends in a DST gap"))?
                .timestamp();
            let count:i64=self.conn.query_row("SELECT COUNT(*) FROM history h JOIN tasks t ON t.id=json_extract(h.event,'$.task.id') JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?1 WHERE h.occurred_at>=?2 AND h.occurred_at<?3 AND json_extract(h.event,'$.task.complete')=1 AND json_extract(h.before_event,'$.complete')=0 AND json_extract(h.event,'$.countable_leaf')=1 AND a.rank>=1 AND t.list_id NOT IN(SELECT value FROM json_each(?4))",params![job.user,from,to,serde_json::to_string(&prefs.list_opt_out).unwrap()],|r|r.get(0))?;
            body.push_str(&format!(
                "{count} leaf completion transitions this week; repeat completions count again.\n"
            ));
        }
        for row in rows.iter().take(100) {
            body.push_str(&format!(
                "- {} [{}] ({})\n",
                row.task.title,
                row.dates.join(", "),
                row.task.reference
            ));
        }
        if rows.len() > 100 {
            body.push_str(
                "Further tasks are omitted from this email; open Todoer for the full view.\n",
            );
        }
        body.push_str("\nOnly tasks you can currently access and have not muted are included. Change notification preferences in Todoer to unsubscribe.\n");
        let result = Decision::Send(Delivery {
            to,
            subject: format!("Todoer {} summary", job.kind),
            body,
        });
        tx.commit()?;
        Ok(result)
    }
    fn finish_digest(
        &mut self,
        job: &DigestJob,
        outcome: &str,
        when: i64,
        now: i64,
        error: Option<&str>,
    ) -> Result<(), Error> {
        let when = if error.is_some() {
            let attempts: i64 = self.conn.query_row(
                "SELECT attempts FROM digest_outbox WHERE id=?1",
                [&job.id],
                |r| r.get(0),
            )?;
            now + (60 * (1_i64 << attempts.min(6))).min(3600)
        } else {
            when
        };
        self.conn.execute("UPDATE digest_outbox SET status=?1,available_at=?2,attempts=CASE WHEN ?1='pending' AND ?7 IS NULL THEN MAX(0,attempts-1) ELSE attempts END,lease_owner=NULL,lease_token=NULL,lease_until=NULL,last_error=?7 WHERE id=?3 AND status='leased' AND lease_owner=?4 AND lease_token=?5 AND lease_until>?6",params![outcome,when,job.id,job.owner,job.token,now,error])?;
        Ok(())
    }
}
pub(crate) async fn database<T: Send + 'static>(
    store: &Arc<Mutex<Store>>,
    work: impl FnOnce(&mut Store) -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    let store = store.clone();
    tokio::task::spawn_blocking(move || {
        let mut db = store
            .lock()
            .map_err(|_| invalid("notification database lock failed"))?;
        work(&mut db)
    })
    .await
    .map_err(|_| invalid("notification database task failed"))?
}
/// A bounded iteration; callers schedule it repeatedly and stop it during shutdown.
pub async fn worker_tick(
    store: &Arc<Mutex<Store>>,
    config: &Notifications,
) -> Result<WorkerReport, Error> {
    worker_tick_until(store, config, None).await
}
pub(crate) async fn worker_tick_until(
    store: &Arc<Mutex<Store>>,
    config: &Notifications,
    stopping: Option<&std::sync::atomic::AtomicBool>,
) -> Result<WorkerReport, Error> {
    let mut report = WorkerReport::default();
    let now = Utc::now().timestamp();
    report.digests_queued = database(store, move |db| db.queue_digests(now)).await?;
    let owner = uuid::Uuid::new_v4().to_string();
    for _ in 0..10 {
        if stopping.is_some_and(|v| v.load(std::sync::atomic::Ordering::SeqCst)) {
            break;
        }
        let actor = owner.clone();
        let now = Utc::now().timestamp();
        let job = database(store, move |db| {
            Ok(db
                .claim_reminders_channel(&actor, now, 1, Channel::Email)?
                .pop())
        })
        .await?;
        let Some(job) = job else { break };
        let copy = job.clone();
        let recipient = config.test_recipient.clone();
        let decision = database(store, move |db| {
            db.prepare_reminder(&copy, Utc::now().timestamp(), recipient.as_deref())
        })
        .await?;
        match decision {
            Decision::Cancel => {
                let copy = job.clone();
                database(store, move |db| {
                    db.disposition_reminder(&copy, None, Utc::now().timestamp())
                })
                .await?;
                report.cancelled += 1;
            }
            Decision::Defer(until) => {
                let copy = job.clone();
                database(store, move |db| {
                    db.disposition_reminder(&copy, Some(until), Utc::now().timestamp())
                })
                .await?;
                report.deferred += 1;
            }
            Decision::Send(delivery) => {
                let sent = config.send(&job.id, delivery).await.is_ok();
                let copy = job.clone();
                database(store, move |db| {
                    if sent {
                        db.acknowledge_reminder(
                            &copy.id,
                            &copy.lease_owner,
                            &copy.lease_token,
                            Utc::now().timestamp(),
                        )
                    } else {
                        db.fail_reminder(
                            &copy.id,
                            &copy.lease_owner,
                            &copy.lease_token,
                            Utc::now().timestamp(),
                            "SMTP delivery failed or ambiguous",
                        )
                    }
                })
                .await?;
                if sent {
                    report.sent += 1
                } else {
                    report.failed += 1
                }
            }
        }
    }
    for _ in 0..5 {
        if stopping.is_some_and(|v| v.load(std::sync::atomic::Ordering::SeqCst)) {
            break;
        }
        let actor = owner.clone();
        let job = database(store, move |db| {
            db.claim_digest(&actor, Utc::now().timestamp())
        })
        .await?;
        let Some(job) = job else { break };
        let copy = job.clone();
        let recipient = config.test_recipient.clone();
        let decision = database(store, move |db| {
            db.prepare_digest(&copy, Utc::now().timestamp(), recipient.as_deref())
        })
        .await?;
        match decision {
            Decision::Cancel => {
                database(store, move |db| {
                    db.finish_digest(
                        &job,
                        "cancelled",
                        Utc::now().timestamp(),
                        Utc::now().timestamp(),
                        None,
                    )
                })
                .await?;
                report.cancelled += 1
            }
            Decision::Defer(until) => {
                database(store, move |db| {
                    db.finish_digest(&job, "pending", until, Utc::now().timestamp(), None)
                })
                .await?;
                report.deferred += 1
            }
            Decision::Send(delivery) => {
                let sent = config.send(&job.id, delivery).await.is_ok();
                database(store, move |db| {
                    db.finish_digest(
                        &job,
                        if sent { "delivered" } else { "pending" },
                        Utc::now().timestamp() + 300,
                        Utc::now().timestamp(),
                        if sent {
                            None
                        } else {
                            Some("SMTP delivery failed or ambiguous")
                        },
                    )
                })
                .await?;
                if sent {
                    report.sent += 1
                } else {
                    report.failed += 1
                }
            }
        }
    }
    Ok(report)
}
#[cfg(test)]
mod delivery_tests {
    use super::*;
    #[test]
    fn quiet_hours_and_digest_keys_are_local_and_idempotent() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-04-10T21:00:00Z")
            .unwrap()
            .timestamp();
        let prefs = Preferences {
            timezone: "Europe/Zurich".into(),
            quiet_start: Some("22:00".into()),
            quiet_end: Some("07:00".into()),
            ..Default::default()
        };
        let end = quiet_until(&prefs, now).unwrap().unwrap();
        assert_eq!(
            Utc.timestamp_opt(end, 0).single().unwrap().to_rfc3339(),
            "2026-04-11T05:00:00+00:00"
        );
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.set_notification_preferences(
            "dev",
            Preferences {
                email_opt_in: true,
                daily_opt_in: true,
                weekly_opt_in: true,
                weekly_day: 5,
                timezone: "Europe/Zurich".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(db.queue_digests(now).unwrap(), 2);
        assert_eq!(db.queue_digests(now).unwrap(), 0);
        let first = db.claim_digest("a", now).unwrap().unwrap();
        assert!(db.claim_digest("b", now).unwrap().is_some());
        assert!(db.claim_digest("c", now).unwrap().is_none());
        assert!(matches!(
            db.prepare_digest(&first, now, Some("dev@todoer.invalid"))
                .unwrap(),
            Decision::Send(_)
        ));
        db.set_notification_preferences("dev", Preferences::default())
            .unwrap();
        assert!(matches!(
            db.prepare_digest(&first, now, Some("dev@todoer.invalid"))
                .unwrap(),
            Decision::Cancel
        ));
    }
    #[tokio::test]
    #[ignore = "run scripts/rebuild-smtp-check.py against its isolated SMTP capture server"]
    async fn smtp_capture_roundtrip() {
        let config = Notifications::from_env()
            .unwrap()
            .expect("local SMTP test config");
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.set_notification_preferences(
            "dev",
            Preferences {
                email_opt_in: true,
                ..Default::default()
            },
        )
        .unwrap();
        let task = db
            .capture(
                "dev",
                "smtp",
                crate::core::Capture {
                    list_id: "inbox".into(),
                    tasks: vec![crate::core::NewTask {
                        title: "SMTP <test> & café".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let local = (Utc::now() - Duration::minutes(1))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        db.set_schedule(
            "dev",
            &task.id,
            1,
            Schedule {
                reminders: vec![crate::scheduling::Reminder {
                    channel: Channel::Email,
                    at: Moment::Timed {
                        local,
                        timezone: "UTC".into(),
                    },
                }],
                ..Default::default()
            },
        )
        .unwrap();
        let store = Arc::new(Mutex::new(db));
        let report = worker_tick(&store, &config).await.unwrap();
        assert_eq!(report.sent, 1);
        assert_eq!(worker_tick(&store, &config).await.unwrap().sent, 0);
    }
    #[test]
    fn digest_contents_recheck_access_and_list_mutes() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let now = Utc::now().timestamp();
        let today = Utc::now().date_naive().to_string();
        for (actor, list, title) in [
            ("dev", "inbox", "Visible task"),
            ("dev", "groceries", "Muted secret"),
            ("other", "private", "Private secret"),
        ] {
            let task = db
                .capture(
                    actor,
                    title,
                    crate::core::Capture {
                        list_id: list.into(),
                        tasks: vec![crate::core::NewTask {
                            title: title.into(),
                            description: String::new(),
                            parent_id: None,
                        }],
                    },
                )
                .unwrap()
                .remove(0);
            db.set_schedule(
                actor,
                &task.id,
                1,
                Schedule {
                    planned: Some(Moment::Date {
                        date: today.clone(),
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        db.set_notification_preferences(
            "dev",
            Preferences {
                email_opt_in: true,
                daily_opt_in: true,
                daily_time: "00:00".into(),
                list_opt_out: vec!["groceries".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(db.queue_digests(now).unwrap(), 1);
        let job = db.claim_digest("worker", now).unwrap().unwrap();
        let Decision::Send(delivery) = db
            .prepare_digest(&job, now, Some("dev@todoer.invalid"))
            .unwrap()
        else {
            panic!("digest should be deliverable")
        };
        assert!(delivery.body.contains("Visible task"));
        assert!(!delivery.body.contains("Muted secret"));
        assert!(!delivery.body.contains("Private secret"));
        db.conn
            .execute(
                "DELETE FROM memberships WHERE list_id='inbox' AND user_id='dev'",
                [],
            )
            .unwrap();
        let Decision::Send(delivery) = db
            .prepare_digest(&job, now, Some("dev@todoer.invalid"))
            .unwrap()
        else {
            panic!("empty digest is still deliverable")
        };
        assert!(!delivery.body.contains("Visible task"));
    }
}
