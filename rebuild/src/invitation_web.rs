//! Invitation review never discloses a resource title before fresh Google proof.
use super::*;
use crate::invitations::InviteInput;
#[derive(Deserialize)]
struct Target {
    target_kind: String,
    target_id: String,
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/invitations", get(list).post(create))
        .route("/api/v1/invitations/{id}/revoke", post(revoke))
        .route("/invitations/new", get(form).post(submit))
        .route("/invitations/{token}", get(accept))
        .route("/invitations/manage/{id}/revoke", post(revoke_form))
}
fn signer(s: &AppState) -> Result<Arc<crate::invitations::InvitationSigner>, ApiError> {
    s.invitations.clone().ok_or(ApiError(
        StatusCode::SERVICE_UNAVAILABLE,
        "invitations_disabled",
        "Invitation signing is not configured".into(),
    ))
}
async fn list(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<Target>,
) -> Result<Json<Value>, ApiError> {
    let a = s.auth(&h, false)?;
    Ok(Json(
        serde_json::to_value(
            s.run(move |db| db.invitations(&a, &q.target_kind, &q.target_id))
                .await?,
        )
        .unwrap(),
    ))
}
async fn create(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<InviteInput>,
) -> Result<Json<crate::invitations::InvitationGrant>, ApiError> {
    let actor = s.auth(&h, true)?;
    let key = h
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError(
            StatusCode::BAD_REQUEST,
            "idempotency_key",
            "Idempotency-Key is required".into(),
        ))?
        .to_owned();
    let signer = signer(&s)?;
    Ok(Json(
        s.run(move |db| db.create_invitation(&actor, &key, input, &signer))
            .await?,
    ))
}
async fn revoke(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, true)?;
    s.run(move |db| db.revoke_invitation(&actor, &id)).await?;
    Ok(Json(json!({"revoked":true})))
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
async fn form(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<Target>,
) -> Result<Html<String>, ApiError> {
    let actor = s.auth(&h, false)?;
    let kind = q.target_kind.clone();
    let id = q.target_id.clone();
    let existing = s.run(move |db| db.invitations(&actor, &kind, &id)).await?;
    let mut history = String::new();
    for invitation in existing {
        let state = if invitation.claimed_by.is_some() {
            "Accepted"
        } else if invitation.revoked {
            "Revoked"
        } else if invitation.expires_at <= chrono::Utc::now().timestamp() {
            "Expired"
        } else {
            "Pending"
        };
        history.push_str(&format!(
            "<li class=\"py-3 border-b border-[var(--line)]\">{} · {} · {}",
            escape(&invitation.email),
            escape(&invitation.role),
            state
        ));
        if state == "Pending" {
            history.push_str(&format!("<form method=\"post\" action=\"/invitations/manage/{}/revoke\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button>Revoke invitation</button></form>",escape(&invitation.id),escape(&s.csrf_for(&h))));
        }
        history.push_str("</li>");
    }
    signer(&s)?;
    Ok(Html(format!(
        "<!doctype html><html lang=\"en\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Invite someone · Todoer</title><link rel=\"stylesheet\" href=\"/assets/app.css\"><main><a href=\"/workspaces\">← Workspaces</a><h1>Invite someone</h1><p>The recipient must verify this exact email with Google. This grants access to this {} and its content.</p><form class=\"grid gap-3\" method=\"post\" action=\"/invitations/new\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"target_kind\" value=\"{}\"><input type=\"hidden\" name=\"target_id\" value=\"{}\"><input type=\"hidden\" name=\"idempotency_key\" value=\"{}\"><label for=\"email\">Google account email</label><input id=\"email\" type=\"email\" name=\"email\" required><label for=\"role\">Access</label><select id=\"role\" name=\"role\"><option value=\"viewer\">Viewer</option><option value=\"editor\">Editor</option></select><label class=\"flex min-h-11 items-center gap-3\"><input type=\"checkbox\" name=\"acknowledge_access_change\" value=\"yes\" required>I approve granting this access.</label><button class=\"primary\">Create invitation</button></form><h2 class=\"mt-8\">Invitations</h2><ul>{history}</ul></main></html>",
        escape(&q.target_kind),
        escape(&s.csrf_for(&h)),
        escape(&q.target_kind),
        escape(&q.target_id),
        uuid::Uuid::new_v4()
    )))
}
#[derive(Deserialize)]
struct InviteForm {
    csrf: String,
    target_kind: String,
    target_id: String,
    email: String,
    role: String,
    acknowledge_access_change: Option<String>,
    idempotency_key: String,
}
async fn submit(
    State(s): State<AppState>,
    mut h: HeaderMap,
    Form(f): Form<InviteForm>,
) -> Result<Html<String>, ApiError> {
    if !s.origin_ok(&h) {
        return Err(Error::Forbidden.into());
    }
    h.insert(
        "x-csrf-token",
        f.csrf.parse().map_err(|_| Error::Forbidden)?,
    );
    let actor = s.auth(&h, true)?;
    let signer = signer(&s)?;
    let grant = s
        .run(move |db| {
            db.create_invitation(
                &actor,
                &f.idempotency_key,
                InviteInput {
                    target_kind: f.target_kind,
                    target_id: f.target_id,
                    email: f.email,
                    role: f.role,
                    acknowledge_access_change: f.acknowledge_access_change.as_deref()
                        == Some("yes"),
                },
                &signer,
            )
        })
        .await?;
    Ok(Html(format!(
        "<!doctype html><html lang=\"en\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Invitation created · Todoer</title><link rel=\"stylesheet\" href=\"/assets/app.css\"><main><h1>Invitation created</h1><p>This link expires in seven days and requires the invited Google email.</p><label for=\"link\">Invitation link</label><textarea id=\"link\" rows=\"4\" readonly>{}</textarea><p>Email delivery is queued when SMTP is configured. You can also share this link yourself.</p><a href=\"/workspaces\">Back to workspaces</a></main></html>",
        escape(&grant.link)
    )))
}
async fn accept(
    State(s): State<AppState>,
    Path(token): Path<String>,
) -> Result<Response, ApiError> {
    let signer = signer(&s)?;
    let value = token.clone();
    s.run(move |db| db.check_invitation(&value, None, &signer))
        .await?;
    let mut url = reqwest::Url::parse(&format!("{}/auth/google", s.origin))
        .map_err(|_| Error::Invalid("Invalid origin".into()))?;
    url.query_pairs_mut().append_pair("invitation", &token);
    Ok(Redirect::to(url.as_str().strip_prefix(&s.origin).unwrap()).into_response())
}

