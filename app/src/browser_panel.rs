use super::*;

const COOKIE: &str = "__Host-todoer_panel";

fn extension_origin(origin: &str) -> bool {
    if let Some(id) = origin.strip_prefix("chrome-extension://") {
        return id.len() == 32 && id.bytes().all(|ch| (b'a'..=b'p').contains(&ch));
    }
    origin.strip_prefix("moz-extension://").is_some_and(|id| {
        uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.hyphenated().to_string() == id)
    })
}

pub(super) async fn connect(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let origin = h
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .filter(|origin| extension_origin(origin));
    // A credentialed browser extension request requires host access. This
    // custom header prevents a regular website from sending a form POST here.
    let Some(origin) = origin.filter(|_| {
        h.get("x-todoer-browser-panel").is_some_and(|v| v == "1")
            && !h.contains_key(header::AUTHORIZATION)
    }) else {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "browser_panel",
            "Open the Todoer browser extension to connect the panel.".into(),
        ));
    };
    s.auth(&h, false)?;
    Ok((
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            format!("{COOKIE}={origin}; Secure; HttpOnly; SameSite=Lax; Path=/"),
        )],
    )
        .into_response())
}

pub(super) fn frame_ancestor<'a>(path: &str, h: &'a HeaderMap) -> Option<&'a str> {
    if h.get("sec-fetch-dest").is_none_or(|v| v != "iframe")
        || matches!(
            path,
            "/signup" | "/forgot-password" | "/reset-password" | "/verify-email"
        )
        || ["/auth/", "/cli/", "/oauth/"]
            .iter()
            .any(|prefix| path.starts_with(prefix))
    {
        return None;
    }
    browser_auth::cookie(h, COOKIE).filter(|origin| extension_origin(origin))
}

pub(super) fn signin_link(next: Option<&str>) -> Html<String> {
    let next = encode_query(&browser_auth::safe_next(next));
    Html(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light dark"><title>Sign in to Todoer</title><link rel="stylesheet" href="/assets/app.css"></head><body class="p-5"><main><h1>Sign in to Todoer</h1><p>Sign in in a browser tab, then reopen the panel.</p><p><a class="primary" href="/signin?next={next}" target="_blank" rel="noopener">Open sign-in</a></p></main></body></html>"#
    ))
}
