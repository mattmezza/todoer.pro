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
        AuthError::Forbidden | AuthError::ReauthenticationRequired => OAuthError(
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
    let public = request.uri().path().starts_with("/.well-known/")
        || matches!(
            request.uri().path(),
            "/oauth/register" | "/oauth/token" | "/oauth/revoke"
        );
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
        .route("/settings/ai", get(settings))
        .route(
            "/settings/agents",
            get(|| async { Redirect::to("/settings/ai") }),
        )
        .route("/settings/credentials", post(issue_form))
        .route("/settings/credentials/{id}/revoke", post(revoke_form))
        .route("/api/v1/credentials", get(credentials).post(issue_json))
        .route("/api/v1/credentials/{id}/revoke", post(revoke_json))
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
// RFC 7591 metadata extensions must be tolerated; only supported security fields are applied.
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialInput {
    audience: String,
    permissions: Vec<Permission>,
    #[serde(default)]
    list_ids: Vec<String>,
    #[serde(default)]
    all_lists: bool,
    ttl_secs: Option<i64>,
}
fn secure_browser(s: &AppState, h: &HeaderMap) -> Result<String, OAuthError> {
    if !s.origin_ok(h) {
        return Err(OAuthError(
            StatusCode::FORBIDDEN,
            "access_denied",
            "Origin rejected".into(),
        ));
    }
    browser(s, h, true)
}
fn form_headers(mut h: HeaderMap, form: &HashMap<String, String>) -> Result<HeaderMap, OAuthError> {
    h.insert(
        "x-csrf-token",
        form.get("csrf")
            .ok_or_else(|| bad("Missing CSRF token"))?
            .parse()
            .map_err(|_| bad("Invalid CSRF token"))?,
    );
    Ok(h)
}
async fn mint_credential(
    s: &AppState,
    h: &HeaderMap,
    input: CredentialInput,
) -> Result<crate::auth::AgentGrant, OAuthError> {
    let actor = secure_browser(s, h)?;
    if input.permissions.is_empty()
        || input.permissions.len() > 3
        || input.list_ids.len() > 100
        || input.all_lists == !input.list_ids.is_empty()
    {
        return Err(bad(
            "Select permissions and either individual lists or explicit all-list access",
        ));
    }
    let ttl = input.ttl_secs.unwrap_or(2_592_000);
    if !(1..=7_776_000).contains(&ttl) {
        return Err(bad(
            "Credential lifetime must be between 1 second and 90 days",
        ));
    }
    let audience = match input.audience.as_str() {
        "rest" => format!("{}/api/v1", s.origin.trim_end_matches('/')),
        "mcp" => canonical(s),
        _ => return Err(bad("Audience must be rest or mcp")),
    };
    let required = if input.permissions.contains(&Permission::Admin) {
        3
    } else if input.permissions.contains(&Permission::Write) {
        2
    } else {
        1
    };
    let a = actor.clone();
    let chosen = input.list_ids.clone();
    let all = input.all_lists;
    s.run(move |store| {
        let lists = if all {
            store.lists(&a)?.into_iter().map(|l| l.id).collect()
        } else {
            chosen
        };
        if required == 3 && lists.is_empty() {
            return Err(Error::Forbidden);
        }
        for id in lists {
            if store.list_permission(&a, &id)? < required {
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
            "Permissions exceed current access to selected lists".into(),
        )
    })?;
    let production = s.production.as_ref().ok_or_else(unavailable)?;
    production
        .store
        .lock()
        .map_err(|_| unavailable())?
        .issue_agent(
            &actor,
            &audience,
            input.permissions,
            if all { None } else { Some(input.list_ids) },
            None,
            ttl,
        )
        .map_err(storage)
}
async fn credentials(State(s): State<AppState>, h: HeaderMap) -> OAuthResult {
    let actor = browser(&s, &h, false)?;
    let production = s.production.as_ref().ok_or_else(unavailable)?;
    let rows = production
        .store
        .lock()
        .map_err(|_| unavailable())?
        .list_agents(&actor)
        .map_err(storage)?;
    Ok(Json(rows).into_response())
}
async fn issue_json(
    State(s): State<AppState>,
    h: HeaderMap,
    input: Result<Json<CredentialInput>, axum::extract::rejection::JsonRejection>,
) -> OAuthResult {
    let Json(input) = input.map_err(|_| bad("Invalid credential JSON"))?;
    Ok((
        StatusCode::CREATED,
        Json(mint_credential(&s, &h, input).await?),
    )
        .into_response())
}
async fn revoke_json(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> OAuthResult {
    let actor = secure_browser(&s, &h)?;
    let production = s.production.as_ref().ok_or_else(unavailable)?;
    production
        .store
        .lock()
        .map_err(|_| unavailable())?
        .revoke_agent(&actor, &id)
        .map_err(storage)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
async fn revoke_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    input: Result<Form<HashMap<String, String>>, axum::extract::rejection::FormRejection>,
) -> OAuthResult {
    let Form(form) = input.map_err(|_| bad("Invalid revocation form"))?;
    let h = form_headers(h, &form)?;
    let actor = secure_browser(&s, &h)?;
    let production = s.production.as_ref().ok_or_else(unavailable)?;
    production
        .store
        .lock()
        .map_err(|_| unavailable())?
        .revoke_agent(&actor, &id)
        .map_err(storage)?;
    Ok(Redirect::to("/settings").into_response())
}
async fn issue_form(
    State(s): State<AppState>,
    h: HeaderMap,
    input: Result<Form<HashMap<String, String>>, axum::extract::rejection::FormRejection>,
) -> OAuthResult {
    let Form(form) = input.map_err(|_| bad("Invalid credential form"))?;
    let h = form_headers(h, &form)?;
    let mut permissions = Vec::new();
    for (name, permission) in [
        ("read", Permission::Read),
        ("write", Permission::Write),
        ("admin", Permission::Admin),
    ] {
        if form.get(name).map(String::as_str) == Some("yes") {
            permissions.push(permission)
        }
    }
    let list_ids = form
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix("list_")
                .filter(|_| value.as_str() == "yes")
                .map(str::to_owned)
        })
        .collect();
    let ttl_secs = form
        .get("ttl_days")
        .and_then(|v| v.parse::<i64>().ok().and_then(|v| v.checked_mul(86_400)))
        .ok_or_else(|| bad("Choose a valid number of days"))?;
    let grant = mint_credential(
        &s,
        &h,
        CredentialInput {
            audience: form.get("audience").cloned().unwrap_or_default(),
            permissions,
            list_ids,
            all_lists: form.get("allow_all").map(String::as_str) == Some("yes"),
            ttl_secs: Some(ttl_secs),
        },
    )
    .await?;
    Ok(Html(format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Save your credential</title><link rel=\"stylesheet\" href=\"/assets/app.css\"><main><h1>Copy your credential now</h1><p>This is its only display. Store it securely. Anyone holding it can use the approved access until it expires or is revoked.</p><label>Credential<textarea readonly rows=\"3\" cols=\"72\">{}</textarea></label><p>ID: <code>{}</code></p><a href=\"/settings\">Return to settings</a></main></html>",escaped(&grant.token),escaped(&grant.id))).into_response())
}
async fn settings(State(s): State<AppState>, h: HeaderMap) -> OAuthResult {
    let actor = browser(&s, &h, false)?;
    let Some(production) = s.production.as_ref() else {
        return super::scheduling_http::render_settings(&s, &h, "AI", r#"<section class="settings-preference-card"><h2>Connected tools</h2><p class="muted">Connect an AI agent or your own scripts to Todoer.</p><p class="muted">Credential management is available when OAuth is configured for this installation.</p></section>"#.into(), "agents").await.map_err(|_| bad("Could not load settings"));
    };
    let rows = production
        .store
        .lock()
        .map_err(|_| unavailable())?
        .list_agents(&actor)
        .map_err(storage)?;
    let a = actor.clone();
    let lists = s
        .run(move |store| store.lists(&a))
        .await
        .map_err(|_| bad("Could not load lists"))?;
    let mut options = String::new();
    for list in lists {
        options.push_str(&format!(
            "<label class=\"settings-check\"><input type=\"checkbox\" name=\"list_{}\" value=\"yes\">{}</label>",
            escaped(&list.id),
            escaped(&list.title)
        ));
    }
    let csrf = escaped(&s.csrf_for(&h));
    let mut existing = String::new();
    for row in rows {
        let scopes = if let Some(spaces) = row.workspace_ids {
            let mut selected = spaces
                .into_iter()
                .map(|id| format!("Space {id}"))
                .collect::<Vec<_>>();
            selected.extend(
                row.list_ids
                    .unwrap_or_default()
                    .into_iter()
                    .map(|id| format!("List {id}")),
            );
            selected.join(", ")
        } else if let Some(lists) = row.list_ids {
            lists.join(", ")
        } else {
            "All current and future accessible lists".into()
        };
        let permissions = row
            .permissions
            .iter()
            .map(|p| format!("{p:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        existing.push_str(&format!(
            "<li><code>{}</code> — {} — {} — {} — expires <time data-unix-time=\"{}\"></time> {}",
            escaped(&row.id),
            escaped(&row.audience),
            escaped(&permissions),
            escaped(&scopes),
            row.expires_at,
            if row.revoked { "(revoked)" } else { "" }
        ));
        if !row.revoked {
            existing.push_str(&format!("<form method=\"post\" action=\"/settings/credentials/{}/revoke\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button>Revoke access</button></form>",escaped(&row.id),csrf));
        }
        existing.push_str("</li>");
    }
    if existing.is_empty() {
        existing.push_str("<li class=\"muted\">No connected tools yet.</li>");
    }
    let html = format!(
        r#"<section class="settings-preference-card" aria-labelledby="connected-tools-heading"><h2 id="connected-tools-heading">Connected tools</h2><p class="muted">Give an AI agent or script access to the lists you choose.</p><details class="credential-create"><summary>Create a credential…</summary><form method="post" action="/settings/credentials"><input type="hidden" name="csrf" value="{}"><label>Connection type<select name="audience"><option value="mcp">AI agent (MCP)</option><option value="rest">Script (REST API)</option></select></label><fieldset><legend>Permissions</legend><label class="settings-check"><input type="checkbox" name="read" value="yes" checked>Read</label><label class="settings-check"><input type="checkbox" name="write" value="yes">Write (includes read)</label><label class="settings-check"><input type="checkbox" name="admin" value="yes">Admin (requires ownership)</label></fieldset><fieldset><legend>List access</legend><label class="settings-check"><input type="checkbox" name="allow_all" value="yes" data-credential-all>All current and future accessible lists</label><div data-credential-lists>{}</div></fieldset><label>Expires after (days)<input name="ttl_days" type="number" min="1" max="90" value="30" required></label><button class="primary">Create credential</button></form></details><details class="credential-history"><summary>Manage connected tools</summary><ul>{}</ul></details></section>"#,
        csrf, options, existing
    );
    super::scheduling_http::render_settings(&s, &h, "AI", html, "agents")
        .await
        .map_err(|_| bad("Could not load settings"))
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
    fn production_state() -> (AppState, String, String, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("todoer-oauth-http-{}.db", uuid::Uuid::new_v4()));
        let mut db = Store::open(path.to_str().unwrap()).unwrap();
        db.seed_dev().unwrap();
        let auth = crate::auth::AuthStore::open(path.to_str().unwrap()).unwrap();
        let grant = auth.issue_session("dev", 3600).unwrap();
        (
            AppState::production(
                Arc::new(Mutex::new(db)),
                auth,
                crate::oidc::GoogleOidc::test_fixture(),
                "https://todoer.example".into(),
            ),
            grant.token,
            grant.csrf,
            path,
        )
    }
    #[tokio::test]
    async fn credentials_require_cookie_csrf_and_current_permission_then_revoke() {
        use http_body_util::BodyExt;
        let (state, token, csrf, path) = production_state();
        let app = routes().with_state(state.clone());
        let payload =
            r#"{"audience":"mcp","permissions":["read"],"list_ids":["inbox"],"all_lists":false}"#;
        let make = |csrf_value: &str| {
            axum::http::Request::builder()
                .uri("/api/v1/credentials")
                .method("POST")
                .header("content-type", "application/json")
                .header(
                    "cookie",
                    format!("todoer_session={token}; todoer_csrf={csrf}"),
                )
                .header("x-csrf-token", csrf_value)
                .header("origin", "https://todoer.example")
                .body(axum::body::Body::from(payload))
                .unwrap()
        };
        assert!(
            app.clone()
                .oneshot(make("wrong"))
                .await
                .unwrap()
                .status()
                .is_client_error()
        );
        let mut bearer = make(&csrf);
        bearer
            .headers_mut()
            .insert("authorization", "Bearer anything".parse().unwrap());
        assert_eq!(
            app.clone().oneshot(bearer).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        let mut admin = make(&csrf);
        *admin.body_mut() = axum::body::Body::from(
            r#"{"audience":"mcp","permissions":["admin"],"list_ids":["inbox"]}"#,
        );
        assert_eq!(
            app.clone().oneshot(admin).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        let response = app.clone().oneshot(make(&csrf)).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        assert!(
            !response
                .headers()
                .contains_key("access-control-allow-origin")
        );
        let value: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        let secret = value["token"].as_str().unwrap();
        let id = value["id"].as_str().unwrap();
        let principal = state
            .production
            .as_ref()
            .unwrap()
            .store
            .lock()
            .unwrap()
            .authenticate_agent(
                secret,
                "https://todoer.example/mcp",
                Permission::Read,
                None,
                None,
            )
            .unwrap();
        assert_eq!(principal.list_ids, Some(vec!["inbox".into()]));
        let metadata = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/credentials")
                    .header("cookie", format!("todoer_session={token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let metadata = String::from_utf8(
            metadata
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(metadata.contains(id));
        assert!(!metadata.contains(secret));
        assert!(!metadata.contains("token_hash"));
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/v1/credentials/{id}/revoke"))
                    .method("POST")
                    .header("cookie", format!("todoer_session={token}"))
                    .header("x-csrf-token", csrf)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            state
                .production
                .as_ref()
                .unwrap()
                .store
                .lock()
                .unwrap()
                .authenticate_agent(
                    secret,
                    "https://todoer.example/mcp",
                    Permission::Read,
                    None,
                    None
                )
                .is_err()
        );
        std::fs::remove_file(path).unwrap();
    }
    fn encoded(fields: &[(&str, &str)]) -> String {
        let mut url = reqwest::Url::parse("https://example.test/").unwrap();
        for (k, v) in fields {
            url.query_pairs_mut().append_pair(k, v);
        }
        url.query().unwrap_or_default().into()
    }
    #[tokio::test]
    async fn oauth_http_consent_and_pkce_exchange_are_explicit() {
        use http_body_util::BodyExt;
        let (state, session, csrf, path) = production_state();
        let app = routes().with_state(state.clone());
        let client = state
            .production
            .as_ref()
            .unwrap()
            .store
            .lock()
            .unwrap()
            .register_client("<Client>", vec!["http://127.0.0.1:4321/callback".into()])
            .unwrap();
        let request = crate::auth::AuthorizationRequest {
            client_id: client.client_id.clone(),
            redirect_uri: client.redirect_uris[0].clone(),
            resource: "https://todoer.example/mcp".into(),
            scope: "tasks:read".into(),
            state: "a&b".into(),
            code_challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".into(),
            code_challenge_method: "S256".into(),
        };
        let pending = state
            .production
            .as_ref()
            .unwrap()
            .store
            .lock()
            .unwrap()
            .begin_authorization(request, "https://todoer.example/mcp")
            .unwrap();
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/oauth/authorize?request_id={}", pending.id))
                    .header(
                        "cookie",
                        format!("todoer_session={session}; todoer_csrf={csrf}"),
                    )
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let text = String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(text.contains("&lt;Client&gt;"));
        assert!(!text.contains("value=\"yes\" checked"));
        let body = encoded(&[
            ("request_id", &pending.id),
            ("csrf", &csrf),
            ("action", "allow"),
            ("read", "yes"),
            ("list_inbox", "yes"),
        ]);
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/oauth/approve")
                    .method("POST")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("cookie", format!("todoer_session={session}"))
                    .header("origin", "https://todoer.example")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let redirect =
            reqwest::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
        let values: HashMap<String, String> = redirect.query_pairs().into_owned().collect();
        assert_eq!(values["state"], "a&b");
        let body = encoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", &client.client_id),
            ("redirect_uri", &client.redirect_uris[0]),
            ("code", &values["code"]),
            (
                "code_verifier",
                "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
            ),
            ("resource", "https://todoer.example/mcp"),
        ]);
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/oauth/token")
                    .method("POST")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["access-control-allow-origin"], "*");
        let value: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(value["scope"], "tasks:read");
        assert!(value["access_token"].as_str().is_some());
        std::fs::remove_file(path).unwrap();
    }
}
