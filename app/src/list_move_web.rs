//! Owner-reviewed cross-workspace list moves, backed by one common operation.
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
#[derive(Deserialize)]
struct PreviewQuery {
    destination_workspace_id: Option<String>,
}
struct AudienceRow {
    user_id: String,
    before: &'static str,
    after: &'static str,
}
fn role(rank: i64) -> &'static str {
    match rank {
        3 => "Owner",
        2 => "Editor",
        1 => "Viewer",
        _ => "No access",
    }
}
#[derive(Template)]
#[template(path = "list_move.html")]
struct MovePage {
    list: List,
    source: Workspace,
    destinations: Vec<Workspace>,
    destination: String,
    preview: Option<ListWorkspaceMovePreview>,
    audience: Vec<AudienceRow>,
    csrf: String,
}
async fn page(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<PreviewQuery>,
) -> Result<Response, ApiError> {
    let actor = s.auth(&h, false)?;
    let destination = q.destination_workspace_id.unwrap_or_default();
    let selected = destination.clone();
    let (list, source, destinations, preview) = s
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
            let preview = if selected.is_empty() {
                None
            } else {
                Some(db.list_workspace_move_preview(&actor, &id, &selected)?)
            };
            Ok((info.list, source, destinations, preview))
        })
        .await?;
    let audience = preview
        .as_ref()
        .map(|p| {
            p.audience
                .iter()
                .map(|a| AudienceRow {
                    user_id: a.user_id.clone(),
                    before: role(a.before_rank),
                    after: role(a.after_rank),
                })
                .collect()
        })
        .unwrap_or_default();
    let html = MovePage {
        list,
        source,
        destinations,
        destination,
        preview,
        audience,
        csrf: s.csrf_for(&h),
    }
    .render()
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "template",
            "Cannot render list move review".into(),
        )
    })?;
    Ok(Html(html).into_response())
}
#[derive(Deserialize)]
struct MoveForm {
    csrf: String,
    destination_workspace_id: String,
    list_revision: i64,
    source_workspace_revision: i64,
    destination_workspace_revision: i64,
    policy_hash: String,
    #[serde(default)]
    acknowledge_access_change: bool,
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
    let url = format!("/lists/{id}/sharing");
    let mut review_url = reqwest::Url::parse("https://review.invalid/").unwrap();
    review_url
        .path_segments_mut()
        .unwrap()
        .extend(["lists", &id, "move"]);
    review_url
        .query_pairs_mut()
        .append_pair("destination_workspace_id", &f.destination_workspace_id);
    let review_url = format!(
        "{}?{}",
        review_url.path(),
        review_url.query().unwrap_or_default()
    );
    let result = s
        .run(move |db| {
            db.move_list_workspace(
                &actor,
                &id,
                ListWorkspaceMoveInput {
                    destination_workspace_id: f.destination_workspace_id,
                    list_revision: f.list_revision,
                    source_workspace_revision: f.source_workspace_revision,
                    destination_workspace_revision: f.destination_workspace_revision,
                    policy_hash: f.policy_hash,
                    acknowledge_access_change: f.acknowledge_access_change,
                },
            )
        })
        .await;
    match result {
        Ok(_) => Ok(Redirect::to(&url).into_response()),
        Err(error) if error.0 == StatusCode::CONFLICT => {
            let html = MoveConflict {
                message: error.2,
                review_url,
            }
            .render()
            .map_err(|_| Error::Invalid("cannot render move conflict".into()))?;
            Ok((StatusCode::CONFLICT, Html(html)).into_response())
        }
        Err(error) => Err(error),
    }
}
#[derive(Template)]
#[template(
    source = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Review list move · Todoer</title><link rel=\"stylesheet\" href=\"/assets/app.css\"></head><body><main><h1>Move not applied</h1><p>{{ message }}</p><p>No changes were committed. Review the current access policy before trying again.</p><a class=\"primary inline-flex min-h-11\" href=\"{{ review_url }}\">Review a fresh preview</a></main></body></html>",
    ext = "html"
)]
struct MoveConflict {
    message: String,
    review_url: String,
}
