#[path = "activity_web.rs"]
mod activity_http;
#[path = "notification_web.rs"]
mod notification_http;
#[path = "planning_web.rs"]
mod planning_http;
#[path = "public_web.rs"]
mod public_http;
#[path = "reports_web.rs"]
mod reports_http;
#[path = "section_move_web.rs"]
mod section_move_http;
#[path = "sidebar_ui.rs"]
mod sidebar_ui;
#[path = "task_page.rs"]
mod task_page;
use sidebar_ui::{Sidebar, build_sidebar};
#[path = "account_web.rs"]
mod account_http;
#[path = "agent_features.rs"]
pub(crate) mod agent_features;
#[path = "agent_web.rs"]
mod agent_http;
#[path = "assist_web.rs"]
mod assist_http;
#[path = "auth_web.rs"]
mod browser_auth;
#[path = "cli_auth_web.rs"]
mod cli_auth_http;
#[path = "discussion_web.rs"]
mod discussion_http;
#[path = "invitation_web.rs"]
mod invitation_http;
#[path = "jobs_web.rs"]
mod jobs_http;
#[path = "list_move_web.rs"]
mod list_move_http;
#[path = "navigation_web.rs"]
mod navigation_http;
#[path = "oauth_web.rs"]
mod oauth_http;
#[path = "onboarding_web.rs"]
mod onboarding_http;
#[path = "workspace_web.rs"]
mod organization;
#[path = "push_web.rs"]
mod push_http;
#[path = "scheduling_web.rs"]
mod scheduling_http;
#[path = "statistics_web.rs"]
mod statistics_http;
#[path = "search_web.rs"]
mod structured_search;
#[path = "taxonomy_web.rs"]
pub(crate) mod taxonomy_http;
#[path = "transfer_web.rs"]
mod transfer_http;
use crate::core::{Capture, Change, Error, List, Store, Task};
use askama::Template;
use axum::{
    Form, Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, patch, post},
};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    store: Arc<Mutex<Store>>,
    accounts: Arc<account_http::AccountRuntime>,
    production: Option<Arc<browser_auth::ProductionAuth>>,
    storage: Option<Arc<crate::object_storage::ObjectStorage>>,
    invitations: Option<Arc<crate::invitations::InvitationSigner>>,
    push: Option<Arc<crate::push::PushTransport>>,
    pub(crate) assist: crate::assist::AssistService,
    token_hash: [u8; 32],
    sessions: Arc<Mutex<std::collections::HashMap<[u8; 32], std::time::Instant>>>,
    csrf: String,
    origin: String,
    stopping: Arc<std::sync::atomic::AtomicBool>,
}
impl AppState {
    pub fn new(store: Arc<Mutex<Store>>, token: String, origin: String) -> Self {
        Self {
            store,
            production: None,
            accounts: Arc::new(account_http::AccountRuntime::new(
                &origin,
                uuid::Uuid::new_v4().to_string(),
            )),
            storage: None,
            invitations: None,
            push: None,
            assist: crate::assist::AssistService::default(),
            token_hash: Sha256::digest(token.as_bytes()).into(),
            sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
            csrf: uuid::Uuid::new_v4().to_string(),
            origin,
            stopping: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
    pub fn with_account_key(mut self, key: &str) -> Self {
        self.accounts = Arc::new(account_http::AccountRuntime::new(
            &self.origin,
            key.to_owned(),
        ));
        self
    }
    pub fn with_assist(mut self, assist: crate::assist::AssistService) -> Self {
        self.assist = assist;
        self
    }
    pub fn with_storage(mut self, storage: Option<crate::object_storage::ObjectStorage>) -> Self {
        self.storage = storage.map(Arc::new);
        self
    }
    pub fn with_invitations(mut self, signer: crate::invitations::InvitationSigner) -> Self {
        self.invitations = Some(Arc::new(signer));
        self
    }
    pub fn with_push(mut self, push: Option<crate::push::PushTransport>) -> Self {
        self.push = push.map(Arc::new);
        self
    }
    pub fn start_push_worker(&self) -> Option<tokio::task::JoinHandle<()>> {
        self.push
            .as_ref()
            .map(|transport| push_http::start_push_worker(self.clone(), transport.as_ref().clone()))
    }
    pub fn start_statistics_worker(&self) -> tokio::task::JoinHandle<()> {
        statistics_http::start_statistics_worker(self.clone())
    }
    pub fn request_shutdown(&self) {
        self.stopping
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    fn auth(&self, h: &HeaderMap, write: bool) -> Result<String, ApiError> {
        if self.production.is_some() {
            return self.production_actor(h, write, false);
        }
        if let Some(token) = h
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
        {
            let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
            let diff = hash
                .iter()
                .zip(self.token_hash)
                .fold(0u8, |a, (x, y)| a | (x ^ y));
            if diff == 0 {
                return Ok("dev".into());
            }
            if let Ok(db) = self.store.lock()
                && let Ok(actor) = db.cli_bearer(
                    token,
                    &format!("{}/api/v1", self.origin.trim_end_matches('/')),
                    write,
                )
            {
                return Ok(actor);
            }
        }
        let cookie_ok = session_cookie(h).is_some_and(|value| {
            let hash: [u8; 32] = Sha256::digest(value.as_bytes()).into();
            self.sessions
                .lock()
                .ok()
                .and_then(|sessions| sessions.get(&hash).copied())
                .is_some_and(|created| {
                    created.elapsed() < std::time::Duration::from_secs(12 * 60 * 60)
                })
        });
        if cookie_ok {
            if write && h.get("x-csrf-token").and_then(|v| v.to_str().ok()) != Some(&self.csrf) {
                return Err(ApiError(
                    StatusCode::FORBIDDEN,
                    "csrf",
                    "Reload before saving".into(),
                ));
            }
            return Ok("dev".into());
        }
        Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Authentication required".into(),
        ))
    }
    pub(crate) async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Store) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, ApiError> {
        let store = self.store.clone();
        let request_id = crate::audit::REQUEST_ID.try_with(Clone::clone).ok();
        tokio::task::spawn_blocking(move || {
            let mut guard = store
                .lock()
                .map_err(|_| Error::Invalid("Database unavailable".into()))?;
            crate::audit::with_request(request_id, || f(&mut guard))
        })
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "Operation failed".into(),
            )
        })?
        .map_err(Into::into)
    }
}
#[derive(Debug)]
pub struct ApiError(
    pub(crate) StatusCode,
    pub(crate) &'static str,
    pub(crate) String,
);
impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        match e {
            Error::Invalid(s) => Self(StatusCode::BAD_REQUEST, "invalid", s),
            Error::NotFound => Self(StatusCode::NOT_FOUND, "not_found", "Not found".into()),
            Error::Forbidden => Self(StatusCode::FORBIDDEN, "forbidden", "Access denied".into()),
            Error::Conflict(s) => Self(StatusCode::CONFLICT, "conflict", s),
            Error::Database(_) => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database",
                "Database operation failed".into(),
            ),
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(json!({"error":{"code":self.1,"message":self.2}})),
        )
            .into_response()
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/sw.js", get(service_worker))
        .route("/signin", get(signin).post(login))
        .route("/signup", get(signup))
        .route("/forgot-password", get(forgot_screen))
        .route("/reset-password", get(reset_screen))
        .route("/verify-email", get(verify_screen))
        .route("/auth/google", get(browser_auth::google_start))
        .route("/auth/google/callback", get(browser_auth::google_callback))
        .route("/logout", post(logout))
        .route("/capture", get(capture_page).post(capture_form))
        .route("/lists/{id}", get(legacy_list_page))
        .route("/health", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/ready", get(ready))
        .route("/api/v1/lists", get(lists))
        .route("/api/v1/lists/{id}/tasks", get(tasks))
        .route("/api/v1/capture", post(capture))
        .route("/api/v1/capture/options", get(capture_options))
        .route(
            "/api/v1/capture/preferences",
            get(capture_preferences).put(save_capture_preferences),
        )
        .route("/api/v1/search", get(search))
        .route("/api/v1/tasks/{id}/move-preview", get(move_preview))
        .route("/api/v1/tasks/{id}", patch(update))
        .route("/api/v1/tasks/{id}/subtree", get(subtree))
        .route("/api/v1/tasks/{id}/subtree/query", post(subtree_query))
        .route("/api/v1/tasks/{id}/move", post(move_task))
        .route("/api/v1/changes", get(changes))
        .route("/api/v1/events", get(events))
        .route("/api/v1/trash", get(trash_tasks))
        .route("/api/v1/tasks/{id}/trash", post(trash_task))
        .route("/api/v1/tasks/{id}/restore", post(restore_task))
        .route("/api/v1/history/{seq}/undo", post(undo))
        .route("/api/v1/capabilities", get(capabilities))
        .route("/api/v1/openapi.json", get(openapi))
        .route("/search", get(search_page))
        .route("/trash", get(trash_page))
        .route("/activity", get(home))
        .fallback(not_found)
        .route("/fragments/lists/{id}/tasks", get(task_fragment))
        .route(
            "/fragments/tasks/{id}/description",
            get(description_fragment),
        )
        .nest_service("/assets", ServeDir::new("app/assets"))
        .nest_service("/legacy-assets", ServeDir::new("app/assets/icons"))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .merge(activity_http::routes())
        .merge(planning_http::routes())
        .merge(reports_http::routes())
        .merge(public_http::routes())
        .merge(oauth_http::routes())
        .merge(account_http::routes())
        .merge(onboarding_http::routes())
        .merge(cli_auth_http::routes())
        .merge(organization::routes())
        .merge(agent_http::routes())
        .merge(agent_features::routes())
        .merge(invitation_http::routes())
        .merge(navigation_http::routes())
        .merge(list_move_http::routes())
        .merge(push_http::routes())
        .merge(structured_search::routes())
        .merge(statistics_http::routes())
        .merge(assist_http::routes())
        .merge(jobs_http::routes())
        .merge(notification_http::routes())
        .merge(task_page::routes())
        .merge(section_move_http::routes())
        .merge(taxonomy_http::routes())
        .merge(transfer_http::routes())
        .merge(discussion_http::routes())
        .merge(scheduling_http::routes())
        .with_state(state.clone())
        .merge(
            crate::mcp::router(state.clone()).layer(axum::middleware::from_fn_with_state(
                state.clone(),
                mcp_auth,
            )),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            public_http::metadata_layer,
        ))
        .layer(axum::middleware::from_fn(response_security))
}
async fn home(State(s): State<AppState>, h: HeaderMap) -> Result<Redirect, ApiError> {
    Ok(Redirect::to(if s.auth(&h, false).is_ok() {
        "/inbox"
    } else {
        "/signin"
    }))
}

