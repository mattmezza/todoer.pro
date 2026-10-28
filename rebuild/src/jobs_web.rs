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
    format!("<time data-unix-time=\"{value}\"></time>")
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
    let paged = q.cursor.is_some();
    let result = load(&s, &h, q).await?;
    let mut content = String::from(r#"<form method="get" class="delivery-toolbar"><label>Show<select name="status">"#);
    for (value, label) in [
        ("", "All deliveries"), ("failed", "Needs attention"),
        ("pending", "Waiting"), ("leased", "Sending"),
        ("dead", "Stopped"), ("cancelled", "Cancelled"),
        ("delivered", "Delivered reminders"), ("sent", "Sent"),
    ] {
        content.push_str(&format!("<option value=\"{value}\" {}>{label}</option>", if status == value { "selected" } else { "" }));
    }
    content.push_str(&format!(r#"</select></label><input type="hidden" name="limit" value="{limit}"><button class="secondary">Apply filter</button><button type="button" data-refresh-deliveries class="delivery-refresh" title="Refresh this view"><svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#repeat"/></svg>Refresh</button></form>"#));
    if result.items.is_empty() {
        let message = if status.is_empty() { "Reminders, summaries, and invitations will appear here when there’s something to send." } else { "There are no deliveries matching this filter." };
        content.push_str(&format!(r#"<section class="settings-preference-card settings-empty delivery-empty"><svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#check"/></svg><h2>{}</h2><p>{message}</p></section>"#, if status.is_empty() { "Nothing to send yet" } else { "All clear here" }));
    } else {
        content.push_str(r#"<div class="delivery-list" aria-label="Delivery status">"#);
    }
    for row in &result.items {
        let title = match row.kind.as_str() { "daily" => "Daily summary".into(), "weekly" => "Weekly summary".into(), "reminder" => "Task reminder".into(), "invitation" => "Sharing invitation".into(), _ => row.kind.replace('_', " ") };
        let channel = match row.channel.as_str() { "email" => "Email", "push" => "Push notification", "in_app" => "In app", _ => &row.channel };
        let (label, tone, description) = match row.status.as_str() {
            "pending" => ("Waiting", "waiting", if row.attempts == 0 { "Waiting for its scheduled time or quiet hours to end." } else { "Waiting for another delivery attempt." }),
            "leased" => ("Sending", "waiting", "This delivery is being processed."),
            "dead" => ("Stopped", "warning", "Delivery has stopped. See the details below."),
            "cancelled" => ("Cancelled", "neutral", "This delivery was cancelled."),
            "delivered" | "sent" => ("Sent", "success", "Accepted for delivery."),
            _ => (row.status.as_str(), "neutral", ""),
        };
        content.push_str(&format!(r#"<article class="settings-preference-card delivery-entry"><header><span class="settings-card-icon"><svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#{}"/></svg></span><div><h2>{}</h2><span class="muted delivery-channel">{}</span></div><span class="delivery-badge delivery-{}">{}</span></header><p class="delivery-description">{}</p>"#, if row.channel == "email" { "inbox" } else { "bell" }, esc(&title), esc(channel), tone, esc(label), esc(description)));
        if let Some(code) = row.failure_code.as_deref() {
            let reason = match code {
                "subscription_expired" => "A browser subscription expired. Enable it again in Devices.",
                "provider_temporarily_unavailable" => "The delivery service was temporarily unavailable.",
                "provider_rejected_delivery" => "The delivery service rejected the last attempt.",
                "retry_exhausted" => "The retry limit was reached.",
                "device_delivery_failed" => "One or more devices could not be reached.",
                _ => "The last attempt could not be confirmed.",
            };
            content.push_str(&format!(r#"<p class="delivery-reason"><svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#info"/></svg>{reason}</p>"#));
        }
        if row.status == "pending" {
            content.push_str(&format!(r#"<p class="delivery-schedule"><span>Next eligible attempt</span>{}</p>"#, instant(row.available_at)));
        }
        if row.channel == "push" {
            content.push_str(&format!(r#"<div class="device-delivery-counts"><span>{} waiting</span><span>{} sent</span><span>{} stopped</span></div>"#, row.devices_pending, row.devices_sent, row.devices_dead));
        }
        content.push_str(&format!(r#"<details class="delivery-details"><summary>Delivery details</summary><dl><div><dt>Attempts</dt><dd>{}</dd></div><div><dt>Available since</dt><dd>{}</dd></div>"#, row.attempts, instant(row.available_at)));
        if let Some(until) = row.lease_until {
            content.push_str(&format!("<div><dt>Processing window ends</dt><dd>{}</dd></div>", instant(until)));
        }
        content.push_str(&format!(r#"<div><dt>Reference</dt><dd><code>{} / {}</code></dd></div></dl></details></article>"#, esc(&row.source), esc(&row.id)));
    }
    if !result.items.is_empty() { content.push_str("</div>"); }
    if paged || result.next_cursor.is_some() {
        content.push_str(r#"<nav class="delivery-pagination" aria-label="Delivery pages">"#);
        if paged {
            let mut query = reqwest::Url::parse("https://pagination.invalid/settings/jobs").unwrap();
            query.query_pairs_mut().append_pair("status", &status).append_pair("limit", &limit.to_string());
            content.push_str(&format!(r#"<a href="{}">Back to first page</a>"#, esc(&format!("{}?{}", query.path(), query.query().unwrap()))));
        }
        if let Some(cursor) = result.next_cursor {
            let mut query = reqwest::Url::parse("https://pagination.invalid/settings/jobs").unwrap();
            query.query_pairs_mut().append_pair("cursor", &cursor).append_pair("status", &status).append_pair("limit", &limit.to_string());
            content.push_str(&format!(r#"<a class="secondary" href="{}">More deliveries →</a>"#, esc(&format!("{}?{}", query.path(), query.query().unwrap()))));
        }
        content.push_str("</nav>");
    }
    content.push_str(r#"<div class="settings-footer-help"><a href="/settings/notifications">Manage notifications <span aria-hidden="true">→</span></a><details><summary>About delivery statuses</summary><p>This is a record of reminders, summaries, and invitations. Sent means the delivery service accepted the message; it doesn’t confirm that it was read. “Needs attention” includes deliveries with a previous failed attempt, even if a later attempt succeeded.</p><p>Deliveries are grouped by type, rather than ordered by date. Refresh checks for updates without sending anything.</p></details></div><script type="module" src="/assets/delivery-settings.js"></script>"#);
    let mut response = super::scheduling_http::render_settings(&s, &h, "Delivery", content, "jobs").await?;
    response.headers_mut().insert(header::CACHE_CONTROL, "no-store, private".parse().unwrap());
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
