//! Live-authorized weekly review and actual daily overdue observations.
use crate::core::{Error, Store, Task, access_rank, actor_identity, raw_task};
use crate::scheduling::{Moment, Schedule};
use chrono::{Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRange {
    pub from: String,
    pub to: String,
    pub list_id: Option<String>,
    pub workspace_id: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct CompletionDay {
    pub date: String,
    pub completed_leaf: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Commitment {
    pub task: Task,
    pub on: String,
    pub planned: Option<String>,
    pub due: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct OverdueObservation {
    pub date: String,
    pub overdue_leaf: usize,
    pub observed_lists: usize,
    pub current_lists: usize,
    pub first_observed_at: i64,
    pub last_observed_at: i64,
    pub complete_coverage: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct WeeklyReview {
    pub from: String,
    pub to: String,
    pub timezone: String,
    pub today: String,
    pub completed_leaf: usize,
    pub planned_leaf: usize,
    pub planned_completed_leaf: usize,
    pub overdue_now: usize,
    pub upcoming_total: usize,
    pub upcoming: Vec<Commitment>,
    pub daily: Vec<CompletionDay>,
    pub overdue_trend: Vec<OverdueObservation>,
    pub notes: Vec<String>,
}
fn invalid(s: &str) -> Error {
    Error::Invalid(s.into())
}
fn date(s: &str) -> Result<NaiveDate, Error> {
    let value =
        NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| invalid("use YYYY-MM-DD dates"))?;
    if s.len() != 10 || value.to_string() != s {
        return Err(invalid("use YYYY-MM-DD dates"));
    }
    Ok(value)
}
fn zone(s: &str) -> Result<Tz, Error> {
    s.parse()
        .map_err(|_| invalid("invalid stored review timezone"))
}
fn user_timezone(c: &Connection, user: &str) -> Result<String, Error> {
    let value:Option<String>=c.query_row("SELECT json_extract(document,'$.timezone') FROM notification_preferences WHERE user_id=?1",[user],|r|r.get(0)).optional()?.flatten();
    let value = value.unwrap_or_else(|| "UTC".into());
    zone(&value)?;
    Ok(value)
}
/// First real instant on/after the local midnight, including midnight DST gaps
/// and the rare skipped calendar day. No 24-hour arithmetic for calendar bounds.
fn midnight(tz: Tz, day: NaiveDate) -> Result<i64, Error> {
    let start = day.and_hms_opt(0, 0, 0).unwrap();
    for minute in 0..=2880 {
        let value = start
            .checked_add_signed(Duration::minutes(minute))
            .ok_or_else(|| invalid("review date overflow"))?;
        if let Some(at) = tz.from_local_datetime(&value).earliest() {
            return Ok(at.timestamp());
        }
    }
    Err(invalid("cannot resolve local review boundary"))
}
fn instant(moment: &Moment) -> Result<i64, Error> {
    let Moment::Timed { local, timezone } = moment else {
        return Err(invalid("timed moment required"));
    };
    let value = NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M:%S")
        .map_err(|_| invalid("invalid stored scheduling time"))?;
    zone(timezone)?
        .from_local_datetime(&value)
        .earliest()
        .map(|at| at.timestamp())
        .ok_or_else(|| invalid("nonexistent stored scheduling time"))
}
fn moment_date(moment: &Moment, tz: Tz) -> Result<NaiveDate, Error> {
    match moment {
        Moment::Date { date: value } => date(value),
        Moment::Timed { .. } => Ok(Utc
            .timestamp_opt(instant(moment)?, 0)
            .single()
            .ok_or_else(|| invalid("invalid scheduling timestamp"))?
            .with_timezone(&tz)
            .date_naive()),
    }
}
fn overdue(schedule: &Schedule, today: NaiveDate, now: i64) -> Result<bool, Error> {
    match schedule.due.as_ref() {
        Some(Moment::Date { date: value }) => Ok(date(value)? < today),
        Some(value) => Ok(instant(value)? < now),
        None => Ok(false),
    }
}
fn schedule(s: &str) -> Result<Schedule, Error> {
    serde_json::from_str(s).map_err(|_| invalid("invalid stored scheduling document"))
}
fn list_ids(c: &Connection, actor: &str, range: &ReviewRange) -> Result<BTreeSet<String>, Error> {
    let mut q=c.prepare("SELECT l.id FROM lists l JOIN actor_list_access a ON a.list_id=l.id WHERE a.actor_id=?1 AND a.rank>=1 AND (?2 IS NULL OR l.id=?2) AND (?3 IS NULL OR l.workspace_id=?3)")?;
    Ok(
        q.query_map(params![actor, range.list_id, range.workspace_id], |r| {
            r.get(0)
        })?
        .collect::<Result<_, _>>()?,
    )
}
impl Store {
    /// Only the timezone is used internally; scoped credentials cannot read the
    /// account's notification preferences through this method.
    pub fn review_timezone(&self, actor: &str) -> Result<String, Error> {
        let user = actor_identity(&self.conn, actor)?.0;
        if !self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE id=?1)",
            [&user],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(Error::Forbidden);
        }
        user_timezone(&self.conn, &user)
    }
    pub fn weekly_review(
        &self,
        actor: &str,
        range: ReviewRange,
        now: i64,
    ) -> Result<WeeklyReview, Error> {
        let from = date(&range.from)?;
        let to = date(&range.to)?;
        if to < from || (to - from).num_days() >= 366 {
            return Err(invalid(
                "review range must contain 1–366 ordered calendar days",
            ));
        }
        if range.list_id.is_some() && range.workspace_id.is_some() {
            return Err(invalid("choose list or workspace scope"));
        }
        let tx = self.conn.unchecked_transaction()?;
        if let Some(id) = range.list_id.as_deref() {
            access_rank(&tx, actor, id, 1)?;
        }
        if let Some(id) = range.workspace_id.as_deref()
            && self.workspace_permission(actor, id)? < 1
        {
            return Err(Error::Forbidden);
        }
        let timezone = self.review_timezone(actor)?;
        let tz = zone(&timezone)?;
        let today = Utc
            .timestamp_opt(now, 0)
            .single()
            .ok_or_else(|| invalid("invalid review timestamp"))?
            .with_timezone(&tz)
            .date_naive();
        let start = midnight(tz, from)?;
        let end = midnight(
            tz,
            to.succ_opt()
                .ok_or_else(|| invalid("review date overflow"))?,
        )?;
        let allowed = list_ids(&tx, actor, &range)?;
        // Both source-history and current task access apply before grouping.
        let mut completed = BTreeMap::<String, i64>::new();
        {
            let mut q=tx.prepare("SELECT t.id,h.list_id,t.list_id,h.occurred_at FROM history h JOIN tasks t ON t.id=json_extract(h.event,'$.task.id') JOIN actor_list_access live ON live.list_id=t.list_id AND live.actor_id=?1 JOIN actor_list_access old ON old.list_id=h.list_id AND old.actor_id=?1 WHERE live.rank>=1 AND old.rank>=1 AND h.occurred_at>=?2 AND h.occurred_at<?3 AND json_extract(h.event,'$.kind')='update' AND json_extract(h.event,'$.task.complete')=1 AND json_extract(h.before_event,'$.complete')=0 AND json_extract(h.event,'$.countable_leaf')=1")?;
            let rows = q.query_map(params![actor, start, end], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?;
            for row in rows {
                let (id, source, current, at) = row?;
                if allowed.contains(&source) && allowed.contains(&current) {
                    completed
                        .entry(id)
                        .and_modify(|old| *old = (*old).min(at))
                        .or_insert(at);
                }
            }
        }
        let mut daily = BTreeMap::<String, usize>::new();
        for at in completed.values() {
            let day = Utc
                .timestamp_opt(*at, 0)
                .single()
                .ok_or_else(|| invalid("invalid recorded history timestamp"))?
                .with_timezone(&tz)
                .date_naive()
                .to_string();
            *daily.entry(day).or_default() += 1;
        }
        let daily = (0..=(to - from).num_days())
            .map(|offset| {
                let day = (from + Duration::days(offset)).to_string();
                CompletionDay {
                    completed_leaf: *daily.get(&day).unwrap_or(&0),
                    date: day,
                }
            })
            .collect();
        let upcoming_end = today
            .checked_add_signed(Duration::days(6))
            .ok_or_else(|| invalid("upcoming date overflow"))?;
        let mut planned_ids = BTreeSet::new();
        let mut overdue_now = 0;
        let mut commitments = Vec::<(String, String, Option<String>, Option<String>)>::new();
        {
            let mut q=tx.prepare("SELECT t.id,t.list_id,t.complete,s.document FROM tasks t JOIN lists l ON l.id=t.list_id JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?1 JOIN task_schedule s ON s.task_id=t.id WHERE a.rank>=1 AND t.deleted_at IS NULL AND l.archived=0 AND NOT EXISTS(SELECT 1 FROM tasks child INDEXED BY tasks_parent WHERE child.parent_id=t.id AND child.deleted_at IS NULL)")?;
            let rows = q.query_map([actor], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?;
            for row in rows {
                let (id, list, complete, document) = row?;
                if !allowed.contains(&list) {
                    continue;
                }
                let schedule = schedule(&document)?;
                let planned = schedule
                    .planned
                    .as_ref()
                    .map(|m| moment_date(m, tz))
                    .transpose()?;
                let due = schedule
                    .due
                    .as_ref()
                    .map(|m| moment_date(m, tz))
                    .transpose()?;
                if planned.is_some_and(|day| day >= from && day <= to) {
                    planned_ids.insert(id.clone());
                }
                if complete {
                    continue;
                }
                if overdue(&schedule, today, now)? {
                    overdue_now += 1;
                }
                let future_due = match schedule.due.as_ref() {
                    Some(Moment::Timed { .. }) => instant(schedule.due.as_ref().unwrap())? >= now,
                    _ => true,
                };
                let on = [planned, due.filter(|_| future_due)]
                    .into_iter()
                    .flatten()
                    .filter(|day| *day >= today && *day <= upcoming_end)
                    .min();
                if let Some(on) = on {
                    commitments.push((
                        on.to_string(),
                        id,
                        planned.map(|d| d.to_string()),
                        due.map(|d| d.to_string()),
                    ));
                }
            }
        }
        commitments.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        let upcoming_total = commitments.len();
        let upcoming = commitments
            .into_iter()
            .take(20)
            .map(|(on, id, planned, due)| {
                Ok(Commitment {
                    task: raw_task(&tx, &id)?,
                    on,
                    planned,
                    due,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let user = actor_identity(&tx, actor)?.0;
        let overdue_trend = observations(&tx, actor, &user, &timezone, &range, &allowed)?;
        let planned_completed_leaf = planned_ids
            .iter()
            .filter(|id| completed.contains_key(*id))
            .count();
        tx.commit()?;
        Ok(WeeklyReview{from:range.from,to:range.to,timezone,today:today.to_string(),completed_leaf:completed.len(),planned_leaf:planned_ids.len(),planned_completed_leaf,overdue_now,upcoming_total,upcoming,daily,overdue_trend,notes:vec!["Completion totals count distinct tasks whose recorded completion was leaf work; repeat completion within this range counts once, on its first completion day. Skips and undo events are excluded.".into(),"Planned versus completed uses current leaf schedules, not reconstructed historical plans. Current overdue and upcoming totals are live, independent of the selected history range.".into(),"Overdue trend contains actual first observations per list and local day only. Missing days are absent, not zero; timezone changes and current access can remove observations. Partial coverage is marked.".into(),"History without a timestamp, before image or leaf snapshot is excluded; older imported work may be unavailable.".into()]})
    }
    /// Bounded, fair batch for the standalone worker; records no titles or secrets.
    pub fn record_stats_observations(&mut self, now: i64) -> Result<usize, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let users = {
            let mut q=tx.prepare("SELECT u.id FROM users u LEFT JOIN stats_observer_checks c ON c.user_id=u.id ORDER BY COALESCE(c.checked_at,0),u.id LIMIT 50")?;
            q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut captured = 0;
        for user in users {
            let timezone = user_timezone(&tx, &user)?;
            let tz = zone(&timezone)?;
            let today = Utc
                .timestamp_opt(now, 0)
                .single()
                .ok_or_else(|| invalid("invalid observation timestamp"))?
                .with_timezone(&tz)
                .date_naive();
            let lists = {
                let mut q=tx.prepare("SELECT l.id FROM lists l JOIN actor_list_access a ON a.list_id=l.id AND a.actor_id=?1 LEFT JOIN stats_observation_lists o ON o.user_id=?1 AND o.list_id=l.id AND o.local_date=?2 AND o.timezone=?3 WHERE a.rank>=1 AND o.list_id IS NULL ORDER BY l.id")?;
                q.query_map(params![user, today.to_string(), timezone], |r| {
                    r.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?
            };
            for list in lists {
                tx.execute("INSERT INTO stats_observation_lists(user_id,local_date,timezone,list_id,observed_at) VALUES(?1,?2,?3,?4,?5)",params![user,today.to_string(),timezone,list,now])?;
                let mut q=tx.prepare("SELECT t.id,s.document FROM tasks t JOIN task_schedule s ON s.task_id=t.id JOIN lists l ON l.id=t.list_id WHERE t.list_id=?1 AND t.complete=0 AND t.deleted_at IS NULL AND l.archived=0 AND NOT EXISTS(SELECT 1 FROM tasks child INDEXED BY tasks_parent WHERE child.parent_id=t.id AND child.deleted_at IS NULL)")?;
                for row in q.query_map([&list], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })? {
                    let (id, document) = row?;
                    if overdue(&schedule(&document)?, today, now)? {
                        tx.execute("INSERT INTO stats_overdue_items(user_id,local_date,timezone,list_id,task_id) VALUES(?1,?2,?3,?4,?5)",params![user,today.to_string(),timezone,list,id])?;
                    }
                }
                captured += 1;
            }
            tx.execute("INSERT INTO stats_observer_checks(user_id,checked_at) VALUES(?1,?2) ON CONFLICT(user_id) DO UPDATE SET checked_at=excluded.checked_at",params![user,now])?;
        }
        tx.commit()?;
        Ok(captured)
    }
}
fn observations(
    c: &Connection,
    actor: &str,
    user: &str,
    timezone: &str,
    range: &ReviewRange,
    allowed: &BTreeSet<String>,
) -> Result<Vec<OverdueObservation>, Error> {
    let mut points = BTreeMap::<String, OverdueObservation>::new();
    let mut q=c.prepare("SELECT o.local_date,o.list_id,o.observed_at FROM stats_observation_lists o JOIN actor_list_access a ON a.list_id=o.list_id AND a.actor_id=?1 WHERE o.user_id=?2 AND o.timezone=?3 AND o.local_date>=?4 AND o.local_date<=?5 AND a.rank>=1 ORDER BY o.local_date,o.list_id")?;
    for row in q.query_map(params![actor, user, timezone, range.from, range.to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (day, list, at) = row?;
        if !allowed.contains(&list) {
            continue;
        }
        let point = points
            .entry(day.clone())
            .or_insert_with(|| OverdueObservation {
                date: day,
                overdue_leaf: 0,
                observed_lists: 0,
                current_lists: allowed.len(),
                first_observed_at: at,
                last_observed_at: at,
                complete_coverage: false,
            });
        point.observed_lists += 1;
        point.first_observed_at = point.first_observed_at.min(at);
        point.last_observed_at = point.last_observed_at.max(at);
    }
    let mut seen = BTreeSet::new();
    let mut q=c.prepare("SELECT o.local_date,o.list_id,t.list_id,t.id FROM stats_overdue_items o JOIN tasks t ON t.id=o.task_id JOIN actor_list_access old ON old.list_id=o.list_id AND old.actor_id=?1 JOIN actor_list_access live ON live.list_id=t.list_id AND live.actor_id=?1 WHERE o.user_id=?2 AND o.timezone=?3 AND o.local_date>=?4 AND o.local_date<=?5 AND old.rank>=1 AND live.rank>=1")?;
    for row in q.query_map(params![actor, user, timezone, range.from, range.to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (day, source, current, task) = row?;
        if allowed.contains(&source)
            && allowed.contains(&current)
            && seen.insert((day.clone(), task))
            && let Some(point) = points.get_mut(&day)
        {
            point.overdue_leaf += 1;
        }
    }
    for point in points.values_mut() {
        point.complete_coverage = point.observed_lists == point.current_lists;
    }
    Ok(points.into_values().collect())
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
    fn task(s: &mut Store, actor: &str, list: &str, parent: Option<&str>, title: &str) -> Task {
        s.capture(
            actor,
            &uuid::Uuid::new_v4().to_string(),
            Capture {
                list_id: list.into(),
                tasks: vec![NewTask {
                    title: title.into(),
                    description: String::new(),
                    parent_id: parent.map(str::to_owned),
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    fn at(value: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(value)
            .unwrap()
            .timestamp()
    }
    fn range(from: &str, to: &str) -> ReviewRange {
        ReviewRange {
            from: from.into(),
            to: to.into(),
            list_id: None,
            workspace_id: None,
        }
    }
    fn scheduled(s: &mut Store, actor: &str, id: &str, planned: Option<&str>, due: Option<&str>) {
        let t = raw_task(&s.conn, id).unwrap();
        s.set_schedule(
            actor,
            id,
            t.revision,
            Schedule {
                planned: planned.map(|v| Moment::Date { date: v.into() }),
                due: due.map(|v| Moment::Date { date: v.into() }),
                ..Default::default()
            },
        )
        .unwrap();
    }
    fn complete(s: &mut Store, actor: &str, id: &str, time: i64, subtree: bool) {
        let before: i64 = s
            .conn
            .query_row("SELECT COALESCE(MAX(seq),0) FROM history", [], |r| r.get(0))
            .unwrap();
        let t = raw_task(&s.conn, id).unwrap();
        s.update(
            actor,
            id,
            Change {
                revision: t.revision,
                complete: Some(true),
                complete_subtree: subtree,
                ..Default::default()
            },
        )
        .unwrap();
        s.conn
            .execute(
                "UPDATE history SET occurred_at=?1 WHERE seq>?2",
                params![time, before],
            )
            .unwrap();
    }
    #[test]
    fn leaf_completions_are_distinct_and_current_plans_are_honest() {
        let mut s = db();
        let root = task(&mut s, "dev", "inbox", None, "Container");
        let branch = task(&mut s, "dev", "inbox", Some(&root.id), "Branch");
        let leaf = task(&mut s, "dev", "inbox", Some(&branch.id), "Leaf work");
        let sibling = task(&mut s, "dev", "inbox", Some(&root.id), "Other work");
        scheduled(
            &mut s,
            "dev",
            &root.id,
            Some("2026-01-05"),
            Some("2026-01-01"),
        );
        scheduled(
            &mut s,
            "dev",
            &leaf.id,
            Some("2026-01-05"),
            Some("2026-01-04"),
        );
        scheduled(&mut s, "dev", &sibling.id, Some("2026-01-10"), None);
        complete(&mut s, "dev", &root.id, at("2026-01-05T12:00:00Z"), true);
        let current = raw_task(&s.conn, &leaf.id).unwrap();
        s.update(
            "dev",
            &leaf.id,
            Change {
                revision: current.revision,
                complete: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        complete(&mut s, "dev", &leaf.id, at("2026-01-06T12:00:00Z"), false);
        let late = task(&mut s, "dev", "inbox", None, "Late leaf");
        scheduled(&mut s, "dev", &late.id, None, Some("2026-01-04"));
        let next = task(&mut s, "dev", "inbox", None, "Upcoming leaf");
        scheduled(&mut s, "dev", &next.id, Some("2026-01-08"), None);
        let value = s
            .weekly_review(
                "dev",
                range("2026-01-05", "2026-01-11"),
                at("2026-01-07T12:00:00Z"),
            )
            .unwrap();
        assert_eq!(value.completed_leaf, 2);
        assert_eq!(
            value.daily.iter().map(|d| d.completed_leaf).sum::<usize>(),
            2
        );
        assert_eq!(value.daily[0].completed_leaf, 2);
        assert_eq!(value.daily[1].completed_leaf, 0);
        assert_eq!(value.planned_leaf, 3);
        assert_eq!(value.planned_completed_leaf, 2);
        assert_eq!(value.overdue_now, 1);
        assert_eq!(value.upcoming_total, 1);
        assert_eq!(value.upcoming[0].task.id, next.id);
        assert!(value.overdue_trend.is_empty());
        // Adding children later does not rewrite operation-time leaf history.
        task(&mut s, "dev", "inbox", Some(&leaf.id), "Later child");
        assert_eq!(
            s.weekly_review(
                "dev",
                range("2026-01-05", "2026-01-11"),
                at("2026-01-07T12:00:00Z")
            )
            .unwrap()
            .completed_leaf,
            2
        );
    }
    #[test]
    fn snapshots_are_real_immutable_local_days_and_zero_requires_observation() {
        let mut s = db();
        let mut prefs = s.notification_preferences("dev").unwrap();
        prefs.timezone = "Europe/Zurich".into();
        s.set_notification_preferences("dev", prefs).unwrap();
        let t = task(&mut s, "dev", "inbox", None, "Late");
        scheduled(&mut s, "dev", &t.id, None, Some("2026-03-27"));
        let first = at("2026-03-28T08:00:00Z");
        assert!(s.record_stats_observations(first).unwrap() > 0);
        complete(&mut s, "dev", &t.id, first + 3600, false);
        assert_eq!(s.record_stats_observations(first + 3600).unwrap(), 0);
        let second = at("2026-03-29T07:00:00Z");
        s.record_stats_observations(second).unwrap();
        let value = s
            .weekly_review("dev", range("2026-03-27", "2026-03-30"), second)
            .unwrap();
        assert_eq!(value.overdue_now, 0);
        assert_eq!(value.overdue_trend.len(), 2);
        assert_eq!(value.overdue_trend[0].date, "2026-03-28");
        assert_eq!(value.overdue_trend[0].overdue_leaf, 1);
        assert_eq!(value.overdue_trend[1].date, "2026-03-29");
        assert_eq!(value.overdue_trend[1].overdue_leaf, 0);
        assert_eq!(value.overdue_trend[0].last_observed_at, first);
        assert!(value.overdue_trend.iter().all(|p| p.complete_coverage));
        let mut prefs = s.notification_preferences("dev").unwrap();
        prefs.timezone = "UTC".into();
        s.set_notification_preferences("dev", prefs).unwrap();
        assert!(
            s.weekly_review("dev", range("2026-03-27", "2026-03-30"), second)
                .unwrap()
                .overdue_trend
                .is_empty()
        );
        s.record_stats_observations(second).unwrap();
        assert_eq!(
            s.weekly_review("dev", range("2026-03-27", "2026-03-30"), second)
                .unwrap()
                .overdue_trend
                .len(),
            1
        );
    }
    #[test]
    fn scoped_credentials_and_current_revocation_filter_before_counts() {
        let mut s = db();
        let visible = task(&mut s, "dev", "inbox", None, "Visible");
        let hidden = task(&mut s, "dev", "groceries", None, "Outside credential");
        let private = task(
            &mut s,
            "other",
            "private",
            None,
            "Private title must not leak",
        );
        for (actor, id) in [
            ("dev", &visible.id),
            ("dev", &hidden.id),
            ("other", &private.id),
        ] {
            scheduled(&mut s, actor, id, None, Some("2026-01-01"));
        }
        s.conn.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,list_ids,expires_at) VALUES('review-scope','dev','synthetic-review','mcp','[\"read\"]','[\"inbox\"]',unixepoch()+3600)",[]).unwrap();
        let now = at("2026-01-07T12:00:00Z");
        s.record_stats_observations(now).unwrap();
        let scoped = s
            .weekly_review(
                "credential:review-scope",
                range("2026-01-05", "2026-01-11"),
                now,
            )
            .unwrap();
        assert_eq!(scoped.overdue_now, 1);
        assert_eq!(scoped.overdue_trend[0].overdue_leaf, 1);
        assert_eq!(scoped.overdue_trend[0].observed_lists, 1);
        assert!(
            !serde_json::to_string(&scoped)
                .unwrap()
                .contains("Private title")
        );
        let mut wrong = range("2026-01-05", "2026-01-11");
        wrong.list_id = Some("groceries".into());
        assert!(
            s.weekly_review("credential:review-scope", wrong, now)
                .is_err()
        );
        s.conn
            .execute(
                "DELETE FROM memberships WHERE user_id='dev' AND list_id='inbox'",
                [],
            )
            .unwrap();
        let revoked = s
            .weekly_review(
                "credential:review-scope",
                range("2026-01-05", "2026-01-11"),
                now,
            )
            .unwrap();
        assert_eq!(revoked.overdue_now, 0);
        assert!(revoked.overdue_trend.is_empty());
        assert!(revoked.upcoming.is_empty());
        s.conn
            .execute(
                "UPDATE auth_agents SET revoked=1 WHERE id='review-scope'",
                [],
            )
            .unwrap();
        assert!(
            s.weekly_review(
                "credential:review-scope",
                range("2026-01-05", "2026-01-11"),
                now
            )
            .is_err()
        );
    }
    #[test]
    fn ranges_timezone_boundaries_and_date_only_due_do_not_shift() {
        let mut s = db();
        let tz: Tz = "Europe/Zurich".parse().unwrap();
        assert_eq!(
            midnight(tz, date("2026-03-30").unwrap()).unwrap()
                - midnight(tz, date("2026-03-29").unwrap()).unwrap(),
            23 * 3600
        );
        assert_eq!(
            midnight(tz, date("2026-10-26").unwrap()).unwrap()
                - midnight(tz, date("2026-10-25").unwrap()).unwrap(),
            25 * 3600
        );
        assert!(
            midnight(
                "America/Sao_Paulo".parse().unwrap(),
                date("2018-11-04").unwrap()
            )
            .is_ok()
        );
        let mut prefs = s.notification_preferences("dev").unwrap();
        prefs.timezone = "Europe/Zurich".into();
        s.set_notification_preferences("dev", prefs).unwrap();
        let t = task(&mut s, "dev", "inbox", None, "Calendar deadline");
        scheduled(&mut s, "dev", &t.id, None, Some("2026-03-29"));
        assert_eq!(
            s.weekly_review(
                "dev",
                range("2026-03-29", "2026-03-29"),
                at("2026-03-29T21:59:00Z")
            )
            .unwrap()
            .overdue_now,
            0
        );
        assert_eq!(
            s.weekly_review(
                "dev",
                range("2026-03-29", "2026-03-29"),
                at("2026-03-29T22:01:00Z")
            )
            .unwrap()
            .overdue_now,
            1
        );
        assert!(
            s.weekly_review("dev", range("2024-01-01", "2024-12-31"), 0)
                .is_ok()
        );
        assert!(
            s.weekly_review("dev", range("2024-01-01", "2025-01-01"), 0)
                .is_err()
        );
        assert!(
            s.weekly_review("dev", range("2026-02-30", "2026-03-01"), 0)
                .is_err()
        );
        assert!(
            s.weekly_review("dev", range("2026-03-02", "2026-03-01"), 0)
                .is_err()
        );
    }
    #[test]
    fn completion_dates_use_user_timezone_and_workspace_credentials_stay_scoped() {
        let mut s = db();
        let mut prefs = s.notification_preferences("dev").unwrap();
        prefs.timezone = "America/Los_Angeles".into();
        s.set_notification_preferences("dev", prefs).unwrap();
        let t = task(&mut s, "dev", "inbox", None, "Local day work");
        complete(&mut s, "dev", &t.id, at("2026-01-06T01:00:00Z"), false);
        assert_eq!(
            s.weekly_review(
                "dev",
                range("2026-01-05", "2026-01-05"),
                at("2026-01-06T02:00:00Z")
            )
            .unwrap()
            .completed_leaf,
            1
        );
        assert_eq!(
            s.weekly_review(
                "dev",
                range("2026-01-06", "2026-01-06"),
                at("2026-01-06T02:00:00Z")
            )
            .unwrap()
            .completed_leaf,
            0
        );
        s.conn.execute_batch("INSERT INTO workspaces(id,title) VALUES('review-workspace','Workspace'); INSERT INTO workspace_members VALUES('review-workspace','dev','owner'); UPDATE lists SET workspace_id='review-workspace' WHERE id='groceries'; INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,workspace,expires_at) VALUES('review-workspace-credential','dev','synthetic-workspace','mcp','[\"read\"]','review-workspace',unixepoch()+3600)").unwrap();
        let late = task(&mut s, "dev", "groceries", None, "Workspace late work");
        scheduled(&mut s, "dev", &late.id, None, Some("2026-01-01"));
        let mut selected = range("2026-01-05", "2026-01-11");
        selected.workspace_id = Some("review-workspace".into());
        let value = s
            .weekly_review(
                "credential:review-workspace-credential",
                selected,
                at("2026-01-07T12:00:00Z"),
            )
            .unwrap();
        assert_eq!(value.overdue_now, 1);
        assert_eq!(value.completed_leaf, 0);
        assert_eq!(value.timezone, "America/Los_Angeles");
    }
    #[test]
    fn moved_tasks_and_missing_coverage_cannot_leak_old_counts() {
        let mut s = db();
        s.conn
            .execute(
                "INSERT INTO memberships(list_id,user_id) VALUES('inbox','other')",
                [],
            )
            .unwrap();
        let late = task(&mut s, "dev", "inbox", None, "Late shared work");
        scheduled(&mut s, "dev", &late.id, None, Some("2026-01-01"));
        let done = task(&mut s, "dev", "inbox", None, "Done shared work");
        complete(&mut s, "dev", &done.id, at("2026-01-05T12:00:00Z"), false);
        let now = at("2026-01-07T12:00:00Z");
        s.record_stats_observations(now).unwrap();
        let before = s
            .weekly_review("other", range("2026-01-05", "2026-01-11"), now)
            .unwrap();
        assert_eq!(before.completed_leaf, 1);
        assert_eq!(before.overdue_trend[0].overdue_leaf, 1);
        for id in [&late.id, &done.id] {
            let t = raw_task(&s.conn, id).unwrap();
            s.move_subtree("dev", id, "groceries", t.revision, false)
                .unwrap();
        }
        let after = s
            .weekly_review("other", range("2026-01-05", "2026-01-11"), now)
            .unwrap();
        assert_eq!(after.completed_leaf, 0);
        assert_eq!(after.overdue_now, 0);
        assert_eq!(after.overdue_trend[0].overdue_leaf, 0);
        s.conn.execute_batch("INSERT INTO lists(id,title) VALUES('new-review-list','New list'); INSERT INTO memberships(list_id,user_id) VALUES('new-review-list','dev')").unwrap();
        assert!(
            !s.weekly_review("dev", range("2026-01-05", "2026-01-11"), now)
                .unwrap()
                .overdue_trend[0]
                .complete_coverage
        );
        s.record_stats_observations(now + 60).unwrap();
        assert!(
            s.weekly_review("dev", range("2026-01-05", "2026-01-11"), now + 60)
                .unwrap()
                .overdue_trend[0]
                .complete_coverage
        );
    }
    #[test]
    fn observations_survive_reopen_and_worker_batches_are_bounded() {
        let path = std::env::temp_dir().join(format!("todoer-stats-{}.db", uuid::Uuid::new_v4()));
        let now = at("2026-01-07T12:00:00Z");
        {
            let mut s = Store::open(path.to_str().unwrap()).unwrap();
            s.seed_dev().unwrap();
            let late = task(&mut s, "dev", "inbox", None, "Persistent late work");
            scheduled(&mut s, "dev", &late.id, None, Some("2026-01-01"));
            for n in 0..52 {
                s.conn
                    .execute(
                        "INSERT INTO users(id,name) VALUES(?1,'Synthetic observer')",
                        [format!("stats-user-{n}")],
                    )
                    .unwrap();
            }
            s.record_stats_observations(now).unwrap();
            assert_eq!(
                s.conn
                    .query_row("SELECT COUNT(*) FROM stats_observer_checks", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                50
            );
            s.record_stats_observations(now).unwrap();
            assert_eq!(
                s.conn
                    .query_row("SELECT COUNT(*) FROM stats_observer_checks", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                54
            );
        }
        {
            let mut s = Store::open(path.to_str().unwrap()).unwrap();
            assert_eq!(
                s.weekly_review("dev", range("2026-01-05", "2026-01-11"), now)
                    .unwrap()
                    .overdue_trend[0]
                    .overdue_leaf,
                1
            );
            assert_eq!(s.record_stats_observations(now).unwrap(), 0);
        }
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn recurring_container_and_skips_do_not_inflate_completed_work() {
        let mut s = db();
        let root = task(&mut s, "dev", "inbox", None, "Recurring container");
        let leaf = task(&mut s, "dev", "inbox", Some(&root.id), "Recurring work");
        let series = s
            .start_series(
                "dev",
                &root.id,
                1,
                crate::scheduling::RecurrenceSpec {
                    rule: crate::scheduling::Rule::Daily { interval_days: 1 },
                    start_local: "2026-01-05T09:00:00".into(),
                    timezone: "UTC".into(),
                    end_date: None,
                    count: Some(3),
                },
            )
            .unwrap();
        complete(&mut s, "dev", &root.id, at("2026-01-05T10:00:00Z"), true);
        let current = raw_task(&s.conn, &leaf.id).unwrap();
        s.update(
            "dev",
            &leaf.id,
            Change {
                revision: current.revision,
                complete: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        complete(&mut s, "dev", &root.id, at("2026-01-06T10:00:00Z"), true);
        let next = s.occurrences("dev", &series.id).unwrap()[1].task_id.clone();
        let next_task = raw_task(&s.conn, &next).unwrap();
        s.skip_occurrence("dev", &next, next_task.revision, at("2026-01-07T10:00:00Z"))
            .unwrap();
        let value = s
            .weekly_review(
                "dev",
                range("2026-01-01", "2026-01-31"),
                at("2026-01-08T10:00:00Z"),
            )
            .unwrap();
        assert_eq!(value.completed_leaf, 1);
        assert_eq!(
            value.daily.iter().map(|d| d.completed_leaf).sum::<usize>(),
            1
        );
    }
}
