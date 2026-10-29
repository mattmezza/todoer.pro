//! Durable per-user inbox. Identity keys are independent of transport retries.
use crate::core::{Error, Store, Task, actor_user};
use crate::notifications::Preferences;
use crate::scheduling::{Moment, Schedule};
use chrono::{NaiveDate, NaiveDateTime, TimeZone};
use chrono_tz::Tz;
use rusqlite::{Connection, params};
use serde::Serialize;

fn prefs(c: &Connection, user: &str) -> Result<Preferences, Error> {
    let json: String = c.query_row(
        "SELECT COALESCE((SELECT document FROM notification_preferences WHERE user_id=?1),'{}')",
        [user],
        |r| r.get(0),
    )?;
    serde_json::from_str(&json)
        .map_err(|_| Error::Invalid("Invalid notification preferences".into()))
}
fn enabled(p: &Preferences, kind: &str, list: &str) -> bool {
    !p.list_opt_out.iter().any(|v| v == list)
        && match kind {
            "due" => p.inbox_due,
            "reminder" => p.inbox_reminders,
            _ => p.inbox_updates,
        }
}
#[allow(clippy::too_many_arguments)] // Mirrors the notification row fields.
fn insert(
    c: &Connection,
    user: &str,
    task: &str,
    list: &str,
    kind: &str,
    key: &str,
    source: &str,
    at: i64,
) -> Result<(), Error> {
    let p = prefs(c, user)?;
    if !enabled(&p, kind, list) {
        return Ok(());
    }
    let changed = c.execute("INSERT OR IGNORE INTO notification_inbox(user_id,task_id,kind,event_key,source,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![user,task,kind,key,source,at])?;
    // Explicit reminders already have channel-specific delivery jobs. Do not
    // send a second push for the inbox copy of the same reminder.
    if changed == 1 && kind != "reminder" {
        let id = c.last_insert_rowid();
        c.execute("INSERT INTO notification_push_outbox(id,notification_id,user_id,available_at) SELECT ?1,?2,?3,?4 WHERE EXISTS(SELECT 1 FROM push_preferences WHERE user_id=?3 AND opt_in=1)",params![format!("inbox:{id}"),id,user,at])?;
    }
    Ok(())
}
pub(crate) fn record_change(
    c: &Connection,
    t: &Task,
    kind: &str,
    actor: &str,
    seq: i64,
) -> Result<(), Error> {
    if !matches!(kind, "update" | "undo" | "create" | "move" | "schedule") {
        return Ok(());
    }
    let users = c.prepare("SELECT DISTINCT a.user_id FROM effective_list_access a JOIN lists l ON l.id=a.list_id LEFT JOIN workspace_lifecycle w ON w.workspace_id=l.workspace_id WHERE a.list_id=?1 AND a.rank>=1 AND a.user_id<>?2 AND l.archived=0 AND COALESCE(w.archived,0)=0 AND w.deleted_at IS NULL")?.query_map(params![t.list_id,actor],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
    for user in users {
        insert(
            c,
            &user,
            &t.id,
            &t.list_id,
            "update",
            &format!("history:{seq}"),
            actor,
            chrono::Utc::now().timestamp(),
        )?;
    }
    Ok(())
}
#[derive(Debug, Serialize)]
pub struct Notice {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub task_reference: String,
    pub list_title: String,
    pub actor_name: String,
    pub created_at: i64,
    pub unread: bool,
}
#[derive(Debug, Serialize)]
pub struct Inbox {
    pub items: Vec<Notice>,
    pub unread: i64,
    pub latest_id: i64,
    pub next_cursor: Option<i64>,
}
// Live access, lifecycle and schedule checks apply equally to badges, pages and
// push. No historical task titles survive a permission revocation.
const VISIBLE: &str = "FROM notification_inbox n JOIN tasks t ON t.id=n.task_id JOIN lists l ON l.id=t.list_id JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=n.user_id LEFT JOIN workspace_lifecycle w ON w.workspace_id=l.workspace_id LEFT JOIN task_schedule s ON s.task_id=t.id WHERE n.user_id=?1 AND a.rank>=1 AND t.deleted_at IS NULL AND l.archived=0 AND COALESCE(w.archived,0)=0 AND w.deleted_at IS NULL AND (n.kind='update' OR t.complete=0) AND (n.kind<>'due' OR n.source=COALESCE(json_extract(s.document,'$.due'),'null')) AND (n.kind<>'reminder' OR EXISTS(SELECT 1 FROM reminder_outbox r WHERE r.task_id=t.id AND r.recipient=n.user_id AND r.schedule_revision=s.revision AND r.at=CAST(n.source AS INTEGER)))";
impl Store {
    pub fn notification_inbox(
        &self,
        actor: &str,
        before: Option<i64>,
        unread_only: bool,
        limit: usize,
    ) -> Result<Inbox, Error> {
        let user = actor_user(&self.conn, actor, 1)?;
        let limit = limit.clamp(1, 50);
        let (unread, latest_id) = self.conn.query_row(
            &format!("SELECT COALESCE(SUM(n.read_at IS NULL),0),COALESCE(MAX(n.id),0) {VISIBLE}"),
            [&user],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut q = self.conn.prepare(&format!("SELECT n.id,n.kind,t.title,COALESCE((SELECT short_id FROM task_public_ids WHERE task_id=t.id),t.id),l.title,n.created_at,n.read_at IS NULL,COALESCE((SELECT name FROM users WHERE id=n.source AND n.kind='update'),'') {VISIBLE} AND (?2 IS NULL OR n.id<?2) AND (?3=0 OR n.read_at IS NULL) ORDER BY n.id DESC LIMIT ?4"))?;
        let mut items = q
            .query_map(params![user, before, unread_only, limit as i64 + 1], |r| {
                Ok(Notice {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    title: r.get(2)?,
                    task_reference: r.get(3)?,
                    list_title: r.get(4)?,
                    created_at: r.get(5)?,
                    unread: r.get(6)?,
                    actor_name: r.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > limit;
        items.truncate(limit);
        Ok(Inbox {
            next_cursor: if more {
                items.last().map(|v| v.id)
            } else {
                None
            },
            items,
            unread,
            latest_id,
        })
    }
    pub fn read_notifications(
        &mut self,
        actor: &str,
        through: i64,
        only: Option<i64>,
    ) -> Result<(), Error> {
        let user = actor_user(&self.conn, actor, 3)?;
        self.conn.execute("UPDATE notification_inbox SET read_at=COALESCE(read_at,unixepoch()) WHERE user_id=?1 AND id<=?2 AND (?3 IS NULL OR id=?3)",params![user,through,only])?;
        Ok(())
    }
    pub fn queue_inbox_notifications(&mut self, now: i64) -> Result<(), Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let rows = tx.prepare("SELECT t.id,t.list_id,s.document,a.user_id FROM task_schedule s JOIN tasks t ON t.id=s.task_id JOIN lists l ON l.id=t.list_id JOIN effective_list_access a ON a.list_id=t.list_id LEFT JOIN workspace_lifecycle w ON w.workspace_id=l.workspace_id WHERE t.complete=0 AND t.deleted_at IS NULL AND l.archived=0 AND a.rank>=1 AND COALESCE(w.archived,0)=0 AND w.deleted_at IS NULL AND json_extract(s.document,'$.due') IS NOT NULL")?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?.collect::<Result<Vec<_>,_>>()?;
        for (task, list, json, user) in rows {
            let schedule: Schedule = serde_json::from_str(&json)
                .map_err(|_| Error::Invalid("Invalid schedule".into()))?;
            let Some(due) = schedule.due else { continue };
            let p = prefs(&tx, &user)?;
            let deadline = match &due {
                Moment::Date { date } => NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .ok()
                    .and_then(|d| d.succ_opt())
                    .and_then(|d| {
                        p.timezone.parse::<Tz>().ok().and_then(|tz| {
                            tz.from_local_datetime(&d.and_hms_opt(0, 0, 0)?).earliest()
                        })
                    })
                    .map(|d| d.timestamp()),
                Moment::Timed { local, timezone } => {
                    NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M:%S")
                        .ok()
                        .and_then(|d| {
                            timezone
                                .parse::<Tz>()
                                .ok()
                                .and_then(|tz| tz.from_local_datetime(&d).earliest())
                        })
                        .map(|d| d.timestamp())
                }
            };
            if let Some(at) = deadline.filter(|at| *at <= now) {
                let source = serde_json::to_string(&due).unwrap();
                // SQLite's json_extract compacts JSON too; serde field order is
                // the stored Schedule order for Moment.
                insert(
                    &tx,
                    &user,
                    &task,
                    &list,
                    "due",
                    &format!("due:{task}:{source}"),
                    &source,
                    at,
                )?;
            }
        }
        let rows=tx.prepare("SELECT DISTINCT r.task_id,t.list_id,r.recipient,r.at FROM reminder_outbox r JOIN tasks t ON t.id=r.task_id JOIN lists l ON l.id=t.list_id JOIN task_schedule s ON s.task_id=t.id JOIN effective_list_access a ON a.list_id=t.list_id AND a.user_id=r.recipient LEFT JOIN workspace_lifecycle w ON w.workspace_id=l.workspace_id WHERE r.at<=?1 AND r.schedule_revision=s.revision AND t.complete=0 AND t.deleted_at IS NULL AND a.rank>=1 AND l.archived=0 AND COALESCE(w.archived,0)=0 AND w.deleted_at IS NULL")?.query_map([now],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?)))?.collect::<Result<Vec<_>,_>>()?;
        for (task, list, user, at) in rows {
            insert(
                &tx,
                &user,
                &task,
                &list,
                "reminder",
                &format!("reminder:{task}:{at}"),
                &at.to_string(),
                at,
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn inbox_push_live(
        &self,
        id: &str,
        user: &str,
        owner: &str,
        token: &str,
        now: i64,
    ) -> Result<bool, Error> {
        Ok(self.conn.query_row(&format!("SELECT EXISTS(SELECT 1 {VISIBLE} AND n.read_at IS NULL AND EXISTS(SELECT 1 FROM notification_push_outbox o WHERE o.notification_id=n.id AND o.id=?2 AND o.status='leased' AND o.lease_until>?3 AND o.lease_owner=?4 AND o.lease_token=?5) AND EXISTS(SELECT 1 FROM push_preferences WHERE user_id=?1 AND opt_in=1))"),params![user,id,now,owner,token],|r|r.get(0))? && {
            let list:String=self.conn.query_row("SELECT t.list_id FROM notification_push_outbox o JOIN notification_inbox n ON n.id=o.notification_id JOIN tasks t ON t.id=n.task_id WHERE o.id=?1",[id],|r|r.get(0))?;
            let kind:String=self.conn.query_row("SELECT n.kind FROM notification_push_outbox o JOIN notification_inbox n ON n.id=o.notification_id WHERE o.id=?1",[id],|r|r.get(0))?;
            enabled(&prefs(&self.conn,user)?,&kind,&list)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Capture, Change, NewTask};
    use crate::scheduling::{Channel, Reminder};
    fn db() -> Store {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        s
    }
    fn task(s: &mut Store) -> Task {
        s.capture(
            "dev",
            "inbox-test",
            Capture {
                list_id: "inbox".into(),
                tasks: vec![NewTask {
                    title: "Buy milk".into(),
                    description: String::new(),
                    parent_id: None,
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    #[test]
    fn due_timezone_completion_and_deadline_identity() {
        let mut s = db();
        let t = task(&mut s);
        let mut p = s.notification_preferences("dev").unwrap();
        p.timezone = "Europe/Zurich".into();
        s.set_notification_preferences("dev", p).unwrap();
        let due = Moment::Date {
            date: "2026-10-28".into(),
        };
        s.set_schedule(
            "dev",
            &t.id,
            t.revision,
            Schedule {
                due: Some(due.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        let before = chrono::DateTime::parse_from_rfc3339("2026-10-28T22:59:59Z")
            .unwrap()
            .timestamp();
        s.queue_inbox_notifications(before).unwrap();
        assert_eq!(
            s.notification_inbox("dev", None, false, 25).unwrap().unread,
            0
        );
        s.queue_inbox_notifications(before + 1).unwrap();
        s.queue_inbox_notifications(before + 1).unwrap();
        let inbox = s.notification_inbox("dev", None, false, 25).unwrap();
        assert_eq!(inbox.unread, 1);
        assert_eq!(inbox.items[0].kind, "due");
        s.read_notifications("dev", inbox.latest_id, None).unwrap();
        let t = crate::core::task(&s.conn, &t.id).unwrap();
        s.set_schedule(
            "dev",
            &t.id,
            t.revision,
            Schedule {
                due: Some(due),
                priority: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        s.queue_inbox_notifications(before + 2).unwrap();
        assert_eq!(
            s.notification_inbox("dev", None, false, 25).unwrap().unread,
            0
        );
        let t = crate::core::task(&s.conn, &t.id).unwrap();
        s.update(
            "dev",
            &t.id,
            Change {
                revision: t.revision,
                complete: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            s.notification_inbox("dev", None, false, 25)
                .unwrap()
                .items
                .is_empty()
        );
    }
    #[test]
    fn channels_share_one_reminder_and_stale_schedules_disappear() {
        let mut s = db();
        let t = task(&mut s);
        let at = Moment::Timed {
            local: "2026-10-28T12:00:00".into(),
            timezone: "UTC".into(),
        };
        let schedule = Schedule {
            reminders: vec![
                Reminder {
                    channel: Channel::Push,
                    at: at.clone(),
                },
                Reminder {
                    channel: Channel::Email,
                    at,
                },
            ],
            ..Default::default()
        };
        s.set_schedule("dev", &t.id, t.revision, schedule).unwrap();
        s.queue_inbox_notifications(1800000000).unwrap();
        s.queue_inbox_notifications(1800000000).unwrap();
        s.conn
            .execute("UPDATE reminder_outbox SET status='cancelled'", [])
            .unwrap();
        // Disabling a transport must never remove the in-app reminder.
        let inbox = s.notification_inbox("dev", None, false, 25).unwrap();
        assert_eq!(inbox.unread, 1);
        assert_eq!(inbox.items[0].kind, "reminder");
        assert_eq!(
            s.conn
                .query_row("SELECT count(*) FROM notification_push_outbox", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
        let t = crate::core::task(&s.conn, &t.id).unwrap();
        s.set_schedule("dev", &t.id, t.revision, Schedule::default())
            .unwrap();
        assert!(
            s.notification_inbox("dev", None, false, 25)
                .unwrap()
                .items
                .is_empty()
        );
    }
    #[test]
    fn shared_changes_are_transactional_private_paginated_and_user_scoped() {
        let mut s = db();
        let t = task(&mut s);
        s.conn
            .execute("INSERT INTO memberships VALUES('inbox','other')", [])
            .unwrap();
        let mut t = t;
        for i in 0..4 {
            t = s
                .update(
                    "dev",
                    &t.id,
                    Change {
                        revision: t.revision,
                        title: Some(format!("Change {i}")),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        assert_eq!(
            s.notification_inbox("dev", None, false, 25).unwrap().unread,
            0
        );
        let first = s.notification_inbox("other", None, false, 2).unwrap();
        assert_eq!(first.unread, 4);
        assert_eq!(first.items.len(), 2);
        let next = s
            .notification_inbox("other", first.next_cursor, false, 2)
            .unwrap();
        assert_eq!(next.items.len(), 2);
        assert!(next.items[0].id < first.items[1].id);
        s.read_notifications("dev", first.latest_id, None).unwrap();
        assert_eq!(
            s.notification_inbox("other", None, false, 25)
                .unwrap()
                .unread,
            4
        );
        t = s
            .update(
                "dev",
                &t.id,
                Change {
                    revision: t.revision,
                    description: Some("Later update".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        s.read_notifications("other", first.latest_id, None)
            .unwrap();
        assert_eq!(
            s.notification_inbox("other", None, true, 25)
                .unwrap()
                .unread,
            1
        );
        s.conn
            .execute(
                "DELETE FROM memberships WHERE list_id='inbox' AND user_id='other'",
                [],
            )
            .unwrap();
        assert!(
            s.notification_inbox("other", None, false, 25)
                .unwrap()
                .items
                .is_empty()
        );
        assert!(!t.id.is_empty());
    }
    #[test]
    fn mutes_and_category_preferences_prevent_new_notifications() {
        let mut s = db();
        s.conn
            .execute("INSERT INTO memberships VALUES('inbox','other')", [])
            .unwrap();
        let mut p = s.notification_preferences("other").unwrap();
        p.inbox_updates = false;
        s.set_notification_preferences("other", p).unwrap();
        let t = task(&mut s);
        assert_eq!(
            s.notification_inbox("other", None, false, 25)
                .unwrap()
                .unread,
            0
        );
        let mut p = s.notification_preferences("dev").unwrap();
        p.list_opt_out = vec!["inbox".into()];
        s.set_notification_preferences("dev", p).unwrap();
        s.set_schedule(
            "dev",
            &t.id,
            t.revision,
            Schedule {
                due: Some(Moment::Timed {
                    local: "2026-10-28T12:00:00".into(),
                    timezone: "UTC".into(),
                }),
                ..Default::default()
            },
        )
        .unwrap();
        s.queue_inbox_notifications(1800000000).unwrap();
        assert_eq!(
            s.notification_inbox("dev", None, false, 25).unwrap().unread,
            0
        );
    }
}
