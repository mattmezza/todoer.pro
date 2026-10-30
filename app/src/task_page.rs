//! Dedicated stable task URLs. Rendering uses the same live authorization as API reads.
use super::*;
use crate::subtree_query::{SubtreeRead, TaskField};

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/tasks/{id}", get(legacy_page))
}
struct Crumb {
    label: String,
    url: String,
}
struct Child {
    id: String,
    title: String,
    complete: bool,
    revision: i64,
    depth: usize,
}
#[derive(Template)]
#[template(path = "task-page.html")]
struct TaskPage {
    sidebar: Sidebar,
    task: Task,
    public_reference: String,
    change_cursor: i64,
    description_html: String,
    crumbs: Vec<Crumb>,
    children: Vec<Child>,
    destinations: Vec<List>,
    csrf: String,
    account_id: String,
    can_write: bool,
    section_id: String,
    has_more: bool,
}
async fn legacy_page(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    if super::public_http::requested(&s, &h, "task", &id).await? {
        return super::public_http::show(&s, "task", &id, 1).await;
    }
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let requested = id.clone();
    let alias = s
        .run(move |db| {
            let id = crate::defaults::resolve(&db.conn, "task", &id)?;
            let task = crate::core::task(&db.conn, &id)?;
            crate::core::access_rank(&db.conn, &actor, &task.list_id, 1)?;
            crate::defaults::public_id(&db.conn, "task", &task.id)
        })
        .await?;
    if requested == alias {
        return page(State(s), h, Path(requested)).await;
    }
    Ok(Redirect::to(&format!("/tasks/{alias}")).into_response())
}
async fn page(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let actor = match s.auth(&h, false) {
        Ok(a) => a,
        Err(_) if !h.contains_key(header::AUTHORIZATION) => {
            return Ok(Redirect::to(&format!(
                "/signin?next={}",
                encode_query(&format!("/tasks/{id}"))
            ))
            .into_response());
        }
        Err(e) => return Err(e),
    };
    let alias = id.clone();
    let id = s
        .run(move |db| crate::defaults::resolve(&db.conn, "task", &alias))
        .await?;
    let csrf = s.csrf_for(&h);
    let a = actor.clone();
    let page = s
        .run(move |db| {
            let task = crate::core::task(&db.conn, &id)?;
            crate::core::access_rank(&db.conn, &a, &task.list_id, 1)?;
            let can_write = crate::core::access_rank(&db.conn, &a, &task.list_id, 2).is_ok() && db.list_archive_note(&a, &task.list_id)?.is_none();
            let account_id = crate::core::actor_identity(&db.conn, &a)?.0;
            let info = db.list_info(&a, &task.list_id)?;
            let slugs = db.entity_slugs(&a)?;
            let mut crumbs = Vec::new();
            if let Some(workspace) = info.workspace_id {
                let workspace = db.workspace(&a, &workspace)?;
                crumbs.push(Crumb {
                    label: format!(
                        "@{}",
                        slugs
                            .workspaces
                            .get(&workspace.id)
                            .unwrap_or(&workspace.title)
                    ),
                    url: format!("/workspaces/{}", workspace.id),
                });
            }
            let list = db
                .lists(&a)?
                .into_iter()
                .find(|l| l.id == task.list_id)
                .ok_or(Error::NotFound)?;
            crumbs.push(Crumb {
                label: format!("#{}", slugs.lists.get(&list.id).unwrap_or(&list.title)),
                url: format!("/lists/{}", list.id),
            });
            let mut section_root = task.clone();
            while let Some(parent) = &section_root.parent_id {
                section_root = crate::core::task(&db.conn, parent)?;
            }
            let section_id = db.task_section(&a, &section_root.id)?.unwrap_or_default();
            if !section_id.is_empty()
                && !db.conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM list_defaults WHERE section_id=?1)",
                    [&section_id],
                    |r| r.get::<_, bool>(0),
                )?
                && let Some(section) = db
                    .sections(&a, &task.list_id)?
                    .into_iter()
                    .find(|s| s.id == section_id)
            {
                crumbs.push(Crumb {
                    label: format!(
                        "${}",
                        slugs.sections.get(&section.id).unwrap_or(&section.title)
                    ),
                    url: format!("/lists/{}?section={}", list.id, section.id),
                });
            }
            let subtree = db.read_subtree(
                &a,
                &task.id,
                SubtreeRead {
                    limit: 100,
                    fields: vec![TaskField::Title, TaskField::Complete, TaskField::Revision],
                    cursor: None,
                },
            )?;
            for ancestor in subtree
                .context
                .breadcrumbs
                .iter()
                .filter(|x| x.id != task.id)
            {
                crumbs.push(Crumb {
                    label: format!("{}{}", crate::core::task(&db.conn, &ancestor.id)?.reference_prefix(), crate::defaults::public_id(&db.conn, "task", &ancestor.id)?),
                    url: format!("/tasks/{}", crate::defaults::public_id(&db.conn, "task", &ancestor.id)?),
                });
            }
            let children = subtree
                .items
                .into_iter()
                .filter(|x| x["id"] != task.id)
                .map(|x| Child {
                    id: x["id"].as_str().unwrap_or_default().into(),
                    title: x["title"].as_str().unwrap_or_default().into(),
                    complete: x["complete"].as_bool().unwrap_or(false),
                    revision: x["revision"].as_i64().unwrap_or(0),
                    depth: x["depth"].as_u64().unwrap_or(0) as usize,
                })
                .collect();
            let destinations = db
                .lists(&a)?
                .into_iter()
                .filter(|l| crate::core::access_rank(&db.conn, &a, &l.id, 2).is_ok())
                .collect();
            let description_html = crate::taxonomy::render_references(
                &db.conn,
                &a,
                "task",
                &task.id,
                &task.description,
            )?;
            let sidebar = build_sidebar(db, &a, &task.list_id, csrf.clone())?;
            let public_reference = crate::defaults::public_id(&db.conn, "task", &task.id)?;
            let change_cursor = db.conn.query_row(
                "SELECT COALESCE(MAX(h.seq),0) FROM history h JOIN actor_list_access a ON a.list_id=h.list_id WHERE a.actor_id=?1",
                [&a], |row| row.get(0),
            )?;
            Ok(TaskPage {
                change_cursor,
                public_reference,
                sidebar,
                task,
                description_html,
                crumbs,
                children,
                destinations,
                csrf,
                account_id,
                can_write,
                section_id,
                has_more: subtree.has_more,
            })
        })
        .await?;
    Ok(Html(
        page.render()
            .map_err(|_| ApiError::from(Error::Invalid("cannot render task page".into())))?,
    )
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    #[tokio::test]
    async fn dedicated_task_page_is_authorized_sanitized_and_hides_guest_workspace_context() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn.execute_batch("INSERT INTO workspaces(id,title) VALUES('hidden-workspace','Hidden workspace sentinel');INSERT INTO workspace_members VALUES('hidden-workspace','other','owner');UPDATE lists SET workspace_id='hidden-workspace' WHERE id='inbox';").unwrap();
        let public = db
            .capture(
                "dev",
                "page-public",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![crate::core::NewTask {
                        title: "Dedicated task sentinel".into(),
                        description: "**Preserved brief** <script>danger()</script>".into(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let private = db
            .capture(
                "other",
                "page-private",
                Capture {
                    list_id: "private".into(),
                    tasks: vec![crate::core::NewTask {
                        title: "Private task sentinel".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let public_url = crate::defaults::public_id(&db.conn, "task", &public.id).unwrap();
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            "task-page-synthetic-token-24plus".into(),
            "http://127.0.0.1:8092".into(),
        );
        let app = router(state);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/tasks/{}", public_url))
                    .header("authorization", "Bearer task-page-synthetic-token-24plus")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!response.headers().contains_key("location"));
        let html = String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(html.contains("dedicated-task-page"));
        assert!(html.contains("<strong>Preserved brief</strong>"));
        assert!(!html.contains("<script>danger()"));
        assert!(!html.contains("Hidden workspace sentinel"));
        assert!(!html.contains("id=\"task-editor\""));
        assert!(html.contains("data-can-write=\"true\""));
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/tasks/{}", private.id))
                    .header("authorization", "Bearer task-page-synthetic-token-24plus")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(!String::from_utf8_lossy(&body).contains("Private task sentinel"));
    }
}
