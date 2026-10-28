//! A small resumable welcome over the normal account destinations and mutations.
use crate::core::{Error, Store, actor_user};
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
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finish {
    pub revision: i64,
    #[serde(default)]
    pub list: String,
    #[serde(default)]
    pub color: String,
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
        let title = welcome_name(&input.space)?;
        let user = actor_user(&write.conn, actor, 3)?;
        let workspace = write.ensure_user_defaults(actor)?;
        // Never rename an inherited/shared destination through the welcome.
        if write.workspace_permission(actor, &workspace)? < 3 {
            return Err(Error::Forbidden);
        }
        let space = write.workspace(actor, &workspace)?;
        write.update_workspace(
            actor,
            &workspace,
            WorkspaceChange {
                revision: space.revision,
                title: Some(title),
                description: None,
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
        // A lost response can be retried without repeating mutations.
        if current.status != "pending" {
            return Ok(current.destination);
        }
        if current.revision != input.revision {
            return Err(Error::Conflict(
                "Welcome setup changed in another tab. Reload to continue.".into(),
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
        if !skip {
            if current.step != 1 {
                return Err(Error::Invalid("Choose your space first.".into()));
            }
            let title = welcome_name(&input.list)?;
            crate::navigation::validate_list_color(&Some(input.color.clone()))?;
            let info = write.list_info(actor, &list)?;
            write.update_list(
                actor,
                &list,
                ListChange {
                    revision: info.list.revision,
                    title: Some(title),
                    description: None,
                    archived: None,
                    color: Some(input.color),
                },
            )?;
        }
        write.conn.execute("UPDATE user_onboarding SET status=?2,step=2,revision=revision+1,destination=?3 WHERE user_id=?1",params![user,if skip {"skipped"} else {"complete"},destination])?;
        write.commit()?;
        Ok(destination)
    }
}

fn welcome_name(value: &str) -> Result<String, Error> {
    if value.chars().count() > 100 || !value.chars().any(char::is_alphanumeric) {
        return Err(Error::Invalid(
            "Choose a name with letters or numbers (up to 100 characters).".into(),
        ));
    }
    Ok(crate::slugs::slugify(value))
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
            space: "My Life".into(),
        }
    }
    fn finish(revision: i64) -> Finish {
        Finish {
            revision,
            list: "Little Things".into(),
            color: "violet".into(),
        }
    }
    #[test]
    fn welcome_reuses_defaults_resumes_and_completes_without_tasks() {
        let mut db = account();
        let initial = db.welcome("dev").unwrap();
        let workspace = db.ensure_user_defaults("dev").unwrap();
        let list = db.default_list("dev", &workspace).unwrap();
        let saved = db.welcome_setup("dev", setup(initial.revision)).unwrap();
        assert_eq!(saved.space, "my-life");
        assert_eq!(saved.step, 1);
        assert_eq!(db.welcome("dev").unwrap().revision, saved.revision);
        let destination = db
            .welcome_finish("dev", finish(saved.revision), false)
            .unwrap();
        assert_eq!(
            db.welcome_finish("dev", finish(saved.revision), false)
                .unwrap(),
            destination
        );
        assert_eq!(db.default_list("dev", &workspace).unwrap(), list);
        assert_eq!(db.ensure_user_defaults("dev").unwrap(), workspace);
        let result = db.welcome("dev").unwrap();
        assert_eq!(result.list, "little-things");
        assert_eq!(result.color, "violet");
        assert!(db.tasks("dev", &list).unwrap().is_empty());
        assert!(!db.welcome_pending("dev").unwrap());
    }
    #[test]
    fn skip_leaves_names_alone_and_reopening_is_supported() {
        let mut db = account();
        let initial = db.welcome("dev").unwrap();
        db.welcome_finish("dev", finish(initial.revision), true)
            .unwrap();
        assert_eq!(db.welcome("dev").unwrap().list, initial.list);
        let revision = db.welcome("dev").unwrap().revision;
        assert_eq!(db.welcome_setup("dev", setup(revision)).unwrap().step, 1);
    }
    #[test]
    fn validation_and_stale_tabs_roll_back() {
        let mut db = account();
        let initial = db.welcome("dev").unwrap();
        assert!(
            db.welcome_finish("dev", finish(initial.revision), false)
                .is_err()
        );
        assert!(
            db.welcome_setup(
                "dev",
                Setup {
                    revision: initial.revision,
                    space: " --- ".into()
                }
            )
            .is_err()
        );
        let saved = db.welcome_setup("dev", setup(initial.revision)).unwrap();
        assert!(db.welcome_setup("dev", setup(initial.revision)).is_err());
        let mut bad = finish(saved.revision);
        bad.color = "invalid".into();
        assert!(db.welcome_finish("dev", bad, false).is_err());
        assert_eq!(db.welcome("dev").unwrap().list, initial.list);
        assert_eq!(db.welcome("dev").unwrap().revision, saved.revision);
    }
}
