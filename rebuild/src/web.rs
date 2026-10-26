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
            token_hash: Sha256::digest(token.as_bytes()).into(),
            sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
            csrf: uuid::Uuid::new_v4().to_string(),
            origin,
            stopping: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
    pub fn request_shutdown(&self) {
        self.stopping
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    fn auth(&self, h: &HeaderMap, write: bool) -> Result<&'static str, ApiError> {
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
                return Ok("dev");
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
            return Ok("dev");
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
        tokio::task::spawn_blocking(move || {
            let mut guard = store
                .lock()
                .map_err(|_| Error::Invalid("Database unavailable".into()))?;
            f(&mut guard)
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
        .route("/logout", post(logout))
        .route("/lists/{id}", get(page))
        .route(
            "/health",
            get(|| async { Json(json!({"status":"ok","mode":"development-only"})) }),
        )
        .route("/ready", get(ready))
        .route("/api/v1/lists", get(lists))
        .route("/api/v1/lists/{id}/tasks", get(tasks))
        .route("/api/v1/capture", post(capture))
        .route("/api/v1/search", get(search))
        .route("/api/v1/tasks/{id}/move-preview", get(move_preview))
        .route("/api/v1/tasks/{id}", patch(update))
        .route("/api/v1/tasks/{id}/subtree", get(subtree))
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
        .nest_service("/assets", ServeDir::new("rebuild/assets"))
        .nest_service("/legacy-assets", ServeDir::new("public"))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .with_state(state.clone())
        .merge(
            crate::mcp::router(state.clone())
                .layer(axum::middleware::from_fn_with_state(state, mcp_auth)),
        )
        .layer(axum::middleware::from_fn(response_security))
}
async fn home() -> Redirect {
    Redirect::to("/lists/groceries")
}
async fn signin(State(s): State<AppState>) -> Html<String> {
    Html(format!(
        r#"<!doctype html><html lang="en"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Sign in · Todoer</title><link rel="stylesheet" href="/assets/app.css"><main class="login"><a class="brand" href="/">todoer<span>.pro</span></a><h1>Your day, with a little more clarity.</h1><p>This isolated rebuild uses synthetic data. Google sign-in is still being built.</p><form method="post" action="/signin"><input type="hidden" name="csrf" value="{}"><button class="primary">Enter development workspace</button></form><p class="muted">Local development only · no access to your existing account</p></main></html>"#,
        s.csrf
    ))
}
#[derive(Deserialize)]
struct Login {
    csrf: String,
}
async fn login(
    State(s): State<AppState>,
    h: HeaderMap,
    Form(f): Form<Login>,
) -> Result<Response, ApiError> {
    if f.csrf != s.csrf
        || h.get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|o| o != s.origin)
    {
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
    tasks: Vec<Task>,
    csrf: String,
}
#[derive(Template)]
#[template(path = "tasks.html")]
struct TaskRows {
    tasks: Vec<Task>,
}
async fn page(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    if s.auth(&h, false).is_err() {
        return Ok(Redirect::to("/signin").into_response());
    }
    let lists = s.run(|db| db.lists("dev")).await?;
    let list = lists.iter().find(|l| l.id == id).cloned().ok_or(ApiError(
        StatusCode::NOT_FOUND,
        "not_found",
        "List not found".into(),
    ))?;
    let tasks = s.run(move |db| db.tasks("dev", &id)).await?;
    let html = ListPage {
        lists,
        list,
        tasks,
        csrf: s.csrf.clone(),
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
            (header::HeaderName::from_static("x-todoer-account"), "dev"),
        ],
        Html(html),
    )
        .into_response())
}
async fn task_fragment(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>, ApiError> {
    s.auth(&h, false)?;
    let tasks = s.run(move |db| db.tasks("dev", &id)).await?;
    Ok(Html(TaskRows { tasks }.render().map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "template",
            "Render failed".into(),
        )
    })?))
}
async fn lists(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Vec<List>>, ApiError> {
    s.auth(&h, false)?;
    Ok(Json(s.run(|db| db.lists("dev")).await?))
}
async fn tasks(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<Task>>, ApiError> {
    s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.tasks("dev", &id)).await?))
}
async fn capture(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(c): Json<Capture>,
) -> Result<Json<Vec<Task>>, ApiError> {
    s.auth(&h, true)?;
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
    Ok(Json(s.run(move |db| db.capture("dev", &key, c)).await?))
}
async fn update(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<Change>,
) -> Result<Json<Task>, ApiError> {
    s.auth(&h, true)?;
    Ok(Json(s.run(move |db| db.update("dev", &id, c)).await?))
}
async fn subtree(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<Task>>, ApiError> {
    s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.subtree("dev", &id)).await?))
}
#[derive(Deserialize)]
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
    Json(c): Json<Move>,
) -> Result<Json<Task>, ApiError> {
    s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| {
            db.move_subtree(
                "dev",
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
struct Since {
    #[serde(default)]
    after: i64,
}
async fn changes(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<Since>,
) -> Result<Json<Vec<Value>>, ApiError> {
    s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.changes("dev", q.after)).await?))
}

