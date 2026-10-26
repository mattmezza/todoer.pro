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
    pub email_opt_in: bool,
    pub quiet_start: Option<String>,
    pub quiet_end: Option<String>,
    pub daily_opt_in: bool,
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
            email_opt_in: false,
            quiet_start: None,
            quiet_end: None,
            daily_opt_in: false,
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
        zone(&value.timezone)?;
        time(&value.daily_time)?;
        time(&value.weekly_time)?;
        if !(1..=7).contains(&value.weekly_day) || value.list_opt_out.len() > 1000 {
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
        let tz = zone(&self.notification_preferences(actor)?.timezone)?;
        let mut q=self.conn.prepare("SELECT t.id,s.revision,s.document FROM tasks t JOIN task_schedule s ON s.task_id=t.id JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?1 JOIN lists l ON l.id=t.list_id WHERE t.deleted_at IS NULL AND t.complete=0 AND l.archived=0 AND a.rank>=1 ORDER BY t.id LIMIT 10000")?;
        let rows = q.query_map([actor], |r| {
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
            let dates = dates(&schedule, tz)?;
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
        let prefs = self.notification_preferences(actor)?;
        let tz = zone(&prefs.timezone)?;
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
        Ok(WeeklyStatistics{from:start.to_string(),to:end.to_string(),timezone:prefs.timezone,completed_transitions:count,scheduled_open:self.planning_tasks(actor,&start.to_string(),&end.to_string())?.len(),history_timestamp_note:"History without timestamps or leaf snapshots is excluded; repeat leaf completion transitions are counted.".into()})
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
