//! Reversible task and section archives. Trash and archive are distinct states.
use super::*;
use rusqlite::{OptionalExtension, params};

#[derive(Deserialize)]
pub(super) struct LifecycleInput {
    archive: bool,
}

pub(super) async fn change(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((kind, reference)): Path<(String, String)>,
    Json(input): Json<LifecycleInput>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| change_archive(db, &actor, &kind, &reference, input.archive))
            .await?,
    ))
}

fn change_archive(
    db: &mut Store,
    actor: &str,
    kind: &str,
    reference: &str,
    archive: bool,
) -> Result<Value, Error> {
    let entity_kind = match kind {
        "task" | "subtask" => "task",
        "section" => "section",
        _ => return Err(Error::Invalid("unknown archive type".into())),
    };
    let id = crate::defaults::resolve(&db.conn, entity_kind, reference)?;
    let tx = crate::agent_ops::begin_atomic(&mut db.conn)?;
    let (list, group, ids) = if entity_kind == "section" {
        let section: (String, bool) = tx.query_row("SELECT list_id,archived FROM sections WHERE id=?1 AND NOT EXISTS(SELECT 1 FROM list_defaults d WHERE d.section_id=sections.id)", [&id], |r| Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or(Error::NotFound)?;
        crate::core::access_rank(&tx, actor, &section.0, 2)?;
        if section.1 == archive {
            return Err(Error::Conflict(
                if archive {
                    "section is already archived"
                } else {
                    "section is not archived"
                }
                .into(),
            ));
        }
        let group = format!("section:{id}");
        let ids = if archive {
            tx.prepare("SELECT t.id FROM tasks t JOIN task_locations loc ON loc.task_id=t.id WHERE t.list_id=?1 AND loc.section_id=?2 AND t.deleted_at IS NULL ORDER BY t.position,t.id")?
                .query_map(params![section.0,id], |r| r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?
        } else {
            tx.prepare("SELECT id FROM tasks WHERE archive_group=?1 ORDER BY position,id")?
                .query_map([&group], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        tx.execute(
            "UPDATE sections SET archived=?2,revision=revision+1 WHERE id=?1",
            params![id, archive],
        )?;
        tx.execute(
            "UPDATE lists SET revision=revision+1 WHERE id=?1",
            [&section.0],
        )?;
        (section.0, group, ids)
    } else {
        let root = crate::core::raw_task(&tx, &id)?;
        crate::core::access_rank(&tx, actor, &root.list_id, 2)?;
        if (kind == "subtask") != root.parent_id.is_some() {
            return Err(Error::Invalid(
                "reference prefix does not match task type".into(),
            ));
        }
        let (deleted, group): (bool, Option<String>) = tx.query_row(
            "SELECT deleted_at IS NOT NULL,archive_group FROM tasks WHERE id=?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if archive && deleted {
            return Err(Error::Conflict(
                "task is already archived or in trash".into(),
            ));
        }
        if !archive && group.as_deref() != Some(id.as_str()) {
            return Err(Error::Conflict(
                "restore the archived parent or section first".into(),
            ));
        }
        if !archive {
            if let Some(parent) = &root.parent_id {
                crate::core::task(&tx, parent)
                    .map_err(|_| Error::Conflict("restore the parent task first".into()))?;
            }
            let section_archived: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM task_locations l JOIN sections s ON s.id=l.section_id WHERE l.task_id=?1 AND s.archived=1)", [&id], |r| r.get(0))?;
            if section_archived && root.parent_id.is_none() {
                return Err(Error::Conflict("restore the section first".into()));
            }
        }
        let ids = if archive {
            tx.prepare("WITH RECURSIVE tree(id) AS (SELECT id FROM tasks WHERE id=?1 AND deleted_at IS NULL UNION ALL SELECT t.id FROM tasks t JOIN tree ON t.parent_id=tree.id WHERE t.deleted_at IS NULL) SELECT id FROM tree")?
                .query_map([&id], |r| r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?
        } else {
            tx.prepare("SELECT id FROM tasks WHERE archive_group=?1 ORDER BY position,id")?
                .query_map([&id], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        (root.list_id, id.clone(), ids)
    };
    if !archive && entity_kind == "task" {
        let root = crate::core::raw_task(&tx, &id)?;
        if let Some(parent) = root.parent_id {
            let section = crate::taxonomy::location(&tx, &parent)?;
            crate::taxonomy::assign_location(&tx, &ids, section.as_deref())?;
        }
    }
    for task_id in &ids {
        let before = crate::core::raw_task(&tx, task_id)?;
        if archive {
            tx.execute("UPDATE tasks SET deleted_at=CURRENT_TIMESTAMP,archive_group=?2,revision=revision+1 WHERE id=?1 AND deleted_at IS NULL",params![task_id,group])?;
        } else {
            tx.execute("UPDATE tasks SET deleted_at=NULL,archive_group=NULL,revision=revision+1 WHERE id=?1 AND archive_group=?2",params![task_id,group])?;
        }
        let after = crate::core::raw_task(&tx, task_id)?;
        crate::core::event(
            &tx,
            &after,
            if archive { "archive" } else { "unarchive" },
            actor,
            Some(&before),
        )?;
    }
    if !archive {
        for task_id in &ids {
            if !crate::core::raw_task(&tx, task_id)?.complete {
                crate::core::reopen(&tx, task_id, actor)?;
            }
        }
    }
    tx.commit()?;
    Ok(json!({"id":id,"kind":kind,"list_id":list,"archived":archive,"affected_tasks":ids.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::NewTask;

    #[test]
    fn archives_keep_trash_and_nested_archive_groups_separate() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let capture = |db: &mut Store, key: &str, title: &str, parent: Option<String>| {
            db.capture(
                "dev",
                key,
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![NewTask {
                        title: title.into(),
                        description: String::new(),
                        parent_id: parent,
                    }],
                },
            )
            .unwrap()
            .remove(0)
        };
        let root = capture(&mut db, "archive-root", "Root", None);
        let child = capture(&mut db, "archive-child", "Child", Some(root.id.clone()));
        change_archive(&mut db, "dev", "subtask", &child.id, true).unwrap();
        change_archive(&mut db, "dev", "task", &root.id, true).unwrap();
        let bundle = db
            .export_bundle("dev", crate::transfer::ExportScope::Account)
            .unwrap();
        let mut imported = Store::memory().unwrap();
        crate::transfer::import_bundle(
            &mut imported,
            &bundle,
            &crate::transfer::IdentityMap::default(),
            true,
        )
        .unwrap();
        assert_eq!(
            imported
                .conn
                .query_row(
                    "SELECT archive_group FROM tasks WHERE id=?1",
                    [&child.id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            child.id
        );
        change_archive(&mut imported, "import:dev", "task", &root.id, false).unwrap();
        assert!(crate::core::task(&imported.conn, &child.id).is_err());
        assert!(db.trash_tasks("dev").unwrap().is_empty());
        assert!(
            db.tasks("dev", "inbox")
                .unwrap()
                .iter()
                .all(|t| t.id != root.id)
        );
        change_archive(&mut db, "dev", "task", &root.id, false).unwrap();
        assert!(crate::core::task(&db.conn, &child.id).is_err());
        let current = crate::core::task(&db.conn, &root.id).unwrap();
        let section = db
            .create_section(
                "dev",
                "inbox",
                crate::workspace::SectionInput {
                    title: "Moved".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        db.set_section("dev", &root.id, current.revision, Some(section.id.clone()))
            .unwrap();
        let current = crate::core::task(&db.conn, &root.id).unwrap();
        db.update(
            "dev",
            &root.id,
            Change {
                revision: current.revision,
                complete: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        change_archive(&mut db, "dev", "subtask", &child.id, false).unwrap();
        assert!(!crate::core::task(&db.conn, &root.id).unwrap().complete);
        assert_eq!(db.task_section("dev", &child.id).unwrap(), Some(section.id));
        assert!(change_archive(&mut db, "other", "task", &root.id, true).is_err());
    }
}
