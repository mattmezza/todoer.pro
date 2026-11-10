//! Release-specific URLs keep the entire ES module graph in one browser cache.
use super::*;

pub(super) const PALETTE_BASE: &str = concat!("/palette/", env!("TODOER_ASSET_VERSION"), "/");

pub(super) const BASE: &str = concat!("/static/", env!("TODOER_ASSET_VERSION"), "/");

pub(super) const SCRIPT_BASE: &str = concat!(
    "/static/",
    env!("TODOER_ASSET_VERSION"),
    "/",
    env!("TODOER_SCRIPT_PREFIX")
);

pub(super) fn fingerprinted_image(filename: &str) -> Option<&'static str> {
    // Direct Cargo builds without the Node assets step retain revalidating aliases.
    if env!("TODOER_BUNDLED_ENTRIES").is_empty() {
        return None;
    }
    match filename {
        "icons.svg" => Some(env!("TODOER_ICONS_URL")),
        "icon192-v2.png" => Some(env!("TODOER_ICON_URL")),
        _ => None,
    }
}

pub(super) async fn manifest() -> Response {
    let icon = fingerprinted_image("icon192-v2.png")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{BASE}icon192-v2.png"));
    let json = include_str!("../assets/manifest.webmanifest")
        .replace("/assets/icon192-v2.png", &icon)
        .replace("/assets/icon512-v2.png", &format!("{BASE}icon512-v2.png"))
        .replace(
            "/assets/icon512-maskable-v2.png",
            &format!("{BASE}icon512-maskable-v2.png"),
        );
    ([(header::CONTENT_TYPE, "application/manifest+json")], json).into_response()
}

pub(super) async fn palette(Path(colors): Path<String>) -> Response {
    let colors = colors.split(',').collect::<Vec<_>>();
    if colors.len() > 512
        || colors
            .iter()
            .any(|color| color.len() != 6 || !color.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let css = colors.iter().map(|color| format!("[data-entity-color=\"#{color}\"]{{background-color:#{color}}}[data-list-color=\"#{color}\"]{{--list-color:#{color}}}")).collect::<String>();
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], css).into_response()
}

pub(super) fn directory() -> &'static str {
    if std::path::Path::new("app/assets").is_dir() {
        "app/assets"
    } else {
        "assets"
    }
}

pub(super) fn version_html(html: &str) -> String {
    // Only asset attributes are rewritten. Task text and user-provided URLs keep
    // their meaning; installation metadata retains its revalidating public URL.
    let mut result = html.to_owned();
    for attribute in ["src=\"/assets/", "href=\"/assets/"] {
        let mut output = String::with_capacity(result.len());
        let mut rest = result.as_str();
        while let Some(start) = rest.find(attribute) {
            let end = start + attribute.len();
            output.push_str(&rest[..start]);
            let path = rest[end..].split('"').next().unwrap_or_default();
            let filename = path.split(['?', '#']).next().unwrap_or_default();
            if let Some(url) = fingerprinted_image(filename) {
                output.push_str(&attribute[..attribute.len() - "/assets/".len()]);
                output.push_str(url);
                rest = &rest[end + filename.len()..];
                continue;
            }
            if filename.ends_with(".js")
                || filename.ends_with(".css")
                || filename.ends_with(".png")
                || filename.ends_with(".svg")
            {
                output.push_str(&attribute[..attribute.len() - "/assets/".len()]);
                let bundled = env!("TODOER_BUNDLED_ENTRIES")
                    .split(',')
                    .any(|entry| entry == filename);
                output.push_str(if bundled { SCRIPT_BASE } else { BASE });
            } else {
                output.push_str(attribute);
            }
            rest = &rest[end..];
        }
        output.push_str(rest);
        result = output;
    }
    let mut colors = std::collections::BTreeSet::new();
    for attribute in ["data-entity-color=\"#", "data-list-color=\"#"] {
        for suffix in html.split(attribute).skip(1) {
            if let Some(color) = suffix.split('"').next()
                && color.len() == 6
                && color.bytes().all(|c| c.is_ascii_hexdigit())
            {
                colors.insert(color);
            }
        }
    }
    let palette = colors
        .into_iter()
        .collect::<Vec<_>>()
        .chunks(128)
        .map(|colors| {
            format!(
                "<link rel=\"stylesheet\" href=\"{PALETTE_BASE}{}\">",
                colors.join(",")
            )
        })
        .collect::<String>();
    let font = if html.contains("data-font=\"inter\"") {
        "inter"
    } else {
        "geist"
    };
    let font_preload = format!(
        "<link rel=\"preload\" as=\"font\" href=\"{BASE}fonts/{font}-latin-wght-normal.woff2\" type=\"font/woff2\" crossorigin>"
    );
    result.replacen(
        "</head>",
        &format!(
            "{palette}{font_preload}<meta name=\"asset-base\" content=\"{SCRIPT_BASE}\"></head>"
        ),
        1,
    )
}

