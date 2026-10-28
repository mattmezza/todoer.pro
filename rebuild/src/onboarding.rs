//! A small resumable welcome over the normal account destinations and mutations.
use crate::core::{Capture, Error, NewTask, Store, actor_user};
use crate::scheduling::{Moment, Schedule};
use crate::workspace::{ListChange, WorkspaceChange};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct Welcome {
    pub status: String,
    pub step: i64,
    pub revision: i64,
    pub space: String,
    pub list: String,
    pub color: String,
    pub name: String,
    pub destination: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    pub revision: i64,
    pub space: String,
    pub list: String,
    pub color: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finish {
    pub revision: i64,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub today: bool,
}

impl Store {
    pub fn welcome_pending(&self, actor: &str) -> Result<bool, Error> {
        let user = actor_user(&self.conn, actor, 3)?;
        Ok(self
            .conn
            .query_row(
                "SELECT status='pending' FROM user_onboarding WHERE user_id=?1",
                [user],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(false))
    }
    pub fn welcome(&mut self, actor: &str) -> Result<Welcome, Error> {
        let user = actor_user(&self.conn, actor, 3)?;
        let workspace = self.ensure_user_defaults(actor)?;
        let list = self.default_list(actor, &workspace)?;
        let space = self.workspace(actor, &workspace)?;
        let info = self.list_info(actor, &list)?;
        let (status, step, revision, destination) = self.conn.query_row(
            "SELECT status,step,revision,destination FROM user_onboarding WHERE user_id=?1",
            [&user],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let name = self
            .conn
            .query_row("SELECT name FROM users WHERE id=?1", [user], |r| r.get(0))?;
        let color = self.conn.query_row(
            "SELECT coalesce(color_hex,color,'blue') FROM lists WHERE id=?1",
            [list],
            |r| r.get(0),
        )?;
        Ok(Welcome {
            status,
            step,
            revision,
            destination,
            space: space.title,
            list: info.list.title,
            color,
            name,
        })
    }
    pub fn welcome_setup(&mut self, actor: &str, input: Setup) -> Result<Welcome, Error> {
        let mut write = crate::agent_ops::StoreWrite::new(self)?;
        let current = write.welcome(actor)?;
        if current.revision != input.revision {
            return Err(Error::Conflict(
                "Welcome setup changed in another tab. Reload to continue.".into(),
            ));
        }
        if input.space.trim().is_empty()
            || input.list.trim().is_empty()
            || input.space.chars().count() > 100
            || input.list.chars().count() > 100
        {
            return Err(Error::Invalid(
                "Choose a space and list name between 1 and 100 characters.".into(),
            ));
        }
        crate::navigation::validate_list_color(&Some(input.color.clone()))?;
        let user = actor_user(&write.conn, actor, 3)?;
        let workspace = write.ensure_user_defaults(actor)?;
        let list = write.default_list(actor, &workspace)?;
        // Never rename an inherited/shared destination through the welcome.
        if write.workspace_permission(actor, &workspace)? < 3 {
            return Err(Error::Forbidden);
        }
        let space = write.workspace(actor, &workspace)?;
        let info = write.list_info(actor, &list)?;
        write.update_workspace(
            actor,
            &workspace,
            WorkspaceChange {
                revision: space.revision,
                title: Some(input.space),
                description: None,
            },
        )?;
        write.update_list(
            actor,
            &list,
            ListChange {
                revision: info.list.revision,
                title: Some(input.list),
                description: None,
                archived: None,
                color: Some(input.color),
            },
        )?;
        let run = if current.status == "pending" {
            write.conn.query_row(
                "SELECT run_id FROM user_onboarding WHERE user_id=?1",
                [&user],
                |r| r.get::<_, String>(0),
            )?
        } else {
            uuid::Uuid::new_v4().to_string()
        };
        write.conn.execute("UPDATE user_onboarding SET status='pending',step=1,revision=revision+1,run_id=?2 WHERE user_id=?1",params![user,run])?;
        let result = write.welcome(actor)?;
        write.commit()?;
        Ok(result)
    }
    pub fn welcome_finish(
        &mut self,
        actor: &str,
        input: Finish,
        skip: bool,
    ) -> Result<String, Error> {
        let mut write = crate::agent_ops::StoreWrite::new(self)?;
        let current = write.welcome(actor)?;
        // A lost response can be retried without creating a second first task.
        if current.status != "pending" {
            return Ok(current.destination);
        }
        if current.revision != input.revision {
            return Err(Error::Conflict(
                "Welcome setup changed in another tab. Reload to continue.".into(),
            ));
        }
        if input.task.chars().count() > 500 {
            return Err(Error::Invalid(
                "Keep the task name under 500 characters.".into(),
            ));
        }
        let user = actor_user(&write.conn, actor, 3)?;
        let workspace = write.ensure_user_defaults(actor)?;
        let list = write.default_list(actor, &workspace)?;
        let reference = crate::defaults::public_id(&write.conn, "list", &list)?;
        let mut destination = format!("/lists/{reference}");
        if skip && current.step == 0 {
            destination = "/inbox".into();
        }
        if !skip && !input.task.trim().is_empty() {
            let run: String = write.conn.query_row(
                "SELECT run_id FROM user_onboarding WHERE user_id=?1",
                [&user],
                |r| r.get(0),
            )?;
            let task = write
                .capture(
                    actor,
                    &format!("welcome:{run}"),
                    Capture {
                        list_id: list,
                        tasks: vec![NewTask {
                            title: input.task.trim().into(),
                            description: String::new(),
                            parent_id: None,
                        }],
                    },
                )?
                .remove(0);
            if input.today {
                let prefs = write.notification_preferences(actor)?;
                let timezone = prefs
                    .timezone
                    .parse::<chrono_tz::Tz>()
                    .map_err(|_| Error::Invalid("Invalid account timezone".into()))?;
                let date = chrono::Utc::now()
                    .with_timezone(&timezone)
                    .date_naive()
                    .to_string();
                write.set_schedule(
                    actor,
                    &task.id,
                    task.revision,
                    Schedule {
                        planned: Some(Moment::Date { date }),
                        priority: Some(3),
                        ..Default::default()
                    },
                )?;
                destination = "/today".into();
            }
        }
        write.conn.execute("UPDATE user_onboarding SET status=?2,step=2,revision=revision+1,destination=?3 WHERE user_id=?1",params![user,if skip {"skipped"} else {"complete"},destination])?;
        write.commit()?;
        Ok(destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn account() -> Store {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db
    }
    fn setup(revision: i64) -> Setup {
        Setup {
            revision,
            space: "My life".into(),
            list: "Little things".into(),
            color: "violet".into(),
        }
    }
    #[test]
    fn welcome_reuses_defaults_resumes_and_can_be_skipped() {
        let mut db = account();
        assert!(db.welcome_pending("dev").unwrap());
        let initial = db.welcome("dev").unwrap();
        let workspace = db.ensure_user_defaults("dev").unwrap();
        let list = db.default_list("dev", &workspace).unwrap();
        let count: i64 = db
            .conn
            .query_row("SELECT count(*) FROM workspaces", [], |r| r.get(0))
            .unwrap();
        let saved = db.welcome_setup("dev", setup(initial.revision)).unwrap();
        assert_eq!(saved.step, 1);
        assert_eq!(saved.space, "My life");
        assert_eq!(db.ensure_user_defaults("dev").unwrap(), workspace);
        assert_eq!(db.default_list("dev", &workspace).unwrap(), list);
        assert_eq!(
            db.conn
                .query_row("SELECT count(*) FROM workspaces", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            count
        );
        assert_eq!(db.welcome("dev").unwrap().revision, saved.revision);
        db.welcome_finish(
            "dev",
            Finish {
                revision: saved.revision,
                task: "".into(),
                today: false,
            },
            true,
        )
        .unwrap();
        assert!(!db.welcome_pending("dev").unwrap());
        assert_eq!(db.welcome("dev").unwrap().status, "skipped");
    }
    #[test]
    fn first_task_is_atomic_planned_and_replay_safe() {
        let mut db = account();
        let initial = db.welcome("dev").unwrap();
        let saved = db.welcome_setup("dev", setup(initial.revision)).unwrap();
        let finish = || Finish {
            revision: saved.revision,
            task: "Cook carbonara".into(),
            today: true,
        };
        assert_eq!(db.welcome_finish("dev", finish(), false).unwrap(), "/today");
        assert_eq!(db.welcome_finish("dev", finish(), false).unwrap(), "/today");
        let workspace = db.ensure_user_defaults("dev").unwrap();
        let list = db.default_list("dev", &workspace).unwrap();
        let tasks = db.tasks("dev", &list).unwrap();
        assert_eq!(tasks.len(), 1);
        let schedule = db.get_schedule("dev", &tasks[0].id).unwrap();
        assert_eq!(schedule.schedule.priority, Some(3));
        assert!(schedule.schedule.planned.is_some());
        assert!(!db.welcome_pending("dev").unwrap());
        let revision = db.welcome("dev").unwrap().revision;
        let reopened = db.welcome_setup("dev", setup(revision)).unwrap();
        db.welcome_finish(
            "dev",
            Finish {
                revision: reopened.revision,
                task: "".into(),
                today: false,
            },
            false,
        )
        .unwrap();
        assert_eq!(db.tasks("dev", &list).unwrap().len(), 1);
    }
    #[test]
    fn validation_and_stale_tabs_do_not_partially_change_setup() {
        let mut db = account();
        let initial = db.welcome("dev").unwrap();
        let mut bad = setup(initial.revision);
        bad.color = "not-a-color".into();
        assert!(db.welcome_setup("dev", bad).is_err());
        assert_eq!(db.welcome("dev").unwrap().space, initial.space);
        db.welcome_setup("dev", setup(initial.revision)).unwrap();
        assert!(db.welcome_setup("dev", setup(initial.revision)).is_err());
        assert_eq!(db.welcome("dev").unwrap().revision, initial.revision + 1);
        assert!(db.welcome("missing-user").is_err());
    }
}
