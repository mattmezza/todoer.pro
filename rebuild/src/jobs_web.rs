//! Read-only account job visibility; no queue mutation or transport disclosure.
use super::*;
use crate::jobs::{JobPage, JobQuery};
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/jobs", get(api))
        .route("/settings/jobs", get(page))
}
fn esc(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn instant(value: i64) -> String {
    chrono::DateTime::from_timestamp(value, 0)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| "Unknown time".into())
}
async fn load(s: &AppState, h: &HeaderMap, q: JobQuery) -> Result<JobPage, ApiError> {
    let actor = s.auth(h, false)?;
    s.run(move |db| db.job_status(&actor, q)).await
}
async fn api(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<JobQuery>,
) -> Result<Response, ApiError> {
    Ok((
        [(header::CACHE_CONTROL, "no-store, private")],
        Json(load(&s, &h, q).await?),
    )
        .into_response())
}
async fn page(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<JobQuery>,
) -> Result<Response, ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    let status = q.status.clone().unwrap_or_default();
    let limit = q.limit.unwrap_or(25);
    let result = load(&s, &h, q).await?;
    let mut content = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Job status · Todoer</title><link rel=\"stylesheet\" href=\"/assets/app.css\"></head><body><header class=\"mobile-header\"><a class=\"brand\" href=\"/\">todoer<span>.pro</span></a><a class=\"nav-link\" href=\"/settings/notifications\">Notifications</a></header><aside aria-label=\"Navigation\"><a class=\"brand\" href=\"/\">todoer<span>.pro</span></a><nav><a class=\"nav-link\" href=\"/\">Tasks</a><a class=\"nav-link\" href=\"/settings/notifications\">Notifications</a><a class=\"nav-link\" href=\"/settings/devices\">Push devices</a></nav></aside><main><h1>Job status</h1><p class=\"muted\">Your reminder, summary and invitation delivery jobs. Times are UTC. This page is read-only; it does not send, retry or discard work.</p><form method=\"get\" class=\"grid gap-3 mt-8\"><label>Status<select name=\"status\">",
    );
    for (value, label) in [
        ("", "All statuses"),
        ("failed", "Recorded failures (including devices)"),
        ("pending", "Waiting or retrying"),
        ("leased", "Processing"),
        ("dead", "Stopped after failure"),
        ("cancelled", "Cancelled"),
        ("delivered", "Reminder delivered"),
        ("sent", "Sent"),
    ] {
        content.push_str(&format!(
            "<option value=\"{value}\" {}>{label}</option>",
            if status == value { "selected" } else { "" }
        ));
    }
    content.push_str("</select></label><button class=\"nav-link min-h-11 justify-self-start\">Filter jobs</button></form><p class=\"muted mt-8\">Ordered by queue then stable job ID, not newest first. Refresh to see current status. A delivered/sent status records provider acceptance, not proof that a person received it. Pending jobs may be waiting for their schedule, quiet hours or a retry. Leases can be reclaimed after expiry.</p>");
    if result.items.is_empty() {
        content.push_str("<p class=\"mt-8\">No jobs match this view.</p>");
    }
    for row in result.items {
        let clock_label = if row.status == "pending" {
            "Next eligible attempt"
        } else if row.status == "leased" {
            "Available since"
        } else {
            "Recorded availability"
        };
        content.push_str(&format!("<article class=\"mt-8 border-t border-[var(--line)] pt-6\"><h2>{} · {}</h2><p>Status: <strong>{}</strong> · Attempts: {}</p><p>{}: <time>{}</time></p>",esc(&row.kind.replace('_'," ")),esc(&row.channel),esc(&row.status),row.attempts,clock_label,instant(row.available_at)));
        if let Some(until) = row.lease_until {
            content.push_str(&format!(
                "<p>Lease expires: <time>{}</time></p>",
                instant(until)
            ));
        }
        if let Some(code) = row.failure_code {
            content.push_str(&format!(
                "<p>Last failure: {}</p>",
                esc(&code.replace('_', " "))
            ));
        }
        if row.channel == "push" {
            content.push_str(&format!("<p>Device deliveries: {} waiting · {} sent · {} stopped · {} with failure recorded.</p>",row.devices_pending,row.devices_sent,row.devices_dead,row.devices_failed));
        }
        content.push_str(&format!(
            "<p class=\"muted break-all text-sm\">{} / {}</p></article>",
            esc(&row.source),
            esc(&row.id)
        ));
    }
    if let Some(cursor) = result.next_cursor {
        let mut query = reqwest::Url::parse("https://pagination.invalid/settings/jobs").unwrap();
        query
            .query_pairs_mut()
            .append_pair("cursor", &cursor)
            .append_pair("status", &status)
            .append_pair("limit", &limit.to_string());
        content.push_str(&format!(
            "<a class=\"nav-link min-h-11 mt-8\" href=\"{}\">Next jobs</a>",
            esc(&format!("{}?{}", query.path(), query.query().unwrap()))
        ));
    }
    content.push_str("<a class=\"nav-link min-h-11 mt-8\" href=\"/settings/jobs\">Refresh all jobs</a></main></body></html>");
    let mut response =
        super::scheduling_http::render_settings(&s, &h, "Delivery status", content, "jobs").await?;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store, private".parse().unwrap());
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthStore, Permission};
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    struct Fixture {
        state: AppState,
        dir: std::path::PathBuf,
        cookie: String,
        token: String,
        admin: crate::auth::AgentGrant,
        scoped: crate::auth::AgentGrant,
        reader: crate::auth::AgentGrant,
        other: crate::auth::AgentGrant,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("todoer-jobs-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&dir).unwrap();
            let path = dir.join("synthetic.db");
            let mut db = Store::open(path.to_str().unwrap()).unwrap();
            db.seed_dev().unwrap();
            db.conn.execute("INSERT INTO digest_outbox(id,user_id,kind,period,status,attempts,available_at,lease_owner,lease_token,last_error) VALUES('own-job','dev','daily','2026-10-26','pending',2,1,'secret-owner','secret-lease','secret-endpoint-or-recipient'),('other-job','other','daily','2026-10-26','dead',5,2,NULL,NULL,'other-secret')",[]).unwrap();
            let auth = AuthStore::open(path.to_str().unwrap()).unwrap();
            let session = auth.issue_session("dev", 3600).unwrap();
            let audience = "https://todoer.example/api/v1";
            let admin = auth
                .issue_agent("dev", audience, vec![Permission::Admin], None, None, 3600)
                .unwrap();
            let scoped = auth
                .issue_agent(
                    "dev",
                    audience,
                    vec![Permission::Admin],
                    Some(vec!["inbox".into()]),
                    None,
                    3600,
                )
                .unwrap();
            let reader = auth
                .issue_agent("dev", audience, vec![Permission::Read], None, None, 3600)
                .unwrap();
            let other = auth
                .issue_agent("other", audience, vec![Permission::Admin], None, None, 3600)
                .unwrap();
            let cookie = format!(
                "todoer_session={}; todoer_csrf={}",
                session.token, session.csrf
            );
            let state = AppState::production(
                Arc::new(Mutex::new(db)),
                auth,
                crate::oidc::GoogleOidc::test_fixture(),
                "https://todoer.example".into(),
            );
            Self {
                state,
                dir,
                cookie,
                token: session.token,
                admin,
                scoped,
                reader,
                other,
            }
        }
        async fn get(
            &self,
            path: &str,
            credential: Option<&str>,
        ) -> (StatusCode, String, Option<String>) {
            let mut req = Request::builder().uri(path);
            if let Some(token) = credential {
                req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
            } else {
                req = req.header(header::COOKIE, &self.cookie);
            }
            let response = routes()
                .with_state(self.state.clone())
                .oneshot(req.body(Body::empty()).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let cache = response
                .headers()
                .get(header::CACHE_CONTROL)
                .map(|v| v.to_str().unwrap().to_owned());
            let body = response.into_body().collect().await.unwrap().to_bytes();
            (status, String::from_utf8(body.to_vec()).unwrap(), cache)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    #[tokio::test]
    async fn authenticated_router_hides_other_accounts_and_transport_data() {
        let f = Fixture::new();
        let (status, body, cache) = f.get("/api/v1/jobs", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(cache.as_deref(), Some("no-store, private"));
        assert!(body.contains("own-job"));
        assert!(!body.contains("other-job"));
        assert!(!body.contains("secret"));
        assert!(body.contains("delivery_failed_or_ambiguous"));
        let (status, html, _) = f.get("/settings/jobs", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Delivery status"));
        assert!(html.contains("aria-label=\"Settings\""));
        assert!(!html.contains("secret"));
        assert!(!html.contains("other-job"));
        let (status, body, _) = f.get("/api/v1/jobs", Some(&f.other.token)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("other-job"));
        assert!(!body.contains("own-job"));
        assert!(
            !f.get("/api/v1/jobs?status=dead", Some(&f.admin.token))
                .await
                .1
                .contains("own-job")
        );
        assert_eq!(
            f.get("/api/v1/jobs?limit=101", None).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    #[tokio::test]
    async fn job_counts_require_live_unbounded_admin_and_page_requires_browser() {
        let f = Fixture::new();
        assert_eq!(
            f.get("/api/v1/jobs", Some(&f.scoped.token)).await.0,
            StatusCode::FORBIDDEN
        );
        let workspace = f
            .state
            .production
            .as_ref()
            .unwrap()
            .store
            .lock()
            .unwrap()
            .issue_agent(
                "dev",
                "https://todoer.example/api/v1",
                vec![Permission::Admin],
                None,
                Some("synthetic-workspace".into()),
                3600,
            )
            .unwrap();
        assert_eq!(
            f.get("/api/v1/jobs", Some(&workspace.token)).await.0,
            StatusCode::FORBIDDEN
        );

        assert_eq!(
            f.get("/api/v1/jobs", Some(&f.reader.token)).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            f.get("/api/v1/jobs", Some(&f.admin.token)).await.0,
            StatusCode::OK
        );
        assert_eq!(
            f.get("/settings/jobs", Some(&f.admin.token)).await.0,
            StatusCode::FORBIDDEN
        );
        f.state
            .store
            .lock()
            .unwrap()
            .conn
            .execute(
                "UPDATE auth_agents SET expires_at=0 WHERE id=?1",
                [&workspace.id],
            )
            .unwrap();
        assert_eq!(
            f.get("/api/v1/jobs", Some(&workspace.token)).await.0,
            StatusCode::UNAUTHORIZED
        );

        f.state
            .production
            .as_ref()
            .unwrap()
            .store
            .lock()
            .unwrap()
            .revoke_agent("dev", &f.admin.id)
            .unwrap();
        assert_eq!(
            f.get("/api/v1/jobs", Some(&f.admin.token)).await.0,
            StatusCode::UNAUTHORIZED
        );
        f.state
            .production
            .as_ref()
            .unwrap()
            .store
            .lock()
            .unwrap()
            .revoke_session(&f.token)
            .unwrap();
        assert_eq!(
            f.get("/api/v1/jobs", None).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
}
