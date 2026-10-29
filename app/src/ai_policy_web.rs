//! Browser-only account AI controls; agent credentials cannot administer keys or billing.
use super::*;
use crate::ai_policy::{self, AdminUpdate, PolicyUpdate};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/account/ai/settings",
            get(settings).put(update_settings),
        )
        .route("/api/account/ai/access-request", post(request_access))
        .route("/api/account/ai/notices", get(notices).post(read_notices))
        .route(
            "/api/account/ai/key",
            axum::routing::put(save_key).delete(remove_key),
        )
        .route("/api/admin/ai/users", get(users))
        .route("/api/admin/ai/audit", get(audit))
        .route("/api/admin/ai/users/{id}", axum::routing::put(update_user))
        .route("/api/admin/ai/access-requests", get(requests))
        .route("/api/admin/ai/keys/rotate", post(rotate_keys))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("private, no-store"),
        ))
}
async fn session(
    s: &AppState,
    h: &HeaderMap,
    write: bool,
    recent: bool,
) -> Result<String, ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    let actor = if write && s.production.is_some() {
        account_http::session_user(s, h, recent).await?
    } else {
        s.auth(h, write)?
    };
    if actor.starts_with("credential:") {
        return Err(Error::Forbidden.into());
    }
    Ok(actor)
}
// Recheck the persisted session after waiting for the application-store lock.
// This prevents a queued key/admin change from surviving a completed logout.
async fn change<T: Send + 'static>(
    s: &AppState,
    h: &HeaderMap,
    actor: String,
    recent: bool,
    f: impl FnOnce(&mut Store, &str) -> Result<T, Error> + Send + 'static,
) -> Result<T, ApiError> {
    let token_hash = if s.production.is_some() {
        let token = h
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                v.split(';')
                    .find_map(|part| part.trim().strip_prefix("todoer_session="))
            })
            .ok_or(Error::Forbidden)?;
        Some(crate::auth::hash(token))
    } else {
        None
    };
    s.run(move|db|{
        if let Some(hash)=token_hash {
            let live:bool=db.conn.query_row("SELECT EXISTS(SELECT 1 FROM auth_sessions s WHERE s.token_hash=?1 AND s.user_id=?2 AND s.revoked=0 AND s.expires_at>unixepoch() AND (?3=0 OR EXISTS(SELECT 1 FROM auth_recent_sessions r WHERE r.token_hash=s.token_hash AND r.authenticated_at>unixepoch()-300)))",rusqlite::params![hash,actor,recent],|r|r.get(0))?;
            if !live{return Err(Error::Forbidden);}
        }
        f(db,&actor)
    }).await
}
async fn settings(
    State(s): State<AppState>,
    h: HeaderMap,
) -> Result<Json<ai_policy::AiPolicy>, ApiError> {
    let actor = session(&s, &h, false, false).await?;
    s.run(move |db| db.ai_policy(&actor)).await.map(Json)
}
async fn update_settings(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<PolicyUpdate>,
) -> Result<Json<ai_policy::AiPolicy>, ApiError> {
    let actor = session(&s, &h, true, false).await?;
    change(&s, &h, actor, false, move |db, actor| {
        db.ai_update_policy(actor, input)
    })
    .await
    .map(Json)
}
async fn request_access(
    State(s): State<AppState>,
    h: HeaderMap,
) -> Result<Json<ai_policy::AiPolicy>, ApiError> {
    let actor = session(&s, &h, true, false).await?;
    change(&s, &h, actor, false, move |db, actor| {
        db.ai_request_access(actor)
    })
    .await
    .map(Json)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveKey {
    key: String,
    model_id: String,
    revision: i64,
}
async fn validate_remote_key(key: &str) -> Result<(), Error> {
    ai_policy::validate_key_format(key)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| Error::Invalid("Key validation unavailable".into()))?;
    let mut response = client
        .get("https://openrouter.ai/api/v1/key")
        .bearer_auth(key)
        .send()
        .await
        .map_err(|_| Error::Invalid("OpenRouter key validation unavailable".into()))?;
    if !response.status().is_success() {
        return Err(Error::Invalid(
            "OpenRouter could not validate this key".into(),
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Error::Invalid("OpenRouter key validation unavailable".into()))?
    {
        if bytes.len() + chunk.len() > 65_536 {
            return Err(Error::Invalid("Invalid key validation response".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| Error::Invalid("Invalid key validation response".into()))?;
    if !value["data"].is_object()
        || value["data"]["is_management_key"] == true
        || value["data"]["is_provisioning_key"] == true
    {
        return Err(Error::Invalid("Use an OpenRouter inference API key".into()));
    }
    Ok(())
}
async fn save_key(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<SaveKey>,
) -> Result<Json<ai_policy::AiPolicy>, ApiError> {
    let actor = session(&s, &h, true, true).await?;
    ai_policy::validate_key_format(&input.key)?;
    // Confirm key encryption is configured before making any external request.
    let material = ai_policy::encryption_material()?;
    let model = input.model_id.clone();
    let revision = input.revision;
    change(&s, &h, actor.clone(), true, move |db, actor| {
        db.ai_key_check(actor, &model, revision)
    })
    .await?;
    validate_remote_key(&input.key).await?;
    // The user might have logged out, revoked access or edited settings during validation.
    let current = session(&s, &h, true, true).await?;
    if current != actor {
        return Err(Error::Forbidden.into());
    }
    change(&s, &h, actor, true, move |db, actor| {
        db.ai_save_verified_key(
            actor,
            &input.key,
            &input.model_id,
            input.revision,
            &material,
        )
    })
    .await
    .map(Json)
}
async fn remove_key(
    State(s): State<AppState>,
    h: HeaderMap,
) -> Result<Json<ai_policy::AiPolicy>, ApiError> {
    let actor = session(&s, &h, true, true).await?;
    change(&s, &h, actor, true, move |db, actor| {
        db.ai_remove_key(actor)
    })
    .await
    .map(Json)
}
#[derive(Deserialize, Default)]
struct Page {
    after: Option<String>,
    limit: Option<usize>,
}
async fn users(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(page): Query<Page>,
) -> Result<Json<Value>, ApiError> {
    let actor = session(&s, &h, false, false).await?;
    let users = s
        .run(move |db| db.ai_admin_users(&actor, page.after.as_deref(), page.limit.unwrap_or(50)))
        .await?;
    let next = users.last().map(|p| p.user_id.clone());
    Ok(Json(json!({"users":users,"next_cursor":next})))
}
async fn requests(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(page): Query<Page>,
) -> Result<Json<Value>, ApiError> {
    let actor = session(&s, &h, false, false).await?;
    let requests = s
        .run(move |db| {
            db.ai_admin_requests(&actor, page.after.as_deref(), page.limit.unwrap_or(50))
        })
        .await?;
    let next = requests.last().map(|p| p.user_id.clone());
    Ok(Json(json!({"requests":requests,"next_cursor":next})))
}
async fn update_user(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<AdminUpdate>,
) -> Result<Json<ai_policy::AiPolicy>, ApiError> {
    let actor = session(&s, &h, true, true).await?;
    change(&s, &h, actor, true, move |db, actor| {
        db.ai_admin_update(actor, &id, input)
    })
    .await
    .map(Json)
}
async fn rotate_keys(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    let actor = session(&s, &h, true, true).await?;
    let current = ai_policy::encryption_material()?;
    let previous = std::env::var("TODOER_BYOK_PREVIOUS_ENCRYPTION_KEY")
        .map_err(|_| Error::Invalid("Previous encryption key is not configured".into()))?;
    let rotated = change(&s, &h, actor, true, move |db, actor| {
        db.ai_rotate_keys(actor, &current, &previous)
    })
    .await?;
    Ok(Json(json!({"rotated":rotated,"batch_limit":100})))
}

#[derive(Deserialize)]
struct NoticePage {
    after: Option<i64>,
    limit: Option<usize>,
}
async fn notices(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(page): Query<NoticePage>,
) -> Result<Json<Value>, ApiError> {
    let actor = session(&s, &h, false, false).await?;
    let notices = s
        .run(move |db| db.ai_account_notices(&actor, page.after, page.limit.unwrap_or(50)))
        .await?;
    let next = notices.last().map(|n| n.id);
    Ok(Json(json!({"notices":notices,"next_cursor":next})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadNotices {
    through: i64,
}
async fn read_notices(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<ReadNotices>,
) -> Result<Json<Value>, ApiError> {
    let actor = session(&s, &h, true, false).await?;
    change(&s, &h, actor, false, move |db, actor| {
        db.ai_read_notices(actor, input.through)
    })
    .await?;
    Ok(Json(json!({"read":true})))
}

async fn audit(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(page): Query<NoticePage>,
) -> Result<Json<Value>, ApiError> {
    let actor = session(&s, &h, false, false).await?;
    let entries = s
        .run(move |db| db.ai_admin_audit(&actor, page.after, page.limit.unwrap_or(50)))
        .await?;
    let next = entries.last().and_then(|entry| entry["id"].as_i64());
    Ok(Json(json!({"entries":entries,"next_cursor":next})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    async fn fixture(admin: bool) -> (Router, String, String) {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        if admin {
            db.conn
                .execute("INSERT INTO auth_admins VALUES('dev')", [])
                .unwrap();
        }
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            "synthetic-ai-policy-token-at-least-24".into(),
            "http://127.0.0.1:8092".into(),
        );
        let csrf = state.csrf.clone();
        let app = router(state);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/signin")
                    .header("origin", "http://127.0.0.1:8092")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(format!("csrf={csrf}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        (app, csrf, cookie)
    }
    #[tokio::test]
    async fn browser_only_settings_and_access_requests_require_csrf() {
        let (app, csrf, cookie) = fixture(false).await;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/account/ai/settings")
                    .header(
                        "authorization",
                        "Bearer synthetic-ai-policy-token-at-least-24",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/account/ai/settings")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let p: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(p["user_ai_enabled"], false);
        assert_eq!(p["managed_ai_allowed"], false);
        assert_eq!(p["byok_allowed"], true);
        for authorized in [false, true] {
            let mut req = Request::builder()
                .method("POST")
                .uri("/api/account/ai/access-request")
                .header("origin", "http://127.0.0.1:8092")
                .header("cookie", &cookie);
            if authorized {
                req = req.header("x-csrf-token", &csrf);
            }
            let response = app
                .clone()
                .oneshot(req.body(Body::empty()).unwrap())
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
        }
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/admin/ai/users")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    #[tokio::test]
    async fn platform_admin_is_distinct_and_invalid_key_never_reaches_provider() {
        let (app, csrf, cookie) = fixture(true).await;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/admin/ai/users")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/account/ai/key")
                    .header("origin", "http://127.0.0.1:8092")
                    .header("cookie", &cookie)
                    .header("x-csrf-token", &csrf)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"key":"bad key","model_id":"openai/gpt-4.1-mini","revision":1})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
