//! Browser shells for account AI controls, billing, usage and platform operations.
use super::*;

pub(super) fn ai_settings_body() -> &'static str {
    include_str!("../templates/ai-billing-settings.html")
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/settings/billing", get(billing))
        .route("/settings/ai/usage", get(usage))
        .route("/admin", get(admin))
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
    let user = session(&s, &h)?;
    s.run(move |db| {
        if !crate::account_security::is_admin(&db.conn, &user).map_err(|_| Error::Forbidden)? {
            return Err(Error::Forbidden);
        }
        Ok(())
    })
    .await?;
    scheduling_http::render_settings(
        &s,
        &h,
        "Admin",
        include_str!("../templates/ai-admin.html").into(),
        "admin",
    )
    .await
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
        for authorization in [false, true] {
            let mut request = Request::builder().uri("/admin");
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
    }
}
