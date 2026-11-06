//! Cross-workspace list moves, backed by one common operation.
use super::*;
use crate::workspace::{ListWorkspaceMoveInput, ListWorkspaceMovePreview, Workspace};
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/lists/{id}/workspace-move/preview", post(preview))
        .route("/api/v1/lists/{id}/workspace-move", post(apply))
        .route("/lists/{id}/move", get(page).post(apply_form))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Destination {
    destination_workspace_id: String,
}
async fn preview(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<Destination>,
) -> Result<Json<ListWorkspaceMovePreview>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| {
            db.list_workspace_move_preview(&actor, &id, &input.destination_workspace_id)
        })
        .await?,
    ))
}
async fn apply(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<ListWorkspaceMoveInput>,
) -> Result<Json<List>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.move_list_workspace(&actor, &id, input))
            .await?,
    ))
}
#[derive(Template)]
#[template(path = "list_move.html")]
struct MovePage {
    list: List,
    source: Workspace,
    destinations: Vec<Workspace>,
    csrf: String,
    moved: bool,
    error: String,
}
async fn render_page(
    s: &AppState,
    h: &HeaderMap,
    id: String,
    moved: bool,
    error: String,
) -> Result<Response, ApiError> {
    let actor = s.auth(h, false)?;
    let (list, source, destinations) = s
        .run(move |db| {
            let info = db.list_info(&actor, &id)?;
            let source_id = info
                .workspace_id
                .ok_or_else(|| Error::Invalid("list has no source workspace".into()))?;
            if db.workspace_permission(&actor, &source_id)? < 3 {
                return Err(Error::Forbidden);
            }
            let source = db.workspace(&actor, &source_id)?;
            let mut destinations = Vec::new();
            for w in db.workspaces(&actor)? {
                if w.id != source_id && db.workspace_permission(&actor, &w.id)? == 3 {
                    destinations.push(w);
                }
            }
            Ok((info.list, source, destinations))
        })
        .await?;
    let html = MovePage {
        list,
        source,
        destinations,
        csrf: s.csrf_for(h),
        moved,
        error,
    }
    .render()
    .map_err(|_| Error::Invalid("cannot render list move".into()))?;
    Ok(Html(html).into_response())
}
async fn page(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    render_page(&s, &h, id, false, String::new()).await
}
#[derive(Deserialize)]
struct MoveForm {
    csrf: String,
    destination_workspace_id: String,
}
async fn apply_form(
    State(s): State<AppState>,
    mut h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<MoveForm>,
) -> Result<Response, ApiError> {
    if !s.origin_ok(&h) {
        return Err(Error::Forbidden.into());
    }
    h.insert(
        "x-csrf-token",
        f.csrf.parse().map_err(|_| Error::Forbidden)?,
    );
    let actor = s.auth(&h, true)?;
    let list_id = id.clone();
    let result = s
        .run(move |db| {
            let preview =
                db.list_workspace_move_preview(&actor, &list_id, &f.destination_workspace_id)?;
            db.move_list_workspace(
                &actor,
                &list_id,
                ListWorkspaceMoveInput {
                    destination_workspace_id: f.destination_workspace_id,
                    list_revision: preview.list_revision,
                    source_workspace_revision: preview.source_workspace_revision,
                    destination_workspace_revision: preview.destination_workspace_revision,
                    policy_hash: preview.policy_hash,
                    acknowledge_access_change: true,
                },
            )
        })
        .await;
    match result {
        Ok(_) => render_page(&s, &h, id, true, String::new()).await,
        Err(error) => {
            let mut response = render_page(&s, &h, id, false, error.2).await?;
            *response.status_mut() = error.0;
            Ok(response)
        }
    }
}