#[derive(Deserialize)]
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
    s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| {
            db.search(
                "dev",
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
struct PreviewQuery {
    destination: String,
}
async fn move_preview(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<PreviewQuery>,
) -> Result<Json<Value>, ApiError> {
    s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| db.move_preview("dev", &id, &q.destination))
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
    if req.headers().get(header::AUTHORIZATION).is_none() {
        return ApiError(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Development MCP requires a Todoer bearer token; OAuth discovery is not implemented"
                .into(),
        )
        .into_response();
    }
    match s.auth(req.headers(), true) {
        Ok(_) => next.run(req).await,
        Err(e) => e.into_response(),
    }
}

mod filters {
    #[askama::filter_fn]
    pub fn markdown(s: &str, _values: &dyn askama::Values) -> askama::Result<String> {
        Ok(crate::markdown::render(s))
    }
}
#[derive(Deserialize)]
struct Revision {
    revision: i64,
}
async fn trash_tasks(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Vec<Task>>, ApiError> {
    s.auth(&h, false)?;
    Ok(Json(s.run(|db| db.trash_tasks("dev")).await?))
}
async fn trash_task(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<Revision>,
) -> Result<Json<Task>, ApiError> {
    s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.trash("dev", &id, c.revision)).await?,
    ))
}
async fn restore_task(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<Revision>,
) -> Result<Json<Task>, ApiError> {
    s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.restore("dev", &id, c.revision)).await?,
    ))
}
async fn undo(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(seq): Path<i64>,
    Json(c): Json<Revision>,
) -> Result<Json<Task>, ApiError> {
    s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.undo_update("dev", seq, c.revision))
            .await?,
    ))
}
async fn capabilities(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    s.auth(&h, false)?;
    Ok(Json(
        json!({"version":"0.1.0-dev","mode":"development-only","interfaces":["rest","cli","mcp-development"],"operations":["lists.read","tasks.read","capture","tasks.update","subtree.read","subtree.move","move.preview","search","changes","trash","restore","undo.update"],"limits":{"capture_batch":100,"search_limit":100,"changes_page":1000,"title_characters":500,"description_bytes":100000},"search":{"matching":"unicode-token-prefix","fuzzy":false,"sort":"id"},"production_auth":false}),
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
async fn search_page(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<SearchQuery>,
) -> Result<Response, ApiError> {
    if s.auth(&h, false).is_err() {
        return Ok(Redirect::to("/signin").into_response());
    }
    let query = q.query.clone();
    let tasks = s
        .run(move |db| {
            db.search(
                "dev",
                &q.query,
                q.list.as_deref(),
                q.complete,
                q.limit,
                q.after.as_deref(),
            )
        })
        .await?;
    render_review(ReviewPage {
        title: "Search".into(),
        mode: "search".into(),
        query,
        csrf: s.csrf,
        tasks,
        events: vec![],
    })
}
async fn trash_page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    if s.auth(&h, false).is_err() {
        return Ok(Redirect::to("/signin").into_response());
    }
    let tasks = s.run(|db| db.trash_tasks("dev")).await?;
    render_review(ReviewPage {
        title: "Trash".into(),
        mode: "trash".into(),
        query: String::new(),
        csrf: s.csrf,
        tasks,
        events: vec![],
    })
}
async fn activity_page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    if s.auth(&h, false).is_err() {
        return Ok(Redirect::to("/signin").into_response());
    }
    let values = s.run(|db| db.changes("dev", 0)).await?;
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
        csrf: s.csrf,
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
    let mut res = next.run(req).await;
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
        .insert("referrer-policy", "same-origin".parse().unwrap());
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

async fn events(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<Since>,
) -> Result<impl IntoResponse, ApiError> {
    s.auth(&h, false)?;
    let last = h
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(q.after)
        .max(0);
    let stream = futures_util::stream::unfold(
        (s, h, last, false),
        |(s, h, mut cursor, ended)| async move {
            if ended || s.stopping.load(std::sync::atomic::Ordering::SeqCst) {
                return None;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if s.auth(&h, false).is_err() {
                return Some((
                    Ok::<_, std::convert::Infallible>(
                        axum::response::sse::Event::default()
                            .event("session-expired")
                            .data("{}"),
                    ),
                    (s, h, cursor, true),
                ));
            }
            let values = s.run(move |db| db.changes("dev", cursor)).await;
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
                (s, h, cursor, false),
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
    s.run(|db| db.lists("dev")).await?;
    Ok(Json(json!({"ready":true,"mode":"development-only"})))
}
