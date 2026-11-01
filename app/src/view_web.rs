//! Immediate, authorized children for the CLI's explorable view.
use super::*;
use rusqlite::params;

pub(super) async fn view(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((kind, reference)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| view_data(db, &actor, &kind, &reference))
            .await?,
    ))
}

fn task_children(
    db: &Store,
    list: &str,
    parent: Option<&str>,
    section: Option<&str>,
) -> Result<Vec<Task>, Error> {
    let mut stmt = db.conn.prepare("SELECT t.id,t.reference,t.list_id,t.parent_id,t.title,'' AS description,t.complete,t.revision,t.position FROM tasks t LEFT JOIN task_locations loc ON loc.task_id=t.id LEFT JOIN list_defaults d ON d.section_id=loc.section_id WHERE t.list_id=?1 AND t.deleted_at IS NULL AND ((?2 IS NOT NULL AND t.parent_id=?2) OR (?2 IS NULL AND t.parent_id IS NULL AND ((?3 IS NOT NULL AND loc.section_id=?3) OR (?3 IS NULL AND (loc.section_id IS NULL OR d.section_id IS NOT NULL))))) ORDER BY t.position,t.id")?;
    Ok(stmt
        .query_map(params![list, parent, section], crate::core::task_row)?
        .collect::<Result<Vec<_>, _>>()?)
}

fn task_summary(db: &Store, task: &Task) -> Result<Value, Error> {
    let reference = crate::defaults::public_id(&db.conn, "task", &task.id)?;
    Ok(
        json!({"kind":if task.parent_id.is_some(){"subtask"}else{"task"},"id":task.id,"reference":reference,"title":task.title,"complete":task.complete,"revision":task.revision}),
    )
}

fn counts(db: &Store, list: &str, section: Option<&str>) -> Result<Value, Error> {
    let (open, done): (i64, i64) = db.conn.query_row("SELECT coalesce(sum(t.complete=0),0),coalesce(sum(t.complete=1),0) FROM tasks t LEFT JOIN task_locations loc ON loc.task_id=t.id WHERE t.list_id=?1 AND t.parent_id IS NULL AND t.deleted_at IS NULL AND (?2 IS NULL OR loc.section_id=?2)", params![list, section], |r| Ok((r.get(0)?,r.get(1)?)))?;
    Ok(json!({"open":open,"done":done}))
}

