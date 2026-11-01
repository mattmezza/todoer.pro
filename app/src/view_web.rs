//! Immediate, authorized children for the CLI's explorable view.
use super::*;
use rusqlite::params;
use std::collections::HashMap;

#[derive(Default, Deserialize)]
pub(super) struct ViewOptions {
    #[serde(default)]
    full: bool,
    #[serde(default)]
    sub: bool,
}

#[derive(Default, Deserialize)]
pub(super) struct OverviewOptions {
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    archived_only: bool,
    list: Option<String>,
}

pub(super) async fn tag(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| Ok(json!(db.visible_tag_info(&actor, &id)?)))
            .await?,
    ))
}

pub(super) async fn overview(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(kind): Path<String>,
    Query(options): Query<OverviewOptions>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| {
        let mut rows = Vec::new();
        match kind.as_str() {
            "spaces" => {
                for space in db.workspaces(&actor)? {
                    let archived = db.conn.query_row("SELECT coalesce((SELECT archived FROM workspace_lifecycle WHERE workspace_id=?1),0)", [&space.id], |r| r.get::<_,bool>(0))?;
                    if (options.archived_only && !archived) || (!options.archived && !options.archived_only && archived) { continue; }
                    let mut row = serde_json::to_value(&space).map_err(|_| Error::Invalid("could not serialize space".into()))?;
                    row["archived"] = json!(archived);
                    row["public_reference"] = json!(crate::defaults::public_id(&db.conn, "workspace", &space.id)?);
                    rows.push(row);
                }
            }
            "lists" => {
                for list in db.lists(&actor)? {
                    let info = db.list_info(&actor, &list.id)?;
                    if (options.archived_only && !info.archived) || (!options.archived && !options.archived_only && info.archived) { continue; }
                    let mut row = serde_json::to_value(&info).map_err(|_| Error::Invalid("could not serialize list".into()))?;
                    row["public_reference"] = json!(crate::defaults::public_id(&db.conn, "list", &list.id)?);
                    rows.push(row);
                }
            }
            "sections" => {
                let reference = options.list.as_deref().ok_or_else(|| Error::Invalid("list is required".into()))?;
                let list = crate::defaults::resolve(&db.conn, "list", reference)?;
                crate::core::access_rank(&db.conn, &actor, &list, 1)?;
                let mut stmt = db.conn.prepare("SELECT id,title,description,revision,archived FROM sections WHERE list_id=?1 AND NOT EXISTS(SELECT 1 FROM list_defaults d WHERE d.section_id=sections.id) ORDER BY position,id")?;
                let sections = stmt.query_map([&list], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,bool>(4)?)))?.collect::<Result<Vec<_>,_>>()?;
                for (id,title,description,revision,archived) in sections {
                    if (options.archived_only && !archived) || (!options.archived && !options.archived_only && archived) { continue; }
                    rows.push(json!({"id":id,"list_id":list,"title":title,"description":description,"revision":revision,"archived":archived,"public_reference":crate::defaults::public_id(&db.conn,"section",&id)?}));
                }
            }
            "tasks" => {
                let reference = options.list.as_deref().ok_or_else(|| Error::Invalid("list is required".into()))?;
                let list = crate::defaults::resolve(&db.conn, "list", reference)?;
                crate::core::access_rank(&db.conn, &actor, &list, 1)?;
                let mut stmt = db.conn.prepare("SELECT id,archive_group IS NOT NULL FROM tasks WHERE list_id=?1 AND (deleted_at IS NULL OR archive_group IS NOT NULL) ORDER BY position,id")?;
                let tasks = stmt.query_map([&list], |r| Ok((r.get::<_,String>(0)?,r.get::<_,bool>(1)?)))?.collect::<Result<Vec<_>,_>>()?;
                for (id,archived) in tasks {
                    if (options.archived_only && !archived) || (!options.archived && !options.archived_only && archived) { continue; }
                    let task = crate::core::raw_task(&db.conn,&id)?;
                    let mut row = json!(task);
                    row["archived"] = json!(archived);
                    row["public_reference"] = json!(crate::defaults::public_id(&db.conn,"task",&id)?);
                    rows.push(row);
                }
            }
            _ => return Err(Error::Invalid("unknown overview type".into())),
        }
        Ok(json!(rows))
    }).await?))
}

