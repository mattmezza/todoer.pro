//! Thin taxonomy adapters; shared dispatchers call the existing domain operations.
use super::*;
use crate::taxonomy::TagSelector;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/organize", post(organize_request))
        .route("/api/v1/references", post(reference_request))
        .route("/api/v1/lists/{id}/tags", get(tags).post(create_tag))
        .route("/api/v1/tags/{id}", patch(rename_tag))
        .route("/api/v1/tasks/{id}/tags", get(task_tags).put(set_tags))
        .route("/api/v1/tasks/{id}/section", get(section).put(set_section))
        .route("/api/v1/tasks/{id}/reparent", post(reparent))
        .route("/api/v1/lists/{id}/task-order", post(reorder))
        .route("/api/v1/references/resolve", get(resolve))
        .route("/api/v1/references/autocomplete", get(autocomplete))
        .route("/api/v1/references/{kind}/{id}/unresolved", get(unresolved))
        .route("/api/v1/references/{kind}/{id}/backlinks", get(backlinks))
        .route("/api/v1/references/{kind}/{id}/bind", post(bind))
}
fn default_limit() -> usize {
    20
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum OrganizeAction {
    TagsGet {
        id: String,
    },
    TagsSet {
        id: String,
        revision: i64,
        tags: Vec<TagSelector>,
    },
    TagList {
        list_id: String,
        #[serde(default)]
        prefix: String,
        #[serde(default = "default_limit")]
        limit: usize,
    },
    TagCreate {
        list_id: String,
        name: String,
    },
    TagRename {
        id: String,
        revision: i64,
        name: String,
    },
    SectionGet {
        id: String,
    },
    SectionSet {
        id: String,
        revision: i64,
        section_id: Option<String>,
    },
    Reparent {
        id: String,
        revision: i64,
        parent_id: Option<String>,
    },
    Reorder {
        list_id: String,
        parent_id: Option<String>,
        revision: i64,
        ids: Vec<String>,
    },
}
impl OrganizeAction {
    fn writes(&self) -> bool {
        !matches!(
            self,
            Self::TagsGet { .. } | Self::TagList { .. } | Self::SectionGet { .. }
        )
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ReferenceAction {
    Resolve {
        reference: String,
    },
    Autocomplete {
        query: String,
        #[serde(default = "default_limit")]
        limit: usize,
    },
    Unresolved {
        source_kind: String,
        source_id: String,
    },
    Bind {
        source_kind: String,
        source_id: String,
        revision: i64,
        occurrence: usize,
        target_kind: String,
        target_id: String,
    },
    Backlinks {
        target_kind: String,
        target_id: String,
    },
}
impl ReferenceAction {
    fn writes(&self) -> bool {
        matches!(self, Self::Bind { .. })
    }
}
fn value<T: serde::Serialize>(v: T) -> Result<Value, Error> {
    serde_json::to_value(v).map_err(|_| Error::Invalid("cannot encode taxonomy response".into()))
}
pub(crate) fn organize(
    db: &mut Store,
    actor: &str,
    action: OrganizeAction,
) -> Result<Value, Error> {
    match action {
        OrganizeAction::TagsGet { id } => value(db.task_tags(actor, &id)?),
        OrganizeAction::TagsSet { id, revision, tags } => {
            value(db.set_task_tags(actor, &id, revision, tags)?)
        }
        OrganizeAction::TagList {
            list_id,
            prefix,
            limit,
        } => value(db.list_tags(actor, &list_id, &prefix, limit)?),
        OrganizeAction::TagCreate { list_id, name } => {
            value(db.create_tag(actor, &list_id, &name)?)
        }
        OrganizeAction::TagRename { id, revision, name } => {
            value(db.rename_tag(actor, &id, revision, &name)?)
        }
        OrganizeAction::SectionGet { id } => Ok(json!({"section_id":db.task_section(actor,&id)?})),
        OrganizeAction::SectionSet {
            id,
            revision,
            section_id,
        } => value(db.set_section(actor, &id, revision, section_id)?),
        OrganizeAction::Reparent {
            id,
            revision,
            parent_id,
        } => value(db.reparent_subtree(actor, &id, parent_id, revision)?),
        OrganizeAction::Reorder {
            list_id,
            parent_id,
            revision,
            ids,
        } => value(db.reorder_siblings(actor, &list_id, parent_id, revision, ids)?),
    }
}
pub(crate) fn references(
    db: &mut Store,
    actor: &str,
    action: ReferenceAction,
) -> Result<Value, Error> {
    match action {
        ReferenceAction::Resolve { reference } => value(db.resolve_reference(actor, &reference)?),
        ReferenceAction::Autocomplete { query, limit } => {
            value(db.reference_autocomplete(actor, &query, limit)?)
        }
        ReferenceAction::Unresolved {
            source_kind,
            source_id,
        } => value(db.source_unresolved_references(actor, &source_kind, &source_id)?),
        ReferenceAction::Bind {
            source_kind,
            source_id,
            revision,
            occurrence,
            target_kind,
            target_id,
        } => value(db.bind_reference(
            actor,
            &source_kind,
            &source_id,
            revision,
            occurrence,
            (&target_kind, &target_id),
        )?),
        ReferenceAction::Backlinks {
            target_kind,
            target_id,
        } => value(db.reference_backlinks(actor, &target_kind, &target_id)?),
    }
}
async fn organize_request(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(action): Input<OrganizeAction>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, action.writes())?;
    Ok(Json(s.run(move |db| organize(db, &actor, action)).await?))
}
async fn reference_request(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(action): Input<ReferenceAction>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, action.writes())?;
    Ok(Json(s.run(move |db| references(db, &actor, action)).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TagsQuery {
    #[serde(default)]
    prefix: String,
    #[serde(default = "default_limit")]
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameInput {
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameInput {
    revision: i64,
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TagsInput {
    revision: i64,
    tags: Vec<TagSelector>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionInput {
    revision: i64,
    section_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParentInput {
    revision: i64,
    parent_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderInput {
    revision: i64,
    parent_id: Option<String>,
    ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveQuery {
    reference: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AutocompleteQuery {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindInput {
    revision: i64,
    occurrence: usize,
    target_kind: String,
    target_id: String,
}
async fn tags(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<TagsQuery>,
) -> Result<Json<Value>, ApiError> {
    organize_request(
        State(s),
        h,
        Input(OrganizeAction::TagList {
            list_id: id,
            prefix: q.prefix,
            limit: q.limit,
        }),
    )
    .await
}
async fn create_tag(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(p): Input<NameInput>,
) -> Result<Json<Value>, ApiError> {
    organize_request(
        State(s),
        h,
        Input(OrganizeAction::TagCreate {
            list_id: id,
            name: p.name,
        }),
    )
    .await
}
async fn rename_tag(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(p): Input<RenameInput>,
) -> Result<Json<Value>, ApiError> {
    organize_request(
        State(s),
        h,
        Input(OrganizeAction::TagRename {
            id,
            revision: p.revision,
            name: p.name,
        }),
    )
    .await
}
async fn task_tags(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    organize_request(State(s), h, Input(OrganizeAction::TagsGet { id })).await
}
async fn set_tags(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(p): Input<TagsInput>,
) -> Result<Json<Value>, ApiError> {
    organize_request(
        State(s),
        h,
        Input(OrganizeAction::TagsSet {
            id,
            revision: p.revision,
            tags: p.tags,
        }),
    )
    .await
}
async fn section(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    organize_request(State(s), h, Input(OrganizeAction::SectionGet { id })).await
}
async fn set_section(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(p): Input<SectionInput>,
) -> Result<Json<Value>, ApiError> {
    organize_request(
        State(s),
        h,
        Input(OrganizeAction::SectionSet {
            id,
            revision: p.revision,
            section_id: p.section_id,
        }),
    )
    .await
}
async fn reparent(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(p): Input<ParentInput>,
) -> Result<Json<Value>, ApiError> {
    organize_request(
        State(s),
        h,
        Input(OrganizeAction::Reparent {
            id,
            revision: p.revision,
            parent_id: p.parent_id,
        }),
    )
    .await
}
async fn reorder(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(p): Input<OrderInput>,
) -> Result<Json<Value>, ApiError> {
    organize_request(
        State(s),
        h,
        Input(OrganizeAction::Reorder {
            list_id: id,
            parent_id: p.parent_id,
            revision: p.revision,
            ids: p.ids,
        }),
    )
    .await
}
async fn resolve(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<ResolveQuery>,
) -> Result<Json<Value>, ApiError> {
    reference_request(
        State(s),
        h,
        Input(ReferenceAction::Resolve {
            reference: q.reference,
        }),
    )
    .await
}
async fn autocomplete(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<AutocompleteQuery>,
) -> Result<Json<Value>, ApiError> {
    reference_request(
        State(s),
        h,
        Input(ReferenceAction::Autocomplete {
            query: q.query,
            limit: q.limit,
        }),
    )
    .await
}
async fn unresolved(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    reference_request(
        State(s),
        h,
        Input(ReferenceAction::Unresolved {
            source_kind: kind,
            source_id: id,
        }),
    )
    .await
}
async fn backlinks(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    reference_request(
        State(s),
        h,
        Input(ReferenceAction::Backlinks {
            target_kind: kind,
            target_id: id,
        }),
    )
    .await
}
async fn bind(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((kind, id)): Path<(String, String)>,
    Input(p): Input<BindInput>,
) -> Result<Json<Value>, ApiError> {
    reference_request(
        State(s),
        h,
        Input(ReferenceAction::Bind {
            source_kind: kind,
            source_id: id,
            revision: p.revision,
            occurrence: p.occurrence,
            target_kind: p.target_kind,
            target_id: p.target_id,
        }),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    const TOKEN: &str = "isolated-taxonomy-http-test-token";
    fn app() -> (Router, Arc<Mutex<Store>>, String) {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn.execute_batch("INSERT INTO lists(id,title) VALUES('twin','Inbox');INSERT INTO memberships VALUES('twin','dev');INSERT INTO sections(id,list_id,title,position) VALUES('section','inbox','Plan',0);").unwrap();
        let id = db
            .capture(
                "dev",
                "taxonomy-http",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![crate::core::NewTask {
                        title: "Source".into(),
                        description: "#Inbox".into(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0)
            .id;
        let db = Arc::new(Mutex::new(db));
        (
            router(AppState::new(
                db.clone(),
                TOKEN.into(),
                "http://127.0.0.1:8092".into(),
            )),
            db,
            id,
        )
    }
    async fn send(
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
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    #[tokio::test]
    async fn organization_routes_reuse_cas_location_cycle_and_order_rules() {
        let (app, db, id) = app();
        assert_eq!(
            send(
                &app,
                "GET",
                &format!("/api/v1/tasks/{id}/tags"),
                None,
                false
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        let (status, tag) = send(
            &app,
            "POST",
            "/api/v1/lists/inbox/tags",
            Some(json!({"name":"Launch"})),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, task) = send(
            &app,
            "PUT",
            &format!("/api/v1/tasks/{id}/tags"),
            Some(json!({"revision":1,"tags":[{"id":tag["id"]}]})),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(task["revision"], 2);
        assert_eq!(
            send(
                &app,
                "PUT",
                &format!("/api/v1/tasks/{id}/tags"),
                Some(json!({"revision":1,"tags":[]})),
                true
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let (status, task) = send(
            &app,
            "PUT",
            &format!("/api/v1/tasks/{id}/section"),
            Some(json!({"revision":2,"section_id":"section"})),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(task["revision"], 3);
        assert_eq!(
            send(
                &app,
                "GET",
                &format!("/api/v1/tasks/{id}/section"),
                None,
                true
            )
            .await
            .1["section_id"],
            "section"
        );
        assert_eq!(
            send(
                &app,
                "POST",
                &format!("/api/v1/tasks/{id}/reparent"),
                Some(json!({"revision":3,"parent_id":id})),
                true
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let revision = db
            .lock()
            .unwrap()
            .lists("dev")
            .unwrap()
            .into_iter()
            .find(|l| l.id == "inbox")
            .unwrap()
            .revision;
        assert_eq!(
            send(
                &app,
                "POST",
                "/api/v1/lists/inbox/task-order",
                Some(json!({"revision":revision,"ids":[]})),
                true
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let (status, ordered) = send(
            &app,
            "POST",
            "/api/v1/lists/inbox/task-order",
            Some(json!({"revision":revision,"ids":[id]})),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ordered.as_array().unwrap().len(), 1);
        assert_eq!(
            send(&app, "GET", "/api/v1/lists/private/tags", None, true)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn reference_routes_return_ambiguity_then_bind_exact_choice_with_revision() {
        let (app, _, id) = app();
        let (status, resolved) = send(
            &app,
            "GET",
            "/api/v1/references/resolve?reference=%23Inbox",
            None,
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(resolved["choices"].as_array().unwrap().len(), 2);
        let path = format!("/api/v1/references/task/{id}/bind");
        let body = json!({"revision":1,"occurrence":0,"target_kind":"list","target_id":"inbox"});
        let (status, bound) = send(&app, "POST", &path, Some(body.clone()), true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(bound["source_revision"], 2);
        assert_eq!(bound["unresolved"], json!([]));
        assert_eq!(
            send(&app, "POST", &path, Some(body), true).await.0,
            StatusCode::CONFLICT
        );
        let (_, backlinks) = send(
            &app,
            "GET",
            "/api/v1/references/list/inbox/backlinks",
            None,
            true,
        )
        .await;
        assert_eq!(backlinks[0]["source_id"], id);
        let (_, unresolved) = send(
            &app,
            "GET",
            &format!("/api/v1/references/task/{id}/unresolved"),
            None,
            true,
        )
        .await;
        assert_eq!(unresolved, json!([]));
        let (_, private) = send(
            &app,
            "GET",
            "/api/v1/references/resolve?reference=%23Private",
            None,
            true,
        )
        .await;
        assert_eq!(private["choices"], json!([]));
    }
}