fn encode_query(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
#[derive(Deserialize, Default)]
struct SigninQuery {
    next: Option<String>,
    step: Option<String>,
}
async fn forgot_screen(
    State(s): State<AppState>,
    Query(mut q): Query<SigninQuery>,
) -> Html<String> {
    q.step = Some("forgot".into());
    auth_screen(s, q, false)
}
async fn reset_screen(State(s): State<AppState>, Query(mut q): Query<SigninQuery>) -> Html<String> {
    q.step = Some("reset".into());
    auth_screen(s, q, false)
}
async fn verify_screen(
    State(s): State<AppState>,
    Query(mut q): Query<SigninQuery>,
) -> Html<String> {
    q.step = Some("verify".into());
    auth_screen(s, q, false)
}
async fn signup(State(s): State<AppState>, Query(q): Query<SigninQuery>) -> Html<String> {
    auth_screen(s, q, true)
}
async fn signin(State(s): State<AppState>, Query(q): Query<SigninQuery>) -> Html<String> {
    auth_screen(s, q, false)
}
fn auth_screen(s: AppState, q: SigninQuery, signup: bool) -> Html<String> {
    let next = browser_auth::safe_next(q.next.as_deref());
    let encoded = encode_query(&next);
    let step = q.step.as_deref().unwrap_or("");
    let title = if step == "mfa" {
        "One more step."
    } else if step == "forgot" {
        "Forgot your password?"
    } else if step == "reset" {
        "Choose a new password."
    } else if step == "verify" {
        "Confirm your email."
    } else if signup {
        "Make room for what matters."
    } else {
        "Welcome back."
    };
    let intro = if signup {
        "A little clarity for everything you want to do."
    } else {
        "Your tasks, plans, and a little peace of mind."
    };
    let action = if signup {
        "Sign up with Google"
    } else {
        "Continue with Google"
    };
    let google = format!(
        r##"<a class="auth-google" href="/auth/google?next={encoded}"><svg viewBox="0 0 48 48" width="20" height="20" aria-hidden="true"><path fill="#4285F4" d="M43.61 24.46c0-1.36-.12-2.66-.35-3.92H24v7.42h11a9.4 9.4 0 0 1-4.08 6.17v5h6.6c3.86-3.55 6.09-8.78 6.09-14.67Z"/><path fill="#34A853" d="M24 44c5.51 0 10.13-1.83 13.51-4.87l-6.6-5c-1.83 1.23-4.17 1.97-6.91 1.97-5.32 0-9.83-3.59-11.44-8.43H5.75v5.16A20.4 20.4 0 0 0 24 44Z"/><path fill="#FBBC05" d="M12.56 27.67a12.2 12.2 0 0 1 0-7.34v-5.16H5.75a20 20 0 0 0 0 17.66l6.81-5.16Z"/><path fill="#EA4335" d="M24 11.9c3 0 5.69 1.03 7.81 3.06l5.86-5.86A19.6 19.6 0 0 0 24 4 20.4 20.4 0 0 0 5.75 15.17l6.81 5.16C14.17 15.49 18.68 11.9 24 11.9Z"/></svg><span>{action}</span></a>"##
    );
    let content = if s.production.is_some() {
        let policy = s
            .production
            .as_ref()
            .and_then(|a| a.store.lock().ok().and_then(|db| db.auth_policy().ok()));
        let Some(policy) = policy else {
            return Html("Sign-in is temporarily unavailable. Please try again.".into());
        };
        let (switch_label, switch_action, switch_url) = if signup {
            ("Already have an account?", "Sign in", "signin")
        } else {
            ("New to Todoer?", "Create an account", "signup")
        };
        let google = if policy.google_enabled && (!signup || policy.signup_enabled) {
            format!("{google}<div class=\"auth-divider\"><span>or use email</span></div>")
        } else {
            String::new()
        };
        let fields = if step == "mfa" {
            r#"<form data-auth-form="mfa"><label>Authenticator or recovery code<input name="code" autocomplete="one-time-code" required></label><button class="primary">Verify and sign in</button></form><button class="auth-passkey" type="button" data-auth-passkey>Use a passkey</button>"#.to_string()
        } else if step == "forgot" {
            r#"<form data-auth-form="forgot"><label>Email<input type="email" name="email" autocomplete="email" required maxlength="320"></label><button class="primary">Send reset link</button></form>"#.into()
        } else if step == "reset" {
            r#"<form data-auth-form="reset"><label>New password<input type="password" name="password" autocomplete="new-password" minlength="15" maxlength="128" required></label><label>Authenticator or recovery code, if enabled<input name="code" autocomplete="one-time-code"></label><button class="primary">Save new password</button></form><button class="auth-passkey" type="button" data-auth-passkey>Verify with a passkey instead</button>"#.into()
        } else if step == "verify" {
            r#"<form data-auth-form="verify"><p>Confirm this email address to finish creating your account.</p><button class="primary">Verify email &amp; create account</button></form>"#.into()
        } else if signup && !policy.signup_enabled {
            "<p class=\"auth-access-note\">Account creation is currently closed.</p>".into()
        } else if signup {
            r#"<form data-auth-form="register"><label>Name<input name="name" autocomplete="name" required maxlength="200"></label><label>Email<input type="email" name="email" autocomplete="email" required maxlength="320"></label><label>Password<input type="password" name="password" autocomplete="new-password" minlength="15" maxlength="128" required></label><p class="auth-access-note">Use at least 15 characters. You will receive an email to verify your account.</p><button class="primary">Create account</button></form>"#.into()
        } else {
            r#"<form data-auth-form="login"><label>Email<input type="email" name="email" autocomplete="username webauthn" required maxlength="320"></label><label>Password<input type="password" name="password" autocomplete="current-password" required maxlength="128"></label><a class="auth-forgot" href="/forgot-password">Forgot password?</a><button class="primary">Sign in</button></form><button class="auth-passkey" type="button" data-auth-passkey hidden>Sign in with a passkey</button>"#.into()
        };
        let google = if step.is_empty() {
            google
        } else {
            String::new()
        };
        let switch = if !signup && !policy.signup_enabled {
            "<p class=\"auth-access-note\">Account creation is currently closed.</p>".to_string()
        } else {
            format!(
                r#"<p class="auth-switch">{switch_label} <a href="/{switch_url}?next={encoded}">{switch_action}</a></p>"#
            )
        };
        format!(
            r#"{google}{fields}<p data-auth-notice role="status" aria-live="polite"></p>{switch}<script type="module" src="/assets/account-auth.js"></script>"#
        )
    } else {
        format!(
            r#"<form method="post" action="/signin"><input type="hidden" name="csrf" value="{}"><button class="primary auth-dev">Enter development workspace</button></form><p class="auth-access-note">Local development workspace · synthetic account</p>"#,
            s.csrf
        )
    };
    Html(format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><meta name="color-scheme" content="dark light"><link rel="manifest" href="/assets/manifest.webmanifest"><link rel="apple-touch-icon" sizes="180x180" href="/assets/apple-touch-icon.png"><meta name="theme-color" content="#030712"><meta name="apple-mobile-web-app-capable" content="yes"><meta name="apple-mobile-web-app-status-bar-style" content="black-translucent"><meta name="apple-mobile-web-app-title" content="Todoer"><title>{} · Todoer</title><link rel="stylesheet" href="/assets/app.css"></head><body class="auth-page"><main class="auth-shell"><section class="auth-card" aria-labelledby="auth-heading"><a class="brand auth-brand" href="/">todoer<span>.pro</span></a><h1 id="auth-heading">{title}</h1><p class="auth-intro">{intro}</p>{content}</section><p class="auth-footer">One thing at a time.</p></main></body></html>"##,
        if signup { "Create account" } else { "Sign in" }
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    csrf: String,
}
async fn login(
    State(s): State<AppState>,
    h: HeaderMap,
    Form(f): Form<Login>,
) -> Result<Response, ApiError> {
    if s.production.is_some() {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "not_found",
            "Development login is disabled".into(),
        ));
    }
    if f.csrf != s.csrf_for(&h) || !s.origin_ok(&h) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "csrf",
            "Invalid sign-in request".into(),
        ));
    }
    let session = uuid::Uuid::new_v4().to_string();
    let hash: [u8; 32] = Sha256::digest(session.as_bytes()).into();
    {
        let mut sessions = s.sessions.lock().map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session",
                "Session unavailable".into(),
            )
        })?;
        sessions
            .retain(|_, created| created.elapsed() < std::time::Duration::from_secs(12 * 60 * 60));
        if sessions.len() >= 1000 {
            return Err(ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "session_limit",
                "Too many development sessions".into(),
            ));
        }
        sessions.insert(hash, std::time::Instant::now());
    }
    let mut r = Redirect::to("/lists/groceries").into_response();
    r.headers_mut().insert(
        header::SET_COOKIE,
        format!(
            "todoer_dev={}; Max-Age=43200; HttpOnly; SameSite=Strict; Path=/",
            session
        )
        .parse()
        .unwrap(),
    );
    Ok(r)
}
async fn logout(
    State(s): State<AppState>,
    mut h: HeaderMap,
    form: Result<Form<Login>, axum::extract::rejection::FormRejection>,
) -> Result<Response, ApiError> {
    if !h.contains_key("x-csrf-token")
        && let Ok(Form(input)) = form
    {
        h.insert(
            "x-csrf-token",
            input
                .csrf
                .parse()
                .map_err(|_| crate::auth::AuthError::Forbidden)?,
        );
    }
    s.auth(&h, true)?;
    if let Some(auth) = &s.production {
        let cookie = h
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = cookie
            .split(';')
            .find_map(|v| v.trim().strip_prefix("todoer_session="))
            .ok_or(crate::auth::AuthError::Unauthorized)?;
        auth.store
            .lock()
            .map_err(|_| crate::auth::AuthError::Unauthorized)?
            .revoke_session(token)?;
        let mut response = Redirect::to("/signin").into_response();
        for name in ["todoer_session", "todoer_csrf"] {
            response.headers_mut().append(
                header::SET_COOKIE,
                format!(
                    "{name}=; Max-Age=0; HttpOnly; SameSite=Lax; Path=/{}",
                    if s.origin.starts_with("https://") {
                        "; Secure"
                    } else {
                        ""
                    }
                )
                .parse()
                .unwrap(),
            );
        }
        return Ok(response);
    }
    if let Some(value) = session_cookie(&h) {
        let hash: [u8; 32] = Sha256::digest(value.as_bytes()).into();
        if let Ok(mut sessions) = s.sessions.lock() {
            sessions.remove(&hash);
        }
    }
    let mut r = Redirect::to("/signin").into_response();
    r.headers_mut().insert(
        header::SET_COOKIE,
        "todoer_dev=; Max-Age=0; HttpOnly; SameSite=Strict; Path=/"
            .parse()
            .unwrap(),
    );
    Ok(r)
}
#[derive(Template)]
#[template(path = "list.html")]
struct ListPage {
    change_cursor: i64,
    reference: String,
    sidebar: Sidebar,
    slug: String,
    lists: Vec<List>,
    list: List,
    groups: Vec<TaskGroup>,
    task_count: usize,
    color: String,
    csrf: String,
    capture_key: String,
    account_id: String,
    can_write: bool,
}
struct RenderedTask {
    task: Task,
    description_html: String,
}
impl std::ops::Deref for RenderedTask {
    type Target = Task;
    fn deref(&self) -> &Task {
        &self.task
    }
}
// Positions are per sibling group. A global ORDER BY position would interleave
// children with unrelated roots after reorder; preserve sibling order in preorder.
fn preorder_tasks(tasks: Vec<Task>) -> Result<Vec<Task>, Error> {
    use std::collections::{HashMap, HashSet};
    let count = tasks.len();
    let ids = tasks.iter().map(|t| t.id.clone()).collect::<HashSet<_>>();
    let mut children: HashMap<Option<String>, Vec<Task>> = HashMap::new();
    for task in tasks {
        if task.parent_id.as_ref().is_some_and(|p| !ids.contains(p)) {
            return Err(Error::Conflict(
                "Task hierarchy contains an unavailable parent".into(),
            ));
        }
        children
            .entry(task.parent_id.clone())
            .or_default()
            .push(task);
    }
    let mut stack = children.remove(&None).unwrap_or_default();
    stack.reverse();
    let mut result = Vec::with_capacity(count);
    while let Some(task) = stack.pop() {
        if let Some(mut descendants) = children.remove(&Some(task.id.clone())) {
            descendants.reverse();
            stack.extend(descendants);
        }
        result.push(task);
    }
    if result.len() != count {
        return Err(Error::Conflict("Task hierarchy contains a cycle".into()));
    }
    Ok(result)
}
fn rendered_tasks(db: &Store, actor: &str, list: &str) -> Result<Vec<RenderedTask>, Error> {
    preorder_tasks(db.tasks(actor, list)?)?
        .into_iter()
        .map(|task| {
            let description_html = if task.description.is_empty() {
                String::new()
            } else {
                crate::taxonomy::render_references(
                    &db.conn,
                    actor,
                    "task",
                    &task.id,
                    &task.description,
                )?
            };
            Ok(RenderedTask {
                task,
                description_html,
            })
        })
        .collect()
}
#[derive(Template)]
#[template(path = "tasks.html")]
struct TaskRows {
    groups: Vec<TaskGroup>,
    can_write: bool,
}
struct TaskGroup {
    progress: usize,
    completed: usize,
    total: usize,
    id: String,
    title: String,
    slug: String,
    description_html: String,
    revision: i64,
    tasks: Vec<RenderedTask>,
}
fn grouped_rows(db: &Store, actor: &str, list: &str) -> Result<(Vec<TaskGroup>, usize), Error> {
    let tasks = rendered_tasks(db, actor, list)?;
    let count = tasks.len();
    let slugs = db.entity_slugs(actor)?;
    let mut groups = vec![TaskGroup {
        progress: 0,
        completed: 0,
        total: 0,
        id: String::new(),
        title: String::new(),
        slug: String::new(),
        description_html: String::new(),
        revision: 0,
        tasks: vec![],
    }];
    for section in db.sections(actor, list)? {
        if db.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM list_defaults WHERE section_id=?1)",
            [&section.id],
            |r| r.get::<_, bool>(0),
        )? {
            continue;
        }
        let description_html = crate::markdown::render(&section.description);
        groups.push(TaskGroup {
            progress: 0,
            completed: 0,
            total: 0,
            slug: slugs
                .sections
                .get(&section.id)
                .cloned()
                .unwrap_or_else(|| section.title.clone()),
            id: section.id,
            title: section.title,
            description_html,
            revision: section.revision,
            tasks: vec![],
        });
    }
    let mut locations = std::collections::HashMap::<String, String>::new();
    let mut q=db.conn.prepare("SELECT t.id,l.section_id FROM task_locations l JOIN tasks t ON t.id=l.task_id WHERE t.list_id=?1 AND t.deleted_at IS NULL")?;
    for row in q.query_map([list], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (id, section) = row?;
        locations.insert(id, section);
    }
    for task in tasks {
        let section = locations.get(&task.id).cloned().or_else(|| {
            task.parent_id
                .as_ref()
                .and_then(|p| locations.get(p).cloned())
        });
        if let Some(section) = &section {
            locations.insert(task.id.clone(), section.clone());
        }
        let index = section
            .and_then(|id| groups.iter().position(|g| g.id == id))
            .unwrap_or(0);
        groups[index].tasks.push(task);
    }
    for group in &mut groups {
        let parents = group
            .tasks
            .iter()
            .filter_map(|t| t.parent_id.clone())
            .collect::<std::collections::HashSet<_>>();
        let leaves = group
            .tasks
            .iter()
            .filter(|t| !parents.contains(&t.id))
            .collect::<Vec<_>>();
        group.total = leaves.len();
        group.completed = leaves.iter().filter(|t| t.complete).count();
        group.progress = (group.completed * 100)
            .checked_div(group.total)
            .unwrap_or(0);
    }
    Ok((groups, count))
}
#[derive(Deserialize, Default)]
pub(super) struct ListQuery {
    task: Option<String>,
    section: Option<String>,
}
async fn legacy_list_page(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<Response, ApiError> {
    if public_http::requested(&s, &h, "list", &id).await? {
        return public_http::show(&s, "list", &id, 1).await;
    }
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let requested = id.clone();
    let target = s
        .run(move |db| {
            let resolved = crate::defaults::resolve(&db.conn, "list", &id)?;
            let id = if db.lists(&actor)?.iter().any(|l| l.id == resolved) {
                resolved
            } else {
                db.resolve_destination(&actor, Some(&id), None, false)?
                    .destination
                    .ok_or(Error::NotFound)?
                    .id
            };
            crate::core::access_rank(&db.conn, &actor, &id, 1)?;
            crate::defaults::public_id(&db.conn, "list", &id)
        })
        .await?;
    if requested == target {
        return page(State(s), h, Path(requested), Query(q)).await;
    }
    let mut url = format!("/lists/{target}");
    if let Some(task) = q.task {
        url.push_str(&format!("?task={}", encode_query(&task)));
    } else if let Some(section) = q.section {
        url.push_str(&format!("?section={}", encode_query(&section)));
    }
    Ok(Redirect::to(&url).into_response())
}
pub(super) async fn page(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let alias = id.clone();
    let id = s
        .run(move |db| crate::defaults::resolve(&db.conn, "list", &alias))
        .await?;
    if let Some(target) = q.task {
        let reader = actor.clone();
        let task = s
            .run(move |db| {
                db.subtree(&reader, &target)?
                    .into_iter()
                    .find(|t| t.id == target || t.reference == target)
                    .ok_or(Error::NotFound)
            })
            .await?;
        return Ok(Redirect::to(&format!("/tasks/{}", task.id)).into_response());
    }
    let _section = q.section;
    let account_id = actor.clone();
    let list_actor = actor.clone();
    let lists = s.run(move |db| db.lists(&list_actor)).await?;
    if !lists.iter().any(|l| l.id == id) {
        let alias_actor = actor.clone();
        let alias_id = id.clone();
        let found = s
            .run(move |db| db.resolve_destination(&alias_actor, Some(&alias_id), None, false))
            .await?;
        if let Some(destination) = found.destination {
            return Ok(Redirect::to(&format!("/lists/{}", destination.id)).into_response());
        }
    }
    let list = lists.iter().find(|l| l.id == id).cloned().ok_or(ApiError(
        StatusCode::NOT_FOUND,
        "not_found",
        "List not found".into(),
    ))?;
    let sidebar_actor = actor.clone();
    let sidebar_id = id.clone();
    let sidebar_csrf = s.csrf_for(&h);
    let sidebar = s
        .run(move |db| build_sidebar(db, &sidebar_actor, &sidebar_id, sidebar_csrf))
        .await?;
    let slug = sidebar
        .lists
        .iter()
        .find(|item| item.id == list.id)
        .map(|item| item.slug.clone())
        .unwrap_or_else(|| list.title.clone());
    let (groups, task_count, can_write, color, change_cursor) = s
        .run(move |db| {
            let (groups, count) = grouped_rows(db, &actor, &id)?;
            Ok((
                groups,
                count,
                db.list_permission(&actor, &id)? >= 2,
                db.list_color(&actor, &id)?.unwrap_or_else(|| "blue".into()),
                db.conn.query_row("SELECT COALESCE(MAX(h.seq),0) FROM history h JOIN actor_list_access a ON a.list_id=h.list_id WHERE a.actor_id=?1",[&actor],|r|r.get::<_,i64>(0))?,
            ))
        })
        .await?;
    let reference_actor = account_id.clone();
    let reference_list = list.id.clone();
    let reference = s
        .run(move |db| {
            crate::core::access_rank(&db.conn, &reference_actor, &reference_list, 1)?;
            crate::defaults::public_id(&db.conn, "list", &reference_list)
        })
        .await?;
    let html = ListPage {
        change_cursor,
        reference,
        sidebar,
        slug,
        color,
        lists,
        list,
        groups,
        task_count,
        csrf: s.csrf_for(&h),
        account_id: account_id.clone(),
        can_write,
        capture_key: uuid::Uuid::new_v4().to_string(),
    }
    .render()
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "template",
            "Render failed".into(),
        )
    })?;
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (
                header::HeaderName::from_static("x-todoer-account"),
                account_id.as_str(),
            ),
        ],
        Html(html),
    )
        .into_response())
}
async fn description_fragment(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let actor = s.auth(&h, false)?;
    let html = s
        .run(move |db| {
            let task = db
                .subtree(&actor, &id)?
                .into_iter()
                .find(|t| t.id == id || t.reference == id)
                .ok_or(Error::NotFound)?;
            crate::taxonomy::render_references(
                &db.conn,
                &actor,
                "task",
                &task.id,
                &task.description,
            )
        })
        .await?;
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Html(html)).into_response())
}
async fn task_fragment(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>, ApiError> {
    let actor = s.auth(&h, false)?;
    let (groups, can_write) = s
        .run(move |db| {
            Ok((
                grouped_rows(db, &actor, &id)?.0,
                db.list_permission(&actor, &id)? >= 2,
            ))
        })
        .await?;
    Ok(Html(TaskRows { groups, can_write }.render().map_err(
        |_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "template",
                "Render failed".into(),
            )
        },
    )?))
}
async fn lists(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Vec<List>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.lists(&actor)).await?))
}
async fn tasks(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<Task>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.tasks(&actor, &id)).await?))
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapturePreferences {
    smart_enabled: bool,
    date_enabled: bool,
    repeat_capture: bool,
    ai_instructions: String,
}
impl Default for CapturePreferences {
    fn default() -> Self {
        Self {
            smart_enabled: true,
            date_enabled: true,
            repeat_capture: false,
            ai_instructions: String::new(),
        }
    }
}
fn read_capture_preferences(db: &Store, actor: &str) -> Result<CapturePreferences, Error> {
    let user = crate::core::actor_identity(&db.conn, actor)?.0;
    use rusqlite::OptionalExtension;
    Ok(db.conn.query_row("SELECT smart_enabled,date_enabled,repeat_capture,ai_instructions FROM capture_preferences WHERE user_id=?1", [&user], |r| Ok(CapturePreferences {smart_enabled:r.get(0)?,date_enabled:r.get(1)?,repeat_capture:r.get(2)?,ai_instructions:r.get(3)?})).optional()?.unwrap_or_default())
}
async fn capture_preferences(
    State(s): State<AppState>,
    h: HeaderMap,
) -> Result<Json<CapturePreferences>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| read_capture_preferences(db, &actor))
            .await?,
    ))
}
async fn save_capture_preferences(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(value): Input<CapturePreferences>,
) -> Result<Json<CapturePreferences>, ApiError> {
    let actor = s.auth(&h, true)?;
    if value.ai_instructions.len() > 1000 {
        return Err(ApiError::from(Error::Invalid(
            "AI instructions are limited to 1000 bytes".into(),
        )));
    }
    let result = value.clone();
    s.run(move |db| {
        let user=crate::core::actor_identity(&db.conn,&actor)?.0;
        db.conn.execute("INSERT INTO capture_preferences(user_id,smart_enabled,date_enabled,repeat_capture,ai_instructions) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(user_id) DO UPDATE SET smart_enabled=excluded.smart_enabled,date_enabled=excluded.date_enabled,repeat_capture=excluded.repeat_capture,ai_instructions=excluded.ai_instructions",rusqlite::params![user,value.smart_enabled,value.date_enabled,value.repeat_capture,value.ai_instructions])?;
        Ok(())
    }).await?;
    Ok(Json(result))
}
async fn capture_options(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| {
        let prefs=read_capture_preferences(db,&actor)?;
        let mut destinations=Vec::new();
        let mut list_colors=std::collections::BTreeMap::new();
        let mut tags = std::collections::BTreeSet::new();
        for list in db.lists(&actor)? {
            if crate::core::access_rank(&db.conn,&actor,&list.id,2).is_err() {continue;}
            list_colors.insert(list.id.clone(), db.list_color(&actor, &list.id)?);
            for tag in db.list_tags(&actor, &list.id, "", 100)? { tags.insert(tag.name); }
            let workspace = db.list_info(&actor, &list.id)?;
            if workspace.archived { continue; }
            if let Some(id) = workspace.workspace_id
                && let Ok(space) = db.workspace(&actor,&id) {
                    let default_list:bool=db.conn.query_row("SELECT EXISTS(SELECT 1 FROM workspace_defaults WHERE workspace_id=?1 AND list_id=?2)",rusqlite::params![id,list.id],|r|r.get(0))?;
                    destinations.push(json!({"space":space.title,"space_id":id,"list":list.title,"list_id":list.id,"default_list":default_list}));
                }
        }
        Ok(json!({"preferences":{"smart_enabled":prefs.smart_enabled,"date_enabled":prefs.date_enabled,"repeat_capture":prefs.repeat_capture},"destinations":destinations,"list_colors":list_colors,"tags":tags}))
    }).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureAt {
    list_id: String,
    tasks: Vec<crate::core::NewTask>,
    #[serde(default)]
    section_id: Option<String>,
    #[serde(default)]
    metadata: Option<crate::capture_at::CaptureMetadata>,
}
async fn capture(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(c): Input<CaptureAt>,
) -> Result<Json<Vec<Task>>, ApiError> {
    let actor = s.auth(&h, true)?;
    let key = h
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty() && s.len() <= 200)
        .ok_or(ApiError(
            StatusCode::BAD_REQUEST,
            "invalid",
            "Idempotency-Key required (1–200 characters)".into(),
        ))?
        .to_owned();
    Ok(Json(
        s.run(move |db| {
            let input = Capture {
                list_id: c.list_id,
                tasks: c.tasks,
            };
            match c.metadata {
                Some(metadata) => {
                    db.capture_with_metadata(&actor, &key, input, c.section_id, metadata)
                }
                None => db.capture_at(&actor, &key, input, c.section_id),
            }
        })
        .await?,
    ))
}
async fn update(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(c): Input<Change>,
) -> Result<Json<Task>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(s.run(move |db| db.update(&actor, &id, c)).await?))
}
async fn subtree_query(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<crate::subtree_query::SubtreeRead>,
) -> Result<Json<crate::subtree_query::SubtreePage>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| db.read_subtree(&actor, &id, input)).await?,
    ))
}
async fn subtree(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<Task>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.subtree(&actor, &id)).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Move {
    destination: String,
    revision: i64,
    #[serde(default)]
    acknowledge_access_change: bool,
}
async fn move_task(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(c): Input<Move>,
) -> Result<Json<Task>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| {
            db.move_subtree(
                &actor,
                &id,
                &c.destination,
                c.revision,
                c.acknowledge_access_change,
            )
        })
        .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Since {
    #[serde(default)]
    after: i64,
}
async fn changes(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<Since>,
) -> Result<Json<Vec<Value>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.changes(&actor, q.after)).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchQuery {
    #[serde(default)]
    query: String,
    list: Option<String>,
    complete: Option<bool>,
    #[serde(default = "search_limit")]
    limit: i64,
    after: Option<String>,
}
fn search_limit() -> i64 {
    50
}
async fn search(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<Task>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| {
            db.search(
                &actor,
                &q.query,
                q.list.as_deref(),
                q.complete,
                q.limit,
                q.after.as_deref(),
            )
        })
        .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewQuery {
    destination: String,
}
async fn move_preview(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<PreviewQuery>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| db.move_preview(&actor, &id, &q.destination))
            .await?,
    ))
}

