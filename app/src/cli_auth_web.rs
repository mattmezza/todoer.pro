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
   spaces.push_str(&format!("<label class=\"cli-choice\"><input type=\"checkbox\" name=\"space_{}\" value=\"yes\"><span>@{}</span></label>",esc(&space.id),esc(&space.title)));
  }
  for list in db.lists(&actor)? {
   let info=db.list_info(&actor,&list.id)?;
   let space=match info.workspace_id {Some(id) if db.workspace_permission(&actor,&id).unwrap_or(0)>=1=>db.workspace(&actor,&id)?.title,_=>"Shared with you".into()};
   lists.push_str(&format!("<label class=\"cli-choice\"><input type=\"checkbox\" name=\"list_{}\" value=\"yes\"><span>#{}</span><small>{}</small></label>",esc(&list.id),esc(&list.title),esc(&space)));
  } Ok((spaces,lists))
 }).await?;
    let mut permissions = String::new();
    for (value, label, description) in [
        ("read", "Read", "View your selected spaces and lists"),
        ("write", "Read and write", "View and make changes"),
        (
            "admin",
            "Admin",
            "Manage selected resources; requires ownership",
        ),
    ] {
        if rank(value)? <= request.requested_rank {
            permissions.push_str(&format!("<label class=\"cli-permission\"><input type=\"radio\" name=\"permission\" value=\"{value}\" {}><span><strong>{label}</strong><small>{description}</small></span></label>", if value == "read" { "checked" } else { "" }));
        }
    }
    let permission_hint = if request.requested_rank == 1 {
        r#"<p class="cli-note">This CLI requested read-only access. To enable changes, run <code>todoer login --permission write</code>.</p>"#
    } else {
        ""
    };
    let body = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="dark light"><title>Connect Todoer CLI</title><link rel="stylesheet" href="/assets/app.css"><script type="module" src="/assets/cli-consent.js"></script></head><body class="cli-approval-page"><main class="cli-approval"><div class="cli-brand">todoer<span> pro</span></div><div class="cli-app-icon" aria-hidden="true">&gt;_</div><h1>Connect Todoer CLI</h1><p class="cli-intro">Allow the CLI on your device to access your Todoer account.</p><form method="post" action="/cli/authorize" class="cli-consent"><input type="hidden" name="id" value="{}"><input type="hidden" name="csrf" value="{}"><fieldset><legend>Permissions</legend><div class="cli-permissions">{}</div>{}</fieldset><fieldset><legend>Access to</legend><label class="cli-choice cli-all"><input type="checkbox" name="all_resources" value="yes" data-cli-all><span>All current and future spaces and lists</span></label><details class="cli-resources" open><summary>Or select spaces and lists</summary><div class="cli-resource-groups"><div><h2>Spaces</h2><p>Includes current and future lists.</p>{}</div><div><h2>Individual lists</h2>{}</div></div></details></fieldset><label class="cli-expiry">Access expires<select name="expiration"><option value="30">After 30 days</option><option value="never">Never expires</option></select></label><p class="cli-note">Access is limited to your own permissions. Never-expiring access requires an up-to-date CLI. You can revoke access anytime in Settings → Connected tools.</p><div class="cli-actions"><button name="action" value="deny" formnovalidate>Cancel</button><button class="primary" name="action" value="allow">Authorize CLI</button></div></form></main></body></html>"#,
        esc(&request.id),
        esc(&s.csrf_for(&h)),
        permissions,
        permission_hint,
        choices.0,
        choices.1
    );
    Ok(private(Html(body).into_response()))
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
                    match form.get("expiration").map(String::as_str) {
                        Some("never") => true,
                        Some("30") | None => false,
                        _ => return Err(Error::Invalid("Invalid expiration".into())),
                    },
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
