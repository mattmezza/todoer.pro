//! Browser identity binding and live credential authentication.
use super::*;
use crate::auth::{AuthError, AuthStore, Permission};
use crate::oidc::{GoogleOidc, PendingLogin};
use std::collections::HashMap;

pub(super) struct ProductionAuth {
    pub store: Mutex<AuthStore>,
    pub oidc: Option<GoogleOidc>,
    pending: Mutex<HashMap<[u8; 32], PendingSignIn>>,
}
struct PendingSignIn {
    oidc: PendingLogin,
    next: String,
    invitation: Option<String>,
}
#[derive(Deserialize, Default)]
pub(super) struct LoginContext {
    pub next: Option<String>,
    pub invitation: Option<String>,
}
pub(super) fn safe_next(value: Option<&str>) -> String {
    value
        .filter(|s| {
            s.starts_with('/')
                && !s.starts_with("//")
                && !s.contains('\\')
                && !s.chars().any(char::is_control)
                && s.len() <= 2048
        })
        .unwrap_or("/")
        .to_string()
}
fn cookie<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    h.get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|s| {
            let (key, value) = s.trim().split_once('=')?;
            (key == name).then_some(value)
        })
}
impl From<AuthError> for ApiError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::Unauthorized => Self(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Authentication required".into(),
            ),
            AuthError::Forbidden => {
                Self(StatusCode::FORBIDDEN, "forbidden", "Access denied".into())
            }
            AuthError::Invalid(s) => Self(StatusCode::BAD_REQUEST, "invalid", s),
            AuthError::Database(_) => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database",
                "Authentication unavailable".into(),
            ),
        }
    }
}
impl AppState {
    pub fn production(
        store: Arc<Mutex<Store>>,
        auth: AuthStore,
        oidc: GoogleOidc,
        origin: String,
    ) -> Self {
        Self::production_optional(store, auth, Some(oidc), origin)
    }
    pub fn production_optional(store: Arc<Mutex<Store>>, auth: AuthStore, oidc: Option<GoogleOidc>, origin: String) -> Self {
        let mut state = Self::new(store, uuid::Uuid::new_v4().to_string(), origin);
        state.production = Some(Arc::new(ProductionAuth {
            store: Mutex::new(auth),
            oidc,
            pending: Mutex::new(HashMap::new()),
        }));
        state
    }
    pub(super) fn production_actor(
        &self,
        h: &HeaderMap,
        write: bool,
        mcp: bool,
    ) -> Result<String, ApiError> {
        let auth = self.production.as_ref().expect("production authentication");
        let store = auth.store.lock().map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth",
                "Authentication unavailable".into(),
            )
        })?;
        if let Some(token) = h
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
        {
            let audience = if mcp {
                format!("{}/mcp", self.origin)
            } else {
                format!("{}/api/v1", self.origin)
            };
            let principal = store.authenticate_agent(
                token,
                &audience,
                if write {
                    Permission::Write
                } else {
                    Permission::Read
                },
                None,
                None,
            )?;
            return Ok(format!(
                "credential:{}",
                principal.credential_id.expect("agent credential")
            ));
        }
        if mcp {
            return Err(AuthError::Unauthorized.into());
        }
        let token = cookie(h, "todoer_session").ok_or(AuthError::Unauthorized)?;
        let csrf = h.get("x-csrf-token").and_then(|v| v.to_str().ok());
        let principal = store.authenticate_session(token, csrf, write)?;
        Ok(principal.actor)
    }
    pub(crate) fn mcp_actor(&self, h: &HeaderMap) -> Result<String, ApiError> {
        if self.production.is_some() {
            self.production_actor(h, false, true)
        } else {
            let actor = self.auth(h, false)?;
            if actor.starts_with("credential:") {
                return Err(Error::Forbidden.into());
            }
            Ok(actor)
        }
    }
    pub(super) fn csrf_for(&self, h: &HeaderMap) -> String {
        if self.production.is_some() {
            cookie(h, "todoer_csrf").unwrap_or("").to_owned()
        } else {
            self.csrf.clone()
        }
    }
    pub(super) fn origin_ok(&self, h: &HeaderMap) -> bool {
        h.get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .is_none_or(|origin| origin == self.origin)
    }
}
pub(super) async fn google_start(
    State(s): State<AppState>,
    Query(context): Query<LoginContext>,
) -> Result<Response, ApiError> {
    let auth = s.production.as_ref().ok_or(ApiError(
        StatusCode::NOT_FOUND,
        "not_found",
        "Google login is disabled in development mode".into(),
    ))?;
    if let Some(token) = context.invitation.clone() {
        let signer = s.invitations.clone().ok_or(AuthError::Forbidden)?;
        s.run(move |db| db.check_invitation(&token, None, &signer))
            .await?;
    }
    if !auth.store.lock().map_err(|_|AuthError::Unauthorized)?.auth_policy()?.google_enabled { return Err(AuthError::Forbidden.into()); }
    let start = auth.oidc.as_ref().ok_or(AuthError::Forbidden)?.begin_login();
    let binding = uuid::Uuid::new_v4().to_string();
    let hash = Sha256::digest(binding.as_bytes()).into();
    let mut pending = auth.pending.lock().map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "auth",
            "Sign-in unavailable".into(),
        )
    })?;
    pending.retain(|_, v| !v.oidc.is_expired());
    if pending.len() >= 512 {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "busy",
            "Try signing in later".into(),
        ));
    }
    pending.insert(
        hash,
        PendingSignIn {
            oidc: start.pending,
            next: safe_next(context.next.as_deref()),
            invitation: context.invitation,
        },
    );
    let mut response = Redirect::temporary(&start.authorization_url).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        format!(
            "todoer_login={binding}; Max-Age=600; HttpOnly; SameSite=Lax; Path=/auth{}",
            if s.origin.starts_with("https://") {
                "; Secure"
            } else {
                ""
            }
        )
        .parse()
        .unwrap(),
    );
    Ok(response)
}
#[derive(Deserialize)]
pub(super) struct Callback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}
pub(super) async fn google_callback(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<Callback>,
) -> Result<Response, ApiError> {
    let auth = s.production.as_ref().ok_or(AuthError::Unauthorized)?;
    let binding = cookie(&h, "todoer_login").ok_or(AuthError::Unauthorized)?;
    let hash: [u8; 32] = Sha256::digest(binding.as_bytes()).into();
    // Consume before any network operation so callbacks cannot replay.
    let pending = auth
        .pending
        .lock()
        .map_err(|_| AuthError::Unauthorized)?
        .remove(&hash)
        .ok_or(AuthError::Unauthorized)?;
    if q.error.is_some() {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "google_denied",
            "Google sign-in was cancelled".into(),
        ));
    }
    if !auth.store.lock().map_err(|_|AuthError::Unauthorized)?.auth_policy()?.google_enabled { return Err(AuthError::Forbidden.into()); }
    let identity = auth
        .oidc.as_ref().ok_or(AuthError::Forbidden)?
        .complete_login(
            pending.oidc,
            q.state.as_deref().unwrap_or(""),
            q.code.as_deref().unwrap_or(""),
        )
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::UNAUTHORIZED,
                "google_invalid",
                "Google sign-in could not be verified; start again".into(),
            )
        })?;
    let email = identity
        .registration_email()
        .map_err(|_| AuthError::Forbidden)?;
    if let Some(token) = pending.invitation.clone() {
        let signer = s.invitations.clone().ok_or(AuthError::Forbidden)?;
        let email = email.to_owned();
        s.run(move |db| db.check_invitation(&token, Some(&email), &signer))
            .await?;
    }
    let user = {
        let mut store = auth.store.lock().map_err(|_| AuthError::Unauthorized)?;
        if pending.invitation.is_some() {
            store.accept_invited_identity(&identity.issuer, &identity.subject, email)?
        } else {
            store.accept_identity(&identity.issuer, &identity.subject, email, true)?
        }
    };
    if let Some(token) = pending.invitation {
        let actor = user.id.clone();
        let signer = s.invitations.clone().ok_or(AuthError::Forbidden)?;
        let email = email.to_owned();
        s.run(move |db| db.claim_invitation(&actor, &token, &email, &signer))
            .await?;
    }
    // New users receive a private destination using normal authorized operations.
    let actor = user.id.clone();
    let issuer = identity.issuer.clone();
    let subject = identity.subject.clone();
    let picture = identity.picture_url.clone();
    let display_name = identity.display_name.clone();
    s.run(move |store| {
        store.conn.execute("UPDATE auth_identities SET picture_url=?1 WHERE issuer=?2 AND subject=?3 AND user_id=?4", rusqlite::params![picture,issuer,subject,actor])?;
        if let Some(name) = display_name.as_deref().map(str::trim).filter(|n| !n.is_empty() && n.chars().count() <= 200) {
            store.conn.execute("UPDATE users SET name=?1 WHERE id=?2 AND name='Todoer user'", rusqlite::params![name,actor])?;
        }
        store.ensure_user_defaults(&actor)
    })
        .await?;
    let epoch = auth.store.lock().map_err(|_|AuthError::Unauthorized)?.epoch(&user.id)?;
    if auth.store.lock().map_err(|_|AuthError::Unauthorized)?.has_mfa(&user.id)? {
        return account_http::mfa_redirect(&s,user.id,epoch,pending.next);
    }
    let grant = auth
        .store
        .lock()
        .map_err(|_| AuthError::Unauthorized)?
        .issue_verified_session(&user.id, epoch)?;
    let mut response = Redirect::to(&pending.next).into_response();
    let secure = if s.origin.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    for (name, value) in [("todoer_session", grant.token), ("todoer_csrf", grant.csrf)] {
        response.headers_mut().append(
            header::SET_COOKIE,
            format!("{name}={value}; Max-Age=43200; HttpOnly; SameSite=Lax; Path=/{secure}")
                .parse()
                .unwrap(),
        );
    }
    response.headers_mut().append(
        header::SET_COOKIE,
        format!("todoer_login=; Max-Age=0; HttpOnly; SameSite=Lax; Path=/auth{secure}")
            .parse()
            .unwrap(),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Permission;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    struct Fixture {
        state: AppState,
        path: String,
        token: String,
        cookie: String,
        csrf: String,
    }
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("todoer-auth-http-{}.db", uuid::Uuid::new_v4()))
                .to_string_lossy()
                .to_string();
            let mut store = Store::open(&path).unwrap();
            store.seed_dev().unwrap();
            let auth = AuthStore::open(&path).unwrap();
            let session = auth.issue_session("dev", 3600).unwrap();
            let token = auth
                .issue_agent(
                    "dev",
                    "https://todoer.example/api/v1",
                    vec![Permission::Write],
                    Some(vec!["inbox".into()]),
                    None,
                    3600,
                )
                .unwrap()
                .token;
            let state = AppState::production(
                Arc::new(Mutex::new(store)),
                auth,
                GoogleOidc::test_fixture(),
                "https://todoer.example".into(),
            );
            Self {
                state,
                path,
                token,
                cookie: format!(
                    "todoer_session={}; todoer_csrf={}",
                    session.token, session.csrf
                ),
                csrf: session.csrf,
            }
        }
        async fn request(
            &self,
            method: &str,
            path: &str,
            body: Value,
            bearer: bool,
        ) -> (StatusCode, Value) {
            let mut builder = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .header("idempotency-key", uuid::Uuid::new_v4().to_string());
            if bearer {
                builder = builder.header("authorization", format!("Bearer {}", self.token));
            } else {
                builder = builder
                    .header("cookie", &self.cookie)
                    .header("x-csrf-token", &self.csrf);
            }
            let response = router(self.state.clone())
                .oneshot(builder.body(Body::from(body.to_string())).unwrap())
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
            for suffix in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffix}", self.path));
            }
        }
    }
    #[tokio::test]
    async fn production_credentials_apply_live_scope_to_discovery_writes_search_and_history() {
        let f = Fixture::new();
        let (status, lists) = f.request("GET", "/api/v1/lists", Value::Null, true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(lists.as_array().unwrap().len(), 1);
        assert_eq!(lists[0]["id"], "inbox");
        let body = json!({"list_id":"groceries","tasks":[{"title":"Denied"}]});
        assert_eq!(
            f.request("POST", "/api/v1/capture", body, true).await.0,
            StatusCode::FORBIDDEN
        );
        let captured = f
            .state
            .run(|s| {
                s.capture(
                    "dev",
                    "scope-fixture",
                    Capture {
                        list_id: "groceries".into(),
                        tasks: vec![crate::core::NewTask {
                            title: "Hidden secret".into(),
                            description: String::new(),
                            parent_id: None,
                        }],
                    },
                )
            })
            .await
            .unwrap();
        assert_eq!(
            f.request("GET", "/api/v1/search?query=Hidden", Value::Null, true)
                .await
                .1,
            json!([])
        );
        assert_eq!(
            f.request("GET", "/api/v1/changes", Value::Null, true)
                .await
                .1,
            json!([])
        );
        assert_eq!(
            f.request(
                "GET",
                &format!("/api/v1/tasks/{}/subtree", captured[0].id),
                Value::Null,
                true
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        let auth = f.state.production.as_ref().unwrap();
        let principal = auth
            .store
            .lock()
            .unwrap()
            .authenticate_agent(
                &f.token,
                "https://todoer.example/api/v1",
                Permission::Read,
                None,
                None,
            )
            .unwrap();
        auth.store
            .lock()
            .unwrap()
            .revoke_agent("dev", principal.credential_id.as_deref().unwrap())
            .unwrap();
        assert_eq!(
            f.request("GET", "/api/v1/lists", Value::Null, true).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    #[tokio::test]
    async fn browser_identity_is_account_scoped_and_logout_revokes_persisted_session() {
        let f = Fixture::new();
        let path = {
            let db = f.state.store.lock().unwrap();
            format!(
                "/lists/{}",
                crate::defaults::public_id(&db.conn, "list", "inbox").unwrap()
            )
        };
        let response = router(f.state.clone())
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("cookie", &f.cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.headers()["x-todoer-account"], "dev");
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
        assert!(html.contains("name=\"account-id\" content=\"dev\""));
        let response = router(f.state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/logout")
                    .header("cookie", &f.cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            f.request("POST", "/logout", Value::Null, false).await.0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            f.request("GET", "/api/v1/lists", Value::Null, false)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            f.request("POST", "/signin", json!({}), false).await.0,
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }
    #[tokio::test]
    async fn mcp_audience_is_distinct_and_open_http_stream_observes_revocation() {
        let f = Fixture::new();
        assert!(
            f.state
                .mcp_actor(&{
                    let mut h = HeaderMap::new();
                    h.insert(
                        header::AUTHORIZATION,
                        format!("Bearer {}", f.token).parse().unwrap(),
                    );
                    h
                })
                .is_err()
        );
        let response = router(f.state.clone())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/events?list_id=inbox")
                    .header("cookie", &f.cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        f.state
            .run(|s| {
                s.conn.execute(
                    "DELETE FROM memberships WHERE list_id='inbox' AND user_id='dev'",
                    [],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        let mut body = response.into_body();
        let frame = tokio::time::timeout(std::time::Duration::from_secs(5), body.frame())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(String::from_utf8_lossy(frame.data_ref().unwrap()).contains("access-revoked"));
    }
    #[tokio::test]
    #[ignore = "explicit official Python SDK OAuth integration; requires uv and loopback networking"]
    async fn official_sdk_discovers_oauth_and_edits_over_real_http() {
        let mut fixture = Fixture::new();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        fixture.state.origin = format!("http://{}", listener.local_addr().unwrap());
        let url = format!("{}/mcp", fixture.state.origin);
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let state = fixture.state.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, router(state))
                .with_graceful_shutdown(async {
                    let _ = stop_rx.await;
                })
                .await
                .unwrap()
        });
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            tokio::process::Command::new("uv")
                .args(["run", "--no-project", "scripts/rebuild-oauth-mcp-check.py"])
                .current_dir(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .parent()
                        .unwrap(),
                )
                .env("TODOER_MCP_URL", url)
                .env("TODOER_TEST_SESSION_COOKIE", &fixture.cookie)
                .env("TODOER_TEST_CSRF", &fixture.csrf)
                .env("TODOER_TEST_REFRESH", "1")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .expect("SDK test timeout")
        .expect("uv must be installed");
        fixture.state.request_shutdown();
        let _ = stop_tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(10), server)
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.status.success(),
            "SDK check failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
}