async fn mcp_auth(
    State(s): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|o| o != s.origin)
    {
        return ApiError(StatusCode::FORBIDDEN, "origin", "Untrusted origin".into())
            .into_response();
    }
    match s.mcp_actor(req.headers()) {
        Ok(_) => next.run(req).await,
        Err(e) => {
            let mut response = e.into_response();
            if response.status() == StatusCode::UNAUTHORIZED && s.production.is_some() {
                response.headers_mut().insert(
                    header::WWW_AUTHENTICATE,
                    format!(
                        "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\"",
                        s.origin
                    )
                    .parse()
                    .unwrap(),
                );
            }
            response
        }
    }
}

mod filters {
    #[askama::filter_fn]
    pub fn markdown(s: &str, _values: &dyn askama::Values) -> askama::Result<String> {
        Ok(crate::markdown::render(s))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    revision: i64,
}
async fn trash_tasks(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Vec<Task>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.trash_tasks(&actor)).await?))
}
async fn trash_task(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(c): Input<Revision>,
) -> Result<Json<Task>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.trash(&actor, &id, c.revision)).await?,
    ))
}
async fn restore_task(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(c): Input<Revision>,
) -> Result<Json<Task>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.restore(&actor, &id, c.revision)).await?,
    ))
}
async fn undo(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(seq): Path<i64>,
    Input(c): Input<Revision>,
) -> Result<Json<Task>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.undo_update(&actor, seq, c.revision))
            .await?,
    ))
}
async fn capabilities(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    s.auth(&h, false)?;
    Ok(Json(
        json!({"version":format!("{}{}", env!("TODOER_BUILD_VERSION"), if s.production.is_some() { "" } else { "-dev" }),"mode":if s.production.is_some(){"private-preview"}else{"development-only"},"interfaces":["rest","cli","mcp"],"operations":["lists.read","tasks.read","capture","tasks.update","subtree.read","subtree.move","move.preview","search","changes","trash","restore","undo.update","workspaces","sections","sharing","comments","attachments","schedules","recurrence","statistics","bulk.preview","bulk.apply","destinations.resolve","destinations.preferences","destinations.search","query","tags","references","favorites","exports","imports","invitations","review","lists.workspace.move","assist"],"limits":{"capture_batch":100,"bulk_operations":50,"bulk_affected_tasks":500,"bulk_preview_ttl_seconds":600,"search_limit":100,"subtree_page":100,"subtree_response_bytes":262144,"changes_page":1000,"title_characters":500,"description_bytes":100000},"search":{"matching":"unicode-token-prefix","fuzzy":false,"legacy_endpoint":"/api/v1/search","structured_endpoint":"/api/v1/query","sorts":["relevance","title","created","updated","planned","due","id"],"entities":["workspace","list","section","task","comment","tag","attachment"],"cursor_resync_status":409},"production_auth":s.production.is_some()}),
    ))
}
async fn openapi() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json")],
        include_str!("../openapi.json"),
    )
}
#[derive(Template)]
#[template(path = "review.html")]
struct ReviewPage {
    title: String,
    mode: String,
    query: String,
    csrf: String,
    tasks: Vec<Task>,
    events: Vec<Activity>,
}
struct Activity {
    seq: i64,
    kind: String,
    actor: String,
    task: Task,
    can_undo: bool,
}
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct SearchForm {
    query: String,
    status: String,
    list: String,
    workspace: String,
    tag: String,
    priority: String,
    planned_from: String,
    planned_to: String,
    due_from: String,
    due_to: String,
    recurrence: String,
    cursor: Option<String>,
}
#[derive(Template)]
#[template(path = "search.html")]
struct StructuredSearchPage {
    sidebar: Sidebar,
    today: String,
    week_start: u32,
    form: SearchForm,
    lists: Vec<List>,
    workspaces: Vec<crate::workspace::Workspace>,
    hits: Vec<SearchResultView>,
    total: usize,
    next: String,
}
struct SearchResultView {
    task_id: String,
    title: String,
    kind: String,
    snippet: String,
    path: String,
    url: String,
    complete: bool,
}
async fn search_page(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(mut form): Query<SearchForm>,
) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    if form.status.is_empty() {
        form.status = "all".into();
    }
    let optional = |value: &String| {
        if value.is_empty() {
            None
        } else {
            Some(value.clone())
        }
    };
    let query = crate::search::Query {
        text: form.query.clone(),
        status: form.status.clone(),
        list_id: optional(&form.list),
        workspace_id: optional(&form.workspace),
        tag: optional(&form.tag),
        priority: if form.priority.is_empty() {
            None
        } else {
            Some(
                form.priority
                    .parse::<u8>()
                    .map_err(|_| Error::Invalid("Priority must be 1, 2 or 3".into()))?,
            )
        },
        planned_from: optional(&form.planned_from),
        planned_to: optional(&form.planned_to),
        due_from: optional(&form.due_from),
        due_to: optional(&form.due_to),
        recurrence: optional(&form.recurrence),
        cursor: form.cursor.clone(),
        ..Default::default()
    };
    let csrf = s.csrf_for(&h);
    let (page, lists, workspaces, sidebar, prefs) = s
        .run(move |db| {
            let inactive=db.conn.prepare("SELECT workspace_id FROM workspace_lifecycle WHERE archived=1 OR deleted_at IS NOT NULL")?.query_map([],|r|r.get::<_,String>(0))?.collect::<Result<std::collections::HashSet<_>,_>>()?;
            let mut lists=db.lists(&actor)?;
            lists.retain(|list| db.list_info(&actor,&list.id).is_ok_and(|info| !info.archived && !info.workspace_id.is_some_and(|id|inactive.contains(&id))));
            let mut spaces=db.workspaces(&actor)?;spaces.retain(|space| !inactive.contains(&space.id));
            Ok((db.query(&actor, &query)?, lists, spaces, build_sidebar(db,&actor,"",csrf)?,db.notification_preferences(&actor)?))
        })
        .await?;
    let next = if let Some(cursor) = page.next_cursor {
        let mut url = reqwest::Url::parse("https://todoer.invalid/search").unwrap();
        for (key, value) in [
            ("query", form.query.as_str()),
            ("status", &form.status),
            ("list", &form.list),
            ("workspace", &form.workspace),
            ("tag", &form.tag),
            ("planned_from", &form.planned_from),
            ("planned_to", &form.planned_to),
            ("due_from", &form.due_from),
            ("due_to", &form.due_to),
            ("recurrence", &form.recurrence),
            ("cursor", &cursor),
        ] {
            if !value.is_empty() {
                url.query_pairs_mut().append_pair(key, value);
            }
        }
        if !form.priority.is_empty() {
            url.query_pairs_mut()
                .append_pair("priority", &form.priority);
        }
        format!("/search?{}", url.query().unwrap_or(""))
    } else {
        String::new()
    };
    let hits = page
        .hits
        .into_iter()
        .map(|hit| {
            let path = hit
                .context
                .unwrap_or_default()
                .into_iter()
                .map(|c| c.title)
                .collect::<Vec<_>>()
                .join(" / ");
            let url = if let Some(task) = hit.task_id {
                format!("/tasks/{task}")
            } else {
                match hit.kind.as_str() {
                    "task" => format!("/tasks/{}", hit.id),
                    "workspace" => format!("/workspaces/{}", hit.id),
                    "list" => format!("/lists/{}", hit.id),
                    "section" => format!(
                        "/lists/{}?section={}",
                        hit.list_id.unwrap_or_default(),
                        hit.id
                    ),
                    "tag" => format!("/search?tag={}", encode_query(&hit.id)),
                    _ => "/search".into(),
                }
            };
            SearchResultView {
                task_id: url.strip_prefix("/tasks/").unwrap_or("").to_string(),
                title: hit.title.unwrap_or_else(|| hit.kind.clone()),
                kind: hit.kind,
                snippet: hit.snippet.unwrap_or_default(),
                path,
                url,
                complete: hit.complete.unwrap_or(false),
            }
        })
        .collect();
    Ok(Html(
        StructuredSearchPage {
            sidebar,
            today: chrono::Utc::now()
                .with_timezone(
                    &prefs
                        .timezone
                        .parse::<chrono_tz::Tz>()
                        .unwrap_or(chrono_tz::UTC),
                )
                .date_naive()
                .to_string(),
            week_start: prefs.week_start,
            form,
            lists,
            workspaces,
            hits,
            total: page.total,
            next,
        }
        .render()
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "template",
                "Search render failed".into(),
            )
        })?,
    )
    .into_response())
}
#[derive(Deserialize, Default)]
struct TrashQuery {
    page: Option<usize>,
}
async fn trash_page(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<TrashQuery>,
) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let page = q.page.unwrap_or(1).clamp(1, 100_000);
    let body = s.run(move |db| {
        let (roots, more) = db.trash_roots(&actor, page)?;
        let esc = scheduling_http::esc;
        let mut body = String::from("<div id=\"notice\" role=\"status\" aria-live=\"polite\"></div>");
        if roots.is_empty() {
            body.push_str("<div class=\"planning-empty\"><svg class=\"ui-icon\" width=\"40\" height=\"40\" aria-hidden=\"true\"><use href=\"/assets/icons.svg#trash\"/></svg><h2>All clear.</h2><p>No deleted tasks on this page.</p></div>");
        }
        for root in roots {
            let info = db.list_info(&actor, &root.list_id)?;
            let mut context = Vec::new();
            if let Some(space) = &info.workspace_id {
                context.push(format!("@{}", db.workspace(&actor, space)?.title));
            }
            context.push(format!("#{}",info.list.title));
            let mut parents = Vec::new();
            let mut parent = root.parent_id.clone();
            let mut blocked = false;
            while let Some(id) = parent {
                let ancestor = crate::core::raw_task(&db.conn, &id)?;
                if ancestor.list_id != root.list_id { return Err(Error::Forbidden); }
                let deleted: bool = db.conn.query_row("SELECT deleted_at IS NOT NULL FROM tasks WHERE id=?1", [&id], |r|r.get(0))?;
                blocked |= deleted;
                parents.push(ancestor.title);
                parent = ancestor.parent_id;
            }
            parents.reverse();
            context.extend(parents);
            let tree = db.trash_tree(&actor, &root)?;
            let count = tree.len();
            body.push_str(&format!("<article class=\"trash-group\"><div class=\"trash-group-head\"><p class=\"trash-context\">{}</p>",esc(&context.join(" / "))));
            if db.list_permission(&actor,&root.list_id)? >= 2 {
                body.push_str(&format!("<button class=\"restore\" data-id=\"{}\" data-revision=\"{}\" {}>{}</button>",esc(&root.id),root.revision,if blocked { "disabled title=\"Restore the deleted parent first\"" } else { "" },if blocked { "Restore parent first" } else { "Restore" }));
            }
            body.push_str("</div><ul class=\"trash-tree\">");
            for (task,depth) in tree {
                body.push_str(&format!("<li style=\"--depth:{}\"><span aria-hidden=\"true\">{}</span>{}</li>",depth.min(30),if depth>0 {"↳"} else {""},esc(&task.title)));
            }
            body.push_str(&format!("</ul><p class=\"trash-summary\">{} task{} · Restores to the original location</p></article>",count,if count==1 {""} else {"s"}));
        }
        if page > 1 || more {
        body.push_str("<nav class=\"trash-pagination\" aria-label=\"Trash pages\">");
        if page>1 { body.push_str(&format!("<a href=\"/trash?page={}\">← Previous</a>",page-1)); }
        body.push_str(&format!("<span>Page {page}</span>"));
        if more { body.push_str(&format!("<a href=\"/trash?page={}\">Next →</a>",page+1)); }
        body.push_str("</nav>");
        }
        body.push_str("<script type=\"module\" src=\"/assets/review.js\"></script>");
        Ok(body)
    }).await?;
    scheduling_http::render_feature(&s, &h, "Trash", body, false).await
}

