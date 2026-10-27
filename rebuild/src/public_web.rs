use super::*;
use crate::public_access::PublicChange;
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/public-access/{kind}/{id}", get(state).put(change))
        .route("/public/{kind}/{id}", get(public_page))
        .route("/og/{kind}/{id}", get(image))
        .route("/og/default.png", get(default_image))
        .route(
            "/favicon.ico",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "image/x-icon")],
                    include_bytes!("../../public/favicon.ico").as_slice(),
                )
            }),
        )
}
async fn state(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Json<crate::public_access::PublicState>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| db.public_state(&actor, &kind, &id)).await?,
    ))
}
async fn change(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((kind, id)): Path<(String, String)>,
    Input(input): Input<PublicChange>,
) -> Result<Json<crate::public_access::PublicState>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.set_public_access(&actor, &kind, &id, input))
            .await?,
    ))
}
#[derive(Deserialize)]
struct PageQuery {
    page: Option<usize>,
}
async fn public_page(
    State(s): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    Query(q): Query<PageQuery>,
) -> Result<Response, ApiError> {
    show(&s, &kind, &id, q.page.unwrap_or(1)).await
}
pub(super) async fn requested(
    s: &AppState,
    h: &HeaderMap,
    kind: &str,
    id: &str,
) -> Result<bool, ApiError> {
    let actor = s.auth(h, false).ok();
    let kind = kind.to_string();
    let id = id.to_string();
    s.run(move |db| {
        let entity = if kind == "space" {
            "workspace"
        } else {
            kind.as_str()
        };
        let id = crate::defaults::resolve(&db.conn, entity, &id)?;
        let readable = if let Some(actor) = actor {
            match kind.as_str() {
                "space" => db.workspace_permission(&actor, &id).is_ok_and(|rank| rank >= 1),
                "list" => crate::core::access_rank(&db.conn, &actor, &id, 1).is_ok(),
                "task" => crate::core::task(&db.conn, &id).is_ok_and(|t| {
                    crate::core::access_rank(&db.conn, &actor, &t.list_id, 1).is_ok()
                }),
                _ => false,
            }
        } else {
            false
        };
        if readable {
            return Ok(false);
        }
        match db.public_document(&kind, &id, 1) {
            Ok(_) => Ok(true),
            Err(Error::NotFound) => Ok(false),
            Err(e) => Err(e),
        }
    })
    .await
    .map_err(Into::into)
}
fn plain(text: &str) -> String {
    use pulldown_cmark::{Event, Parser};
    let mut s = String::new();
    for event in Parser::new(text) {
        match event {
            Event::Text(t) | Event::Code(t) => s.push_str(&t),
            Event::SoftBreak | Event::HardBreak | Event::End(_) => s.push(' '),
            _ => {}
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn esc(s: &str) -> String {
    super::scheduling_http::esc(s)
}
fn metadata(origin: &str, path: &str, title: &str, description: &str, image: &str) -> String {
    let url = format!("{origin}{path}");
    let image = format!("{origin}{image}");
    format!(
        r#"<link rel="icon" href="/assets/favicon.svg" type="image/svg+xml"><link rel="alternate icon" href="/favicon.ico"><meta name="description" content="{}"><meta property="og:type" content="website"><meta property="og:site_name" content="Todoer"><meta property="og:title" content="{}"><meta property="og:description" content="{}"><meta property="og:url" content="{}"><meta property="og:image" content="{}"><meta property="og:image:type" content="image/png"><meta property="og:image:width" content="1200"><meta property="og:image:height" content="630"><meta property="og:image:alt" content="{}"><meta name="twitter:card" content="summary_large_image"><meta name="twitter:title" content="{}"><meta name="twitter:description" content="{}"><meta name="twitter:image" content="{}">"#,
        esc(description),
        esc(title),
        esc(description),
        esc(&url),
        esc(&image),
        esc(title),
        esc(title),
        esc(description),
        esc(&image)
    )
}
pub(super) async fn show(
    s: &AppState,
    kind: &str,
    id: &str,
    page: usize,
) -> Result<Response, ApiError> {
    let kind = kind.to_string();
    let id = id.to_string();
    let doc = s
        .run(move |db| db.public_document(&kind, &id, page))
        .await?;
    let path = match doc.kind.as_str() {
        "space" => format!("/space/{}", doc.reference),
        "list" => format!("/lists/{}", doc.reference),
        _ => format!("/tasks/{}", doc.reference),
    };
    let summary = plain(&doc.description);
    let summary: String = if summary.is_empty() {
        "A shared task list. One thing at a time.".into()
    } else {
        summary.chars().take(240).collect()
    };
    let meta = metadata(
        &s.origin,
        &path,
        &doc.title,
        &summary,
        &format!("/og/{}/{}.png", doc.kind, doc.reference),
    );
    let mut body = String::new();
    for item in &doc.items {
        let url = if item.kind == "list" {
            format!("/lists/{}", item.reference)
        } else {
            format!("/tasks/{}", item.reference)
        };
        body.push_str(&format!("<article class=\"public-item public-depth-{}\"><span class=\"public-check\" aria-label=\"{}\">{}</span><div><a href=\"{}\">{}</a>{}</div></article>",item.depth.min(8),if item.complete{"Completed"}else{"Incomplete"},if item.kind=="list"{"#"}else if item.complete{"✓"}else{"○"},url,esc(&item.title),if item.description.is_empty(){String::new()}else{format!("<div class=\"markdown public-item-description\">{}</div>",crate::markdown::render(&item.description))}));
    }
    if doc.has_more {
        body.push_str(&format!(
            "<a class=\"public-more\" href=\"/public/{}/{}?page={}\">Show more</a>",
            doc.kind,
            doc.reference,
            page.clamp(1, 10000) + 1
        ));
    }
    Ok(Html(format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"robots\" content=\"noindex\"><title>{} · Todoer</title>{meta}<link rel=\"stylesheet\" href=\"/assets/app.css\"></head><body class=\"public-page\"><header class=\"public-header\"><a class=\"brand\" href=\"/\">todoer<span>pro</span></a><span>Shared · Read only</span><a href=\"/signin\">Sign in</a></header><main><h1>{}</h1><div class=\"markdown public-description\">{}</div><section>{body}</section></main></body></html>",esc(&doc.title),esc(&doc.title),crate::markdown::render(&doc.description))).into_response())
}
async fn image(
    State(s): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let id = id.strip_suffix(".png").unwrap_or(&id).to_string();
    let doc = s.run(move |db| db.public_document(&kind, &id, 1)).await?;
    let image = crate::social_image::render(
        &doc.title,
        &plain(&doc.description),
        &doc.kind,
        doc.complete,
    )?;
    Ok(([(header::CONTENT_TYPE, "image/png")], image).into_response())
}
async fn default_image() -> Result<Response, ApiError> {
    Ok((
        [(header::CONTENT_TYPE, "image/png")],
        crate::social_image::render(
            "One thing at a time.",
            "Capture the thought. Make a little room. Enjoy getting it done.",
            "Todoer",
            false,
        )?,
    )
        .into_response())
}
pub(super) async fn metadata_layer(
    State(s): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let path = req.uri().path().to_string();
    let response = next.run(req).await;
    if !response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.to_str().unwrap_or("").starts_with("text/html"))
    {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, 16 * 1024 * 1024).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Ok(mut html) = String::from_utf8(bytes.to_vec()) else {
        return Response::from_parts(parts, bytes.into());
    };
    if !html.contains("property=\"og:title\"") {
        let meta = metadata(
            &s.origin,
            &path,
            "Todoer · One thing at a time.",
            "Capture, plan and make room for what matters.",
            "/og/default.png",
        );
        if let Some(i) = html.find("</head>").or_else(|| html.find("<main")) {
            html.insert_str(i, &meta);
        }
    }
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, html.into())
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    #[tokio::test]
    async fn anonymous_cards_are_public_only_and_never_grant_write_access() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let space = db
            .create_workspace(
                "dev",
                crate::workspace::WorkspaceInput {
                    title: "Public test".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let list = db.default_list("dev", &space.id).unwrap();
        let task = db
            .capture(
                "dev",
                "web-public",
                Capture {
                    list_id: list.clone(),
                    tasks: vec![crate::core::NewTask {
                        title: "Tiny forest sentinel".into(),
                        description: "Bring a shovel".into(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let reference = crate::defaults::public_id(&db.conn, "task", &task.id).unwrap();
        db.set_public_access(
            "dev",
            "list",
            &list,
            PublicChange {
                enabled: true,
                revision: 0,
            },
        )
        .unwrap();
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            "public-test-token-with-24-chars".into(),
            "http://127.0.0.1:8092".into(),
        );
        let app = router(state.clone());
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/tasks/{reference}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(html.contains("property=\"og:title\" content=\"Tiny forest sentinel\""));
        assert!(html.contains(&format!("http://127.0.0.1:8092/og/task/{reference}.png")));
        assert!(!html.contains("data-brief-form"));
        let image = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/og/task/{reference}.png"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(image.status(), StatusCode::OK);
        assert_eq!(image.headers()[header::CONTENT_TYPE], "image/png");
        assert!(
            image
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .starts_with(b"\x89PNG\r\n\x1a\n")
        );
        let mutation = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/tasks/{}", task.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"revision":1,"title":"hacked"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(mutation.status(), StatusCode::UNAUTHORIZED);
        state
            .store
            .lock()
            .unwrap()
            .set_public_access(
                "dev",
                "list",
                &list,
                PublicChange {
                    enabled: false,
                    revision: 1,
                },
            )
            .unwrap();
        let revoked = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/og/task/{reference}.png"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(revoked.status(), StatusCode::NOT_FOUND);
        let missing = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/this-does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let html = String::from_utf8(
            missing
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(html.contains("Page not found."));
        assert!(html.contains("og:image"));
    }
}
