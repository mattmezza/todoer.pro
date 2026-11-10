//! Release-specific URLs keep the entire ES module graph in one browser cache.
use super::*;

pub(super) const BASE: &str = concat!("/static/", env!("TODOER_ASSET_VERSION"), "/");

pub(super) fn directory() -> &'static str {
    if std::path::Path::new("app/assets").is_dir() {
        "app/assets"
    } else {
        "assets"
    }
}

pub(super) fn version_html(html: &str) -> String {
    // Only asset attributes are rewritten. Task text and user-provided URLs keep
    // their meaning; images and installation metadata retain their public URLs.
    let mut result = html.to_owned();
    for attribute in ["src=\"/assets/", "href=\"/assets/"] {
        let mut output = String::with_capacity(result.len());
        let mut rest = result.as_str();
        while let Some(start) = rest.find(attribute) {
            let end = start + attribute.len();
            output.push_str(&rest[..start]);
            let path = rest[end..].split('"').next().unwrap_or_default();
            let filename = path.split(['?', '#']).next().unwrap_or_default();
            if filename.ends_with(".js") || filename.ends_with(".css") {
                output.push_str(&attribute[..attribute.len() - "/assets/".len()]);
                output.push_str(BASE);
            } else {
                output.push_str(attribute);
            }
            rest = &rest[end..];
        }
        output.push_str(rest);
        result = output;
    }
    result.replacen(
        "</head>",
        &format!("<meta name=\"asset-base\" content=\"{BASE}\"></head>"),
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
        assert!(html.contains(&format!("src=\"{BASE}site.js\"")));
        assert!(html.contains(&format!("href=\"{BASE}app.css\"")));
        assert!(html.contains("href=\"/assets/manifest.webmanifest\""));
        assert!(html.contains("src=\"/assets/icon192-v2.png\""));
        assert!(html.contains("<p>/assets/site.js</p>"));
        assert!(html.contains(&format!("name=\"asset-base\" content=\"{BASE}\"")));
    }
}
