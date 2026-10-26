#[path = "agent_features.rs"]
pub(crate) mod agent_features;
#[path = "agent_web.rs"]
mod agent_http;
#[path = "assist_web.rs"]
mod assist_http;
#[path = "auth_web.rs"]
mod browser_auth;
#[path = "discussion_web.rs"]
mod discussion_http;
#[path = "invitation_web.rs"]
mod invitation_http;
#[path = "list_move_web.rs"]
mod list_move_http;
#[path = "navigation_web.rs"]
mod navigation_http;
#[path = "oauth_web.rs"]
mod oauth_http;
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
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    store: Arc<Mutex<Store>>,
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
        .route("/auth/google", get(browser_auth::google_start))
        .route("/auth/google/callback", get(browser_auth::google_callback))
        .route("/logout", post(logout))
        .route("/capture", get(capture_page).post(capture_form))
        .route("/lists/{id}", get(page))
        .route("/tasks/{id}", get(task_link))
        .route("/health", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/ready", get(ready))
        .route("/api/v1/lists", get(lists))
        .route("/api/v1/lists/{id}/tasks", get(tasks))
        .route("/api/v1/capture", post(capture))
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
        .route("/activity", get(activity_page))
        .route("/fragments/lists/{id}/tasks", get(task_fragment))
        .route(
            "/fragments/tasks/{id}/description",
            get(description_fragment),
        )
        .nest_service("/assets", ServeDir::new("rebuild/assets"))
        .nest_service("/legacy-assets", ServeDir::new("public"))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .merge(oauth_http::routes())
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
        .merge(taxonomy_http::routes())
        .merge(transfer_http::routes())
        .merge(discussion_http::routes())
        .merge(scheduling_http::routes())
        .with_state(state.clone())
        .merge(
            crate::mcp::router(state.clone())
                .layer(axum::middleware::from_fn_with_state(state, mcp_auth)),
        )
        .layer(axum::middleware::from_fn(response_security))
}
async fn home(State(s): State<AppState>, h: HeaderMap) -> Result<Redirect, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin"));
    };
    let lists = s.run(move |db| db.lists(&actor)).await?;
    Ok(Redirect::to(
        &lists
            .first()
            .map(|list| format!("/lists/{}", list.id))
            .unwrap_or_else(|| "/signin".into()),
    ))
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
}
async fn signin(State(s): State<AppState>, Query(q): Query<SigninQuery>) -> Html<String> {
    if s.production.is_some() {
        let next = q
            .next
            .as_deref()
            .filter(|v| {
                v.starts_with('/')
                    && !v.starts_with("//")
                    && !v.contains('\\')
                    && !v.chars().any(char::is_control)
            })
            .unwrap_or("/");
        let encoded: String = encode_query(next);
        return Html(format!(
            r#"<!doctype html><html lang="en"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Sign in · Todoer</title><link rel="stylesheet" href="/assets/app.css"><main class="login"><a class="brand" href="/">todoer<span>.pro</span></a><h1>Your tasks, clearly.</h1><p>Sign in with an invited Google account.</p><a class="primary" href="/auth/google?next={encoded}">Continue with Google</a></main></html>"#
        ));
    }
    Html(format!(
        r#"<!doctype html><html lang="en"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Sign in · Todoer</title><link rel="stylesheet" href="/assets/app.css"><main class="login"><a class="brand" href="/">todoer<span>.pro</span></a><h1>Your day, with a little more clarity.</h1><p>This isolated rebuild uses synthetic data. Sign in below to use the isolated development workspace.</p><form method="post" action="/signin"><input type="hidden" name="csrf" value="{}"><button class="primary">Enter development workspace</button></form><p class="muted">Local development only · no access to your existing account</p></main></html>"#,
        s.csrf
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
async fn logout(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
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
    lists: Vec<List>,
    list: List,
    tasks: Vec<RenderedTask>,
    favorites: Vec<crate::navigation::Favorite>,
    color: String,
    csrf: String,
    capture_key: String,
    account_id: String,
    development: bool,
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
    tasks: Vec<RenderedTask>,
    can_write: bool,
}
#[derive(Deserialize, Default)]
struct ListQuery {
    task: Option<String>,
    section: Option<String>,
}
async fn task_link(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Redirect, ApiError> {
    let actor = match s.auth(&h, false) {
        Ok(actor) => actor,
        Err(_) if !h.contains_key(header::AUTHORIZATION) => {
            let next: String = encode_query(&format!("/tasks/{id}"));
            return Ok(Redirect::to(&format!("/signin?next={next}")));
        }
        Err(error) => return Err(error),
    };
    let task = s
        .run(move |db| {
            db.subtree(&actor, &id)?
                .into_iter()
                .find(|t| t.id == id || t.reference == id)
                .ok_or(Error::NotFound)
        })
        .await?;
    Ok(Redirect::to(&format!(
        "/lists/{}?task={}",
        task.list_id, task.id
    )))
}
async fn page(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
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
        if task.list_id != id {
            return Ok(
                Redirect::to(&format!("/lists/{}?task={}", task.list_id, task.id)).into_response(),
            );
        }
    }
    let _section = q.section;
    let account_id = actor.clone();
    let list_actor = actor.clone();
    let lists = s.run(move |db| db.lists(&list_actor)).await?;
    let list = lists.iter().find(|l| l.id == id).cloned().ok_or(ApiError(
        StatusCode::NOT_FOUND,
        "not_found",
        "List not found".into(),
    ))?;
    let (tasks, can_write, favorites, color) = s
        .run(move |db| {
            Ok((
                rendered_tasks(db, &actor, &id)?,
                db.list_permission(&actor, &id)? >= 2,
                db.favorites(&actor)?,
                db.list_color(&actor, &id)?.unwrap_or_else(|| "blue".into()),
            ))
        })
        .await?;
    let html = ListPage {
        favorites,
        color,
        lists,
        list,
        tasks,
        csrf: s.csrf_for(&h),
        account_id: account_id.clone(),
        development: s.production.is_none(),
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
    let (tasks, can_write) = s
        .run(move |db| {
            Ok((
                rendered_tasks(db, &actor, &id)?,
                db.list_permission(&actor, &id)? >= 2,
            ))
        })
        .await?;
    Ok(Html(TaskRows { tasks, can_write }.render().map_err(
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
async fn capture(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(c): Input<Capture>,
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
    Ok(Json(s.run(move |db| db.capture(&actor, &key, c)).await?))
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
        json!({"version":"0.1.0-dev","mode":if s.production.is_some(){"private-preview"}else{"development-only"},"interfaces":["rest","cli","mcp"],"operations":["lists.read","tasks.read","capture","tasks.update","subtree.read","subtree.move","move.preview","search","changes","trash","restore","undo.update","workspaces","sections","sharing","comments","attachments","schedules","recurrence","statistics","bulk.preview","bulk.apply","destinations.resolve","destinations.preferences","destinations.search","query","tags","references","favorites","exports","imports","invitations","review","lists.workspace.move","assist"],"limits":{"capture_batch":100,"bulk_operations":50,"bulk_affected_tasks":500,"bulk_preview_ttl_seconds":600,"search_limit":100,"subtree_page":100,"subtree_response_bytes":262144,"changes_page":1000,"title_characters":500,"description_bytes":100000},"search":{"matching":"unicode-token-prefix","fuzzy":false,"legacy_endpoint":"/api/v1/search","structured_endpoint":"/api/v1/query","sorts":["relevance","title","created","updated","planned","due","id"],"entities":["workspace","list","section","task","comment","tag","attachment"],"cursor_resync_status":409},"production_auth":s.production.is_some()}),
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
    form: SearchForm,
    lists: Vec<List>,
    workspaces: Vec<crate::workspace::Workspace>,
    hits: Vec<SearchResultView>,
    total: usize,
    next: String,
}
struct SearchResultView {
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
    let (page, lists, workspaces) = s
        .run(move |db| {
            Ok((
                db.query(&actor, &query)?,
                db.lists(&actor)?,
                db.workspaces(&actor)?,
            ))
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
async fn trash_page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let tasks = s.run(move |db| db.trash_tasks(&actor)).await?;
    render_review(ReviewPage {
        title: "Trash".into(),
        mode: "trash".into(),
        query: String::new(),
        csrf: s.csrf_for(&h),
        tasks,
        events: vec![],
    })
}
async fn activity_page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let values = s.run(move |db| db.changes(&actor, 0)).await?;
    let events = values
        .into_iter()
        .rev()
        .filter_map(|v| {
            Some(Activity {
                seq: v["seq"].as_i64()?,
                kind: v["kind"].as_str()?.to_owned(),
                actor: v["actor"].as_str().unwrap_or("legacy-unknown").to_owned(),
                task: serde_json::from_value(v["task"].clone()).ok()?,
                can_undo: v["kind"] == "update",
            })
        })
        .collect();
    render_review(ReviewPage {
        title: "Activity".into(),
        mode: "activity".into(),
        query: String::new(),
        csrf: s.csrf_for(&h),
        tasks: vec![],
        events,
    })
}
fn render_review(p: ReviewPage) -> Result<Response, ApiError> {
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Html(p.render().map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "template",
                "Render failed".into(),
            )
        })?),
    )
        .into_response())
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
    let mut res = crate::audit::REQUEST_ID
        .scope(request_id.clone(), next.run(req))
        .await;
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
    res.headers_mut().insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
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
                    json!({"cursor":cursor,"count":changes.len()})
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
    let (lists, default) = s
        .run(move |db| {
            let mut lists = db.lists(&actor)?;
            lists.retain(|l| {
                db.list_permission(&actor, &l.id)
                    .is_ok_and(|rank| rank >= 2)
            });
            let default = db
                .destination_preferences(&actor)?
                .into_iter()
                .find(|p| p.key == "inbox")
                .map(|p| p.destination.id);
            Ok((lists, default))
        })
        .await?;
    let selected = q
        .list
        .or(default)
        .filter(|id| lists.iter().any(|l| l.id == *id))
        .unwrap_or_default();
    Ok(Html(
        CapturePage {
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
