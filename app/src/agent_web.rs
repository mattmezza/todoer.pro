//! Compact agent workflow routes share the same authorization and domain writes.
use super::*;
use crate::agent_ops::{
    BulkOperation, BulkPreview, BulkResult, Destination, DestinationPreference,
    DestinationResolution,
};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/bulk/preview", post(preview))
        .route("/api/v1/bulk/apply", post(apply))
        .route(
            "/api/v1/destinations/preferences",
            get(preferences).post(set_preference),
        )
        .route("/api/v1/destinations/resolve", get(resolve))
        .route("/api/v1/destinations/search", get(search))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewInput {
    operations: Vec<BulkOperation>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyInput {
    preview_id: String,
    expected_hash: String,
    idempotency_key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreferenceInput {
    key: String,
    list_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveQuery {
    destination: Option<String>,
    default_key: Option<String>,
    #[serde(default)]
    fallback_inbox: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchQuery {
    #[serde(default)]
    prefix: String,
    #[serde(default = "default_limit")]
    limit: usize,
}
fn default_limit() -> usize {
    20
}
async fn preview(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<PreviewInput>,
) -> Result<Json<BulkPreview>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.bulk_preview(&actor, input.operations))
            .await?,
    ))
}
async fn apply(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<ApplyInput>,
) -> Result<Json<BulkResult>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| {
            db.bulk_apply(
                &actor,
                &input.preview_id,
                &input.expected_hash,
                &input.idempotency_key,
            )
        })
        .await?,
    ))
}
async fn preferences(
    State(s): State<AppState>,
    h: HeaderMap,
) -> Result<Json<Vec<DestinationPreference>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| db.destination_preferences(&actor)).await?,
    ))
}
async fn set_preference(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<PreferenceInput>,
) -> Result<Json<DestinationPreference>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.set_destination_preference(&actor, &input.key, &input.list_id))
            .await?,
    ))
}
async fn resolve(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<ResolveQuery>,
) -> Result<Json<DestinationResolution>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| {
            db.resolve_destination(
                &actor,
                q.destination.as_deref(),
                q.default_key.as_deref(),
                q.fallback_inbox,
            )
        })
        .await?,
    ))
}
async fn search(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<Destination>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| db.search_destinations(&actor, &q.prefix, q.limit))
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    const TOKEN: &str = "isolated-agent-http-test-token";
    fn app() -> (Router, Arc<Mutex<Store>>) {
        let mut store = Store::memory().unwrap();
        store.seed_dev().unwrap();
        let store = Arc::new(Mutex::new(store));
        (
            router(AppState::new(
                store.clone(),
                TOKEN.into(),
                "http://127.0.0.1:8092".into(),
            )),
            store,
        )
    }
    async fn send(app: &Router, path: &str, body: Value, authorized: bool) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json");
        if authorized {
            req = req.header("authorization", format!("Bearer {TOKEN}"));
        }
        let response = app
            .clone()
            .oneshot(req.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    #[tokio::test]
    async fn preview_apply_routes_require_auth_and_bind_server_stored_plan() {
        let (app, store) = app();
        let t = store
            .lock()
            .unwrap()
            .capture(
                "dev",
                "http-bulk",
                crate::core::Capture {
                    list_id: "inbox".into(),
                    tasks: vec![crate::core::NewTask {
                        title: "Before".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let payload =
            json!({"operations":[{"op":"update","id":t.id,"revision":1,"title":"After"}]});
        assert_eq!(
            send(&app, "/api/v1/bulk/preview", payload.clone(), false)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        let (status, plan) = send(&app, "/api/v1/bulk/preview", payload, true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            store.lock().unwrap().subtree("dev", &t.id).unwrap()[0].title,
            "Before"
        );
        let invalid = json!({"preview_id":plan["preview_id"],"expected_hash":"tampered","idempotency_key":"http-apply"});
        assert_eq!(
            send(&app, "/api/v1/bulk/apply", invalid, true).await.0,
            StatusCode::CONFLICT
        );
        let valid = json!({"preview_id":plan["preview_id"],"expected_hash":plan["expected_hash"],"idempotency_key":"http-apply"});
        let (status, result) = send(&app, "/api/v1/bulk/apply", valid.clone(), true).await;
        assert_eq!(status, StatusCode::OK);
        let (status, retry) = send(&app, "/api/v1/bulk/apply", valid, true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(result, retry);
        assert_eq!(
            store.lock().unwrap().subtree("dev", &t.id).unwrap()[0].title,
            "After"
        );
        let extra = json!({"operations":[],"actor":"other"});
        assert_eq!(
            send(&app, "/api/v1/bulk/preview", extra, true).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    #[tokio::test]
    async fn preference_route_rejects_private_destination() {
        let (app, _) = app();
        let (status, body) = send(
            &app,
            "/api/v1/destinations/preferences",
            json!({"key":"capture","list_id":"private"}),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"]["code"], "forbidden");
    }
}