fn session_cookie(h: &HeaderMap) -> Option<&str> {
    h.get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|cookie| cookie.trim().strip_prefix("todoer_dev="))
}
async fn response_security(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let started = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4().to_string();
    let method = req.method().to_string();
    let destination = req
        .uri()
        .path_and_query()
        .map(|v| v.as_str().to_owned())
        .unwrap_or_else(|| "/".into());
    let browser_page = method == "GET"
        && req
            .headers()
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("text/html"))
        && !req.uri().path().starts_with("/api/")
        && !req.uri().path().starts_with("/auth/");
    let mut res = crate::audit::REQUEST_ID
        .scope(request_id.clone(), next.run(req))
        .await;
    if browser_page && res.status() == StatusCode::UNAUTHORIZED {
        res = Redirect::to(&format!(
            "/signin?next={}",
            encode_query(&browser_auth::safe_next(Some(&destination)))
        ))
        .into_response();
    }
    res.headers_mut()
        .insert("x-request-id", request_id.parse().unwrap());
    eprintln!(
        "{}",
        json!({"event":"http","request_id":request_id,"method":method,"status":res.status().as_u16(),"duration_ms":started.elapsed().as_millis()})
    );
    res.headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    res.headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    res.headers_mut()
        .entry("referrer-policy")
        .or_insert("same-origin".parse().unwrap());
    res.headers_mut().insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self' https://fonts.bunny.net; font-src 'self' https://fonts.bunny.net; img-src 'self' data: https://www.gravatar.com https://gravatar.com https://secure.gravatar.com https://*.googleusercontent.com; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    res
}

