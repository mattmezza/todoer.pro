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
    let actor = s.auth(&h, false)?;
    let enabled = s.run(move |db| Ok(db.ai_resolve(&actor).is_ok())).await?;
    Ok(Json(
        json!({"enabled":enabled,"capture_always_available":true,"classification_is_suggestion_only":true,"draft_requires_review":true}),
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
    let a = actor.clone();
    let funding = s.run(move |db| db.ai_resolve(&a)).await?;
    let service = s.assist.for_account(funding.api_key, &funding.model_id);
    let a = actor.clone();
    let prep_service = service.clone();
    let prepared = s
        .run(move |db| db.assist_prepare_classify(&a, input, &prep_service))
        .await?;
    complete(s, actor, prepared, service, h).await
}
async fn draft(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<DraftInput>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, true)?;
    let a = actor.clone();
    let funding = s.run(move |db| db.ai_resolve(&a)).await?;
    let service = s.assist.for_account(funding.api_key, &funding.model_id);
    let a = actor.clone();
    let prep_service = service.clone();
    let prepared = s
        .run(move |db| db.assist_prepare_draft(&a, input, &prep_service))
        .await?;
    complete(s, actor, prepared, service, h).await
}
async fn complete(
    s: AppState,
    actor: String,
    prepared: Prepared,
    service: crate::assist::AssistService,
    h: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    match prepared {
        Prepared::Cached(value) => Ok(Json(value)),
        Prepared::Dispatch(mut plan) => {
            let quote_id = plan.id.clone();
            let model = plan.body["model"].as_str().unwrap_or("").to_owned();
            let (p, m, route) = s
                .run(move |db| {
                    Ok((
                        db.ai_provider_policy()?,
                        db.ai_models(true)?
                            .into_iter()
                            .find(|v| v["id"] == model)
                            .ok_or(Error::NotFound)?,
                        db.conn
                            .query_row(
                                "SELECT route_json FROM ai_operations WHERE id=?1",
                                [quote_id],
                                |r| r.get::<_, String>(0),
                            )
                            .optional()?
                            .map(|v| serde_json::from_str::<Value>(&v))
                            .transpose()
                            .map_err(|_| Error::Invalid("invalid stored route".into()))?,
                    ))
                })
                .await?;
            // The upstream route stays bounded by the operator's configured rates.
            if let Some(route) = route {
                plan.body["provider"] = route;
            } else if p.usd_eur_ppm > 0 {
                plan.body["provider"]["max_price"] = json!({"prompt":m["input_micro_per_million"].as_i64().unwrap_or(0) as f64/p.usd_eur_ppm as f64,"completion":m["output_micro_per_million"].as_i64().unwrap_or(0) as f64/p.usd_eur_ppm as f64,"request":0});
            } else {
                plan.body["provider"]
                    .as_object_mut()
                    .unwrap()
                    .remove("max_price");
            }
            let a = actor.clone();
            let id = plan.id.clone();
            let kind = plan.kind.clone();
            let key = plan.request_key.clone();
            let body = plan.body.clone();
            let quote = s
                .run(move |db| db.ai_quote(&a, &id, &kind, &key, &body))
                .await?;
            let Some(accepted) = h.get("x-todoer-ai-quote").and_then(|v| v.to_str().ok()) else {
                return Ok(Json(
                    json!({"status":"quote","quote":quote,"provider":"OpenRouter","byok_provider_charges_apply":quote.funding=="byok"}),
                ));
            };
            let a = actor.clone();
            let id = plan.id.clone();
            let body = plan.body.clone();
            let accepted = accepted.to_owned();
            let dispatched = s
                .run(move |db| db.ai_dispatch(&a, &id, &accepted, &body))
                .await?;
            let key_revision = dispatched.policy_revision;
            let owner = dispatched.user_id.clone();
            let service = service.for_account(dispatched.api_key, &dispatched.model_id);
            let (mut output, usage) = service.execute_metered(&plan).await;
            if output.is_ok() && usage.model.as_deref() != Some(dispatched.model_id.as_str()) {
                output = Err("invalid_provider_output");
            }
            if quote.funding == "byok"
                && output.as_ref().is_err_and(|e| *e == "invalid_provider_key")
            {
                let a = owner.clone();
                s.run(move |db| db.ai_invalidate_key_revision(&a, key_revision))
                    .await?;
            }
            let operation = plan.id.clone();
            let a = actor.clone();
            // A completed logout/revocation prevents delivery, but never loses expense accounting.
            let delivery_state = s.clone();
            let delivery_headers = h.clone();
            let result = s
                .run(move |db| {
                    let mut tx = crate::agent_ops::StoreWrite::new(db)?;
                    let delivery = delivery_identity(&delivery_state, &delivery_headers, &tx)
                        .is_some_and(|current| current == a);
                    let result = if delivery {
                        tx.assist_finish(&a, *plan, output)
                    } else {
                        Err(Error::Forbidden)
                    };
                    let usable = result
                        .as_ref()
                        .is_ok_and(|v| v["status"] == "draft" || v["status"] == "suggestion");
                    tx.ai_settle_user(&owner, &operation, &usage, usable)?;
                    tx.commit()?;
                    result
                })
                .await?;
            Ok(Json(result))
        }
    }
}
fn delivery_identity(s: &AppState, h: &HeaderMap, db: &Store) -> Option<String> {
    if s.production.is_none()
        && let Some(token) = h
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
    {
        let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        if hash
            .iter()
            .zip(s.token_hash)
            .fold(0u8, |a, (x, y)| a | (x ^ y))
            == 0
        {
            return Some("dev".into());
        }
        return db
            .cli_bearer(
                token,
                &format!("{}/api/v1", s.origin.trim_end_matches('/')),
                true,
            )
            .ok();
    }
    s.auth(h, true).ok()
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
