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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tower::ServiceExt;

    struct Fixture {
        state: AppState,
        path: String,
        cookie: String,
        csrf: String,
    }
    impl Fixture {
        fn new(admin: bool, email: Option<&str>) -> Self {
            let path = std::env::temp_dir()
                .join(format!("todoer-email-{}.db", uuid::Uuid::new_v4()))
                .to_string_lossy()
                .into_owned();
            let mut db = Store::open(&path).unwrap();
            db.seed_dev().unwrap();
            if admin {
                db.conn
                    .execute("INSERT INTO auth_admins VALUES('dev')", [])
                    .unwrap();
            }
            if let Some(email) = email {
                db.conn
                    .execute(
                        "INSERT INTO auth_identities(issuer,subject,user_id,email) VALUES('local','dev','dev',?1)",
                        [email],
                    )
                    .unwrap();
            }
            // A different account must never become the test recipient.
            db.conn
                .execute(
                    "INSERT OR IGNORE INTO users(id,name) VALUES('other','Other')",
                    [],
                )
                .unwrap();
            db.conn.execute("INSERT INTO auth_identities(issuer,subject,user_id,email) VALUES('local','other','other','other@synthetic.invalid')", []).unwrap();
            let auth = crate::auth::AuthStore::open(&path).unwrap();
            let grant = auth.issue_session("dev", 3600).unwrap();
            Self {
                state: AppState::production_optional(
                    Arc::new(Mutex::new(db)),
                    auth,
                    None,
                    "https://todoer.example".into(),
                ),
                path,
                cookie: format!("todoer_session={}; todoer_csrf={}", grant.token, grant.csrf),
                csrf: grant.csrf,
            }
        }
        fn request(&self, csrf: bool) -> Request<Body> {
            let mut request = Request::builder()
                .method("POST")
                .uri("/api/admin/email/test")
                .header("origin", "https://todoer.example")
                .header("cookie", &self.cookie)
                .header("content-type", "application/json");
            if csrf {
                request = request.header("x-csrf-token", &self.csrf);
            }
            request
                .body(Body::from(r#"{"recipient":"attacker@synthetic.invalid"}"#))
                .unwrap()
        }
        fn app(&self, mail: Option<crate::notifications::Notifications>) -> Router {
            // Inject a real lettre transport without process-global SMTP environment changes.
            Router::new()
                .route(
                    "/api/admin/email/test",
                    post(move |State(s): State<AppState>, h: HeaderMap| {
                        let mail = mail.clone();
                        async move { deliver_test(&s, &h, mail).await }
                    }),
                )
                .with_state(self.state.clone())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffix}", self.path));
            }
        }
    }
    async fn smtp(
        reject: bool,
    ) -> (
        crate::notifications::Notifications,
        tokio::task::JoinHandle<String>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            socket
                .get_mut()
                .write_all(b"220 synthetic.invalid ESMTP\r\n")
                .await
                .unwrap();
            let mut capture = String::new();
            let mut data = false;
            loop {
                let mut line = String::new();
                if socket.read_line(&mut line).await.unwrap() == 0 {
                    break;
                }
                capture.push_str(&line);
                let response = if data {
                    if line != ".\r\n" {
                        continue;
                    }
                    data = false;
                    if reject {
                        "550 sensitive-provider-secret\r\n"
                    } else {
                        "250 accepted\r\n"
                    }
                } else if line.starts_with("EHLO") {
                    "250 synthetic.invalid\r\n"
                } else if line.starts_with("DATA") {
                    data = true;
                    "354 send message\r\n"
                } else if line.starts_with("QUIT") {
                    break;
                } else {
                    "250 ok\r\n"
                };
                socket
                    .get_mut()
                    .write_all(response.as_bytes())
                    .await
                    .unwrap();
                if line == ".\r\n" {
                    break;
                }
            }
            capture
        });
        (
            crate::notifications::Notifications::synthetic_smtp(port),
            task,
        )
    }
    async fn body(response: Response) -> String {
        String::from_utf8(
            axum::body::to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn authorization_csrf_and_missing_configuration() {
        let regular = Fixture::new(false, Some("admin@synthetic.invalid"));
        assert_eq!(
            regular
                .app(None)
                .oneshot(regular.request(true))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let f = Fixture::new(true, Some("admin@synthetic.invalid"));
        assert_eq!(
            f.app(None)
                .oneshot(f.request(false))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            f.app(None).oneshot(f.request(true)).await.unwrap().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let mut request = f.request(true);
        request.headers_mut().remove("cookie");
        assert_eq!(
            f.app(None).oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let mut request = f.request(true);
        request
            .headers_mut()
            .insert("authorization", "Bearer synthetic".parse().unwrap());
        assert_eq!(
            f.app(None).oneshot(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        let mut request = f.request(true);
        request
            .headers_mut()
            .insert("origin", "https://attacker.invalid".parse().unwrap());
        assert_eq!(
            f.app(None).oneshot(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        for (fixture, expected) in [(&regular, StatusCode::FORBIDDEN), (&f, StatusCode::OK)] {
            let response = router(fixture.state.clone())
                .oneshot(
                    Request::builder()
                        .uri("/admin/email")
                        .header("cookie", &fixture.cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::OK {
                assert_eq!(
                    response.headers()[header::CACHE_CONTROL],
                    "private, no-store"
                );
                let html = body(response).await;
                assert!(html.contains("href=\"/admin/email\""));
                assert!(html.contains("admin@synthetic.invalid"));
            }
        }
    }

    #[tokio::test]
    async fn revoked_session_and_connection_failure() {
        let f = Fixture::new(true, Some("admin@synthetic.invalid"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mail = crate::notifications::Notifications::synthetic_smtp(port);
        assert_eq!(
            f.app(Some(mail.clone()))
                .oneshot(f.request(true))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_GATEWAY
        );
        f.state
            .store
            .lock()
            .unwrap()
            .conn
            .execute("UPDATE auth_sessions SET revoked=1", [])
            .unwrap();
        assert_eq!(
            f.app(Some(mail))
                .oneshot(f.request(true))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn sender_and_identity_are_html_escaped() {
        let html = EmailBody {
            status: "SMTP is configured.".into(),
            sender: "<sender>@synthetic.invalid".into(),
            recipient: "<admin>@synthetic.invalid".into(),
            enabled: true,
        }
        .render()
        .unwrap();
        assert!(!html.contains("<sender>"));
        assert!(!html.contains("<admin>"));
        assert!(html.contains("SMTP is configured."));
    }

    #[tokio::test]
    async fn missing_or_invalid_identity_cannot_send() {
        for email in [None, Some("invalid address")] {
            let f = Fixture::new(true, email);
            let mail = crate::notifications::Notifications::synthetic_smtp(1);
            let response = f.app(Some(mail)).oneshot(f.request(true)).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert!(body(response).await.contains("no valid verified email"));
            assert_eq!(
                f.state
                    .store
                    .lock()
                    .unwrap()
                    .conn
                    .query_row("SELECT count(*) FROM auth_throttle", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }

    #[tokio::test]
    async fn smtp_acceptance_rejection_recipient_and_persistent_throttle() {
        for reject in [false, true] {
            let f = Fixture::new(true, Some("admin@synthetic.invalid"));
            let (mail, capture) = smtp(reject).await;
            let app = f.app(Some(mail.clone()));
            let (first, second) = tokio::join!(
                app.clone().oneshot(f.request(true)),
                app.oneshot(f.request(true))
            );
            let first = first.unwrap();
            let second = second.unwrap();
            let (sent, limited) = if first.status() == StatusCode::TOO_MANY_REQUESTS {
                (second, first)
            } else {
                (first, second)
            };
            assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
            assert_eq!(
                sent.status(),
                if reject {
                    StatusCode::BAD_GATEWAY
                } else {
                    StatusCode::OK
                }
            );
            let feedback = body(sent).await;
            assert!(!feedback.contains("sensitive-provider-secret"));
            assert!(feedback.contains(if reject {
                "may have been accepted"
            } else {
                "does not guarantee inbox delivery"
            }));
            let captured = tokio::time::timeout(std::time::Duration::from_secs(5), capture)
                .await
                .unwrap()
                .unwrap();
            assert!(
                captured.contains("RCPT TO:<admin@synthetic.invalid>"),
                "{captured}"
            );
            assert!(captured.contains("MAIL FROM:<sender@synthetic.invalid>"));
            assert!(!captured.contains("attacker@"));
            assert!(!captured.contains("other@"));
            // New router/transport still sees the database reservation, including after failures.
            assert_eq!(
                f.app(Some(mail))
                    .oneshot(f.request(true))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::TOO_MANY_REQUESTS
            );
        }
    }
}