async fn service_worker() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (
                header::HeaderName::from_static("service-worker-allowed"),
                "/",
            ),
        ],
        include_str!("../assets/sw.js").replace("__TODOER_BUILD__", env!("TODOER_ASSET_VERSION")),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Feed {
    #[serde(default)]
    after: i64,
    list_id: Option<String>,
}
async fn events(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<Feed>,
) -> Result<impl IntoResponse, ApiError> {
    let actor = s.auth(&h, false)?;
    if let Some(list) = q.list_id.clone()
        && s.run(move |db| db.list_permission(&actor, &list)).await? < 1
    {
        return Err(Error::Forbidden.into());
    }
    let last = h
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(q.after)
        .max(0);
    let stream = futures_util::stream::unfold(
        (s, h, last, false, q.list_id),
        |(s, h, mut cursor, ended, list)| async move {
            if ended || s.stopping.load(std::sync::atomic::Ordering::SeqCst) {
                return None;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let actor = match s.auth(&h, false) {
                Ok(actor) => actor,
                Err(_) => {
                    return Some((
                        Ok::<_, std::convert::Infallible>(
                            axum::response::sse::Event::default()
                                .event("session-expired")
                                .data("{}"),
                        ),
                        (s, h, cursor, true, list),
                    ));
                }
            };
            if let Some(id) = list.clone() {
                let checked_actor = actor.clone();
                if !s
                    .run(move |db| db.list_permission(&checked_actor, &id))
                    .await
                    .is_ok_and(|rank| rank >= 1)
                {
                    return Some((
                        Ok(axum::response::sse::Event::default()
                            .event("access-revoked")
                            .data("{}")),
                        (s, h, cursor, true, list),
                    ));
                }
            }
            let values = s.run(move |db| db.changes(&actor, cursor)).await;
            let data = match values {
                Ok(changes) => {
                    for v in &changes {
                        cursor = cursor.max(v["seq"].as_i64().unwrap_or(cursor));
                    }
                    let count = changes
                        .iter()
                        .filter(|v| {
                            list.as_ref().is_none_or(|id| {
                                v["task"]["list_id"].as_str() == Some(id.as_str())
                                    || v["source_list_id"].as_str() == Some(id.as_str())
                            })
                        })
                        .count();
                    json!({"cursor":cursor,"count":count})
                }
                Err(_) => json!({"resync":true}),
            };
            Some((
                Ok(axum::response::sse::Event::default()
                    .event("changes")
                    .id(cursor.to_string())
                    .data(data.to_string())),
                (s, h, cursor, false, list),
            ))
        },
    );
    Ok((
        [(header::HeaderName::from_static("x-accel-buffering"), "no")],
        axum::response::Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default()),
    ))
}

