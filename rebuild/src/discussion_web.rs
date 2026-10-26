//! Discussion transport delegates every operation to live core authorization.
use super::*;
use crate::discussion::{
    Attachment, Comment, EditComment, MAX_ATTACHMENT_BYTES, NewComment, UploadInput,
};
use crate::object_storage::{ObjectStorage, StorageError};
use axum::{
    body::Bytes,
    routing::{delete, put},
};
use serde::Serialize;
fn objects(s: &AppState) -> Result<Arc<ObjectStorage>, ApiError> {
    s.storage.clone().ok_or(ApiError(
        StatusCode::SERVICE_UNAVAILABLE,
        "storage_disabled",
        "Attachment storage is not configured".into(),
    ))
}
fn storage_error(error: StorageError) -> ApiError {
    match error {
        StorageError::Integrity => {
            ApiError(StatusCode::BAD_REQUEST, "integrity", error.to_string())
        }
        StorageError::IntentClosed => {
            ApiError(StatusCode::CONFLICT, "intent_closed", error.to_string())
        }
        StorageError::TooLarge => ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            error.to_string(),
        ),
        _ => ApiError(
            StatusCode::BAD_GATEWAY,
            "storage_unavailable",
            "Attachment storage is unavailable; keep the selected file and retry".into(),
        ),
    }
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/tasks/{id}/comments",
            get(comments).post(create_comment),
        )
        .route(
            "/api/v1/comments/{id}",
            patch(edit_comment).delete(delete_comment),
        )
        .route("/api/v1/comments/{id}/delete", post(delete_comment))
        .route("/api/v1/tasks/{id}/uploads", post(create_upload))
        .route("/api/v1/uploads/{id}", get(upload))
        .route(
            "/api/v1/uploads/{id}/content",
            put(upload_content).layer(DefaultBodyLimit::max(MAX_ATTACHMENT_BYTES as usize)),
        )
        .route("/api/v1/uploads/{id}/finalize", post(finalize))
        .route("/api/v1/tasks/{id}/attachments", get(attachments))
        .route("/api/v1/attachments/{id}", delete(delete_attachment))
        .route("/api/v1/attachments/{id}/delete", post(delete_attachment))
        .route("/api/v1/attachments/{id}/download", get(download))
        .route("/fragments/tasks/{id}/discussion", get(fragment))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommentQuery {
    #[serde(default)]
    after_seq: i64,
    #[serde(default = "comment_limit")]
    limit: i64,
}
fn comment_limit() -> i64 {
    200
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedRevision {
    revision: i64,
}
async fn comments(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<CommentQuery>,
) -> Result<Json<Vec<Comment>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| db.comments(&actor, &id, q.after_seq, q.limit))
            .await?,
    ))
}
async fn create_comment(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<NewComment>,
) -> Result<Json<Comment>, ApiError> {
    let actor = s.auth(&h, true)?;
    let key = h
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|key| !key.trim().is_empty() && key.len() <= 200)
        .ok_or(ApiError(
            StatusCode::BAD_REQUEST,
            "idempotency",
            "Idempotency-Key required (1–200 bytes)".into(),
        ))?
        .to_owned();
    Ok(Json(
        s.run(move |db| db.create_comment(&actor, &id, &key, input))
            .await?,
    ))
}
async fn edit_comment(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<EditComment>,
) -> Result<Json<Comment>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.update_comment(&actor, &id, input))
            .await?,
    ))
}
async fn delete_comment(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<ExpectedRevision>,
) -> Result<Json<Comment>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.delete_comment(&actor, &id, input.revision))
            .await?,
    ))
}
pub(crate) async fn create_upload_actor(
    s: AppState,
    actor: String,
    id: String,
    input: UploadInput,
) -> Result<Value, ApiError> {
    let objects = objects(&s)?;
    let intent = s
        .run(move |db| db.create_upload_intent(&actor, &id, input))
        .await?;
    let signed = objects.signed_upload(&intent).map_err(storage_error)?;
    Ok(json!({"intent":intent,"upload":signed}))
}
pub(crate) async fn upload_actor(
    s: AppState,
    actor: String,
    id: String,
) -> Result<Value, ApiError> {
    let objects = objects(&s)?;
    let intent = s.run(move |db| db.upload_intent(&actor, &id)).await?;
    let upload = if intent.finalized {
        None
    } else {
        Some(objects.signed_upload(&intent).map_err(storage_error)?)
    };
    Ok(json!({"intent":intent,"upload":upload}))
}
async fn create_upload(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<UploadInput>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(create_upload_actor(s, actor, id, input).await?))
}
async fn upload(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(upload_actor(s, actor, id).await?))
}
async fn upload_content(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(expected): Query<ExpectedRevision>,
    req: axum::extract::Request,
) -> Result<Json<Value>, ApiError> {
    // Authenticate and authorize BEFORE consuming a potentially 25 MiB body.
    let h = req.headers().clone();
    let actor = s.auth(&h, true)?;
    let storage = objects(&s)?;
    let lookup = id.clone();
    let a = actor.clone();
    let intent = s.run(move |db| db.upload_intent(&a, &lookup)).await?;
    if intent.finalized || intent.revision != expected.revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "intent_closed",
            "Upload intent was finalized or changed".into(),
        ));
    }
    if h.get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|size| size > MAX_ATTACHMENT_BYTES)
    {
        return Err(storage_error(StorageError::TooLarge));
    }
    let bytes: Bytes = axum::body::to_bytes(req.into_body(), MAX_ATTACHMENT_BYTES as usize)
        .await
        .map_err(|_| storage_error(StorageError::TooLarge))?;
    let current = s.run(move |db| db.upload_intent(&actor, &id)).await?;
    if current.finalized || current.revision != expected.revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "intent_closed",
            "Upload intent was finalized or changed".into(),
        ));
    }
    storage
        .put_intent(&current, bytes.to_vec())
        .await
        .map_err(storage_error)?;
    Ok(Json(
        json!({"uploaded":true,"intent_id":current.id,"revision":current.revision}),
    ))
}
pub(crate) async fn finalize_actor(
    s: AppState,
    actor: String,
    id: String,
    revision: i64,
) -> Result<Attachment, ApiError> {
    let storage = objects(&s)?;
    let a = actor.clone();
    let intent = s.run(move |db| db.upload_intent(&a, &id)).await?;
    if revision != intent.revision && !(intent.finalized && revision == intent.revision - 1) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "stale_revision",
            "Upload intent changed".into(),
        ));
    }
    let verified = storage
        .verify_and_promote(&intent)
        .await
        .map_err(storage_error)?;
    s.run(move |db| db.finalize_attachment(&actor, verified))
        .await
}
async fn finalize(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(expected): Input<ExpectedRevision>,
) -> Result<Json<Attachment>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(finalize_actor(s, actor, id, expected.revision).await?))
}
async fn attachments(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<Attachment>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.attachments(&actor, &id)).await?))
}
async fn delete_attachment(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(expected): Input<ExpectedRevision>,
) -> Result<Json<Attachment>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.delete_attachment(&actor, &id, expected.revision))
            .await?,
    ))
}
async fn download(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let actor = s.auth(&h, false)?;
    let a = actor.clone();
    let lookup = id.clone();
    let attachment = s.run(move |db| db.attachment(&a, &lookup)).await?;
    let bytes = objects(&s)?.get(&attachment).await.map_err(storage_error)?;
    // A slow object read must not hand data to a now-revoked/deleted requester.
    let actor = s.auth(&h, false)?;
    let current = s.run(move |db| db.attachment(&actor, &id)).await?;
    let disposition = crate::object_storage::content_disposition(&current.filename);
    let mut response = bytes.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "application/octet-stream".parse().unwrap(),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        disposition.parse().map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "filename",
                "Invalid download filename".into(),
            )
        })?,
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    Ok(response)
}
#[derive(Serialize)]
struct FileView {
    record: Attachment,
    can_delete: bool,
}
struct CommentView {
    body_html: String,
    record: Comment,
    files: Vec<FileView>,
    can_edit: bool,
}
#[derive(Template)]
#[template(path = "discussion.html")]
struct Discussion {
    comments: Vec<CommentView>,
    files: Vec<FileView>,
    can_write: bool,
    storage_enabled: bool,
    next_cursor: Option<i64>,
}
async fn fragment(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<CommentQuery>,
) -> Result<Response, ApiError> {
    let actor = s.auth(&h, false)?;
    let (comments, files, rank, next) = s
        .run(move |db| {
            let records = db.comments(&actor, &id, q.after_seq, q.limit)?;
            let task = crate::core::task(&db.conn, &id)?;
            let rank = db.list_permission(&actor, &task.list_id)?;
            let (user, _) = crate::core::actor_identity(&db.conn, &actor)?;
            let all_files = db.attachments(&actor, &id)?;
            let file_view = |file: &Attachment| FileView {
                record: file.clone(),
                can_delete: rank >= 2 && (file.creator_id == user || rank >= 3),
            };
            let next = match records.last() {
                Some(last) if !db.comments(&actor, &id, last.seq, 1)?.is_empty() => Some(last.seq),
                _ => None,
            };
            let comments = records
                .into_iter()
                .map(|record| {
                    let files = all_files
                        .iter()
                        .filter(|file| file.comment_id.as_deref() == Some(&record.id))
                        .map(file_view)
                        .collect();
                    let can_edit = rank >= 2 && (record.author_id == user || rank >= 3);
                    let body_html = crate::taxonomy::render_references(
                        &db.conn,
                        &actor,
                        "comment",
                        &record.id,
                        &record.body,
                    )?;
                    Ok(CommentView {
                        body_html,
                        record,
                        files,
                        can_edit,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let files = all_files
                .iter()
                .filter(|file| file.comment_id.is_none())
                .map(file_view)
                .collect();
            Ok((comments, files, rank, next))
        })
        .await?;
    let rendered = Discussion {
        comments,
        files,
        can_write: rank >= 2,
        storage_enabled: s.storage.is_some(),
        next_cursor: next,
    }
    .render()
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "template",
            "Discussion could not be rendered".into(),
        )
    })?;
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Html(rendered),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    const TOKEN: &str = "synthetic-discussion-route-token";
    fn app(storage: Option<ObjectStorage>) -> (Router, AppState, String) {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let task = db
            .capture(
                "dev",
                "route-discussion",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![crate::core::NewTask {
                        title: "Discussion route fixture".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            TOKEN.into(),
            "http://127.0.0.1:8092".into(),
        )
        .with_storage(storage);
        (
            routes()
                .layer(DefaultBodyLimit::max(128 * 1024))
                .with_state(state.clone()),
            state,
            task.id,
        )
    }
    async fn request(
        app: &Router,
        method: &str,
        path: &str,
        body: Value,
        key: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/json");
        if let Some(key) = key {
            request = request.header("idempotency-key", key);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    #[tokio::test]
    async fn comment_routes_are_idempotent_revision_checked_and_render_sanitized_markdown() {
        let (app, state, task) = app(None);
        let path = format!("/api/v1/tasks/{task}/comments");
        assert_eq!(
            request(&app, "POST", &path, json!({"body":"Missing key"}), None)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        let body = json!({"body":"**Synthetic** <script>alert(1)</script>"});
        let (status, comment) =
            request(&app, "POST", &path, body.clone(), Some("same-comment")).await;
        assert_eq!(status, StatusCode::OK);
        let (_, retry) = request(&app, "POST", &path, body, Some("same-comment")).await;
        assert_eq!(comment["id"], retry["id"]);
        assert_eq!(
            request(
                &app,
                "POST",
                &path,
                json!({"body":"Changed"}),
                Some("same-comment")
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let id = comment["id"].as_str().unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/fragments/tasks/{task}/discussion"))
                    .header("authorization", format!("Bearer {TOKEN}"))
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
        assert!(html.contains("<strong>Synthetic</strong>"));
        assert!(!html.contains("<script>"));
        assert!(html.contains("data-new-comment"));
        assert_eq!(
            request(
                &app,
                "PATCH",
                &format!("/api/v1/comments/{id}"),
                json!({"revision":1,"body":"Updated"}),
                None
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            request(
                &app,
                "PATCH",
                &format!("/api/v1/comments/{id}"),
                json!({"revision":1,"body":"Stale"}),
                None
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        state
            .store
            .lock()
            .unwrap()
            .conn
            .execute(
                "INSERT INTO membership_roles(list_id,user_id,role) VALUES('inbox','dev','viewer')",
                [],
            )
            .unwrap();
        assert_eq!(
            request(
                &app,
                "PATCH",
                &format!("/api/v1/comments/{id}"),
                json!({"revision":2,"body":"Denied"}),
                None
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    #[ignore = "requires isolated local Moto S3 server; see attachments.md"]
    async fn local_s3_http_routes_bound_uploads_proxy_private_download_and_recheck_revocation() {
        let endpoint = std::env::var("TODOER_TEST_S3_ENDPOINT").expect("local S3 fixture required");
        assert!(endpoint.starts_with("http://127.0.0.1:"));
        let bucket = std::env::var("TODOER_TEST_S3_BUCKET").expect("synthetic bucket required");
        assert!(bucket.starts_with("todoer-synthetic-"));
        let storage = ObjectStorage::new(crate::object_storage::S3Config {
            endpoint,
            bucket,
            region: "us-east-1".into(),
            prefix: "synthetic-http".into(),
            access_key_id: "synthetic-test".into(),
            secret_access_key: "synthetic-test-secret".into(),
        })
        .unwrap();
        let (app, state, task) = app(Some(storage));
        // Larger than the app's 128KiB JSON limit: the raw upload route permits
        // a bounded file body independently of that outer limit.
        let bytes = vec![b'x'; 200 * 1024];
        let(status,created)=request(&app,"POST",&format!("/api/v1/tasks/{task}/uploads"),json!({"filename":"../synthetic.txt","size":bytes.len(),"sha256":crate::object_storage::sha256_hex(&bytes)}),None).await;
        assert_eq!(status, StatusCode::OK);
        let id = created["intent"]["id"].as_str().unwrap();
        assert!(
            !created["intent"]
                .as_object()
                .unwrap()
                .contains_key("object_key")
        );
        let upload_path = format!("/api/v1/uploads/{id}/content?revision=1");
        let send = || {
            Request::builder()
                .method("PUT")
                .uri(&upload_path)
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/octet-stream")
                .body(Body::from(bytes.clone()))
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(send()).await.unwrap().status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone().oneshot(send()).await.unwrap().status(),
            StatusCode::OK
        );
        let (status, file) = request(
            &app,
            "POST",
            &format!("/api/v1/uploads/{id}/finalize"),
            json!({"revision":1}),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, retry) = request(
            &app,
            "POST",
            &format!("/api/v1/uploads/{id}/finalize"),
            json!({"revision":1}),
            None,
        )
        .await;
        assert_eq!(file["id"], retry["id"]);
        assert_eq!(
            app.clone().oneshot(send()).await.unwrap().status(),
            StatusCode::CONFLICT
        );
        let download = format!(
            "/api/v1/attachments/{}/download",
            file["id"].as_str().unwrap()
        );
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&download)
                    .header("authorization", format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/octet-stream"
        );
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert!(
            response.headers()[header::CONTENT_DISPOSITION]
                .to_str()
                .unwrap()
                .starts_with("attachment;")
        );
        assert!(response.headers().get(header::LOCATION).is_none());
        assert_eq!(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .as_ref(),
            bytes
        );
        state
            .store
            .lock()
            .unwrap()
            .conn
            .execute(
                "DELETE FROM memberships WHERE list_id='inbox' AND user_id='dev'",
                [],
            )
            .unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(download)
                    .header("authorization", format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        println!(
            "discussion HTTP routes + real local S3: 200KiB upload override, stable retries, finalized overwrite refusal, octet-stream proxy download, live revocation passed"
        );
    }
}
