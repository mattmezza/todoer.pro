//! HTTP adapter for local OAuth storage; consent is browser-only and explicit.
use super::*;
use crate::auth::{AuthError, AuthorizationRequest, Permission};
use axum::{
    extract::Request,
    middleware::{self, Next},
};
use std::collections::HashMap;

type OAuthResult = Result<Response, OAuthError>;
struct OAuthError(StatusCode, &'static str, String);
impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(json!({"error":self.1,"error_description":self.2})),
        )
            .into_response()
    }
}
fn bad(message: &str) -> OAuthError {
    OAuthError(StatusCode::BAD_REQUEST, "invalid_request", message.into())
}
fn unavailable() -> OAuthError {
    OAuthError(
        StatusCode::SERVICE_UNAVAILABLE,
        "temporarily_unavailable",
        "OAuth is not configured".into(),
    )
}
fn storage(error: AuthError) -> OAuthError {
    match error {
        AuthError::Database(_) => OAuthError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "Authorization storage unavailable".into(),
        ),
        AuthError::Unauthorized => OAuthError(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "Grant is invalid or expired".into(),
        ),
        AuthError::Forbidden => OAuthError(
            StatusCode::FORBIDDEN,
            "access_denied",
            "Access denied".into(),
        ),
        AuthError::Invalid(message) => bad(&message),
    }
}
fn escaped(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn canonical(s: &AppState) -> String {
    format!("{}/mcp", s.origin.trim_end_matches('/'))
}
fn browser(s: &AppState, h: &HeaderMap, write: bool) -> Result<String, OAuthError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(OAuthError(
            StatusCode::FORBIDDEN,
            "access_denied",
            "Consent requires a browser session".into(),
        ));
    }
    let actor = s.auth(h, write).map_err(|_| {
        OAuthError(
            StatusCode::UNAUTHORIZED,
            "access_denied",
            "Sign in before reviewing this request".into(),
        )
    })?;
    if actor.starts_with("credential:") {
        return Err(bad("Consent requires a browser session"));
    }
    Ok(actor)
}
async fn response_policy(request: Request, next: Next) -> Response {
    let public = !matches!(request.uri().path(), "/oauth/authorize" | "/oauth/approve");
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::PRAGMA, "no-cache".parse().unwrap());
    response
        .headers_mut()
        .insert("referrer-policy", "no-referrer".parse().unwrap());
    response
        .headers_mut()
        .insert("x-frame-options", "DENY".parse().unwrap());
    if public {
        for (name, value) in [
            ("access-control-allow-origin", "*"),
            ("access-control-allow-methods", "GET, POST, OPTIONS"),
            ("access-control-allow-headers", "Content-Type"),
        ] {
            response.headers_mut().insert(
                axum::http::HeaderName::from_static(name),
                value.parse().unwrap(),
            );
        }
    }
    response
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/oauth-authorization-server",
            get(as_metadata).options(preflight),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(resource_metadata).options(preflight),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(resource_metadata).options(preflight),
        )
        .route("/oauth/register", post(register).options(preflight))
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/approve", post(approve))
        .route("/oauth/token", post(token).options(preflight))
        .route("/oauth/revoke", post(revoke).options(preflight))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn(response_policy))
}
async fn preflight() -> StatusCode {
    StatusCode::NO_CONTENT
}
async fn as_metadata(State(s): State<AppState>) -> Response {
    let origin = s.origin.trim_end_matches('/');
    Json(json!({"issuer":origin,"authorization_endpoint":format!("{origin}/oauth/authorize"),"token_endpoint":format!("{origin}/oauth/token"),"registration_endpoint":format!("{origin}/oauth/register"),"revocation_endpoint":format!("{origin}/oauth/revoke"),"response_types_supported":["code"],"grant_types_supported":["authorization_code","refresh_token"],"token_endpoint_auth_methods_supported":["none"],"revocation_endpoint_auth_methods_supported":["none"],"code_challenge_methods_supported":["S256"],"scopes_supported":["tasks:read","tasks:write"],"client_id_metadata_document_supported":false})).into_response()
}
async fn resource_metadata(State(s): State<AppState>) -> Response {
    Json(json!({"resource":canonical(&s),"authorization_servers":[s.origin.trim_end_matches('/')],"scopes_supported":["tasks:read","tasks:write"],"bearer_methods_supported":["header"]})).into_response()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    client_name: String,
    redirect_uris: Vec<String>,
    token_endpoint_auth_method: Option<String>,
    grant_types: Option<Vec<String>>,
    response_types: Option<Vec<String>>,
}
async fn register(
    State(s): State<AppState>,
    input: Result<Json<Registration>, axum::extract::rejection::JsonRejection>,
) -> OAuthResult {
    let Json(input) = input.map_err(|_| bad("Expected valid client registration JSON"))?;
    if input
        .token_endpoint_auth_method
        .as_deref()
        .is_some_and(|v| v != "none")
        || input.grant_types.as_ref().is_some_and(|v| {
            v.is_empty()
                || v.iter()
                    .any(|g| g != "authorization_code" && g != "refresh_token")
        })
        || input
            .response_types
            .as_ref()
            .is_some_and(|v| v != &["code"])
    {
        return Err(bad("Only public authorization-code clients are supported"));
    }
    let production = s.production.as_ref().ok_or_else(unavailable)?;
    let client = production
        .store
        .lock()
        .map_err(|_| unavailable())?
        .register_client(&input.client_name, input.redirect_uris)
        .map_err(storage)?;
    Ok((StatusCode::CREATED,Json(json!({"client_id":client.client_id,"client_name":client.client_name,"redirect_uris":client.redirect_uris,"token_endpoint_auth_method":"none","grant_types":["authorization_code","refresh_token"],"response_types":["code"]}))).into_response())
}
async fn authorize(
    State(s): State<AppState>,
    h: HeaderMap,
    input: Result<Query<HashMap<String, String>>, axum::extract::rejection::QueryRejection>,
) -> OAuthResult {
    let Query(q) = input.map_err(|_| bad("Invalid authorization query"))?;
    let production = s.production.as_ref().ok_or_else(unavailable)?;
    let pending = {
        let auth = production.store.lock().map_err(|_| unavailable())?;
        if let Some(id) = q.get("request_id") {
            auth.authorization_request(id).map_err(storage)?
        } else {
            if q.get("response_type").map(String::as_str) != Some("code") {
                return Err(OAuthError(
                    StatusCode::BAD_REQUEST,
                    "unsupported_response_type",
                    "Only response_type=code is supported".into(),
                ));
            }
            let field = |name: &str| {
                q.get(name)
                    .cloned()
                    .ok_or_else(|| bad(&format!("Missing {name}")))
            };
            auth.begin_authorization(
                AuthorizationRequest {
                    client_id: field("client_id")?,
                    redirect_uri: field("redirect_uri")?,
                    resource: field("resource")?,
                    scope: field("scope")?,
                    state: field("state")?,
                    code_challenge: field("code_challenge")?,
                    code_challenge_method: field("code_challenge_method")?,
                },
                &canonical(&s),
            )
            .map_err(storage)?
        }
    };
    let actor = match browser(&s, &h, false) {
        Ok(actor) => actor,
        Err(_) => {
            let resume = format!("/oauth/authorize?request_id={}", pending.id);
            return Ok(Html(format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Sign in to authorize</title><h1>Sign in to review access</h1><p><a href=\"/signin\" target=\"_blank\" rel=\"noopener\">Sign in in another tab</a>, then return here.</p><a href=\"{}\">Continue to access review</a></html>",escaped(&resume))).into_response());
        }
    };
    let actor_for_lists = actor.clone();
    let lists = s
        .run(move |store| store.lists(&actor_for_lists))
        .await
        .map_err(|_| bad("Could not load accessible lists"))?;
    let mut options = String::new();
    for list in lists {
        let a = actor.clone();
        let id = list.id.clone();
        let permission = s
            .run(move |store| store.list_permission(&a, &id))
            .await
            .map_err(|_| bad("Could not verify list access"))?;
        if permission >= 1 {
            options.push_str(&format!(
                "<label><input type=\"checkbox\" name=\"list_{}\" value=\"yes\"> {} {}</label><br>",
                escaped(&list.id),
                escaped(&list.title),
                if permission < 2 { "(read only)" } else { "" }
            ));
        }
    }
    let mut permissions = String::new();
    for scope in pending.request.scope.split_whitespace() {
        let (name, label) = match scope {
            "tasks:read" => ("read", "Read tasks"),
            "tasks:write" => ("write", "Create and change tasks"),
            _ => continue,
        };
        permissions.push_str(&format!(
            "<label><input type=\"checkbox\" name=\"{name}\" value=\"yes\">{label}</label><br>"
        ));
    }
    Ok(Html(format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Review access · Todoer</title><link rel=\"stylesheet\" href=\"/assets/app.css\"><main><h1>Allow {} to access Todoer?</h1><p>Client ID: <code>{}</code></p><p>Resource: <code>{}</code></p><p>Redirect: <code>{}</code></p><p>This client can keep access until you revoke it. Choose permissions and lists explicitly. Write includes reading. Access always follows your current membership.</p><form method=\"post\" action=\"/oauth/approve\"><input type=\"hidden\" name=\"request_id\" value=\"{}\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><fieldset><legend>Permissions</legend>{}</fieldset><fieldset><legend>Lists</legend>{}<label><input type=\"checkbox\" name=\"allow_all\" value=\"yes\">All current and future accessible lists</label></fieldset><button name=\"action\" value=\"allow\">Allow selected access</button> <button name=\"action\" value=\"deny\">Deny</button></form><p>Revoke client access in Settings (/settings).</p></main></html>",escaped(&pending.client_name),escaped(&pending.request.client_id),escaped(&pending.request.resource),escaped(&pending.request.redirect_uri),escaped(&pending.id),escaped(&s.csrf_for(&h)),permissions,options)).into_response())
}
async fn approve(
    State(s): State<AppState>,
    mut h: HeaderMap,
    input: Result<Form<HashMap<String, String>>, axum::extract::rejection::FormRejection>,
) -> OAuthResult {
    let Form(form) = input.map_err(|_| bad("Invalid approval form"))?;
    if !s.origin_ok(&h) {
        return Err(OAuthError(
            StatusCode::FORBIDDEN,
            "access_denied",
            "Origin rejected".into(),
        ));
    }
    let csrf = form.get("csrf").ok_or_else(|| bad("Missing CSRF token"))?;
    h.insert(
        "x-csrf-token",
        csrf.parse().map_err(|_| bad("Invalid CSRF token"))?,
    );
    let actor = browser(&s, &h, true)?;
    let id = form
        .get("request_id")
        .ok_or_else(|| bad("Missing request ID"))?;
    let production = s.production.as_ref().ok_or_else(unavailable)?;
    let pending = production
        .store
        .lock()
        .map_err(|_| unavailable())?
        .authorization_request(id)
        .map_err(storage)?;
    if form.get("action").map(String::as_str) == Some("deny") {
        production
            .store
            .lock()
            .map_err(|_| unavailable())?
            .deny_authorization(id)
            .map_err(storage)?;
        let mut url = reqwest::Url::parse(&pending.request.redirect_uri)
            .map_err(|_| bad("Invalid stored redirect"))?;
        url.query_pairs_mut()
            .append_pair("error", "access_denied")
            .append_pair("state", &pending.request.state);
        return Ok(Redirect::to(url.as_str()).into_response());
    }
    if form.get("action").map(String::as_str) != Some("allow") {
        return Err(bad("Choose allow or deny"));
    }
    let mut permissions = Vec::new();
    if form.get("read").map(String::as_str) == Some("yes") {
        permissions.push(Permission::Read)
    }
    if form.get("write").map(String::as_str) == Some("yes") {
        permissions.push(Permission::Write)
    }
    if permissions.is_empty() {
        return Err(bad("Select at least one permission"));
    }
    let selected: Vec<String> = form
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix("list_")
                .filter(|_| value.as_str() == "yes")
                .map(str::to_owned)
        })
        .collect();
    let all = form.get("allow_all").map(String::as_str) == Some("yes");
    if all && !selected.is_empty() {
        return Err(bad("Choose individual lists or all lists"));
    }
    if !all && selected.is_empty() {
        return Err(bad("Select at least one list"));
    }
    let required = if permissions.contains(&Permission::Write) {
        2
    } else {
        1
    };
    let a = actor.clone();
    let chosen = selected.clone();
    s.run(move |store| {
        let targets = if all {
            store.lists(&a)?.into_iter().map(|l| l.id).collect()
        } else {
            chosen
        };
        for list in targets {
            if store.list_permission(&a, &list)? < required {
                return Err(Error::Forbidden);
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| {
        OAuthError(
            StatusCode::FORBIDDEN,
            "access_denied",
            "Selected permissions exceed current list access".into(),
        )
    })?;
    let code = production
        .store
        .lock()
        .map_err(|_| unavailable())?
        .approve_authorization(
            id,
            &actor,
            permissions,
            if all { None } else { Some(selected) },
            None,
        )
        .map_err(storage)?;
    let mut redirect =
        reqwest::Url::parse(&code.redirect_uri).map_err(|_| bad("Invalid stored redirect"))?;
    redirect
        .query_pairs_mut()
        .append_pair("code", &code.code)
        .append_pair("state", &code.state);
    Ok(Redirect::to(redirect.as_str()).into_response())
}
#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    client_id: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    resource: String,
    refresh_token: Option<String>,
    scope: Option<String>,
}
async fn token(
    State(s): State<AppState>,
    input: Result<Form<TokenForm>, axum::extract::rejection::FormRejection>,
) -> OAuthResult {
    let Form(input) = input.map_err(|_| bad("Invalid token form"))?;
    let production = s.production.as_ref().ok_or_else(unavailable)?;
    let mut auth = production.store.lock().map_err(|_| unavailable())?;
    if input.scope.is_some() {
        return Err(bad("Token endpoint does not accept scope changes"));
    }
    let tokens = match input.grant_type.as_str() {
        "authorization_code" => auth.exchange_code(
            &input.client_id,
            input
                .redirect_uri
                .as_deref()
                .ok_or_else(|| bad("Missing redirect_uri"))?,
            input.code.as_deref().ok_or_else(|| bad("Missing code"))?,
            input
                .code_verifier
                .as_deref()
                .ok_or_else(|| bad("Missing code_verifier"))?,
            &input.resource,
            &canonical(&s),
        ),
        "refresh_token" => auth.refresh_oauth(
            &input.client_id,
            input
                .refresh_token
                .as_deref()
                .ok_or_else(|| bad("Missing refresh_token"))?,
            &input.resource,
            &canonical(&s),
        ),
        _ => {
            return Err(OAuthError(
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                "Unsupported grant type".into(),
            ));
        }
    }
    .map_err(storage)?;
    Ok(Json(tokens).into_response())
}
#[derive(Deserialize)]
struct RevokeForm {
    client_id: String,
    token: String,
}
async fn revoke(
    State(s): State<AppState>,
    input: Result<Form<RevokeForm>, axum::extract::rejection::FormRejection>,
) -> OAuthResult {
    let Form(input) = input.map_err(|_| bad("Invalid revocation form"))?;
    let production = s.production.as_ref().ok_or_else(unavailable)?;
    production
        .store
        .lock()
        .map_err(|_| unavailable())?
        .revoke_oauth(&input.client_id, &input.token)
        .map_err(storage)?;
    Ok(StatusCode::OK.into_response())
}
#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    #[test]
    fn escapes_untrusted_metadata() {
        assert_eq!(
            escaped("<script a='x'>&\""),
            "&lt;script a=&#39;x&#39;&gt;&amp;&quot;"
        );
    }
    #[tokio::test]
    async fn discovery_and_closed_development_endpoints() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            "test".into(),
            "http://127.0.0.1:8000".into(),
        );
        let app = routes().with_state(state);
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/.well-known/oauth-protected-resource/mcp")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()["access-control-allow-origin"], "*");
        let response=app.oneshot(axum::http::Request::builder().uri("/oauth/register").method("POST").header("content-type","application/json").body(axum::body::Body::from(r#"{"client_name":"test","redirect_uris":["https://client.example/callback"]}"#)).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
