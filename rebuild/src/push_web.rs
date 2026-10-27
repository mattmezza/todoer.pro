use super::*;
use crate::push::{PushTransport, RegisterDevice};
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/settings/devices", get(page))
        .route("/api/v1/push/key", get(key))
        .route("/api/v1/push/devices", get(devices).post(register))
        .route("/api/v1/push/devices/{id}", axum::routing::delete(revoke))
        .route("/api/v1/push/preferences", axum::routing::put(preferences))
        .route("/settings/devices/{id}/revoke", post(revoke_form))
}
fn session(s: &AppState, h: &HeaderMap, write: bool) -> Result<(String, String), ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    let actor = s.auth(h, write)?;
    let auth = s.production.as_ref().ok_or_else(|| {
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "push_unavailable",
            "Push requires a production browser login".into(),
        )
    })?;
    let token = h
        .get(header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| {
            h.split(';')
                .find_map(|v| v.trim().strip_prefix("todoer_session="))
        })
        .ok_or(Error::Forbidden)?;
    let store = auth.store.lock().map_err(|_| Error::Forbidden)?;
    let principal = store.authenticate_session(
        token,
        h.get("x-csrf-token").and_then(|v| v.to_str().ok()),
        write,
    )?;
    if principal.actor != actor {
        return Err(Error::Forbidden.into());
    }
    Ok((actor, principal.credential_id.ok_or(Error::Forbidden)?))
}
async fn key(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    session(&s, &h, false)?;
    Ok(Json(
        json!({"public_key":s.push.as_ref().map(|p|p.public_key()),"available":s.push.is_some()}),
    )
    .into_response())
}
async fn devices(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let (a, _) = session(&s, &h, false)?;
    Ok(Json(s.run(move |db| db.push_devices(&a)).await?).into_response())
}
async fn register(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<RegisterDevice>,
) -> Result<Response, ApiError> {
    let (a, id) = session(&s, &h, true)?;
    let transport = s.push.as_ref().ok_or_else(|| {
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "push_unavailable",
            "Push delivery is not configured".into(),
        )
    })?;
    transport.validate_subscription(&input)?;
    Ok((
        StatusCode::CREATED,
        Json(
            s.run(move |db| db.register_push_device(&a, &id, input))
                .await?,
        ),
    )
        .into_response())
}
async fn revoke(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let (a, _) = session(&s, &h, true)?;
    s.run(move |db| db.revoke_push_device(&a, &id)).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preference {
    opt_in: bool,
}
async fn preferences(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<Preference>,
) -> Result<Response, ApiError> {
    let (a, _) = session(&s, &h, true)?;
    s.run(move |db| db.set_push_opt_in(&a, input.opt_in))
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
#[derive(Deserialize)]
struct RevokeForm {
    csrf: String,
}
async fn revoke_form(
    State(s): State<AppState>,
    mut h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<RevokeForm>,
) -> Result<Response, ApiError> {
    if !s.origin_ok(&h) {
        return Err(Error::Forbidden.into());
    }
    h.insert(
        "x-csrf-token",
        f.csrf.parse().map_err(|_| Error::Forbidden)?,
    );
    let (a, _) = session(&s, &h, true)?;
    s.run(move |db| db.revoke_push_device(&a, &id)).await?;
    Ok(Redirect::to("/settings/devices").into_response())
}
async fn page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    let configured = s.production.is_some() && s.push.is_some();
    let mut body = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Devices · Todoer</title><link rel=\"stylesheet\" href=\"/assets/app.css\"></head><body><a class=\"skip\" href=\"#main\">Skip to devices</a><header class=\"mobile-header\"><a class=\"brand\" href=\"/\">todoer<span>.pro</span></a><a class=\"nav-link\" href=\"/settings/devices\">Devices</a></header><aside aria-label=\"Navigation\"><a class=\"brand\" href=\"/\">todoer<span>.pro</span></a><nav class=\"review-nav\"><a class=\"nav-link\" href=\"/\">Tasks</a><a class=\"nav-link\" href=\"/settings/notifications\">Notifications</a><a class=\"nav-link active\" href=\"/settings/devices\">Devices</a></nav></aside><main id=\"main\"><div class=\"breadcrumbs\"><a href=\"/\">Todoer</a><span>/</span><span>Devices</span></div><h1>Devices and push reminders</h1><p>Push notifications show only “Todoer reminder”. Open Todoer to see the task. Signing out stops future delivery for devices registered in that session. Already displayed notifications may remain in your notification center.</p>",
    );
    if configured {
        let settings = s.run(move |db| db.push_devices(&a)).await?;
        body.push_str(&format!("<section class=\"grid gap-3 mt-8\" data-push-settings data-csrf=\"{}\"><p data-push-status role=\"status\">Checking browser support…</p><label>Device name <input data-push-label maxlength=\"100\" value=\"This browser\"></label><button type=\"button\" data-push-enable hidden class=\"primary\">Enable reminders on this browser</button><button type=\"button\" data-push-disable hidden class=\"primary\">Disable this browser</button><label><input type=\"checkbox\" data-push-opt-in {}> Allow push reminders on my devices</label><noscript>Enabling push requires JavaScript and browser permission. You can revoke existing devices below without JavaScript.</noscript></section>",esc(&s.csrf_for(&h)),if settings.opt_in {"checked"}else{""}));
        body.push_str("<h2>Registered devices</h2>");
        if settings.devices.is_empty() {
            body.push_str("<p>No devices registered.</p>");
        }
        for d in settings.devices {
            body.push_str(&format!(
                "<section class=\"task\"><p>{} · {}</p>",
                esc(&d.label),
                if !d.active {
                    "Disabled"
                } else if !d.session_live {
                    "Signed out or expired"
                } else {
                    "Active"
                }
            ));
            if d.active {
                body.push_str(&format!("<form method=\"post\" action=\"/settings/devices/{}/revoke\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button class=\"primary\">Revoke device</button></form>",esc(&d.id),esc(&s.csrf_for(&h))));
            }
            body.push_str("</section>");
        }
        body.push_str("<script type=\"module\" src=\"/assets/push.js\"></script>");
    } else {
        body.push_str("<p>Push delivery is unavailable here. It requires a production browser login and server VAPID configuration. Development sessions do not register push devices.</p>");
    }
    body.push_str("<p>On iPhone or iPad, add Todoer to your Home Screen and open it there before enabling push. Browser or operating-system settings can block notifications. Delivery is best effort.</p><a href=\"/settings/notifications\">Notification quiet hours and muted lists</a></main></body></html>");
    let mut response =
        super::scheduling_http::render_settings(&s, &h, "Devices", body, "devices").await?;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    Ok(response)
}
pub(super) fn start_push_worker(
    s: AppState,
    transport: PushTransport,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while !s.stopping.load(std::sync::atomic::Ordering::SeqCst) {
            if crate::push::worker_tick(&s.store, &transport)
                .await
                .is_err()
            {
                eprintln!("push worker: delivery state unavailable");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    struct Fixture {
        state: AppState,
        dir: std::path::PathBuf,
        cookie: String,
        csrf: String,
        token: String,
    }
    impl Fixture {
        fn new() -> Self {
            let dir =
                std::env::temp_dir().join(format!("todoer-push-http-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&dir).unwrap();
            let path = dir.join("synthetic.db");
            let mut db = Store::open(path.to_str().unwrap()).unwrap();
            db.seed_dev().unwrap();
            let auth = crate::auth::AuthStore::open(path.to_str().unwrap()).unwrap();
            let grant = auth.issue_session("dev", 3600).unwrap();
            let cookie = format!("todoer_session={}; todoer_csrf={}", grant.token, grant.csrf);
            let state = AppState::production(
                Arc::new(Mutex::new(db)),
                auth,
                crate::oidc::GoogleOidc::test_fixture(),
                "https://todoer.example".into(),
            )
            .with_push(Some(PushTransport::test_fixture()));
            Self {
                state,
                dir,
                cookie,
                csrf: grant.csrf,
                token: grant.token,
            }
        }
        async fn request(
            &self,
            method: &str,
            path: &str,
            body: Value,
            csrf: bool,
            bearer: bool,
        ) -> (StatusCode, Value) {
            let mut r = Request::builder()
                .method(method)
                .uri(path)
                .header(header::COOKIE, &self.cookie)
                .header(header::CONTENT_TYPE, "application/json");
            if csrf {
                r = r.header("x-csrf-token", &self.csrf);
            }
            if bearer {
                r = r.header(header::AUTHORIZATION, "Bearer synthetic");
            }
            let response = super::super::router(self.state.clone())
                .oneshot(r.body(Body::from(body.to_string())).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (
                status,
                serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    #[tokio::test]
    async fn production_cookie_csrf_device_scope_and_logout() {
        use base64::Engine;
        let mut f = Fixture::new();
        let (status, key) = f
            .request("GET", "/api/v1/push/key", Value::Null, false, false)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(key["available"], true);
        let (k, a) = ece::generate_keypair_and_auth_secret().unwrap();
        let input = json!({"endpoint":"https://fcm.googleapis.com/synthetic", "keys":{"p256dh":base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(k.pub_as_raw().unwrap()),"auth":base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(a)},"label":"Synthetic browser"});
        assert_eq!(
            f.request("POST", "/api/v1/push/devices", input.clone(), false, false)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            f.request("POST", "/api/v1/push/devices", input.clone(), true, true)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let mut bad = input.clone();
        bad["endpoint"] = json!("https://127.0.0.1/private");
        assert_eq!(
            f.request("POST", "/api/v1/push/devices", bad, true, false)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        let (status, device) = f
            .request("POST", "/api/v1/push/devices", input, true, false)
            .await;
        assert_eq!(status, StatusCode::CREATED);
        let id = device["id"].as_str().unwrap();
        assert!(device.get("endpoint").is_none());
        assert!(device.get("keys").is_none());
        let (_, settings) = f
            .request("GET", "/api/v1/push/devices", Value::Null, false, false)
            .await;
        assert_eq!(settings["devices"][0]["session_live"], true);
        {
            let auth = f.state.production.as_ref().unwrap().store.lock().unwrap();
            auth.revoke_session(&f.token).unwrap();
        }
        assert_eq!(
            f.request("GET", "/api/v1/push/devices", Value::Null, false, false)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        let new = f
            .state
            .production
            .as_ref()
            .unwrap()
            .store
            .lock()
            .unwrap()
            .issue_session("dev", 3600)
            .unwrap();
        f.cookie = format!("todoer_session={}; todoer_csrf={}", new.token, new.csrf);
        f.csrf = new.csrf;
        let (_, settings) = f
            .request("GET", "/api/v1/push/devices", Value::Null, false, false)
            .await;
        assert_eq!(settings["devices"][0]["session_live"], false);
        assert_eq!(
            f.request(
                "DELETE",
                &format!("/api/v1/push/devices/{id}"),
                Value::Null,
                true,
                false
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
    }
    #[tokio::test]
    async fn development_push_registration_is_explicitly_unavailable() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            "synthetic".into(),
            "http://127.0.0.1".into(),
        );
        let response = super::super::router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/settings/devices")
                    .header(header::AUTHORIZATION, "Bearer synthetic")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(body.contains("Development sessions do not register"));
        assert!(!body.contains("data-push-enable"));
        let response = super::super::router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/push/key")
                    .header(header::AUTHORIZATION, "Bearer synthetic")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
