use super::*;
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct ActivityQuery {
    before: Option<i64>,
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/tasks/{id}/activity", get(task))
        .route("/api/v1/workspaces/{id}/activity", get(workspace))
}
async fn task(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<ActivityQuery>,
) -> Result<Json<crate::activity::ActivityPage>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| db.entity_activity(&actor, "task", &id, q.before))
            .await?,
    ))
}
async fn workspace(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<ActivityQuery>,
) -> Result<Json<crate::activity::ActivityPage>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| db.entity_activity(&actor, "workspace", &id, q.before))
            .await?,
    ))
}