async fn ready(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    if s.stopping.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "stopping",
            "Shutting down".into(),
        ));
    }
    s.run(|db| {
        db.conn.query_row("SELECT 1", [], |_| Ok(()))?;
        Ok(())
    })
    .await?;
    Ok(Json(json!({"ready":true})))
}

struct Input<T>(T);
impl<S, T> axum::extract::FromRequest<S> for Input<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = ApiError;
    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|error| ApiError(error.status(), "invalid", error.body_text()))?;
        Ok(Self(value))
    }
}

#[derive(Template)]
#[template(path = "capture.html")]
struct CapturePage {
    account_id: String,
    lists: Vec<List>,
    selected: String,
    csrf: String,
    key: String,
}
#[derive(Deserialize, Default)]
struct CaptureContext {
    list: Option<String>,
}
async fn capture_page(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<CaptureContext>,
) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin?next=%2Fcapture").into_response());
    };
    let account_id = actor.clone();
    let (lists, default) = s
        .run(move |db| {
            let mut lists = db.lists(&actor)?;
            lists.retain(|l| {
                db.list_permission(&actor, &l.id)
                    .is_ok_and(|rank| rank >= 2)
            });
            let inbox = db.personal_inbox(&actor)?;
            if !lists.iter().any(|list| list.id == inbox) {
                lists.insert(
                    0,
                    List {
                        id: inbox.clone(),
                        title: "Inbox".into(),
                        description: "Capture first. Plan or file when ready.".into(),
                        revision: 0,
                    },
                );
            }
            Ok((lists, Some(inbox)))
        })
        .await?;
    let selected = q
        .list
        .or(default)
        .filter(|id| lists.iter().any(|l| l.id == *id))
        .unwrap_or_default();
    Ok(Html(
        CapturePage {
            account_id,
            lists,
            selected,
            csrf: s.csrf_for(&h),
            key: uuid::Uuid::new_v4().to_string(),
        }
        .render()
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "template",
                "Capture render failed".into(),
            )
        })?,
    )
    .into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureForm {
    csrf: String,
    title: String,
    #[serde(default)]
    description: String,
    list_id: String,
    key: String,
}
async fn capture_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Form(f): Form<CaptureForm>,
) -> Result<Redirect, ApiError> {
    let actor = s.auth(&h, false)?;
    if f.csrf != s.csrf_for(&h) || !s.origin_ok(&h) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "csrf",
            "Invalid capture request".into(),
        ));
    }
    let list = f.list_id.clone();
    s.run(move |db| {
        db.capture(
            &actor,
            &f.key,
            Capture {
                list_id: f.list_id,
                tasks: vec![crate::core::NewTask {
                    title: f.title,
                    description: f.description,
                    parent_id: None,
                }],
            },
        )
    })
    .await?;
    Ok(Redirect::to(&format!("/lists/{list}")))
}

#[derive(Template)]
#[template(path = "not-found.html")]
struct NotFoundPage {
    sidebar: Option<Sidebar>,
}
async fn not_found(
    State(s): State<AppState>,
    h: HeaderMap,
    req: axum::extract::Request,
) -> Result<Response, ApiError> {
    if req.uri().path().starts_with("/api/") {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error":{"code":"not_found","message":"Endpoint not found"}})),
        )
            .into_response());
    }
    let sidebar = if let Ok(actor) = s.auth(&h, false) {
        let csrf = s.csrf_for(&h);
        Some(s.run(move |db| build_sidebar(db, &actor, "", csrf)).await?)
    } else {
        None
    };
    let html = NotFoundPage { sidebar }
        .render()
        .map_err(|_| Error::Invalid("Could not render page".into()))?;
    Ok((StatusCode::NOT_FOUND, Html(html)).into_response())
}
