//! Thin JSON adapter over the reviewed, atomic section-move domain operation.
use super::*;
use crate::workspace::{
    Section, SectionMoveInput, SectionMovePreview, SectionTasksMoveInput, SectionTasksMovePreview,
};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/sections/{id}/move/preview", post(preview))
        .route("/api/v1/sections/{id}/move", post(apply))
        .route(
            "/api/v1/sections/{id}/tasks/move/preview",
            post(tasks_preview),
        )
        .route("/api/v1/sections/{id}/tasks/move", post(tasks_apply))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Destination {
    destination_list_id: String,
}
async fn preview(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<Destination>,
) -> Result<Json<SectionMovePreview>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.section_move_preview(&actor, &id, &input.destination_list_id))
            .await?,
    ))
}
async fn apply(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<SectionMoveInput>,
) -> Result<Json<Section>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.move_section(&actor, &id, input)).await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TasksDestination {
    destination_list_id: String,
    destination_section_id: Option<String>,
}
async fn tasks_preview(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<TasksDestination>,
) -> Result<Json<SectionTasksMovePreview>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| {
            db.section_tasks_move_preview(
                &actor,
                &id,
                &input.destination_list_id,
                input.destination_section_id.as_deref(),
            )
        })
        .await?,
    ))
}
async fn tasks_apply(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<SectionTasksMoveInput>,
) -> Result<Json<SectionTasksMovePreview>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.move_section_tasks(&actor, &id, input))
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    const TOKEN: &str = "synthetic-section-move-http-token";
    fn app() -> (Router, AppState, String) {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let section = db
            .create_section(
                "dev",
                "inbox",
                crate::workspace::SectionInput {
                    title: "Synthetic section".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let task = db
            .capture(
                "dev",
                "section-http",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![crate::core::NewTask {
                        title: "Synthetic task".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        db.set_section("dev", &task.id, task.revision, Some(section.id.clone()))
            .unwrap();
        db.conn
            .execute("INSERT INTO memberships VALUES('groceries','other')", [])
            .unwrap();
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            TOKEN.into(),
            "http://127.0.0.1:8092".into(),
        );
        (routes().with_state(state.clone()), state, section.id)
    }
    async fn send(
        app: &Router,
        path: &str,
        value: Value,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        let mut r = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json");
        for (key, value) in headers {
            r = r.header(*key, *value);
        }
        let response = app
            .clone()
            .oneshot(r.body(Body::from(value.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    fn input(p: &Value, ack: bool) -> Value {
        json!({"destination_list_id":p["destination_list_id"],"section_revision":p["section_revision"],"source_list_revision":p["source_list_revision"],"destination_list_revision":p["destination_list_revision"],"policy_hash":p["policy_hash"],"acknowledge_access_change":ack})
    }
    #[tokio::test]
    async fn router_requires_auth_ack_and_fresh_review_then_preserves_section() {
        let (app, state, id) = app();
        let preview_path = format!("/api/v1/sections/{id}/move/preview");
        let apply_path = format!("/api/v1/sections/{id}/move");
        let auth = format!("Bearer {TOKEN}");
        let headers = [("authorization", auth.as_str())];
        assert_eq!(
            send(
                &app,
                &preview_path,
                json!({"destination_list_id":"groceries"}),
                &[]
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        let (status, p) = send(
            &app,
            &preview_path,
            json!({"destination_list_id":"groceries"}),
            &headers,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(p["audience_broadens"], true);
        assert!(p.get("audience").is_none());
        assert_eq!(
            send(&app, &apply_path, input(&p, false), &headers).await.0,
            StatusCode::CONFLICT
        );
        state
            .store
            .lock()
            .unwrap()
            .update_section(
                "dev",
                &id,
                crate::workspace::SectionChange {
                    revision: p["section_revision"].as_i64().unwrap(),
                    title: Some("Edited after review".into()),
                    description: None,
                },
            )
            .unwrap();
        assert_eq!(
            send(&app, &apply_path, input(&p, true), &headers).await.0,
            StatusCode::CONFLICT
        );
        let (_, p) = send(
            &app,
            &preview_path,
            json!({"destination_list_id":"groceries"}),
            &headers,
        )
        .await;
        let (status, moved) = send(&app, &apply_path, input(&p, true), &headers).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(moved["id"], id);
        assert_eq!(moved["list_id"], "groceries");
        assert_eq!(moved["title"], "Edited after review");
        assert!(
            state
                .store
                .lock()
                .unwrap()
                .tasks("dev", "inbox")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            send(
                &app,
                &preview_path,
                json!({"destination_list_id":"private"}),
                &headers
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn cookie_preview_and_apply_require_csrf_and_strict_fields() {
        let (app, state, id) = app();
        let preview_path = format!("/api/v1/sections/{id}/move/preview");
        let apply_path = format!("/api/v1/sections/{id}/move");
        let session = "synthetic-browser-section-session";
        let hash: [u8; 32] = Sha256::digest(session.as_bytes()).into();
        state
            .sessions
            .lock()
            .unwrap()
            .insert(hash, std::time::Instant::now());
        let cookie = format!("todoer_dev={session}");
        let body = json!({"destination_list_id":"groceries"});
        assert_eq!(
            send(&app, &preview_path, body.clone(), &[("cookie", &cookie)])
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let headers = [
            ("cookie", cookie.as_str()),
            ("x-csrf-token", state.csrf.as_str()),
        ];
        let (status, p) = send(&app, &preview_path, body, &headers).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            send(&app, &apply_path, input(&p, true), &[("cookie", &cookie)])
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let mut bad = input(&p, true);
        bad["ignored_extra_field"] = json!(true);
        assert_eq!(
            send(&app, &apply_path, bad, &headers).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            send(&app, &apply_path, input(&p, true), &headers).await.0,
            StatusCode::OK
        );
    }
    #[tokio::test]
    async fn contents_move_http_enforces_auth_csrf_ack_and_strict_inputs() {
        let (app, state, id) = app();
        let preview = format!("/api/v1/sections/{id}/tasks/move/preview");
        let apply = format!("/api/v1/sections/{id}/tasks/move");
        let body = json!({"destination_list_id":"groceries", "destination_section_id":null});
        assert_eq!(
            send(&app, &preview, body.clone(), &[]).await.0,
            StatusCode::UNAUTHORIZED
        );
        let session = "synthetic-browser-contents-move";
        let hash: [u8; 32] = Sha256::digest(session.as_bytes()).into();
        state
            .sessions
            .lock()
            .unwrap()
            .insert(hash, std::time::Instant::now());
        let cookie = format!("todoer_dev={session}");
        assert_eq!(
            send(&app, &preview, body.clone(), &[("cookie", &cookie)])
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let headers = [
            ("cookie", cookie.as_str()),
            ("x-csrf-token", state.csrf.as_str()),
        ];
        let (status, p) = send(&app, &preview, body, &headers).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(p["affected_task_count"], 1);
        let mut body = json!({"destination_list_id":"groceries", "destination_section_id":null, "policy_hash":p["policy_hash"]});
        assert_eq!(
            send(&app, &apply, body.clone(), &headers).await.0,
            StatusCode::CONFLICT
        );
        body["acknowledge_access_change"] = json!(true);
        assert_eq!(
            send(&app, &apply, body.clone(), &[("cookie", &cookie)])
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let mut bad = body.clone();
        bad["ignored_extra_field"] = json!(true);
        assert_eq!(
            send(&app, &apply, bad, &headers).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(send(&app, &apply, body, &headers).await.0, StatusCode::OK);
        let db = state.store.lock().unwrap();
        assert!(db.tasks("dev", "inbox").unwrap().is_empty());
        assert!(
            db.sections("dev", "inbox")
                .unwrap()
                .iter()
                .any(|s| s.id == id)
        );
    }
}
