//! Private, bounded transfer endpoints. Imports require browser approval and CSRF.
use super::*;
use crate::transfer::{self, ExportScope, IdentityMap, TransferError};
use axum::{body::Body, extract::Request};
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ExportQuery {
    format: Option<String>,
    list: Option<String>,
    workspace: Option<String>,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ImportQuery {
    #[serde(default)]
    apply: bool,
}
pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/export", get(export)).route(
        "/api/v1/import",
        post(import).layer(axum::extract::DefaultBodyLimit::disable()),
    )
}
fn error(e: TransferError) -> ApiError {
    match e {
        TransferError::Domain(e) => e.into(),
        TransferError::Invalid(s) => ApiError(StatusCode::BAD_REQUEST, "invalid_transfer", s),
        TransferError::Conflict(s) => ApiError(StatusCode::CONFLICT, "transfer_conflict", s),
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "transfer_failed",
            "Transfer could not finish".into(),
        ),
    }
}
fn internal() -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "transfer_failed",
        "Transfer could not finish".into(),
    )
}
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("todoer-http-transfer-{}", uuid::Uuid::new_v4())))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
async fn export(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let actor = s.auth(&h, false)?;
    let scope = match (q.list, q.workspace) {
        (Some(id), None) => ExportScope::List(id),
        (None, Some(id)) => ExportScope::Workspace(id),
        (None, None) => ExportScope::Account,
        _ => {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "scope",
                "Choose list or workspace".into(),
            ));
        }
    };
    let output = Temp::new();
    let format = q.format.as_deref().unwrap_or("json");
    let (mime, name) = if format == "zip" {
        let legacy = crate::object_storage::LegacyStorageConfig::from_env()
            .map_err(|_| internal())?
            .map(crate::object_storage::LegacyBlobSource::new)
            .transpose()
            .map_err(|_| internal())?;
        transfer::export_zip(
            s.store.clone(),
            actor,
            scope,
            s.storage.as_deref(),
            legacy.as_ref(),
            &output.0,
        )
        .await
        .map_err(error)?;
        ("application/zip", "todoer-export.zip")
    } else {
        let store = s.store.clone();
        let kind = format.to_owned();
        let path = output.0.clone();
        tokio::task::spawn_blocking(move || {
            let bundle = store
                .lock()
                .map_err(|_| TransferError::Invalid("Database lock failed".into()))?
                .export_bundle(&actor, scope)?;
            let bytes = match kind.as_str() {
                "json" => serde_json::to_vec_pretty(&bundle)
                    .map_err(|_| TransferError::Invalid("JSON encoding failed".into()))?,
                "text" => transfer::export_text(&bundle)?.into_bytes(),
                "csv" => transfer::export_csv(&bundle)?,
                _ => return Err(TransferError::Invalid("Unknown export format".into())),
            };
            if bytes.len() > 100 * 1024 * 1024 {
                return Err(TransferError::Invalid("Export exceeds 100 MiB".into()));
            }
            transfer::write_private_new(&path, &bytes)
        })
        .await
        .map_err(|_| internal())?
        .map_err(error)?;
        match format {
            "csv" => ("text/csv; charset=utf-8", "todoer-export.csv"),
            "text" => ("text/plain; charset=utf-8", "todoer-export.txt"),
            _ => ("application/json", "todoer-export.json"),
        }
    };
    let file = tokio::fs::File::open(&output.0)
        .await
        .map_err(|_| internal())?;
    let stream = futures_util::stream::try_unfold((file, output), |(mut file, guard)| async move {
        let mut bytes = vec![0; 65536];
        let count = file.read(&mut bytes).await?;
        if count == 0 {
            Ok::<_, std::io::Error>(None)
        } else {
            bytes.truncate(count);
            Ok(Some((bytes, (file, guard))))
        }
    });
    Ok((
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
            (header::CACHE_CONTROL, "no-store, private".into()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}
async fn import(
    State(s): State<AppState>,
    Query(q): Query<ImportQuery>,
    request: Request,
) -> Result<Response, ApiError> {
    let h = request.headers();
    if h.contains_key(header::AUTHORIZATION) || !s.origin_ok(h) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "browser_approval",
            "Browser session and same-origin approval required".into(),
        ));
    }
    let actor = s.auth(h, true)?;
    if actor.starts_with("credential:") {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "browser_approval",
            "Browser session required".into(),
        ));
    }
    let zip = h
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(';').next() == Some("application/zip"));
    if !zip
        && h.get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_none_or(|v| v.split(';').next() != Some("application/json"))
    {
        return Err(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "content_type",
            "Use application/zip or application/json".into(),
        ));
    }
    let output = Temp::new();
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&output.0).map_err(|_| internal())?;
    let mut file = tokio::fs::File::from_std(file);
    let mut stream = request.into_body().into_data_stream();
    let mut total = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "body", "Incomplete upload".into()))?;
        total = total.saturating_add(chunk.len() as u64);
        if total > if zip { MAX_ARCHIVE } else { 100 * 1024 * 1024 } {
            return Err(ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "size",
                "Transfer upload exceeds limit".into(),
            ));
        }
        file.write_all(&chunk).await.map_err(|_| internal())?;
    }
    file.sync_all().await.map_err(|_| internal())?;
    drop(file);
    let path = output.0.clone();
    let bundle = tokio::task::spawn_blocking(move || {
        if zip {
            transfer::validated_zip_bundle(&path)
        } else {
            let bytes = std::fs::read(path)?;
            serde_json::from_slice::<transfer::ExportBundle>(&bytes)
                .map_err(|_| TransferError::Invalid("Invalid JSON export".into()))
        }
    })
    .await
    .map_err(|_| internal())?
    .map_err(error)?;
    let mapping = IdentityMap {
        users: std::collections::BTreeMap::from([(bundle.exported_for.clone(), actor)]),
    };
    let report = if zip {
        transfer::restore_zip(
            s.store.clone(),
            &output.0,
            &mapping,
            s.storage.as_deref(),
            q.apply,
        )
        .await
        .map_err(error)?
    } else {
        if bundle
            .tables
            .get("attachments")
            .is_some_and(|v| !v.is_empty())
            || bundle
                .tables
                .get("legacy_attachment_refs")
                .is_some_and(|v| !v.is_empty())
        {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "attachment_bytes",
                "Use ZIP to restore attachment bytes".into(),
            ));
        }
        let store = s.store.clone();
        tokio::task::spawn_blocking(move || {
            transfer::import_bundle(
                &mut *store
                    .lock()
                    .map_err(|_| TransferError::Invalid("Database lock failed".into()))?,
                &bundle,
                &mapping,
                q.apply,
            )
        })
        .await
        .map_err(|_| internal())?
        .map_err(error)?
    };
    Ok(([(header::CACHE_CONTROL, "no-store, private")], Json(report)).into_response())
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    fn state() -> AppState {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            "synthetic-token".into(),
            "http://localhost".into(),
        );
        state.sessions.lock().unwrap().insert(
            Sha256::digest(b"synthetic-session").into(),
            std::time::Instant::now(),
        );
        state
    }
    fn payload() -> String {
        serde_json::to_string(&transfer::ExportBundle {
            format: "todoer-business-export".into(),
            version: 1,
            exported_for: "source".into(),
            tables: std::collections::BTreeMap::from([(
                "users".into(),
                vec![json!({"id":"source","name":"Synthetic"})],
            )]),
            notes: vec![],
        })
        .unwrap()
    }
    #[tokio::test]
    async fn import_requires_cookie_csrf_and_origin_and_defaults_to_preview() {
        let state = state();
        let app = routes().with_state(state.clone());
        let bearer = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/import?apply=true")
                    .header("authorization", "Bearer synthetic-token")
                    .header("content-type", "application/json")
                    .body(Body::from(payload()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bearer.status(), StatusCode::FORBIDDEN);
        let no_csrf = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/import")
                    .header("cookie", "todoer_dev=synthetic-session")
                    .header("content-type", "application/json")
                    .body(Body::from(payload()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(no_csrf.status(), StatusCode::FORBIDDEN);
        let cross_origin = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/import")
                    .header("cookie", "todoer_dev=synthetic-session")
                    .header("x-csrf-token", &state.csrf)
                    .header("origin", "https://other.invalid")
                    .header("content-type", "application/json")
                    .body(Body::from(payload()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cross_origin.status(), StatusCode::FORBIDDEN);
        let preview = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/import")
                    .header("cookie", "todoer_dev=synthetic-session")
                    .header("x-csrf-token", &state.csrf)
                    .header("content-type", "application/json")
                    .body(Body::from(payload()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(preview.status(), StatusCode::OK);
        assert_eq!(
            preview.headers()[header::CACHE_CONTROL],
            "no-store, private"
        );
        let body = to_bytes(preview.into_body(), 10000).await.unwrap();
        assert!(
            !serde_json::from_slice::<Value>(&body).unwrap()["applied"]
                .as_bool()
                .unwrap()
        );
        assert_eq!(
            state
                .store
                .lock()
                .unwrap()
                .conn
                .query_row("SELECT COUNT(*) FROM transfer_sources", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn export_is_authorized_and_private() {
        let app = routes().with_state(state());
        let hidden = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/export?list=private")
                    .header("authorization", "Bearer synthetic-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(hidden.status(), StatusCode::FORBIDDEN);
        let visible = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/export?format=json")
                    .header("authorization", "Bearer synthetic-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(visible.status(), StatusCode::OK);
        assert_eq!(
            visible.headers()[header::CACHE_CONTROL],
            "no-store, private"
        );
        let bytes = to_bytes(visible.into_body(), 100000).await.unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("synthetic-token"));
    }
}
