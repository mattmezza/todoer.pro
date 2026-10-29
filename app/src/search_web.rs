//! Structured search uses the same live actor as the other REST routes.
use super::*;
use crate::search::{Query as SearchQuery, SearchPage};

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/query", get(query_get).post(query_post))
}
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct GetQuery {
    /// Canonical JSON query supports arrays and all filters without GET array ambiguity.
    query: Option<String>,
    text: Option<String>,
    workspace_id: Option<String>,
    list_id: Option<String>,
    section_id: Option<String>,
    tag: Option<String>,
    status: Option<String>,
    priority: Option<u8>,
    planned_from: Option<String>,
    planned_to: Option<String>,
    due_from: Option<String>,
    due_to: Option<String>,
    recurrence: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
    sort: Option<crate::search::Sort>,
}
async fn query_get(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(input): Query<GetQuery>,
) -> Result<Json<SearchPage>, ApiError> {
    let actor = s.auth(&h, false)?;
    let query = if let Some(raw) = input.query {
        // Reject mixed forms: otherwise a caller can accidentally overwrite a filter.
        if input.text.is_some()
            || input.workspace_id.is_some()
            || input.list_id.is_some()
            || input.section_id.is_some()
            || input.tag.is_some()
            || input.status.is_some()
            || input.priority.is_some()
            || input.planned_from.is_some()
            || input.planned_to.is_some()
            || input.due_from.is_some()
            || input.due_to.is_some()
            || input.recurrence.is_some()
            || input.limit.is_some()
            || input.cursor.is_some()
            || input.sort.is_some()
        {
            return Err(Error::Invalid(
                "query JSON cannot be mixed with scalar query parameters".into(),
            )
            .into());
        }
        serde_json::from_str(&raw)
            .map_err(|_| ApiError::from(Error::Invalid("invalid query JSON".into())))?
    } else {
        SearchQuery {
            text: input.text.unwrap_or_default(),
            workspace_id: input.workspace_id,
            list_id: input.list_id,
            section_id: input.section_id,
            tag: input.tag,
            status: input.status.unwrap_or_else(|| "open".into()),
            priority: input.priority,
            planned_from: input.planned_from,
            planned_to: input.planned_to,
            due_from: input.due_from,
            due_to: input.due_to,
            recurrence: input.recurrence,
            limit: input.limit.unwrap_or(50),
            cursor: input.cursor,
            sort: input.sort.unwrap_or_default(),
            ..SearchQuery::default()
        }
    };
    Ok(Json(s.run(move |db| db.query(&actor, &query)).await?))
}
async fn query_post(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(query): Input<SearchQuery>,
) -> Result<Json<SearchPage>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.query(&actor, &query)).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    const TOKEN: &str = "isolated-structured-search-http-token";
    fn app() -> Router {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn.execute_batch("INSERT INTO tasks(id,reference,list_id,title,description,position) VALUES('a','T-a','inbox','needle public','Visible description',0),('b','T-b','inbox','needle second','Another public',1),('secret','T-secret','private','needle hidden','Hidden description',0);").unwrap();
        router(AppState::new(
            Arc::new(Mutex::new(db)),
            TOKEN.into(),
            "http://127.0.0.1:8092".into(),
        ))
    }
    async fn request(
        app: &Router,
        method: &str,
        path: &str,
        body: Option<Value>,
        auth: bool,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if auth {
            req = req.header("authorization", format!("Bearer {TOKEN}"));
        }
        let res = app
            .clone()
            .oneshot(
                req.body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let data = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&data).unwrap())
    }
    #[tokio::test]
    async fn real_router_get_post_auth_projection_and_cursor_contract() {
        let app = app();
        assert_eq!(
            request(&app, "GET", "/api/v1/query?text=needle", None, false)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        let (status, page) = request(
            &app,
            "GET",
            "/api/v1/query?text=needle&limit=1&sort=id",
            None,
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["total"], 2);
        assert_eq!(page["hits"][0]["id"], "a");
        assert!(!page.to_string().contains("Hidden description"));
        let payload = json!({"text":"needle","limit":1,"sort":"id","cursor":page["next_cursor"]});
        let (status, next) = request(&app, "POST", "/api/v1/query", Some(payload), true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(next["hits"][0]["id"], "b");
        let (status, _) = request(
            &app,
            "POST",
            "/api/v1/query",
            Some(json!({"text":"different","limit":1,"sort":"id","cursor":page["next_cursor"]})),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, projected) = request(
            &app,
            "POST",
            "/api/v1/query",
            Some(json!({"text":"needle","entities":["task"],"fields":[]})),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(projected["hits"][0].get("snippet").is_none());
        let url = reqwest::Url::parse_with_params(
            "http://localhost/api/v1/query",
            [(
                "query",
                r#"{"text":"needle","entities":["task"],"fields":[]}"#,
            )],
        )
        .unwrap();
        assert_eq!(
            request(
                &app,
                "GET",
                &format!("{}?{}", url.path(), url.query().unwrap()),
                None,
                true
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    #[tokio::test]
    async fn invalid_filters_reject_without_interpreting_fts_operators() {
        let app = app();
        for body in [
            json!({"limit":101}),
            json!({"status":"anything"}),
            json!({"priority":5}),
            json!({"due_from":"2026-99-30"}),
            json!({"fields":["object_key"]}),
        ] {
            assert_eq!(
                request(&app, "POST", "/api/v1/query", Some(body), true)
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        let (status, page) = request(
            &app,
            "POST",
            "/api/v1/query",
            Some(json!({"text":"needle OR hidden"})),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["total"], 0);
        assert_eq!(
            request(
                &app,
                "GET",
                "/api/v1/query?query=%7B%7D&text=needle",
                None,
                true
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
}
