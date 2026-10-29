//! REST and native review share the optional assist service; no client API key.
use super::*;
use crate::assist::{
    ApplyInput, CaptureInput, ClassifyInput, DraftInput, DraftPayload, Prepared, RuleInput,
};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/settings/assist",
            get(|| async { Redirect::to("/settings/ai") }),
        )
        .route("/assist", get(|| async { Redirect::to("/settings/ai") }))
        .route("/api/v1/assist/status", get(status))
        .route("/api/v1/assist/capture", post(capture))
        .route("/api/v1/assist/classify", post(classify))
        .route("/api/v1/assist/drafts", post(draft))
        .route("/api/v1/assist/drafts/{id}/review", post(review))
        .route("/api/v1/assist/apply", post(apply))
        .route("/api/v1/assist/rules", get(rules).put(set_rule))
        .route(
            "/api/v1/assist/rules/{id}",
            axum::routing::delete(delete_rule),
        )
}
async fn status(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    s.auth(&h, false)?;
    Ok(Json(
        json!({"enabled":s.assist.enabled(),"capture_always_available":true,"classification_is_suggestion_only":true,"draft_requires_review":true}),
    ))
}
async fn capture(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<CaptureInput>,
) -> Result<Response, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(s.run(move |db| db.assist_capture(&actor, input)).await?).into_response())
}
async fn classify(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<ClassifyInput>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, true)?;
    let service = s.assist.clone();
    let a = actor.clone();
    let prepared = s
        .run(move |db| db.assist_prepare_classify(&a, input, &service))
        .await?;
    complete(s, actor, prepared).await
}
async fn draft(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<DraftInput>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, true)?;
    let service = s.assist.clone();
    let a = actor.clone();
    let prepared = s
        .run(move |db| db.assist_prepare_draft(&a, input, &service))
        .await?;
    complete(s, actor, prepared).await
}
async fn complete(s: AppState, actor: String, prepared: Prepared) -> Result<Json<Value>, ApiError> {
    match prepared {
        Prepared::Cached(value) => Ok(Json(value)),
        Prepared::Dispatch(plan) => {
            let output = s.assist.execute(&plan).await;
            Ok(Json(
                s.run(move |db| {
                    db.assist_finish(&actor, *plan, output.map_err(|_| "provider_unavailable"))
                })
                .await?,
            ))
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    revision: i64,
    payload: DraftPayload,
}
async fn review(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<Review>,
) -> Result<Response, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.assist_review(&actor, &id, input.revision, input.payload))
            .await?,
    )
    .into_response())
}
async fn apply(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<ApplyInput>,
) -> Result<Response, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(s.run(move |db| db.assist_apply(&actor, input)).await?).into_response())
}
async fn rules(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.assist_rules(&actor)).await?).into_response())
}
async fn set_rule(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<RuleInput>,
) -> Result<Response, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(s.run(move |db| db.assist_set_rule(&actor, input)).await?).into_response())
}
async fn delete_rule(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, true)?;
    s.run(move |db| db.assist_delete_rule(&actor, &id)).await?;
    Ok(Json(json!({"deleted":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    #[tokio::test]
    async fn actual_routes_enforce_auth_csrf_and_capture_without_provider() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            "synthetic-assist-token-at-least-24".into(),
            "http://127.0.0.1:8092".into(),
        );
        let csrf = state.csrf.clone();
        let app = router(state);
        let body = json!({"idempotency_key":"http-capture","text":"Exact captured text","list_id":"inbox","default_key":null});
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/assist/capture")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let signin = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/signin")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("origin", "http://127.0.0.1:8092")
                    .body(Body::from(format!("csrf={csrf}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(signin.status(), StatusCode::SEE_OTHER);
        let cookie = signin.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        for authorized in [false, true] {
            let mut request = Request::builder()
                .method("POST")
                .uri("/api/v1/assist/capture")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .header("origin", "http://127.0.0.1:8092");
            if authorized {
                request = request.header("x-csrf-token", &csrf);
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::from(body.to_string())).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                if authorized {
                    StatusCode::OK
                } else {
                    StatusCode::FORBIDDEN
                }
            );
            if authorized {
                let value: Value = serde_json::from_slice(
                    &response.into_body().collect().await.unwrap().to_bytes(),
                )
                .unwrap();
                assert_eq!(value["task"]["description"], "Exact captured text");
                assert_eq!(value["original_text_preserved"], true);
            }
        }
        let page = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/settings/assist")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::SEE_OTHER);
        assert_eq!(page.headers()["location"], "/settings/ai");
        let status = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/assist/status")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let value: Value =
            serde_json::from_slice(&status.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(value["enabled"], false);
    }
}
