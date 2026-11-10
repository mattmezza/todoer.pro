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
    comment_count: u64,
    subtask_count: u64,
    public_reference: String,
    title: String,
    description_html: String,
    complete: bool,
    parent_id: String,
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
    children_html: String,
    initial_subtree: String,
    initial_schedule: String,
    initial_tags: String,
    discussion_html: String,
    progress_label: String,
    progress_percent: String,
    progress_complete: usize,
    progress_total: usize,
    priority: u8,
    priority_label: &'static str,
    date_label: String,
    date_timezone: String,
    destinations: Vec<List>,
    csrf: String,
    account_id: String,
    can_write: bool,
    section_id: String,
    list_color: String,
    has_more: bool,
}
#[derive(Template)]
#[template(path = "task-child.html")]
struct ChildRow<'a> {
    child: &'a Child,
    children_html: String,
    progress: String,
    can_write: bool,
}
#[derive(Default)]
struct RenderedChildren {
    html: String,
    complete: usize,
    total: usize,
    loaded_complete: usize,
    loaded_total: usize,
}
fn render_children(
    children: &[Child],
    parent: &str,
    can_write: bool,
    has_more: bool,
) -> Result<RenderedChildren, Error> {
    let mut result = RenderedChildren::default();
    for child in children.iter().filter(|child| child.parent_id == parent) {
        let nested = render_children(children, &child.id, can_write, has_more)?;
        let leaf = nested.total == 0;
        let complete = if leaf {
            usize::from(child.complete)
        } else {
            nested.complete
        };
        let total = if leaf { 1 } else { nested.total };
        result.complete += complete;
        result.total += total;
        result.loaded_complete += nested.loaded_complete + usize::from(child.complete);
        result.loaded_total += nested.loaded_total + 1;
        let progress = if leaf {
            String::new()
        } else if has_more {
            format!(
                "{}/{} loaded items",
                nested.loaded_complete + usize::from(child.complete),
                nested.loaded_total + 1
            )
        } else {
            format!("{complete}/{total} leaf items")
        };
        result.html.push_str(
            &ChildRow {
                child,
                children_html: nested.html,
                progress,
                can_write,
            }
            .render()
            .map_err(|_| Error::Invalid("Cannot render sub-task".into()))?,
        );
    }
    Ok(result)
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
    let discussion_id = id.clone();
    let mut page = s
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
            let is_personal_inbox: bool = db.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM user_inboxes WHERE user_id=?1 AND list_id=?2)",
                rusqlite::params![account_id, list.id],
                |row| row.get(0),
            )?;
            if is_personal_inbox {
                crumbs.push(Crumb {
                    label: "Inbox".into(),
                    url: "/inbox".into(),
                });
            } else {
                crumbs.push(Crumb {
                    label: format!("#{}", slugs.lists.get(&list.id).unwrap_or(&list.title)),
                    url: format!("/lists/{}", list.id),
                });
            }
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
                    fields: vec![TaskField::Title, TaskField::Description, TaskField::DescriptionHtml, TaskField::CommentCount, TaskField::SubtaskCount, TaskField::Complete, TaskField::Revision, TaskField::Position, TaskField::PublicReference],
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
            let initial_subtree = serde_json::to_string(&subtree).map_err(|_| Error::Invalid("Cannot render sub-tasks".into()))?;
            let schedule = db.get_schedule(&a, &task.id)?;
            let priority = schedule.schedule.priority.unwrap_or(3);
            let priority_label = ["P0 · Highest priority", "P1 · High priority", "P2 · Medium priority", "P3 · Low priority", "P4 · Lowest priority"][usize::from(priority.min(4))];
            let date_timezone = db.notification_preferences(&a)?.timezone;
            let date_label = schedule.schedule.due.as_ref().or(schedule.schedule.planned.as_ref()).map(|moment| {
                let text = match moment {
                    crate::scheduling::Moment::Date { date } => chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map(|d| d.format("%b %-d, %Y").to_string()).unwrap_or_else(|_| date.clone()),
                    crate::scheduling::Moment::Timed { local, timezone } => {
                        let zone = timezone.parse::<chrono_tz::Tz>().unwrap_or(chrono_tz::UTC);
                        let target = date_timezone.parse::<chrono_tz::Tz>().unwrap_or(chrono_tz::UTC);
                        chrono::NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M:%S")
                            .or_else(|_| chrono::NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M"))
                            .ok().and_then(|date| date.and_local_timezone(zone).earliest())
                            .map(|date| date.with_timezone(&target).format("%b %-d, %Y, %-I:%M:%S %p").to_string())
                            .unwrap_or_else(|| local.replace('T', " "))
                    },
                };
                format!("{} {}", if schedule.schedule.due.is_some() { "Due" } else { "Planned for" }, text)
            }).unwrap_or_default();
            let mut initial_schedule = serde_json::to_value(&schedule).map_err(|_| Error::Invalid("Cannot render planning".into()))?;
            initial_schedule["duration_minutes"] = serde_json::json!(db.task_duration(&a, &task.id)?);
            let initial_schedule = initial_schedule.to_string();
            let children = subtree
                .items
                .into_iter()
                .filter(|x| x["id"] != task.id)
                .map(|x| -> Result<Child, Error> { let id = x["id"].as_str().unwrap_or_default().to_owned(); Ok(Child {
                    public_reference: crate::defaults::public_id(&db.conn, "task", &id)?,
                    id,
                    comment_count: x["comment_count"].as_u64().unwrap_or(0),
                    subtask_count: x["subtask_count"].as_u64().unwrap_or(0),
                    title: x["title"].as_str().unwrap_or_default().into(),
                    description_html: x["description_html"].as_str().unwrap_or_default().into(),
                    complete: x["complete"].as_bool().unwrap_or(false),
                    parent_id: x["parent_id"].as_str().unwrap_or_default().into(),
                })})
                .collect::<Result<Vec<_>, _>>()?;
            let rendered = render_children(&children, &task.id, can_write, subtree.has_more)?;
            let progress_label = if rendered.total == 0 { String::new() } else if subtree.has_more {
                format!("{}/{} loaded items complete; load remaining items for leaf progress", rendered.loaded_complete, rendered.loaded_total)
            } else { format!("{}/{} tasks complete", rendered.complete, rendered.total) };
            let progress_percent = if subtree.has_more || rendered.total == 0 { String::new() } else { format!("{}%", (100.0 * rendered.complete as f64 / rendered.total as f64).round() as usize) };
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
            let initial_tags = serde_json::to_string(&db.task_tags(&a, &task.id)?).map_err(|_| Error::Invalid("Cannot render task tags".into()))?;
            Ok(TaskPage {
                list_color: db.list_color(&a, &task.list_id)?.unwrap_or_else(|| "blue".into()),
                change_cursor,
                public_reference,
                sidebar,
                task,
                description_html,
                crumbs,
                children_html: rendered.html,
                initial_subtree,
                initial_tags,
                initial_schedule,
                discussion_html: String::new(),
                progress_label,
                progress_percent,
                progress_complete: rendered.complete,
                progress_total: rendered.total,
                priority,
                priority_label,
                date_label,
                date_timezone,
                destinations,
                csrf,
                account_id,
                can_write,
                section_id,
                has_more: subtree.has_more,
            })
        })
        .await?;
    page.discussion_html = super::discussion_http::initial(s, h, discussion_id).await?;
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
    async fn personal_inbox_tasks_link_to_inbox_instead_of_a_list() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let inbox = crate::planning::ensure_personal_inbox(&db.conn, "dev").unwrap();
        let parent = db
            .capture(
                "dev",
                "breadcrumb-parent",
                Capture {
                    list_id: inbox.clone(),
                    tasks: vec![crate::core::NewTask {
                        title: "Inbox parent".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let child = db
            .capture(
                "dev",
                "breadcrumb-child",
                Capture {
                    list_id: inbox.clone(),
                    tasks: vec![crate::core::NewTask {
                        title: "Inbox child".into(),
                        description: String::new(),
                        parent_id: Some(parent.id.clone()),
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let regular = db
            .capture(
                "dev",
                "breadcrumb-list",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![crate::core::NewTask {
                        title: "A task in a list".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let urls: Vec<_> = [parent, child, regular]
            .iter()
            .map(|task| crate::defaults::public_id(&db.conn, "task", &task.id).unwrap())
            .collect();
        let app = router(AppState::new(
            Arc::new(Mutex::new(db)),
            "task-page-synthetic-token-24plus".into(),
            "http://127.0.0.1:8092".into(),
        ));
        for (index, url) in urls.iter().enumerate() {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/tasks/{url}"))
                        .header("authorization", "Bearer task-page-synthetic-token-24plus")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let html = String::from_utf8(body.to_vec()).unwrap();
            let breadcrumbs = html
                .split("aria-label=\"Task location\"")
                .nth(1)
                .unwrap()
                .split("</nav>")
                .next()
                .unwrap();
            if index < 2 {
                assert!(breadcrumbs.contains("href=\"/inbox\">Inbox</a>"));
                assert!(!breadcrumbs.contains("/lists/intake:"));
                if index == 1 {
                    assert!(breadcrumbs.contains(&format!("/tasks/{}", urls[0])));
                }
            } else {
                assert!(breadcrumbs.contains("href=\"/lists/inbox\">#"));
            }
        }
    }
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
