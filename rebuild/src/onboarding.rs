//! A two-question welcome: save a draft, then atomically bootstrap its destinations.
use crate::core::{Error, Store, actor_user};
use crate::workspace::{CreateList, ListChange, WorkspaceInput};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Serialize)]
pub struct Welcome {
    pub status: String,
    pub step: i64,
    pub revision: i64,
    pub spaces: Vec<String>,
    pub destination: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    pub revision: i64,
    pub spaces: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListChoice {
    pub space: String,
    pub title: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finish {
    pub revision: i64,
    #[serde(default)]
    pub lists: Vec<ListChoice>,
}

fn validate_spaces(spaces: &[String]) -> Result<(), Error> {
    if spaces.is_empty()
        || spaces.len() > 2
        || spaces
            .iter()
            .any(|s| !matches!(s.as_str(), "personal" | "work"))
        || spaces.iter().collect::<HashSet<_>>().len() != spaces.len()
    {
        return Err(Error::Invalid("Choose Personal, Work, or both.".into()));
    }
    Ok(())
}
fn list_title(title: &str) -> Result<String, Error> {
    if title.chars().count() > 64 || !title.chars().any(char::is_alphanumeric) {
        return Err(Error::Invalid(
            "List names need letters or numbers (up to 64 characters).".into(),
        ));
    }
    Ok(crate::slugs::slugify(title))
}
fn color(title: &str, space: &str) -> &'static str {
    match title {
        "groceries" => "green",
        "home-chores" => "amber",
        "personal-goals" => "pink",
        "project" => "violet",
        "follow-ups" => "amber",
        "ideas" => "green",
        _ if space == "work" => "violet",
        _ => "blue",
    }
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
        let (status, step, revision, destination, spaces): (String,i64,i64,String,String) = self.conn.query_row(
            "SELECT status,step,revision,destination,spaces FROM user_onboarding WHERE user_id=?1", [user],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
        let spaces = serde_json::from_str(&spaces)
            .map_err(|_| Error::Invalid("Could not read welcome progress.".into()))?;
        Ok(Welcome {
            status,
            step,
            revision,
            destination,
            spaces,
        })
    }
    pub fn welcome_setup(&mut self, actor: &str, input: Setup) -> Result<Welcome, Error> {
        validate_spaces(&input.spaces)?;
        let mut write = crate::agent_ops::StoreWrite::new(self)?;
        let current = write.welcome(actor)?;
        if current.revision != input.revision {
            return Err(welcome_conflict());
        }
        let user = actor_user(&write.conn, actor, 3)?;
        let spaces = serde_json::to_string(&input.spaces)
            .map_err(|_| Error::Invalid("Could not save welcome choices.".into()))?;
        write.conn.execute("UPDATE user_onboarding SET status='pending',step=1,revision=revision+1,spaces=?2 WHERE user_id=?1", params![user, spaces])?;
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
        // Completion is persisted in the same transaction as all creations.
        if current.status != "pending" {
            return Ok(current.destination);
        }
        if current.revision != input.revision {
            return Err(welcome_conflict());
        }
        let user = actor_user(&write.conn, actor, 3)?;
        let mut destination = "/inbox".to_string();
        if !skip {
            validate_spaces(&current.spaces)?;
            if input.lists.len() > 12 {
                return Err(Error::Invalid("Choose up to 12 lists to start.".into()));
            }
            let mut seen = HashSet::new();
            let mut choices = Vec::new();
            // Validate the entire request before creating anything.
            for choice in input.lists {
                if !current.spaces.contains(&choice.space) {
                    return Err(Error::Invalid(
                        "Choose a list inside one of your selected spaces.".into(),
                    ));
                }
                let title = list_title(&choice.title)?;
                if !seen.insert((choice.space.clone(), title.clone())) {
                    return Err(Error::Invalid(
                        "Each list only needs to be chosen once.".into(),
                    ));
                }
                choices.push((choice.space, title));
            }
            let mut first_list = None;
            for space in &current.spaces {
                // Only reuse active spaces owned by this account, never inherited access.
                let existing: Option<String> = write.conn.query_row(
                    "SELECT w.id FROM workspaces w JOIN workspace_members m ON m.workspace_id=w.id LEFT JOIN workspace_lifecycle x ON x.workspace_id=w.id WHERE m.user_id=?1 AND m.role='owner' AND lower(w.title)=?2 AND x.deleted_at IS NULL AND coalesce(x.archived,0)=0 ORDER BY w.id LIMIT 1",
                    params![user,space], |r| r.get(0)).optional()?;
                let workspace = match existing {
                    Some(id) => id,
                    None => {
                        write
                            .create_workspace(
                                actor,
                                WorkspaceInput {
                                    title: space.clone(),
                                    description: String::new(),
                                },
                            )?
                            .id
                    }
                };
                if destination == "/inbox" {
                    destination = format!(
                        "/space/{}",
                        crate::defaults::public_id(&write.conn, "workspace", &workspace)?
                    );
                }
                for (_, title) in choices.iter().filter(|(s, _)| s == space) {
                    let existing: Option<String> = write.conn.query_row(
                        "SELECT id FROM lists WHERE workspace_id=?1 AND lower(title)=?2 AND archived=0 ORDER BY position,id LIMIT 1",
                        params![workspace,title], |r|r.get(0)).optional()?;
                    let id = match existing {
                        Some(id) => id,
                        None => {
                            let list = write.create_list(
                                actor,
                                CreateList {
                                    workspace_id: workspace.clone(),
                                    title: title.clone(),
                                    description: String::new(),
                                    restricted: false,
                                },
                            )?;
                            write.update_list(
                                actor,
                                &list.id,
                                ListChange {
                                    revision: list.revision,
                                    title: None,
                                    description: None,
                                    archived: None,
                                    color: Some(color(title, space).into()),
                                },
                            )?;
                            list.id
                        }
                    };
                    if first_list.is_none() {
                        first_list = Some(id);
                    }
                }
            }
            if let Some(id) = first_list {
                destination = format!(
                    "/lists/{}",
                    crate::defaults::public_id(&write.conn, "list", &id)?
                );
            }
        }
        write.conn.execute("UPDATE user_onboarding SET status=?2,step=2,revision=revision+1,destination=?3 WHERE user_id=?1",params![user,if skip {"skipped"} else {"complete"},destination])?;
        write.commit()?;
        Ok(destination)
    }
}
fn welcome_conflict() -> Error {
    Error::Conflict("Welcome setup changed in another tab. Reload to continue.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn account() -> Store {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db
    }
    fn counts(db: &Store) -> (i64, i64, i64) {
        db.conn.query_row("SELECT (SELECT count(*) FROM workspaces),(SELECT count(*) FROM lists),(SELECT count(*) FROM tasks)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
    }
    fn choose(db: &mut Store, spaces: &[&str]) -> Welcome {
        let revision = db.welcome("dev").unwrap().revision;
        db.welcome_setup(
            "dev",
            Setup {
                revision,
                spaces: spaces.iter().map(|s| s.to_string()).collect(),
            },
        )
        .unwrap()
    }
    fn finish(revision: i64) -> Finish {
        Finish {
            revision,
            lists: vec![
                ListChoice {
                    space: "personal".into(),
                    title: "Groceries".into(),
                },
                ListChoice {
                    space: "work".into(),
                    title: "Weekly Priorities".into(),
                },
            ],
        }
    }
    #[test]
    fn draft_creates_nothing_and_completion_is_atomic_replay_safe() {
        let mut db = account();
        let before = counts(&db);
        let saved = choose(&mut db, &["personal", "work"]);
        assert_eq!(counts(&db), before);
        assert_eq!(db.welcome("dev").unwrap().spaces, vec!["personal", "work"]);
        let destination = db
            .welcome_finish("dev", finish(saved.revision), false)
            .unwrap();
        assert!(destination.starts_with("/lists/"));
        let after = counts(&db);
        assert_eq!(after.2, before.2);
        assert_eq!(
            db.welcome_finish("dev", finish(saved.revision), false)
                .unwrap(),
            destination
        );
        assert_eq!(counts(&db), after);
        let saved = choose(&mut db, &["personal", "work"]);
        db.welcome_finish("dev", finish(saved.revision), false)
            .unwrap();
        assert_eq!(counts(&db), after); // reopening reuses the same destinations
    }
    #[test]
    fn validation_stale_tabs_and_skipping_do_not_create_entities() {
        let mut db = account();
        let before = counts(&db);
        let saved = choose(&mut db, &["personal"]);
        assert!(
            db.welcome_finish("dev", finish(saved.revision), false)
                .is_err()
        );
        assert_eq!(counts(&db), before);
        assert!(db.welcome_pending("dev").unwrap());
        assert!(
            db.welcome_setup(
                "dev",
                Setup {
                    revision: saved.revision - 1,
                    spaces: vec!["work".into()]
                }
            )
            .is_err()
        );
        assert!(
            db.welcome_setup(
                "dev",
                Setup {
                    revision: saved.revision,
                    spaces: vec!["other".into()]
                }
            )
            .is_err()
        );
        db.welcome_finish(
            "dev",
            Finish {
                revision: saved.revision,
                lists: vec![],
            },
            true,
        )
        .unwrap();
        assert_eq!(counts(&db), before);
        assert!(!db.welcome_pending("dev").unwrap());
    }
    #[test]
    fn undecided_creates_only_selected_space_and_no_suggested_lists() {
        let mut db = account();
        let saved = choose(&mut db, &["work"]);
        let destination = db
            .welcome_finish(
                "dev",
                Finish {
                    revision: saved.revision,
                    lists: vec![],
                },
                false,
            )
            .unwrap();
        assert!(destination.starts_with("/space/"));
        let count:i64=db.conn.query_row("SELECT count(*) FROM lists l JOIN workspaces w ON l.workspace_id=w.id WHERE w.title='work' AND l.title!='Inbox' AND l.title!='inbox'",[],|r|r.get(0)).unwrap();
        assert_eq!(count, 0);
    }
}
