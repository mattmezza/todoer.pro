use super::*;
#[derive(Template)]
#[template(path = "notifications.html")]
struct Page {
    sidebar: Sidebar,
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(page))
        .route("/api/v1/notifications", get(inbox))
        .route("/api/v1/notifications/read", post(read))
}
#[derive(Default, Deserialize)]
struct QueryInbox {
    before: Option<i64>,
    unread: Option<bool>,
    limit: Option<usize>,
}
fn actor(s: &AppState, h: &HeaderMap, write: bool) -> Result<String, ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    s.auth(h, write)
}
async fn page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let a = actor(&s, &h, false)?;
    let csrf = s.csrf_for(&h);
    let sidebar = s.run(move |db| build_sidebar(db, &a, "", csrf)).await?;
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Html(
            Page { sidebar }
                .render()
                .map_err(|_| Error::Invalid("Could not render notifications".into()))?,
        ),
    )
        .into_response())
}
async fn inbox(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<QueryInbox>,
) -> Result<Response, ApiError> {
    let a = actor(&s, &h, false)?;
    let result = s
        .run(move |db| {
            db.notification_inbox(
                &a,
                q.before,
                q.unread.unwrap_or(false),
                q.limit.unwrap_or(25),
            )
        })
        .await?;
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(result)).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    through: i64,
    id: Option<i64>,
}
async fn read(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(q): Json<Read>,
) -> Result<Response, ApiError> {
    let a = actor(&s, &h, true)?;
    s.run(move |db| db.read_notifications(&a, q.through, q.id))
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
impl AppState {
    pub fn start_inbox_worker(&self) -> tokio::task::JoinHandle<()> {
        let state = self.clone();
        tokio::spawn(async move {
            while !state.stopping.load(std::sync::atomic::Ordering::SeqCst) {
                if state
                    .run(|db| db.queue_inbox_notifications(chrono::Utc::now().timestamp()))
                    .await
                    .is_err()
                {
                    eprintln!("notification inbox projection failed; next tick will retry");
                }
                for _ in 0..15 {
                    if state.stopping.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    #[tokio::test]
    async fn notification_api_is_browser_only_and_writes_require_csrf() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            "agent".into(),
            "http://127.0.0.1:8000".into(),
        );
        state
            .sessions
            .lock()
            .unwrap()
            .insert(Sha256::digest(b"browser").into(), std::time::Instant::now());
        let app = routes().with_state(state);
        for (auth, expected) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some("Bearer agent"), StatusCode::FORBIDDEN),
        ] {
            let mut req = axum::http::Request::builder().uri("/api/v1/notifications");
            if let Some(auth) = auth {
                req = req.header("authorization", auth);
            }
            let response = app
                .clone()
                .oneshot(req.body(axum::body::Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/notifications")
                    .header("cookie", "todoer_dev=browser")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/notifications/read")
                    .header("cookie", "todoer_dev=browser")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"through":999}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
