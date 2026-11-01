//! Atomic creation wrappers over the shared capture, section and list operations.
use crate::core::{Capture, Error, Store, Task};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureMetadata {
    pub priority: Option<u8>,
    pub due_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_minutes: Option<u32>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Store {
    /// Capture and metadata are one durable idempotent operation.
    pub fn capture_with_metadata(
        &mut self,
        actor: &str,
        key: &str,
        input: Capture,
        section: Option<String>,
        metadata: CaptureMetadata,
    ) -> Result<Vec<Task>, Error> {
        if key.trim().is_empty() || key.len() > 200 || input.tasks.len() != 1 {
            return Err(Error::Invalid(
                "smart capture requires one task and an idempotency key".into(),
            ));
        }
        let payload = serde_json::to_string(
            &serde_json::json!({"capture": &input, "section_id": &section, "metadata": &metadata}),
        )
        .map_err(|_| Error::Invalid("invalid capture".into()))?;
        let mut tx = crate::agent_ops::StoreWrite::new(self)?;
        if input.list_id.starts_with("intake:")
            && !tx.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM lists WHERE id=?1)",
                [&input.list_id],
                |r| r.get::<_, bool>(0),
            )?
        {
            let user = crate::core::actor_user(&tx.conn, actor, 2)?;
            if input.list_id != format!("intake:{user}") {
                return Err(Error::Forbidden);
            }
            crate::planning::ensure_personal_inbox(&tx.conn, actor)?;
        }
        crate::core::access_rank(&tx.conn, actor, &input.list_id, 2)?;
        if let Some((old, response)) = tx
            .conn
            .query_row(
                "SELECT payload,response FROM idempotency WHERE actor=?1 AND key=?2",
                params![actor, key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            if old != payload {
                return Err(Error::Conflict("idempotency key payload mismatch".into()));
            }
            let tasks: Vec<Task> = serde_json::from_str(&response)
                .map_err(|_| Error::Invalid("invalid stored response".into()))?;
            for item in &tasks {
                let current = crate::core::task(&tx.conn, &item.id)?;
                crate::core::access_rank(&tx.conn, actor, &current.list_id, 2)?;
            }
            tx.commit()?;
            return Ok(tasks);
        }
        if let Some(ref section_id) = section {
            let valid: bool = tx.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sections WHERE id=?1 AND list_id=?2 AND archived=0)",
                params![section_id, input.list_id],
                |r| r.get(0),
            )?;
            if !valid {
                return Err(Error::Invalid("section is not in destination list".into()));
            }
        }
        let internal = format!("smart-capture:{}", uuid::Uuid::new_v4());
        let mut item = tx.capture(actor, &internal, input)?.remove(0);
        if let Some(section_id) = section
            && tx.task_section(actor, &item.id)?.as_deref() != Some(&section_id)
        {
            item = tx.set_section(actor, &item.id, item.revision, Some(section_id))?;
        }
        if metadata.priority.is_some()
            || metadata.due_date.is_some()
            || metadata.planned_date.is_some()
        {
            let mut schedule = tx.get_schedule(actor, &item.id)?.schedule;
            if let Some(priority) = metadata.priority {
                schedule.priority = Some(priority);
            }
            if let Some(date) = metadata.due_date {
                schedule.due = Some(crate::scheduling::Moment::Date { date });
            }
            if let Some(date) = metadata.planned_date {
                schedule.planned = Some(crate::scheduling::Moment::Date { date });
            }
            tx.set_schedule(actor, &item.id, item.revision, schedule)?;
            item = crate::core::task(&tx.conn, &item.id)?;
        }
        if let Some(minutes) = metadata.duration_minutes {
            if minutes > 5_256_000 {
                return Err(Error::Invalid(
                    "Duration must be between 0 and 5256000 minutes".into(),
                ));
            }
            tx.conn.execute(
                "INSERT INTO task_durations(task_id,minutes) VALUES(?1,?2)",
                params![item.id, minutes],
            )?;
        }
        if !metadata.tags.is_empty() {
            item = tx.set_task_tags(
                actor,
                &item.id,
                item.revision,
                metadata
                    .tags
                    .into_iter()
                    .map(crate::taxonomy::TagSelector::Name)
                    .collect(),
            )?;
        }
        tx.conn.execute(
            "DELETE FROM idempotency WHERE actor=?1 AND key=?2",
            params![actor, internal],
        )?;
        let result = vec![item];
        tx.conn.execute(
            "INSERT INTO idempotency(actor,key,payload,response) VALUES(?1,?2,?3,?4)",
            params![actor, key, payload, serde_json::to_string(&result).unwrap()],
        )?;
        tx.commit()?;
        Ok(result)
    }

    /// List creation and its optional palette choice commit or roll back together.
    pub fn create_list_with_color(
        &mut self,
        actor: &str,
        input: crate::workspace::CreateList,
        color: Option<String>,
    ) -> Result<crate::core::List, Error> {
        let mut write = crate::agent_ops::StoreWrite::new(self)?;
        let created = write.create_list(actor, input)?;
        let result = match color {
            Some(color) => {
                write.set_list_color(actor, &created.id, created.revision, Some(color))?
            }
            None => created,
        };
        write.commit()?;
        Ok(result)
    }
    pub fn capture_at(
        &mut self,
        actor: &str,
        key: &str,
        input: Capture,
        section: Option<String>,
    ) -> Result<Vec<Task>, Error> {
        let Some(section) = section else {
            return self.capture(actor, key, input);
        };
        if key.is_empty() || key.len() > 200 {
            return Err(Error::Invalid(
                "idempotency key required (1–200 bytes)".into(),
            ));
        }
        let payload =
            serde_json::to_string(&serde_json::json!({"capture":&input,"section_id":&section}))
                .map_err(|_| Error::Invalid("invalid capture".into()))?;
        let mut tx = crate::agent_ops::StoreWrite::new(self)?;
        crate::core::access_rank(&tx.conn, actor, &input.list_id, 2)?;
        let destination_exists: bool = tx.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sections WHERE id=?1 AND list_id=?2 AND archived=0)",
            params![section, input.list_id],
            |r| r.get(0),
        )?;
        if !destination_exists {
            return Err(Error::Invalid(
                "section is not in the destination list".into(),
            ));
        }
        let previous: Option<(String, String)> = tx
            .conn
            .query_row(
                "SELECT payload,response FROM idempotency WHERE actor=?1 AND key=?2",
                params![actor, key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((stored, response)) = previous {
            if stored != payload {
                return Err(Error::Conflict("idempotency key payload mismatch".into()));
            }
            let tasks: Vec<Task> = serde_json::from_str(&response)
                .map_err(|_| Error::Invalid("invalid stored response".into()))?;
            for t in &tasks {
                let current = crate::core::task(&tx.conn, &t.id)?;
                crate::core::access_rank(&tx.conn, actor, &current.list_id, 2)?;
            }
            tx.commit()?;
            return Ok(tasks);
        }
        let internal = loop {
            let candidate = format!("capture-at:{}", uuid::Uuid::new_v4());
            let exists: bool = tx.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM idempotency WHERE actor=?1 AND key=?2)",
                params![actor, candidate],
                |r| r.get(0),
            )?;
            if !exists {
                break candidate;
            }
        };
        let tasks = tx.capture(actor, &internal, input)?;
        let mut saved = Vec::new();
        for t in tasks {
            let current = crate::core::task(&tx.conn, &t.id)?;
            if tx.task_section(actor, &t.id)?.as_deref() == Some(&section) {
                saved.push(current)
            } else {
                saved.push(tx.set_section(
                    actor,
                    &t.id,
                    current.revision,
                    Some(section.clone()),
                )?);
            }
        }
        tx.conn.execute(
            "DELETE FROM idempotency WHERE actor=?1 AND key=?2",
            params![actor, internal],
        )?;
        tx.conn.execute(
            "INSERT INTO idempotency(actor,key,payload,response) VALUES(?1,?2,?3,?4)",
            params![
                actor,
                key,
                payload,
                serde_json::to_string(&saved)
                    .map_err(|_| Error::Invalid("invalid capture response".into()))?
            ],
        )?;
        tx.commit()?;
        Ok(saved)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{core::NewTask, workspace::SectionInput};
    #[test]
    fn smart_capture_creates_lazy_inbox_atomically_and_retries() {
        let mut db = Store::memory().unwrap();
        db.conn
            .execute("INSERT INTO users VALUES('new-user','new')", [])
            .unwrap();
        db.conn
            .execute("INSERT INTO users VALUES('other-user','other')", [])
            .unwrap();
        let inbox = db.personal_inbox("new-user").unwrap();
        let request = || Capture {
            list_id: inbox.clone(),
            tasks: vec![NewTask {
                title: "First task".into(),
                description: "".into(),
                parent_id: None,
            }],
        };
        let metadata = CaptureMetadata {
            duration_minutes: Some(0),
            priority: Some(0),
            ..Default::default()
        };
        assert!(
            db.capture_with_metadata("other-user", "bad", request(), None, metadata.clone())
                .is_err()
        );
        let first = db
            .capture_with_metadata("new-user", "first", request(), None, metadata.clone())
            .unwrap();
        let retry = db
            .capture_with_metadata("new-user", "first", request(), None, metadata)
            .unwrap();
        assert_eq!(first[0].id, retry[0].id);
        assert_eq!(db.task_duration("new-user", &first[0].id).unwrap(), Some(0));
        assert_eq!(
            db.get_schedule("new-user", &first[0].id)
                .unwrap()
                .schedule
                .priority,
            Some(0)
        );
    }
    #[test]
    fn smart_capture_retries_and_rolls_back_metadata_errors() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let metadata = CaptureMetadata {
            duration_minutes: None,
            priority: Some(0),
            due_date: Some("2026-11-01".into()),
            planned_date: Some("2026-10-31".into()),
            tags: vec!["home".into()],
        };
        let first = db
            .capture_with_metadata("dev", "smart", capture(), None, metadata.clone())
            .unwrap();
        let retry = db
            .capture_with_metadata("dev", "smart", capture(), None, metadata)
            .unwrap();
        assert_eq!(first[0].id, retry[0].id);
        assert_eq!(
            db.get_schedule("dev", &first[0].id)
                .unwrap()
                .schedule
                .priority,
            Some(0)
        );
        assert_eq!(db.task_tags("dev", &first[0].id).unwrap()[0].name, "home");
        let schedule = db.get_schedule("dev", &first[0].id).unwrap().schedule;
        assert_eq!(
            schedule.planned,
            Some(crate::scheduling::Moment::Date {
                date: "2026-10-31".into()
            })
        );
        assert_eq!(
            schedule.due,
            Some(crate::scheduling::Moment::Date {
                date: "2026-11-01".into()
            })
        );
        let bad_date = CaptureMetadata {
            duration_minutes: None,
            planned_date: Some("2026-02-30".into()),
            ..Default::default()
        };
        assert!(
            db.capture_with_metadata("dev", "bad-date", capture(), None, bad_date)
                .is_err()
        );

        let too_long = CaptureMetadata {
            duration_minutes: Some(5_256_001),
            ..Default::default()
        };
        assert!(
            db.capture_with_metadata("dev", "bad-duration", capture(), None, too_long)
                .is_err()
        );
        let invalid = CaptureMetadata {
            duration_minutes: None,
            priority: Some(9),
            ..Default::default()
        };
        assert!(
            db.capture_with_metadata("dev", "fail", capture(), None, invalid)
                .is_err()
        );
        assert_eq!(db.tasks("dev", "inbox").unwrap().len(), 1);
        assert!(
            db.capture_with_metadata(
                "other",
                "forbidden",
                capture(),
                None,
                CaptureMetadata::default()
            )
            .is_err()
        );
    }
    fn capture() -> Capture {
        Capture {
            list_id: "inbox".into(),
            tasks: vec![NewTask {
                title: "Section item".into(),
                description: "Exact brief".into(),
                parent_id: None,
            }],
        }
    }
    #[test]
    fn section_capture_retry_payload_and_atomic_failure() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let section = db
            .create_section(
                "dev",
                "inbox",
                SectionInput {
                    title: "Monday".into(),
                    description: "".into(),
                },
            )
            .unwrap();
        let first = db
            .capture_at("dev", "one", capture(), Some(section.id.clone()))
            .unwrap();
        let again = db
            .capture_at("dev", "one", capture(), Some(section.id.clone()))
            .unwrap();
        assert_eq!(first[0].id, again[0].id);
        assert_eq!(
            db.task_section("dev", &first[0].id).unwrap(),
            Some(section.id)
        );
        assert!(matches!(
            db.capture_at("dev", "one", capture(), None),
            Err(Error::Conflict(_))
        ));
        let mut invalid = capture();
        invalid.tasks.push(NewTask {
            title: "Invalid parent".into(),
            description: "".into(),
            parent_id: Some("missing".into()),
        });
        assert!(
            db.capture_at("dev", "failed", invalid, Some("missing".into()))
                .is_err()
        );
        assert_eq!(db.tasks("dev", "inbox").unwrap().len(), 1);
        assert!(matches!(
            db.capture_at("other", "forbidden", capture(), None),
            Err(Error::Forbidden)
        ));
    }
    fn make_section(db: &mut Store, list: &str, name: &str) -> crate::workspace::Section {
        db.create_section(
            "dev",
            list,
            SectionInput {
                title: name.into(),
                description: String::new(),
            },
        )
        .unwrap()
    }
    fn counts(db: &Store) -> (i64, i64, i64, i64) {
        db.conn.query_row("SELECT (SELECT count(*) FROM tasks),(SELECT count(*) FROM task_locations),(SELECT count(*) FROM history),(SELECT count(*) FROM idempotency)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap()
    }
    #[test]
    fn failed_mid_assignment_rolls_back_capture_ancestor_reopen_history_and_keys() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let section = make_section(&mut db, "inbox", "Plan");
        let parent = db
            .capture_at("dev", "parent", capture(), Some(section.id.clone()))
            .unwrap()
            .remove(0);
        let parent = db
            .update(
                "dev",
                &parent.id,
                crate::core::Change {
                    revision: parent.revision,
                    complete: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
        let input = || Capture {
            list_id: "inbox".into(),
            tasks: vec![
                NewTask {
                    title: "First assignment".into(),
                    description: String::new(),
                    parent_id: None,
                },
                NewTask {
                    title: "Inherited child".into(),
                    description: String::new(),
                    parent_id: Some(parent.id.clone()),
                },
                NewTask {
                    title: "Fail assignment".into(),
                    description: String::new(),
                    parent_id: None,
                },
            ],
        };
        let before = counts(&db);
        db.conn.execute_batch("CREATE TRIGGER synthetic_capture_assignment_failure BEFORE INSERT ON task_locations WHEN (SELECT title FROM tasks WHERE id=new.task_id)='Fail assignment' BEGIN SELECT RAISE(ABORT,'synthetic mid-assignment failure'); END;").unwrap();
        assert!(matches!(
            db.capture_at(
                "dev",
                "retry-after-failure",
                input(),
                Some(section.id.clone())
            ),
            Err(Error::Database(_))
        ));
        assert_eq!(counts(&db), before);
        let after = crate::core::task(&db.conn, &parent.id).unwrap();
        assert!(after.complete);
        assert_eq!(after.revision, parent.revision);
        assert!(db.conn.is_autocommit());
        db.conn
            .execute_batch("DROP TRIGGER synthetic_capture_assignment_failure")
            .unwrap();
        let saved = db
            .capture_at(
                "dev",
                "retry-after-failure",
                input(),
                Some(section.id.clone()),
            )
            .unwrap();
        assert_eq!(saved.len(), 3);
        for task in &saved {
            assert_eq!(
                db.task_section("dev", &task.id).unwrap(),
                Some(section.id.clone())
            );
        }
        assert!(!crate::core::task(&db.conn, &parent.id).unwrap().complete);
        let internal: i64 = db
            .conn
            .query_row(
                "SELECT count(*) FROM idempotency WHERE key LIKE 'capture-at:%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(internal, 0);
        let again = db
            .capture_at("dev", "retry-after-failure", input(), Some(section.id))
            .unwrap();
        assert_eq!(
            serde_json::to_value(saved).unwrap(),
            serde_json::to_value(again).unwrap()
        );
    }
    #[test]
    fn retries_survive_section_renames_but_recheck_live_destination_visibility() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let section = make_section(&mut db, "inbox", "Plan");
        let first = db
            .capture_at("dev", "retry", capture(), Some(section.id.clone()))
            .unwrap();
        db.update_section(
            "dev",
            &section.id,
            crate::workspace::SectionChange {
                revision: section.revision,
                title: Some("Renamed without recapture".into()),
                description: None,
            },
        )
        .unwrap();
        let before = counts(&db);
        let again = db
            .capture_at("dev", "retry", capture(), Some(section.id.clone()))
            .unwrap();
        assert_eq!(first[0].id, again[0].id);
        assert_eq!(counts(&db), before);
        db.conn
            .execute(
                "DELETE FROM memberships WHERE list_id='inbox' AND user_id='dev'",
                [],
            )
            .unwrap();
        assert!(matches!(
            db.capture_at("dev", "retry", capture(), Some(section.id.clone())),
            Err(Error::Forbidden)
        ));
        assert_eq!(counts(&db), before);
        db.conn
            .execute("INSERT INTO memberships VALUES('inbox','dev')", [])
            .unwrap();
        let section2 = make_section(&mut db, "groceries", "Other list");
        assert!(
            db.capture_at("dev", "wrong-destination", capture(), Some(section2.id))
                .is_err()
        );
        assert_eq!(counts(&db).0, before.0);
        // A section relocation must not create a duplicate capture in the old list.
        let preview = db
            .section_move_preview("dev", &section.id, "groceries")
            .unwrap();
        db.move_section(
            "dev",
            &section.id,
            crate::workspace::SectionMoveInput {
                destination_list_id: "groceries".into(),
                section_revision: preview.section_revision,
                source_list_revision: preview.source_list_revision,
                destination_list_revision: preview.destination_list_revision,
                policy_hash: preview.policy_hash,
                acknowledge_access_change: false,
            },
        )
        .unwrap();
        let before = counts(&db);
        assert!(
            db.capture_at("dev", "retry", capture(), Some(section.id))
                .is_err()
        );
        assert_eq!(counts(&db), before);
    }
    #[test]
    fn inherited_parent_section_is_used_without_reparenting_or_sibling_mutation() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let a = make_section(&mut db, "inbox", "A");
        let b = make_section(&mut db, "inbox", "B");
        let parent = db
            .capture_at("dev", "root", capture(), Some(a.id.clone()))
            .unwrap()
            .remove(0);
        let child_input = || Capture {
            list_id: "inbox".into(),
            tasks: vec![NewTask {
                title: "Child".into(),
                description: String::new(),
                parent_id: Some(parent.reference.clone()),
            }],
        };
        let child = db
            .capture_at("dev", "child", child_input(), Some(a.id.clone()))
            .unwrap()
            .remove(0);
        assert_eq!(child.parent_id, Some(parent.id.clone()));
        assert_eq!(child.revision, 1);
        assert_eq!(
            db.task_section("dev", &child.id).unwrap(),
            Some(a.id.clone())
        );
        let before = counts(&db);
        assert!(matches!(
            db.capture_at("dev", "wrong-parent-section", child_input(), Some(b.id)),
            Err(Error::Invalid(_))
        ));
        assert_eq!(counts(&db), before);
        assert_eq!(
            db.task_section("dev", &parent.id).unwrap(),
            Some(a.id.clone())
        );
        // Public keyspace is shared with ordinary capture in both directions.
        db.capture("dev", "ordinary", capture()).unwrap();
        let before = counts(&db);
        assert!(matches!(
            db.capture_at("dev", "ordinary", capture(), Some(a.id.clone())),
            Err(Error::Conflict(_))
        ));
        assert_eq!(counts(&db), before);
    }
    #[test]
    fn scoped_credential_replay_rechecks_revocation_and_every_current_task_list() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let section = make_section(&mut db, "inbox", "Plan");
        db.conn.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,list_ids,expires_at) VALUES('capture-section','dev','synthetic-capture-section-hash','https://synthetic.invalid/api/v1','[\"write\"]','[\"inbox\"]',unixepoch()+3600)",[]).unwrap();
        let actor = "credential:capture-section";
        let task = db
            .capture_at(
                actor,
                "credential-retry",
                capture(),
                Some(section.id.clone()),
            )
            .unwrap()
            .remove(0);
        db.conn
            .execute(
                "UPDATE auth_agents SET revoked=1 WHERE id='capture-section'",
                [],
            )
            .unwrap();
        let before = counts(&db);
        assert!(matches!(
            db.capture_at(
                actor,
                "credential-retry",
                capture(),
                Some(section.id.clone())
            ),
            Err(Error::Forbidden)
        ));
        assert_eq!(counts(&db), before);
        db.conn
            .execute(
                "UPDATE auth_agents SET revoked=0 WHERE id='capture-section'",
                [],
            )
            .unwrap();
        db.move_subtree("dev", &task.id, "groceries", task.revision, false)
            .unwrap();
        let before = counts(&db);
        assert!(matches!(
            db.capture_at(actor, "credential-retry", capture(), Some(section.id)),
            Err(Error::Forbidden)
        ));
        assert_eq!(counts(&db), before);
    }
    #[test]
    fn list_creation_color_and_slug_commit_together_and_invalid_colors_roll_back() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let workspace = db
            .create_workspace(
                "dev",
                crate::workspace::WorkspaceInput {
                    title: "Synthetic workspace".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let input = || crate::workspace::CreateList {
            workspace_id: workspace.id.clone(),
            title: "Mobile Reading".into(),
            description: "Preserved list brief".into(),
            restricted: false,
        };
        let before = db.lists("dev").unwrap().len();
        let slug_count: i64 = db
            .conn
            .query_row("SELECT count(*) FROM entity_slugs", [], |r| r.get(0))
            .unwrap();
        assert!(matches!(
            db.create_list_with_color("dev", input(), Some("unknown-color".into())),
            Err(Error::Invalid(_))
        ));
        assert_eq!(db.lists("dev").unwrap().len(), before);
        assert_eq!(
            db.workspace("dev", &workspace.id).unwrap().revision,
            workspace.revision
        );
        assert_eq!(
            db.conn
                .query_row("SELECT count(*) FROM entity_slugs", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            slug_count
        );
        assert!(matches!(
            db.create_list_with_color("other", input(), Some("blue".into())),
            Err(Error::Forbidden)
        ));
        assert_eq!(db.lists("dev").unwrap().len(), before);
        let list = db
            .create_list_with_color("dev", input(), Some("amber".into()))
            .unwrap();
        assert_eq!(list.title, "Mobile Reading");
        assert_eq!(list.description, "Preserved list brief");
        assert_eq!(list.revision, 2);
        assert_eq!(
            db.list_color("dev", &list.id).unwrap(),
            Some("amber".into())
        );
        assert_eq!(
            db.entity_slugs("dev").unwrap().lists[&list.id],
            "mobile-reading"
        );
        let plain = db.create_list_with_color("dev", input(), None).unwrap();
        assert_eq!(plain.revision, 1);
        assert_eq!(db.list_color("dev", &plain.id).unwrap(), None);
        db.conn.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,list_ids,expires_at) VALUES('list-create-scope','dev','synthetic-list-create','https://synthetic.invalid/api/v1','[\"write\"]','[\"inbox\"]',unixepoch()+3600)",[]).unwrap();
        assert!(matches!(
            db.create_list_with_color("credential:list-create-scope", input(), Some("blue".into())),
            Err(Error::Forbidden)
        ));
    }
}