pub(super) async fn version_response(mut response: Response) -> Response {
    if !response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| h.starts_with("text/html"))
    {
        return response;
    }
    let body = std::mem::replace(response.body_mut(), axum::body::Body::empty());
    match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => {
            let html = String::from_utf8_lossy(&bytes);
            *response.body_mut() = axum::body::Body::from(version_html(&html));
            response.headers_mut().remove(header::CONTENT_LENGTH);
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cache_headers_distinguish_releases_metadata_private_pages_and_errors() {
        use tower::ServiceExt;
        let mut store = Store::memory().unwrap();
        store.seed_dev().unwrap();
        let app = router(AppState::new(
            Arc::new(Mutex::new(store)),
            "synthetic-cache-test-token".into(),
            "http://127.0.0.1:8092".into(),
        ));
        for (path, status, policy) in [
            (
                format!("{BASE}site.js"),
                StatusCode::OK,
                "public, max-age=31536000, immutable",
            ),
            (
                format!("{BASE}numeric-mono.woff2"),
                StatusCode::OK,
                "public, max-age=31536000, immutable",
            ),
            (
                env!("TODOER_ICONS_URL").into(),
                StatusCode::OK,
                "public, max-age=31536000, immutable",
            ),
            (
                env!("TODOER_ICON_URL").into(),
                StatusCode::OK,
                "public, max-age=31536000, immutable",
            ),
            (
                format!("{PALETTE_BASE}17ac6e,abcdef"),
                StatusCode::OK,
                "public, max-age=31536000, immutable",
            ),
            (
                format!("{PALETTE_BASE}not-a-color"),
                StatusCode::NOT_FOUND,
                "no-store",
            ),
            (
                "/assets/manifest.webmanifest".into(),
                StatusCode::OK,
                "public, no-cache",
            ),
            ("/sw.js".into(), StatusCode::OK, "no-store"),
            ("/signin".into(), StatusCode::OK, "no-store"),
            (
                format!("{BASE}missing.js"),
                StatusCode::NOT_FOUND,
                "no-store",
            ),
            (
                format!("/static/{}/site.js", "wrong-build"),
                StatusCode::NOT_FOUND,
                "no-store",
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .uri(&path)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status, "{path}");
            assert_eq!(response.headers()[header::CACHE_CONTROL], policy, "{path}");
        }
    }

    #[test]
    fn colors_and_icon_fragments_are_ready_in_the_initial_document() {
        let html = version_html(
            "<head></head><span data-entity-color=\"#17ac6e\"></span><article data-list-color=\"#17ac6e\"></article><svg><use href=\"/assets/icons.svg#check\"/></svg>",
        );
        assert!(html.contains(&format!("href=\"{PALETTE_BASE}17ac6e\"")));
        let url = fingerprinted_image("icons.svg")
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{BASE}icons.svg"));
        assert!(html.contains(&format!("href=\"{url}#check\"")));
        assert!(!html.contains("style="));
    }

    #[tokio::test]
    async fn install_manifest_revalidates_but_its_images_are_fingerprinted() {
        let response = manifest().await;
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        for icon in value["icons"].as_array().unwrap() {
            let src = icon["src"].as_str().unwrap();
            assert!(src.starts_with(BASE) || src.starts_with("/assets/images/"));
        }
    }

    #[tokio::test]
    async fn mobile_clients_receive_compressed_versioned_styles() {
        use tower::ServiceExt;
        let store = Store::memory().unwrap();
        let app = router(AppState::new(
            Arc::new(Mutex::new(store)),
            "synthetic-compression-test-token".into(),
            "http://127.0.0.1:8092".into(),
        ));
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("{BASE}app.css"))
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_ENCODING], "gzip");
        assert!(
            response.headers()[header::VARY]
                .to_str()
                .unwrap()
                .contains("accept-encoding")
        );
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let original = std::fs::metadata(format!("{}/app.css", directory()))
            .unwrap()
            .len();
        assert!(bytes.len() as u64 * 2 < original);
    }

    #[test]
    fn versions_executables_but_preserves_install_metadata_and_text() {
        let html = version_html(
            "<head><script src=\"/assets/site.js\"></script><link href=\"/assets/app.css\"><link href=\"/assets/manifest.webmanifest\"></head><img src=\"/assets/icon192-v2.png\"><p>/assets/site.js</p>",
        );
        assert!(html.contains(&format!("src=\"{SCRIPT_BASE}site.js\"")));
        assert!(html.contains(&format!("href=\"{BASE}app.css\"")));
        assert!(html.contains("href=\"/assets/manifest.webmanifest\""));
        let image = fingerprinted_image("icon192-v2.png")
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{BASE}icon192-v2.png"));
        assert!(html.contains(&format!("src=\"{image}\"")));
        assert!(html.contains("<p>/assets/site.js</p>"));
        assert!(html.contains(&format!("name=\"asset-base\" content=\"{SCRIPT_BASE}\"")));
    }
}