pub(super) async fn view(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((kind, reference)): Path<(String, String)>,
    Query(options): Query<ViewOptions>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| {
            if options.sub && !options.full {
                return Err(Error::Invalid("sub requires full".into()));
            }
            if options.full {
                if kind != "list" {
                    return Err(Error::Invalid("full view requires a list".into()));
                }
                full_list(db, &actor, &reference, options.sub)
            } else {
                view_data(db, &actor, &kind, &reference)
            }
        })
        .await?,
    ))
}

fn full_list(db: &Store, actor: &str, reference: &str, sub: bool) -> Result<Value, Error> {
    let id = crate::defaults::resolve(&db.conn, "list", reference)?;
    let info = db.list_info(actor, &id)?;
    let sections = db.sections(actor, &id)?;
    let mut stmt = db.conn.prepare("SELECT t.id,t.reference,t.list_id,t.parent_id,t.title,'' AS description,t.complete,t.revision,t.position,CASE WHEN d.section_id IS NULL THEN loc.section_id ELSE NULL END FROM tasks t LEFT JOIN task_locations loc ON loc.task_id=t.id LEFT JOIN list_defaults d ON d.section_id=loc.section_id WHERE t.list_id=?1 AND t.deleted_at IS NULL AND (?2 OR t.parent_id IS NULL) ORDER BY t.position,t.id")?;
    let tasks = stmt
        .query_map(params![id, sub], |row| {
            Ok((
                crate::core::task_row(row)?,
                row.get::<_, Option<String>>(9)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut roots: HashMap<Option<String>, Vec<&Task>> = HashMap::new();
    let mut descendants: HashMap<&str, Vec<&Task>> = HashMap::new();
    for (task, section) in &tasks {
        if let Some(parent) = task.parent_id.as_deref() {
            descendants.entry(parent).or_default().push(task);
        } else {
            roots.entry(section.clone()).or_default().push(task);
        }
    }
    let render_tasks = |section: Option<String>| -> Result<Vec<Value>, Error> {
        let mut result = Vec::new();
        let mut stack: Vec<(&Task, usize)> = roots
            .get(&section)
            .into_iter()
            .flatten()
            .rev()
            .map(|task| (*task, 0))
            .collect();
        while let Some((task, depth)) = stack.pop() {
            let mut row = task_summary(db, task)?;
            row["depth"] = json!(depth);
            result.push(row);
            if let Some(children) = descendants.get(task.id.as_str()) {
                stack.extend(children.iter().rev().map(|child| (*child, depth + 1)));
            }
        }
        Ok(result)
    };
    let mut children = Vec::new();
    for section in &sections {
        children.push(json!({"kind":"section","id":section.id,"title":section.title,"reference":crate::defaults::public_id(&db.conn, "section", &section.id)?,"revision":section.revision,"counts":counts(db, &id, Some(&section.id))?,"children":render_tasks(Some(section.id.clone()))?}));
    }
    children.extend(render_tasks(None)?);
    let mut total = counts(db, &id, None)?;
    total["sections"] = json!(sections.len());
    Ok(
        json!({"kind":"list","reference":crate::defaults::public_id(&db.conn, "list", &id)?,"entity":info,"counts":total,"full":true,"children":children}),
    )
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
            .clone()
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
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/view/list/groceries?full=true&sub=true")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["full"], true);
        assert!(value["counts"]["sections"].is_number());
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
        let full = full_list(&db, "dev", &list.id, false).unwrap();
        assert_eq!(full["counts"], json!({"sections":1,"open":1,"done":1}));
        assert_eq!(full["children"][0]["children"].as_array().unwrap().len(), 1);
        assert_eq!(full["children"][0]["children"][0]["id"], root.id);
        assert_eq!(full["children"][1]["id"], done.id);
        let full_sub = full_list(&db, "dev", &list.id, true).unwrap();
        let tasks = full_sub["children"][0]["children"].as_array().unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(
            (tasks[0]["id"].as_str(), tasks[0]["depth"].as_u64()),
            (Some(root.id.as_str()), Some(0))
        );
        assert_eq!(
            (tasks[1]["id"].as_str(), tasks[1]["depth"].as_u64()),
            (Some(child.id.as_str()), Some(1))
        );
        assert_eq!(
            (tasks[2]["id"].as_str(), tasks[2]["depth"].as_u64()),
            (Some(subchild.id.as_str()), Some(2))
        );
        assert!(full_list(&db, "other", &list.id, true).is_err());
    }
}
