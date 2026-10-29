use super::*;
use crate::cli_auth::{LoginStart, rank};
use std::collections::HashMap;
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/cli/login/start", post(start))
        .route("/cli/authorize", get(consent).post(approve))
        .route("/api/cli/login/token", post(exchange))
        .route("/api/cli/login/revoke", post(revoke))
        .layer(DefaultBodyLimit::max(32 * 1024))
}
fn private(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("referrer-policy", "same-origin".parse().unwrap());
    response
        .headers_mut()
        .insert("x-frame-options", "DENY".parse().unwrap());
    response
}
fn esc(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
async fn start(
    State(s): State<AppState>,
    Input(input): Input<LoginStart>,
) -> Result<Response, ApiError> {
    let origin = s.origin.clone();
    let request = s.run(move |db| db.begin_cli_login(input)).await?;
    Ok(private(Json(json!({"request_id":request.id,"authorization_url":format!("{}/cli/authorize?id={}",origin.trim_end_matches('/'),request.id)})).into_response()))
}
#[derive(Deserialize)]
struct RequestId {
    id: String,
}
async fn consent(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<RequestId>,
) -> Result<Response, ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    let id = q.id.clone();
    let request = s.run(move |db| db.cli_login_request(&id)).await?;
    let actor = match s.auth(&h, false) {
        Ok(actor) => actor,
        Err(_) => {
            let next = format!("/cli/authorize?id={}", request.id);
            return Ok(private(Html(format!("<!doctype html><html><head><meta charset=\"utf-8\"><link rel=\"stylesheet\" href=\"/assets/app.css\"><title>Sign in for CLI access</title></head><body><main><h1>Connect Todoer CLI</h1><p>Sign in, then return here to approve CLI access.</p><a href=\"/signin?next={}\" target=\"_blank\" rel=\"noopener\">Sign in</a><p><a href=\"{}\">Continue to access review</a></p></main></body></html>",esc(&urlencoding(&next)),esc(&next))).into_response()));
        }
    };
    let choices=s.run(move|db|{
  let mut spaces=String::new();let mut lists=String::new();
  for space in db.workspaces(&actor)? {
   spaces.push_str(&format!("<label><input type=\"checkbox\" name=\"space_{}\" value=\"yes\"> @{}</label>",esc(&space.id),esc(&space.title)));
  }
  for list in db.lists(&actor)? {
   let info=db.list_info(&actor,&list.id)?;
   let space=match info.workspace_id {Some(id) if db.workspace_permission(&actor,&id).unwrap_or(0)>=1=>db.workspace(&actor,&id)?.title,_=>"Shared with you".into()};
   lists.push_str(&format!("<label><input type=\"checkbox\" name=\"list_{}\" value=\"yes\"> #{} <small>{}</small></label>",esc(&list.id),esc(&list.title),esc(&space)));
  } Ok((spaces,lists))
 }).await?;
    let mut permissions = String::new();
    for (value, label) in [
        ("read", "Read"),
        ("write", "Write (includes read)"),
        ("admin", "Admin (includes write)"),
    ] {
        if rank(value)? <= request.requested_rank {
            permissions.push_str(&format!("<label><input type=\"radio\" name=\"permission\" value=\"{value}\" {} required>{label}</label>",if rank(value)?==request.requested_rank{"checked"}else{""}));
        }
    }
    let body = format!(
        "<p>Allow the CLI running on your device to access Todoer? Choose exactly what it can access. Access expires in 30 days and can be revoked in Settings.</p><form method=\"post\" action=\"/cli/authorize\" class=\"cli-consent\"><input type=\"hidden\" name=\"id\" value=\"{}\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><fieldset><legend>Permissions</legend>{}</fieldset><p>Selected spaces include their current and future lists. Selected lists stay limited to those lists. Permissions never exceed your current access.</p><fieldset><legend>Spaces</legend>{}</fieldset><fieldset><legend>Individual lists</legend>{}</fieldset><label><input type=\"checkbox\" name=\"all_resources\" value=\"yes\">All current and future accessible spaces and lists</label><p class=\"muted\">Choose selected resources or all resources. Admin access requires ownership of the selected resources.</p><div class=\"inline-actions\"><button name=\"action\" value=\"deny\">Deny</button><button class=\"primary\" name=\"action\" value=\"allow\">Approve CLI access</button></div></form>",
        esc(&request.id),
        esc(&s.csrf_for(&h)),
        permissions,
        choices.0,
        choices.1
    );
    let response =
        super::scheduling_http::render_settings(&s, &h, "Connect Todoer CLI", body, "agents")
            .await?;
    Ok(private(response))
}
fn urlencoding(value: &str) -> String {
    reqwest::Url::parse_with_params("http://local/", [("next", value)])
        .unwrap()
        .query()
        .unwrap()
        .trim_start_matches("next=")
        .into()
}
async fn approve(
    State(s): State<AppState>,
    mut h: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    if h.contains_key(header::AUTHORIZATION) || !s.origin_ok(&h) {
        return Err(Error::Forbidden.into());
    }
    let csrf = form
        .get("csrf")
        .ok_or_else(|| Error::Invalid("Missing CSRF".into()))?;
    h.insert(
        "x-csrf-token",
        csrf.parse()
            .map_err(|_| Error::Invalid("Invalid CSRF".into()))?,
    );
    let actor = s.auth(&h, true)?;
    let id = form
        .get("id")
        .cloned()
        .ok_or_else(|| Error::Invalid("Missing request".into()))?;
    let action = form.get("action").cloned().unwrap_or_default();
    let redirect = s
        .run(move |db| {
            let request = db.cli_login_request(&id)?;
            let mut redirect = reqwest::Url::parse(&request.redirect_uri)
                .map_err(|_| Error::Invalid("Invalid saved callback".into()))?;
            redirect
                .query_pairs_mut()
                .append_pair("state", &request.state);
            if action == "deny" {
                db.deny_cli_login(&id)?;
                redirect
                    .query_pairs_mut()
                    .append_pair("error", "access_denied");
            } else if action == "allow" {
                let collect = |prefix: &str| {
                    form.iter()
                        .filter_map(|(k, v)| {
                            k.strip_prefix(prefix)
                                .filter(|_| v.as_str() == "yes")
                                .map(str::to_owned)
                        })
                        .collect()
                };
                let code = db.approve_cli_login(
                    &actor,
                    &id,
                    form.get("permission").map(String::as_str).unwrap_or(""),
                    collect("space_"),
                    collect("list_"),
                    form.get("all_resources").is_some_and(|v| v == "yes"),
                )?;
                redirect.query_pairs_mut().append_pair("code", &code);
            } else {
                return Err(Error::Invalid("Choose approve or deny".into()));
            }
            Ok(redirect.to_string())
        })
        .await?;
    Ok(private(Html(format!("<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"0;url={}\"><title>Return to Todoer CLI</title></head><body><p>Returning to your terminal… <a href=\"{}\">Continue</a></p></body></html>", esc(&redirect), esc(&redirect))).into_response()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exchange {
    request_id: String,
    code: String,
    verifier: String,
}
async fn exchange(
    State(s): State<AppState>,
    Input(input): Input<Exchange>,
) -> Result<Response, ApiError> {
    let audience = format!("{}/api/v1", s.origin.trim_end_matches('/'));
    let grant = s
        .run(move |db| {
            db.exchange_cli_login(&input.request_id, &input.code, &input.verifier, &audience)
        })
        .await?;
    Ok(private(Json(grant).into_response()))
}
async fn revoke(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let token = h
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
        .ok_or(Error::Forbidden)?
        .to_owned();
    let audience = format!("{}/api/v1", s.origin.trim_end_matches('/'));
    s.run(move |db| db.revoke_cli_token(&token, &audience))
        .await?;
    Ok(private(StatusCode::NO_CONTENT.into_response()))
}
