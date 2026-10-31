//! Platform SMTP diagnostics, restricted to the administrator's verified identity.
use super::*;
use rusqlite::OptionalExtension;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/email", get(page))
        .route("/api/admin/email/test", post(send_test))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("private, no-store"),
        ))
}

async fn administrator(s: &AppState, h: &HeaderMap, write: bool) -> Result<String, ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    let user = if write && s.production.is_some() {
        account_http::session_user(s, h, false).await?
    } else {
        s.auth(h, write)?
    };
    let actor = user.clone();
    s.run(move |db| {
        if !crate::account_security::is_admin(&db.conn, &actor).map_err(|_| Error::Forbidden)? {
            return Err(Error::Forbidden);
        }
        Ok(())
    })
    .await?;
    Ok(user)
}

fn recipient(db: &Store, user: &str) -> Result<Option<String>, Error> {
    // Identities are created only after local verification or verified provider claims.
    let email: Option<String> = db.conn.query_row(
        "SELECT email FROM auth_identities WHERE user_id=?1 ORDER BY CASE WHEN issuer='local' THEN 0 ELSE 1 END,issuer,subject LIMIT 1",
        [user], |r| r.get(0),
    ).optional()?;
    Ok(email.filter(|e| e.parse::<lettre::Address>().is_ok()))
}

#[derive(Template)]
#[template(path = "admin-email.html")]
struct EmailBody {
    status: String,
    sender: String,
    recipient: String,
    enabled: bool,
}
async fn page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let user = administrator(&s, &h, false).await?;
    let recipient = s.run(move |db| recipient(db, &user)).await?;
    let mail = crate::notifications::Notifications::from_env();
    let (status, sender, configured) = match mail {
        Ok(Some(mail)) => ("SMTP is configured.", mail.sender_address(), true),
        Ok(None) => ("SMTP is not configured.", String::new(), false),
        Err(_) => (
            "SMTP configuration is invalid. Check the server configuration.",
            String::new(),
            false,
        ),
    };
    let body = EmailBody {
        status: status.into(),
        sender,
        enabled: configured && recipient.is_some(),
        recipient: recipient.unwrap_or_default(),
    }
    .render()
    .map_err(|_| Error::Forbidden)?;
    scheduling_http::render_settings(&s, &h, "Email", body, "admin-email").await
}

async fn send_test(
    State(s): State<AppState>,
    h: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    administrator(&s, &h, true).await?;
    let mail = crate::notifications::Notifications::from_env().map_err(|_| {
        problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "SMTP configuration is invalid.",
        )
    })?;
    deliver_test(&s, &h, mail).await
}

async fn deliver_test(
    s: &AppState,
    h: &HeaderMap,
    mail: Option<crate::notifications::Notifications>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = administrator(s, h, true).await?;
    let mail =
        mail.ok_or_else(|| problem(StatusCode::SERVICE_UNAVAILABLE, "SMTP is not configured."))?;
    // Recheck the live session and administrator status after waiting for the store.
    // Reserve before awaiting SMTP; failures count because a timeout may mean acceptance.
    let (email, allowed) = ai_policy_http::change(s, h, user, false, move |db, actor| {
        if !crate::account_security::is_admin(&db.conn, actor).map_err(|_| Error::Forbidden)? {
            return Err(Error::Forbidden);
        }
        let Some(email) = recipient(db, actor)? else { return Ok((None, false)); };
        let key = crate::auth::hash(&format!("admin-email-test:{actor}"));
        let changed = db.conn.execute(
            "INSERT INTO auth_throttle(key,window_start,attempts) VALUES(?1,unixepoch(),1) ON CONFLICT(key) DO UPDATE SET window_start=unixepoch(),attempts=1 WHERE window_start<=unixepoch()-900",
            [key],
        )?;
        Ok((Some(email), changed == 1))
    }).await?;
    let email = email.ok_or_else(|| {
        problem(
            StatusCode::BAD_REQUEST,
            "Your account has no valid verified email address.",
        )
    })?;
    if !allowed {
        return Err(problem(
            StatusCode::TOO_MANY_REQUESTS,
            "Wait 15 minutes between test email attempts.",
        ));
    }
    mail.send_account(&email, "Todoer Pro SMTP test", "This test email was requested from Admin → Email in Todoer Pro. SMTP acceptance does not guarantee inbox delivery.")
        .await.map_err(|_| problem(StatusCode::BAD_GATEWAY, "SMTP delivery failed or timed out. Delivery may have been accepted. Check the server configuration and wait 15 minutes before retrying."))?;
    Ok(Json(
        serde_json::json!({"message":"SMTP accepted the test email for your verified account address. Acceptance does not guarantee inbox delivery; check your inbox and spam folder."}),
    ))
}
fn problem(status: StatusCode, message: &str) -> ApiError {
    ApiError(status, "email", message.into())
}
