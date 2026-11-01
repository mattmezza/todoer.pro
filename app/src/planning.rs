//! Account intake and optional time placement, sharing task/schedule ACLs.
use crate::core::{Error, Store, Task, access_rank, actor_identity, task, task_row};
use crate::scheduling::{Moment, Schedule};
use chrono::{NaiveDate, NaiveTime, TimeZone};
use rusqlite::params;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize)]
pub struct PlanEntry {
    pub task: Task,
    pub schedule: Schedule,
    pub list: String,
    pub workspace_id: Option<String>,
    pub space: Option<String>,
    pub local_time: Option<String>,
    pub duration_minutes: Option<u32>,
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub revision: i64,
    pub date: Option<String>,
    pub time: Option<String>,
    pub timezone: String,
    pub priority: Option<u8>,
    #[serde(default, deserialize_with = "present_duration")]
    pub duration_minutes: Option<Option<u32>>,
}
pub(crate) fn present_duration<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<u32>>, D::Error> {
    Option::<u32>::deserialize(deserializer).map(Some)
}
// Called only inside an explicit task write (or the development fixture).
pub(crate) fn ensure_personal_inbox(
    c: &rusqlite::Connection,
    actor: &str,
) -> Result<String, Error> {
    let user = crate::core::actor_user(c, actor, 2)?;
    if let Some(id) = c
        .query_row(
            "SELECT list_id FROM user_inboxes WHERE user_id=?1",
            [&user],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    let id = format!("intake:{user}");
    c.execute("INSERT INTO lists(id,title,description,workspace_id) VALUES(?1,'Inbox','Capture first. Plan or file when ready.',NULL)",[&id])?;
    c.execute(
        "INSERT INTO memberships(list_id,user_id) VALUES(?1,?2)",
        params![id, user],
    )?;
    c.execute(
        "INSERT INTO user_inboxes(user_id,list_id) VALUES(?1,?2)",
        params![user, id],
    )?;
    crate::defaults::list(c, &id)?;
    crate::slugs::list(c, &id)?;
    crate::defaults::public_id(c, "list", &id)?;
    Ok(id)
}
impl Store {
    pub fn personal_inbox(&self, actor: &str) -> Result<String, Error> {
        let user = actor_identity(&self.conn, actor)?.0;
        let id: Option<String> = self
            .conn
            .query_row(
                "SELECT list_id FROM user_inboxes WHERE user_id=?1",
                [&user],
                |r| r.get(0),
            )
            .optional()?;
        match id {
            Some(id) => {
                access_rank(&self.conn, actor, &id, 1)?;
                Ok(id)
            }
            None => {
                crate::core::actor_user(&self.conn, actor, 1)?;
                Ok(format!("intake:{user}"))
            }
        }
    }
    pub fn intake_tasks(&self, actor: &str) -> Result<Vec<PlanEntry>, Error> {
        let user = actor_identity(&self.conn, actor)?.0;
        self.entries(actor, None, None, None, Some(&user))
    }
    pub fn inbox_page(
        &self,
        actor: &str,
        before: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<(Vec<PlanEntry>, Option<String>), Error> {
        if !(1..=100).contains(&limit) {
            return Err(Error::Invalid(
                "Inbox page size must be between 1 and 100".into(),
            ));
        }
        let user = actor_identity(&self.conn, actor)?.0;
        let mut query = self.conn.prepare("SELECT t.id,t.reference,t.list_id,t.parent_id,t.title,t.description,t.complete,t.revision,t.position FROM inbox_entries i JOIN tasks t ON t.id=i.task_id JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?1 JOIN lists l ON l.id=t.list_id WHERE i.user_id=?2 AND i.settled_at IS NULL AND t.parent_id IS NULL AND t.deleted_at IS NULL AND l.archived=0 AND (?3 IS NULL OR i.created_at<?3 OR (i.created_at=?3 AND i.rowid<?4)) ORDER BY i.created_at DESC,i.rowid DESC LIMIT ?5")?;
        let mut tasks = query
            .query_map(
                params![
                    actor,
                    user,
                    before.map(|v| v.0),
                    before.map(|v| v.1),
                    limit as i64 + 1
                ],
                task_row,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = tasks.len() > limit;
        if has_more {
            tasks.pop();
        }
        let next = if has_more {
            let last = tasks.last().expect("nonzero inbox page size");
            let (created, row): (i64, i64) = self.conn.query_row(
                "SELECT created_at,rowid FROM inbox_entries WHERE user_id=?1 AND task_id=?2",
                params![user, last.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            Some(format!("{created}:{row}"))
        } else {
            None
        };
        Ok((
            self.decorate_entries(actor, tasks, None, None, Some(&user))?,
            next,
        ))
    }
    pub fn set_inbox_settled(&mut self, actor: &str, id: &str, settled: bool) -> Result<(), Error> {
        let user = actor_identity(&self.conn, actor)?.0;
        let t = task(&self.conn, id)?;
        access_rank(&self.conn, actor, &t.list_id, 2)?;
        let changed = self.conn.execute(
            "UPDATE inbox_entries SET settled_at=CASE WHEN ?3 THEN unixepoch() ELSE NULL END WHERE user_id=?1 AND task_id=?2",
            params![user, t.id, settled],
        )?;
        if changed == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }
    pub fn plan_entries(
        &self,
        actor: &str,
        from: Option<&str>,
        to: Option<&str>,
        list: Option<&str>,
    ) -> Result<Vec<PlanEntry>, Error> {
        self.entries(actor, from, to, list, None)
    }
    fn entries(
        &self,
        actor: &str,
        from: Option<&str>,
        to: Option<&str>,
        list: Option<&str>,
        inbox_user: Option<&str>,
    ) -> Result<Vec<PlanEntry>, Error> {
        if let Some(id) = list {
            access_rank(&self.conn, actor, id, 1)?;
        }
        let mut query = self.conn.prepare("SELECT t.id,t.reference,t.list_id,t.parent_id,t.title,t.description,t.complete,t.revision,t.position FROM tasks t JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?1 JOIN lists l ON l.id=t.list_id WHERE t.deleted_at IS NULL AND l.archived=0 AND (?2 IS NULL OR t.list_id=?2) AND (?5 IS NULL OR (t.parent_id IS NULL AND EXISTS(SELECT 1 FROM inbox_entries i WHERE i.user_id=?5 AND i.task_id=t.id AND i.settled_at IS NULL))) AND (?3 IS NULL OR (json_extract((SELECT document FROM task_schedule WHERE task_id=t.id),'$.planned.date') BETWEEN date(?3,'-1 day') AND date(?4,'+1 day'))) ORDER BY CASE WHEN ?5 IS NOT NULL THEN (SELECT created_at FROM inbox_entries WHERE user_id=?5 AND task_id=t.id) END DESC,CASE WHEN ?5 IS NOT NULL THEN (SELECT rowid FROM inbox_entries WHERE user_id=?5 AND task_id=t.id) END DESC,t.position,t.id LIMIT 10001")?;
        let tasks = query
            .query_map(params![actor, list, from, to, inbox_user], task_row)?
            .collect::<Result<Vec<_>, _>>()?;
        if tasks.len() > 10000 {
            return Err(Error::Invalid(
                "More than 10,000 tasks. Choose a narrower range or scope.".into(),
            ));
        }
        self.decorate_entries(actor, tasks, from, to, inbox_user)
    }
    fn decorate_entries(
        &self,
        actor: &str,
        tasks: Vec<Task>,
        from: Option<&str>,
        to: Option<&str>,
        inbox_user: Option<&str>,
    ) -> Result<Vec<PlanEntry>, Error> {
        let zone: chrono_tz::Tz = self
            .notification_preferences(&actor_identity(&self.conn, actor)?.0)?
            .timezone
            .parse()
            .map_err(|_| Error::Invalid("Invalid timezone".into()))?;
        let mut result = Vec::new();
        for t in tasks {
            let (name,workspace,space): (String,Option<String>,Option<String>) = self.conn.query_row("SELECT l.title,CASE WHEN EXISTS(SELECT 1 FROM actor_workspace_access WHERE actor_id=?1 AND workspace_id=l.workspace_id) THEN l.workspace_id ELSE NULL END,CASE WHEN EXISTS(SELECT 1 FROM actor_workspace_access WHERE actor_id=?1 AND workspace_id=l.workspace_id) THEN w.title ELSE NULL END FROM lists l LEFT JOIN workspaces w ON w.id=l.workspace_id WHERE l.id=?2",params![actor,t.list_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let slot: Option<(String, String)> = self
                .conn
                .query_row(
                    "SELECT local_time,timezone FROM plan_slots WHERE task_id=?1",
                    [&t.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let mut schedule = self.get_schedule(actor, &t.id)?.schedule;
            let mut local_time = slot.as_ref().map(|s| s.0.clone());
            if let (Some((time, source)), Some(Moment::Date { date })) = (&slot, &schedule.planned)
            {
                let source: chrono_tz::Tz = source
                    .parse()
                    .map_err(|_| Error::Invalid("Invalid saved timezone".into()))?;
                let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .map_err(|_| Error::Invalid("Invalid saved date".into()))?;
                let time = NaiveTime::parse_from_str(time, "%H:%M")
                    .map_err(|_| Error::Invalid("Invalid saved time".into()))?;
                let projected = source
                    .from_local_datetime(&date.and_time(time))
                    .single()
                    .ok_or_else(|| Error::Invalid("Invalid saved placement".into()))?
                    .with_timezone(&zone);
                local_time = Some(projected.format("%H:%M").to_string());
                schedule.planned = Some(Moment::Date {
                    date: projected.date_naive().to_string(),
                });
            }
            if let Some(from) = from {
                let Some(Moment::Date { date }) = &schedule.planned else {
                    continue;
                };
                if date.as_str() < from || to.is_some_and(|to| date.as_str() > to) {
                    continue;
                }
            }
            let created_at = if let Some(user) = inbox_user {
                Some(self.conn.query_row(
                    "SELECT created_at FROM inbox_entries WHERE user_id=?1 AND task_id=?2",
                    params![user, t.id],
                    |r| r.get(0),
                )?)
            } else {
                None
            };
            let tags = if inbox_user.is_some() {
                self.task_tags(actor, &t.id)?
                    .into_iter()
                    .map(|tag| tag.name)
                    .collect()
            } else {
                Vec::new()
            };
            let duration_minutes = self
                .conn
                .query_row(
                    "SELECT minutes FROM task_durations WHERE task_id=?1",
                    [&t.id],
                    |r| r.get(0),
                )
                .optional()?;
            result.push(PlanEntry {
                duration_minutes,
                created_at,
                tags,
                schedule,
                task: t,
                list: name,
                workspace_id: workspace,
                space,
                local_time,
                timezone: slot.map(|_| zone.to_string()),
            });
        }
        Ok(result)
    }
    pub fn task_duration(&self, actor: &str, id: &str) -> Result<Option<u32>, Error> {
        self.get_schedule(actor, id)?;
        let id = task(&self.conn, id)?.id;
        Ok(self
            .conn
            .query_row(
                "SELECT minutes FROM task_durations WHERE task_id=?1",
                [&id],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn set_schedule_with_duration(
        &mut self,
        actor: &str,
        id: &str,
        revision: i64,
        schedule: Schedule,
        duration: Option<Option<u32>>,
    ) -> Result<crate::scheduling::ScheduleState, Error> {
        if duration
            .flatten()
            .is_some_and(|minutes| minutes > 5_256_000)
        {
            return Err(Error::Invalid(
                "Duration must be between 0 and 5256000 minutes".into(),
            ));
        }
        let mut write = crate::agent_ops::StoreWrite::new(self)?;
        let result = write.set_schedule(actor, id, revision, schedule)?;
        let task_id = task(&write.conn, id)?.id;
        if let Some(value) = duration {
            if let Some(minutes) = value {
                write.conn.execute("INSERT INTO task_durations(task_id,minutes) VALUES(?1,?2) ON CONFLICT(task_id) DO UPDATE SET minutes=excluded.minutes", params![task_id, minutes])?;
            } else {
                write
                    .conn
                    .execute("DELETE FROM task_durations WHERE task_id=?1", [&task_id])?;
            }
        }
        write.commit()?;
        Ok(result)
    }
    pub fn place_task(
        &mut self,
        actor: &str,
        id: &str,
        placement: Placement,
    ) -> Result<crate::scheduling::ScheduleState, Error> {
        let mut write = crate::agent_ops::StoreWrite::new(self)?;
        let t = task(&write.conn, id)?;
        let mut schedule = write.get_schedule(actor, &t.id)?.schedule;
        let zone: chrono_tz::Tz = placement
            .timezone
            .parse()
            .map_err(|_| Error::Invalid("Choose a valid timezone".into()))?;
        if let Some(priority) = placement.priority {
            schedule.priority = Some(priority);
        }
        schedule.planned = if let Some(date) = placement.date.as_ref() {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|_| Error::Invalid("Choose a valid date".into()))?;
            if let Some(time) = placement.time.as_ref() {
                let time = NaiveTime::parse_from_str(time, "%H:%M")
                    .map_err(|_| Error::Invalid("Choose a valid time".into()))?;
                if zone
                    .from_local_datetime(&date.and_time(time))
                    .single()
                    .is_none()
                {
                    return Err(Error::Invalid(
                        "That time is ambiguous or does not exist. Choose another time.".into(),
                    ));
                }
            }
            Some(Moment::Date {
                date: date.to_string(),
            })
        } else {
            if placement.time.is_some() {
                return Err(Error::Invalid("A time requires a date".into()));
            }
            None
        };
        if placement
            .duration_minutes
            .flatten()
            .is_some_and(|minutes| minutes > 5_256_000)
        {
            return Err(Error::Invalid(
                "Duration must be between 0 and 5256000 minutes".into(),
            ));
        }
        let result = write.set_schedule(actor, &t.id, placement.revision, schedule)?;
        if let Some(duration) = placement.duration_minutes {
            if let Some(minutes) = duration {
                write.conn.execute("INSERT INTO task_durations(task_id,minutes) VALUES(?1,?2) ON CONFLICT(task_id) DO UPDATE SET minutes=excluded.minutes",params![t.id, minutes])?;
            } else {
                write
                    .conn
                    .execute("DELETE FROM task_durations WHERE task_id=?1", [&t.id])?;
            }
        }
        if let (Some(time), Some(_)) = (placement.time, placement.date) {
            write.conn.execute("INSERT INTO plan_slots VALUES(?1,?2,?3) ON CONFLICT(task_id) DO UPDATE SET local_time=excluded.local_time,timezone=excluded.timezone",params![t.id,time,placement.timezone])?;
        } else {
            write
                .conn
                .execute("DELETE FROM plan_slots WHERE task_id=?1", [t.id])?;
        }
        write.commit()?;
        Ok(result)
    }
}
use rusqlite::OptionalExtension;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Capture, NewTask};
    fn setup() -> Store {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        let mut prefs = s.notification_preferences("dev").unwrap();
        prefs.timezone = "Europe/Zurich".into();
        s.set_notification_preferences("dev", prefs).unwrap();
        s
    }
    fn capture(s: &mut Store) -> Task {
        let list = s.personal_inbox("dev").unwrap();
        s.capture(
            "dev",
            "planning-capture",
            Capture {
                list_id: list,
                tasks: vec![NewTask {
                    title: "A small step".into(),
                    description: "One thing at a time".into(),
                    parent_id: None,
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    #[test]
    fn inbox_pages_are_stable_for_shared_timestamps_and_live_changes() {
        let mut s = setup();
        let list = s.personal_inbox("dev").unwrap();
        let created = s
            .capture(
                "dev",
                "paged",
                Capture {
                    list_id: list,
                    tasks: (0..6)
                        .map(|n| NewTask {
                            title: format!("Task {n}"),
                            description: String::new(),
                            parent_id: None,
                        })
                        .collect(),
                },
            )
            .unwrap();
        s.conn
            .execute(
                "UPDATE inbox_entries SET created_at=100 WHERE user_id='dev'",
                [],
            )
            .unwrap();
        let (first, next) = s.inbox_page("dev", None, 2).unwrap();
        assert_eq!(
            first.iter().map(|e| &e.task.id).collect::<Vec<_>>(),
            vec![&created[5].id, &created[4].id]
        );
        let cursor = next.unwrap();
        let (seconds, row) = cursor.split_once(':').unwrap();
        s.set_inbox_settled("dev", &created[3].id, true).unwrap();
        let (second, next) = s
            .inbox_page(
                "dev",
                Some((seconds.parse().unwrap(), row.parse().unwrap())),
                2,
            )
            .unwrap();
        assert_eq!(
            second.iter().map(|e| &e.task.id).collect::<Vec<_>>(),
            vec![&created[2].id, &created[1].id]
        );
        let cursor = next.unwrap();
        let (seconds, row) = cursor.split_once(':').unwrap();
        let (last, next) = s
            .inbox_page(
                "dev",
                Some((seconds.parse().unwrap(), row.parse().unwrap())),
                2,
            )
            .unwrap();
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].task.id, created[0].id);
        assert!(next.is_none());
        assert!(s.inbox_page("other", None, 2).unwrap().0.is_empty());
        assert!(s.inbox_page("dev", None, 0).is_err());
    }
    #[test]
    fn personal_intake_is_isolated_and_requires_explicit_settling() {
        let mut s = setup();
        let inbox = s.personal_inbox("dev").unwrap();
        assert_ne!(inbox, "inbox");
        assert_ne!(inbox, s.personal_inbox("other").unwrap());
        let workspace: Option<String> = s
            .conn
            .query_row(
                "SELECT workspace_id FROM lists WHERE id=?1",
                [&inbox],
                |r| r.get(0),
            )
            .unwrap();
        assert!(workspace.is_none());
        let t = capture(&mut s);
        assert_eq!(s.intake_tasks("dev").unwrap().len(), 1);
        assert!(s.intake_tasks("other").unwrap().is_empty());
        assert!(
            s.place_task(
                "other",
                &t.id,
                Placement {
                    duration_minutes: None,
                    priority: None,
                    revision: t.revision,
                    date: Some("2026-10-28".into()),
                    time: None,
                    timezone: "Europe/Zurich".into()
                }
            )
            .is_err()
        );
        let saved = s
            .place_task(
                "dev",
                &t.id,
                Placement {
                    duration_minutes: None,
                    priority: None,
                    revision: t.revision,
                    date: Some("2026-10-28".into()),
                    time: Some("10:30".into()),
                    timezone: "Europe/Zurich".into(),
                },
            )
            .unwrap();
        assert_eq!(s.intake_tasks("dev").unwrap().len(), 1);
        assert!(s.set_inbox_settled("other", &t.id, true).is_err());
        s.set_inbox_settled("dev", &t.id, true).unwrap();
        assert!(s.intake_tasks("dev").unwrap().is_empty());
        s.set_inbox_settled("dev", &t.id, false).unwrap();
        assert_eq!(s.intake_tasks("dev").unwrap().len(), 1);
        let plan = s
            .plan_entries("dev", Some("2026-10-28"), Some("2026-10-28"), None)
            .unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].local_time.as_deref(), Some("10:30"));
        assert!(
            s.place_task(
                "dev",
                &t.id,
                Placement {
                    duration_minutes: None,
                    priority: None,
                    revision: t.revision,
                    date: None,
                    time: None,
                    timezone: "Europe/Zurich".into()
                }
            )
            .is_err()
        );
        s.place_task(
            "dev",
            &t.id,
            Placement {
                duration_minutes: None,
                priority: None,
                revision: saved.task_revision,
                date: None,
                time: None,
                timezone: "Europe/Zurich".into(),
            },
        )
        .unwrap();
        assert_eq!(s.intake_tasks("dev").unwrap().len(), 1);
    }
    #[test]
    fn captured_tasks_in_other_lists_stay_in_the_creators_inbox() {
        let mut s = setup();
        let task = s
            .capture(
                "dev",
                "filed-capture",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![NewTask {
                        title: "Filed from the start".into(),
                        description: "Details".into(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        s.set_task_tags(
            "dev",
            &task.id,
            task.revision,
            vec![crate::taxonomy::TagSelector::Name("errands".into())],
        )
        .unwrap();
        let entries = s.intake_tasks("dev").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tags, vec!["errands"]);
        assert_eq!(entries[0].task.id, task.id);
        assert!(entries[0].created_at.is_some());
        let child = s
            .capture(
                "dev",
                "child-capture",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![NewTask {
                        title: "A subtask".into(),
                        description: String::new(),
                        parent_id: Some(task.id.clone()),
                    }],
                },
            )
            .unwrap()
            .remove(0);
        s.conn
            .execute(
                "INSERT INTO inbox_entries(user_id,task_id) VALUES(?1,?2)",
                params!["dev", child.id],
            )
            .unwrap();
        assert_eq!(s.intake_tasks("dev").unwrap().len(), 1);
        assert_eq!(s.inbox_page("dev", None, 20).unwrap().0.len(), 1);
        assert!(s.intake_tasks("other").unwrap().is_empty());
        s.set_inbox_settled("dev", &task.id, true).unwrap();
        assert_eq!(s.intake_tasks("dev").unwrap().len(), 0);
        assert_eq!(s.tasks("dev", "inbox").unwrap().len(), 2);
    }
    #[test]
    fn migration_preserves_only_previous_intake_items() {
        let mut s = setup();
        let old_inbox_task = capture(&mut s);
        let filed = s
            .capture(
                "dev",
                "old-filed",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![NewTask {
                        title: "Already filed".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        s.conn.execute_batch("DROP TABLE inbox_entries").unwrap();
        s.conn
            .execute_batch(include_str!("../migrations/049_inbox_stream.sql"))
            .unwrap();
        let entries = s.intake_tasks("dev").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].task.id, old_inbox_task.id);
        assert_ne!(entries[0].task.id, filed.id);
    }
    #[test]
    fn duration_migration_preserves_existing_values_and_accepts_zero_and_days() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE tasks(id TEXT PRIMARY KEY); INSERT INTO tasks VALUES ('old'),('immediate'),('long');").unwrap();
        conn.execute_batch(include_str!("../migrations/052_task_durations.sql"))
            .unwrap();
        conn.execute("INSERT INTO task_durations VALUES('old',120)", [])
            .unwrap();
        conn.execute_batch(include_str!("../migrations/053_duration_range.sql"))
            .unwrap();
        conn.execute(
            "INSERT INTO task_durations VALUES('immediate',0),('long',2880)",
            [],
        )
        .unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT minutes FROM task_durations WHERE task_id='old'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
            120
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM task_durations", [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            3
        );
    }
    #[test]
    fn duration_persists_across_replanning_and_unplanning_and_clears_explicitly() {
        let mut s = setup();
        let task = capture(&mut s);
        let parse = |revision, date: Option<&str>, duration: serde_json::Value| -> Placement {
            serde_json::from_value(serde_json::json!({"revision":revision,"date":date,"time":null,"timezone":"Europe/Zurich","duration_minutes":duration})).unwrap()
        };
        let saved = s
            .place_task(
                "dev",
                &task.id,
                parse(task.revision, Some("2026-10-28"), serde_json::json!(120)),
            )
            .unwrap();
        assert_eq!(
            s.intake_tasks("dev").unwrap()[0].duration_minutes,
            Some(120)
        );
        let moved: Placement = serde_json::from_value(serde_json::json!({"revision":saved.task_revision,"date":"2026-10-29","time":null,"timezone":"Europe/Zurich"})).unwrap();
        let saved = s.place_task("dev", &task.id, moved).unwrap();
        assert_eq!(
            s.intake_tasks("dev").unwrap()[0].duration_minutes,
            Some(120)
        );
        assert!(
            s.place_task(
                "dev",
                &task.id,
                parse(
                    saved.task_revision,
                    Some("2026-10-29"),
                    serde_json::json!(5_256_001)
                )
            )
            .is_err()
        );
        let saved = s
            .place_task(
                "dev",
                &task.id,
                parse(
                    saved.task_revision,
                    Some("2026-10-29"),
                    serde_json::Value::Null,
                ),
            )
            .unwrap();
        assert_eq!(s.intake_tasks("dev").unwrap()[0].duration_minutes, None);
        let saved = s
            .place_task(
                "dev",
                &task.id,
                parse(
                    saved.task_revision,
                    Some("2026-10-29"),
                    serde_json::json!(60),
                ),
            )
            .unwrap();
        s.place_task(
            "dev",
            &task.id,
            serde_json::from_value(serde_json::json!({"revision":saved.task_revision,"date":null,"time":null,"timezone":"Europe/Zurich"})).unwrap(),
        )
        .unwrap();
        assert_eq!(s.intake_tasks("dev").unwrap()[0].duration_minutes, Some(60));
    }
    #[test]
    fn nonexistent_or_ambiguous_clock_time_does_not_change_task() {
        let mut s = setup();
        let t = capture(&mut s);
        for (date, time) in [("2026-03-29", "02:30"), ("2026-10-25", "02:30")] {
            assert!(
                s.place_task(
                    "dev",
                    &t.id,
                    Placement {
                        duration_minutes: None,
                        priority: None,
                        revision: t.revision,
                        date: Some(date.into()),
                        time: Some(time.into()),
                        timezone: "Europe/Zurich".into()
                    }
                )
                .is_err()
            );
        }
        assert_eq!(
            s.get_schedule("dev", &t.id).unwrap().task_revision,
            t.revision
        );
        assert_eq!(s.intake_tasks("dev").unwrap().len(), 1);
    }
    #[test]
    fn reports_include_authorized_subtree_and_all_formats() {
        let mut s = setup();
        let t = capture(&mut s);
        let scope = crate::reports::ReportScope {
            kind: "task".into(),
            id: Some(t.id.clone()),
            ..Default::default()
        };
        let report = s.readable_report("dev", &scope).unwrap();
        assert_eq!(report.tasks.len(), 1);
        assert!(s.readable_report("other", &scope).is_err());
        assert!(crate::reports::text(&report).contains("[ ] A small step"));
        assert!(
            crate::reports::delimited(&report, b',')
                .unwrap()
                .starts_with(b"reference,")
        );
        assert!(
            crate::reports::delimited(&report, b'\t')
                .unwrap()
                .starts_with(b"reference\t")
        );
        assert!(crate::reports::pdf(&report).unwrap().starts_with(b"%PDF-"));
    }
}
