//! Browser shells for account AI controls, billing, usage and platform operations.
use super::*;

pub(super) fn ai_settings_body() -> &'static str {
    include_str!("../templates/ai-billing-settings.html")
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/settings/billing", get(billing))
        .route("/settings/ai/setup", get(setup))
        .route("/settings/ai/usage", get(usage))
        .route("/admin", get(admin))
        .route("/admin/{screen}", get(admin_screen))
}

fn session(s: &AppState, h: &HeaderMap) -> Result<String, ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(crate::auth::AuthError::Forbidden.into());
    }
    let user = s.auth(h, false)?;
    if user.starts_with("credential:") {
        return Err(crate::auth::AuthError::Forbidden.into());
    }
    Ok(user)
}

async fn setup(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    session(&s, &h)?;
    scheduling_http::render_settings(
        &s,
        &h,
        "AI setup",
        include_str!("../templates/ai-setup.html").into(),
        "ai-setup",
    )
    .await
}

async fn billing(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    session(&s, &h)?;
    scheduling_http::render_settings(
        &s,
        &h,
        "Billing",
        include_str!("../templates/billing-settings.html").into(),
        "billing",
    )
    .await
}

async fn usage(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    session(&s, &h)?;
    scheduling_http::render_settings(
        &s,
        &h,
        "AI usage",
        include_str!("../templates/ai-usage.html").into(),
        "usage",
    )
    .await
}

async fn admin(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    render_admin(s, h, "finance").await
}

async fn admin_screen(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(screen): Path<String>,
) -> Result<Response, ApiError> {
    render_admin(s, h, &screen).await
}

async fn render_admin(s: AppState, h: HeaderMap, screen: &str) -> Result<Response, ApiError> {
    let (title, body) = match screen {
        "finance" => (
            "Finance",
            include_str!("../templates/ai-admin-finance.html"),
        ),
        "access" => (
            "AI access",
            include_str!("../templates/ai-admin-access.html"),
        ),
        "models" => (
            "Models and pricing",
            include_str!("../templates/ai-admin-models.html"),
        ),
        "credits" => (
            "Free credits",
            include_str!("../templates/ai-admin-credits.html"),
        ),
        "operations" => (
            "Operations",
            include_str!("../templates/ai-admin-operations.html"),
        ),
        "audit" => (
            "Audit history",
            include_str!("../templates/ai-admin-audit.html"),
        ),
        _ => return Ok(StatusCode::NOT_FOUND.into_response()),
    };
    let user = session(&s, &h)?;
    s.run(move |db| {
        if !crate::account_security::is_admin(&db.conn, &user).map_err(|_| Error::Forbidden)? {
            return Err(Error::Forbidden);
        }
        Ok(())
    })
    .await?;
    scheduling_http::render_settings(&s, &h, title, body.into(), &format!("admin-{screen}")).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn admin_page_rejects_regular_and_bearer_accounts() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            "synthetic-ui-token-at-least-24".into(),
            "http://127.0.0.1:8092".into(),
        );
        let csrf = state.csrf.clone();
        let store = state.store.clone();
        let app = router(state);
        let login = app
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
        let cookie = login.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        for (path, required, absent) in [
            ("/settings/ai", "data-capture-preferences", "data-ai-key"),
            (
                "/settings/ai/setup",
                "data-ai-key",
                "data-capture-preferences",
            ),
            ("/settings/ai/tools", "Connected tools", "data-ai-policy"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("cookie", cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body = String::from_utf8(bytes.to_vec()).unwrap();
            assert!(body.contains(required), "{path} is missing {required}");
            assert!(!body.contains(absent), "{path} includes {absent}");
        }
        for (path, authorization) in [
            "/admin",
            "/admin/finance",
            "/admin/access",
            "/admin/models",
            "/admin/credits",
            "/admin/operations",
            "/admin/audit",
        ]
        .into_iter()
        .flat_map(|path| [false, true].map(|bearer| (path, bearer)))
        {
            let mut request = Request::builder().uri(path);
            if authorization {
                request = request.header("authorization", "Bearer synthetic-ui-token-at-least-24");
            } else {
                request = request.header("cookie", cookie);
            }
            assert_eq!(
                app.clone()
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        store
            .lock()
            .unwrap()
            .conn
            .execute("INSERT INTO auth_admins VALUES('dev')", [])
            .unwrap();
        for screen in [
            "finance",
            "access",
            "models",
            "credits",
            "operations",
            "audit",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/admin/{screen}"))
                        .header("cookie", cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body = String::from_utf8(bytes.to_vec()).unwrap();
            assert!(body.contains("aria-label=\"Admin\""));
            assert!(body.contains(&format!("data-admin-screen=\"{screen}\"")));
            assert_eq!(body.matches("data-admin-screen=").count(), 1);
            if screen != "credits" {
                assert!(!body.contains("data-admin-grant"));
            }
        }
    }
}