fn view_data(db: &Store, actor: &str, kind: &str, reference: &str) -> Result<Value, Error> {
    let entity_kind = match kind {
        "space" => "workspace",
        "list" => "list",
        "section" => "section",
        "task" | "subtask" => "task",
        _ => return Err(Error::Invalid("unknown view type".into())),
    };
    let id = crate::defaults::resolve(&db.conn, entity_kind, reference)?;
    let mut children = Vec::new();
    let entity = match kind {
        "space" => {
            let space = db.workspace(actor, &id)?;
            for list in db.lists(actor)? {
                let info = db.list_info(actor, &list.id)?;
                if info.workspace_id.as_deref() != Some(id.as_str()) || info.archived {
                    continue;
                }
                let reference = crate::defaults::public_id(&db.conn, "list", &list.id)?;
                let mut summary = json!({"kind":"list","id":list.id,"title":list.title,"reference":reference,"revision":list.revision,"counts":counts(db, &list.id, None)?});
                summary["counts"]["sections"] = json!(db.sections(actor, &list.id)?.len());
                children.push(summary);
            }
            json!(space)
        }
        "list" => {
            let info = db.list_info(actor, &id)?;
            for section in db.sections(actor, &id)? {
                let reference = crate::defaults::public_id(&db.conn, "section", &section.id)?;
                children.push(json!({"kind":"section","id":section.id,"title":section.title,"reference":reference,"revision":section.revision,"counts":counts(db, &id, Some(&section.id))?}));
            }
            for task in task_children(db, &id, None, None)? {
                children.push(task_summary(db, &task)?);
            }
            json!(info)
        }
        "section" => {
            let section = crate::workspace::section_raw(&db.conn, &id)?;
            crate::core::access_rank(&db.conn, actor, &section.list_id, 1)?;
            for task in task_children(db, &section.list_id, None, Some(&id))? {
                children.push(task_summary(db, &task)?);
            }
            json!(section)
        }
        "task" | "subtask" => {
            let task = crate::core::task(&db.conn, &id)?;
            crate::core::access_rank(&db.conn, actor, &task.list_id, 1)?;
            if (kind == "subtask") != task.parent_id.is_some() {
                return Err(Error::Invalid(
                    "reference prefix does not match task type".into(),
                ));
            }
            for child in task_children(db, &task.list_id, Some(&task.id), None)? {
                children.push(task_summary(db, &child)?);
            }
            json!(task)
        }
        _ => unreachable!(),
    };
    let short = crate::defaults::public_id(&db.conn, entity_kind, &id)?;
    Ok(json!({"kind":kind,"reference":short,"entity":entity,"children":children}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::NewTask;
    use crate::workspace::{CreateList, SectionInput, WorkspaceInput};
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn view_route_requires_auth_and_returns_json_entity_and_children() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let token = "synthetic-view-route-token";
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            token.into(),
            "http://127.0.0.1:8092".into(),
        );
        let app = crate::web::router(state);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/view/list/groceries")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/view/list/groceries")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["entity"]["id"], "groceries");
        assert!(value["entity"]["revision"].is_i64());
        assert!(value["children"].is_array());
    }

    #[test]
    fn view_walks_immediate_children_and_counts_only_root_tasks() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let space = db
            .create_workspace(
                "dev",
                WorkspaceInput {
                    title: "Personal".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let list = db
            .create_list(
                "dev",
                CreateList {
                    workspace_id: space.id.clone(),
                    title: "Groceries".into(),
                    description: String::new(),
                    restricted: false,
                },
            )
            .unwrap();
        let section = db
            .create_section(
                "dev",
                &list.id,
                SectionInput {
                    title: "Veggies".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let new_task = |title: &str, parent: Option<String>| NewTask {
            title: title.into(),
            description: String::new(),
            parent_id: parent,
        };
        let root = db
            .capture(
                "dev",
                "view-root",
                Capture {
                    list_id: list.id.clone(),
                    tasks: vec![new_task("Carrots", None)],
                },
            )
            .unwrap()
            .remove(0);
        let child = db
            .capture(
                "dev",
                "view-child",
                Capture {
                    list_id: list.id.clone(),
                    tasks: vec![new_task("Wash", Some(root.id.clone()))],
                },
            )
            .unwrap()
            .remove(0);
        let subchild = db
            .capture(
                "dev",
                "view-subchild",
                Capture {
                    list_id: list.id.clone(),
                    tasks: vec![new_task("Peel", Some(child.id.clone()))],
                },
            )
            .unwrap()
            .remove(0);
        db.set_section("dev", &root.id, root.revision, Some(section.id.clone()))
            .unwrap();
        let done = db
            .capture(
                "dev",
                "view-done",
                Capture {
                    list_id: list.id.clone(),
                    tasks: vec![new_task("Milk", None)],
                },
            )
            .unwrap()
            .remove(0);
        db.conn
            .execute("UPDATE tasks SET complete=1 WHERE id=?1", [&done.id])
            .unwrap();
        let space_view = view_data(&db, "dev", "space", &space.id).unwrap();
        let list_summary = &space_view["children"][0];
        assert_eq!(
            list_summary["counts"],
            json!({"sections":1,"open":1,"done":1})
        );
        assert!(space_view["entity"]["revision"].is_i64());
        let list_view = view_data(&db, "dev", "list", &list.id).unwrap();
        assert_eq!(list_view["children"].as_array().unwrap().len(), 2);
        assert_eq!(list_view["children"][0]["id"], section.id);
        assert_eq!(list_view["children"][1]["id"], done.id);
        assert_eq!(
            list_view["children"][0]["counts"],
            json!({"open":1,"done":0})
        );
        let section_view = view_data(&db, "dev", "section", &section.id).unwrap();
        assert_eq!(section_view["children"].as_array().unwrap().len(), 1);
        assert_eq!(section_view["children"][0]["id"], root.id);
        let task_view = view_data(&db, "dev", "task", &root.id).unwrap();
        assert_eq!(task_view["children"].as_array().unwrap().len(), 1);
        assert_eq!(task_view["children"][0]["id"], child.id);
        let child_view = view_data(&db, "dev", "subtask", &child.id).unwrap();
        assert_eq!(child_view["children"][0]["id"], subchild.id);
        assert!(view_data(&db, "dev", "task", &child.id).is_err());
        assert!(view_data(&db, "other", "space", &space.id).is_err());
        assert!(view_data(&db, "other", "list", &list.id).is_err());
        assert!(view_data(&db, "other", "section", &section.id).is_err());
        assert!(view_data(&db, "other", "task", &root.id).is_err());
    }
}