#[derive(Deserialize)]
struct RevokeForm {
    csrf: String,
}
async fn revoke_form(
    State(s): State<AppState>,
    mut h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<RevokeForm>,
) -> Result<Redirect, ApiError> {
    if !s.origin_ok(&h) {
        return Err(Error::Forbidden.into());
    }
    h.insert(
        "x-csrf-token",
        f.csrf.parse().map_err(|_| Error::Forbidden)?,
    );
    let a = s.auth(&h, true)?;
    s.run(move |db| db.revoke_invitation(&a, &id)).await?;
    Ok(Redirect::to("/workspaces"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    #[tokio::test]
    async fn actual_router_invitation_retry_mismatch_revocation_and_privacy() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let workspace = db
            .create_workspace(
                "dev",
                crate::workspace::WorkspaceInput {
                    title: "Private title never in public invite".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let signer = crate::invitations::InvitationSigner::new(
            &"11".repeat(32),
            "http://127.0.0.1:8092".into(),
        )
        .unwrap();
        let app = router(
            AppState::new(
                Arc::new(Mutex::new(db)),
                "synthetic-local-invitation-token".into(),
                "http://127.0.0.1:8092".into(),
            )
            .with_invitations(signer),
        );
        let value = json!({"target_kind":"workspace","target_id":workspace.id,"email":"guest@example.invalid","role":"viewer","acknowledge_access_change":true});
        let request = |body: Value| {
            Request::builder()
                .method("POST")
                .uri("/api/v1/invitations")
                .header("Authorization", "Bearer synthetic-local-invitation-token")
                .header("Content-Type", "application/json")
                .header("Idempotency-Key", "same-request")
                .body(Body::from(body.to_string()))
                .unwrap()
        };
        let response = app.clone().oneshot(request(value.clone())).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 10000)
                .await
                .unwrap(),
        )
        .unwrap();
        let again = app.clone().oneshot(request(value.clone())).await.unwrap();
        assert_eq!(again.status(), StatusCode::OK);
        let again: Value = serde_json::from_slice(
            &axum::body::to_bytes(again.into_body(), 10000)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body, again);
        let mut mismatch = value;
        mismatch["role"] = json!("editor");
        assert_eq!(
            app.clone()
                .oneshot(request(mismatch))
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
        let link = body["link"].as_str().unwrap();
        let path = reqwest::Url::parse(link).unwrap().path().to_owned();
        let public = app
            .clone()
            .oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let public_body = axum::body::to_bytes(public.into_body(), 10000)
            .await
            .unwrap();
        assert!(!String::from_utf8_lossy(&public_body).contains("Private title"));
        let revoke = format!(
            "/api/v1/invitations/{}/revoke",
            body["invitation"]["id"].as_str().unwrap()
        );
        assert_eq!(
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(revoke)
                        .header("Authorization", "Bearer synthetic-local-invitation-token")
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
}
