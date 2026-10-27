//! Native forms and JSON routes use the same live workspace authorization core.
use super::filters;
use super::*;
use crate::workspace::{
    CreateList, ListChange, ListRole, Section, SectionChange, SectionInput, Workspace,
    WorkspaceChange, WorkspaceInput, WorkspaceList, WorkspaceRole,
};
use axum::http::HeaderValue;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces",
            get(legacy_overview).post(workspace_create_form),
        )
        .route("/spaces", get(overview).post(workspace_create_form))
        .route(
            "/workspaces/{id}",
            get(space_redirect).post(workspace_edit_form),
        )
        .route(
            "/api/v1/workspaces/{id}/lifecycle",
            post(workspace_lifecycle),
        )
        .route("/space/{id}", get(space_page))
        .route("/workspaces/{id}/settings", get(workspace_page))
        .route("/workspaces/{id}/lists", post(list_create_form))
        .route("/workspaces/{id}/members", post(member_form))
        .route("/workspaces/{id}/members/revoke", post(member_revoke_form))
        .route("/workspaces/{id}/lists/order", post(list_order_form))
        .route(
            "/lists/{id}/sharing",
            get(sharing_page).post(list_edit_form),
        )
        .route("/lists/{id}/sharing/access", post(access_form))
        .route("/lists/{id}/sharing/grants", post(grant_form))
        .route("/lists/{id}/sharing/revoke", post(grant_revoke_form))
        .route("/lists/{id}/sections", post(section_create_form))
        .route("/lists/{id}/sections/order", post(section_order_form))
        .route("/sections/{id}/edit", post(section_edit_form))
        .route(
            "/api/v1/workspaces",
            get(api_workspaces).post(api_create_workspace),
        )
        .route(
            "/api/v1/workspaces/{id}",
            get(api_workspace).patch(api_update_workspace),
        )
        .route("/api/v1/workspaces/{id}/people", get(api_people))
        .route(
            "/api/v1/workspaces/{id}/members",
            get(api_members).post(api_member),
        )
        .route(
            "/api/v1/workspaces/{id}/members/revoke",
            post(api_revoke_member),
        )
        .route(
            "/api/v1/workspaces/{id}/lists/reorder",
            post(api_reorder_lists),
        )
        .route("/api/v1/lists", post(api_create_list))
        .route("/api/v1/lists/{id}", get(api_list).patch(api_update_list))
        .route("/api/v1/lists/{id}/sharing", get(api_sharing))
        .route("/api/v1/lists/{id}/access", post(api_access))
        .route("/api/v1/lists/{id}/grants", post(api_grant))
        .route("/api/v1/lists/{id}/grants/revoke", post(api_revoke_grant))
        .route(
            "/api/v1/lists/{id}/sections",
            get(api_sections).post(api_create_section),
        )
        .route(
            "/api/v1/lists/{id}/sections/reorder",
            post(api_reorder_sections),
        )
        .route("/api/v1/sections/{id}", patch(api_update_section))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberGrant {
    revision: i64,
    user_id: String,
    role: WorkspaceRole,
    #[serde(default)]
    acknowledge_access_change: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectGrant {
    revision: i64,
    user_id: String,
    role: ListRole,
    #[serde(default)]
    acknowledge_access_change: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revoke {
    revision: i64,
    user_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessChange {
    revision: i64,
    restricted: bool,
    #[serde(default)]
    acknowledge_access_change: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reorder {
    revision: i64,
    ids: Vec<String>,
}

async fn api_workspaces(
    State(s): State<AppState>,
    h: HeaderMap,
) -> Result<Json<Vec<Workspace>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.workspaces(&actor)).await?))
}
async fn api_workspace(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Workspace>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.workspace(&actor, &id)).await?))
}
async fn api_create_workspace(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<WorkspaceInput>,
) -> Result<Json<Workspace>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.create_workspace(&actor, input)).await?,
    ))
}
async fn api_update_workspace(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<WorkspaceChange>,
) -> Result<Json<Workspace>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.update_workspace(&actor, &id, input))
            .await?,
    ))
}

#[derive(Deserialize)]
struct PeopleQuery {
    #[serde(default)]
    q: String,
}
async fn api_people(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PeopleQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| {
        if db.workspace_permission(&actor, &id)? < 3 { return Err(Error::Forbidden); }
        let q = query.q.trim().to_lowercase();
        let mut stmt = db.conn.prepare("SELECT DISTINCT u.id,u.name,coalesce(ai.email,'') FROM users u LEFT JOIN auth_identities ai ON ai.user_id=u.id WHERE lower(ai.email)=?1 OR (EXISTS(SELECT 1 FROM workspace_members m WHERE m.workspace_id=?2 AND m.user_id=u.id) AND (instr(lower(ai.email),?1)>0 OR instr(lower(u.name),?1)>0)) ORDER BY ai.email LIMIT 10")?;
        let values = stmt.query_map(rusqlite::params![q,id], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"email":r.get::<_,String>(2)?})))?.collect::<Result<Vec<_>,_>>()?;
        Ok(values)
    }).await?))
}
async fn api_members(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<crate::workspace::WorkspaceMember>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| db.workspace_members(&actor, &id)).await?,
    ))
}
async fn api_member(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<MemberGrant>,
) -> Result<Json<Workspace>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| {
            db.set_workspace_member(
                &actor,
                &id,
                &input.user_id,
                input.role,
                input.revision,
                input.acknowledge_access_change,
            )
        })
        .await?,
    ))
}
async fn api_revoke_member(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<Revoke>,
) -> Result<Json<Workspace>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.remove_workspace_member(&actor, &id, &input.user_id, input.revision))
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateListWithColor {
    workspace_id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    restricted: bool,
    #[serde(default)]
    color: Option<String>,
}
async fn api_create_list(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<CreateListWithColor>,
) -> Result<Json<List>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| {
            db.create_list_with_color(
                &actor,
                CreateList {
                    workspace_id: input.workspace_id,
                    title: input.title,
                    description: input.description,
                    restricted: input.restricted,
                },
                input.color,
            )
        })
        .await?,
    ))
}
async fn api_list(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<WorkspaceList>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.list_info(&actor, &id)).await?))
}
async fn api_update_list(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<ListChange>,
) -> Result<Json<List>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.update_list(&actor, &id, input)).await?,
    ))
}
async fn api_access(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<AccessChange>,
) -> Result<Json<List>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| {
            db.set_list_restricted(
                &actor,
                &id,
                input.restricted,
                input.revision,
                input.acknowledge_access_change,
            )
        })
        .await?,
    ))
}
async fn api_grant(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<DirectGrant>,
) -> Result<Json<List>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| {
            db.grant_list(
                &actor,
                &id,
                &input.user_id,
                input.role,
                input.revision,
                input.acknowledge_access_change,
            )
        })
        .await?,
    ))
}
async fn api_revoke_grant(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<Revoke>,
) -> Result<Json<List>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.revoke_list(&actor, &id, &input.user_id, input.revision))
            .await?,
    ))
}
async fn api_sections(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<Section>>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.sections(&actor, &id)).await?))
}
async fn api_create_section(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<SectionInput>,
) -> Result<Json<Section>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.create_section(&actor, &id, input))
            .await?,
    ))
}
async fn api_update_section(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<SectionChange>,
) -> Result<Json<Section>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.update_section(&actor, &id, input))
            .await?,
    ))
}
async fn api_reorder_lists(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<Reorder>,
) -> Result<Json<Workspace>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.reorder_lists(&actor, &id, input.revision, &input.ids))
            .await?,
    ))
}
async fn api_reorder_sections(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<Reorder>,
) -> Result<Json<List>, ApiError> {
    let actor = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.reorder_sections(&actor, &id, input.revision, &input.ids))
            .await?,
    ))
}

#[derive(Debug, Clone, serde::Serialize)]
struct GrantView {
    user_id: String,
    role: String,
}
fn direct_grants(db: &Store, actor: &str, id: &str) -> Result<Vec<GrantView>, Error> {
    let info = db.list_info(actor, id)?;
    let workspace = info.workspace_id.ok_or(Error::Forbidden)?;
    if db.workspace_permission(actor, &workspace)? < 3 {
        return Err(Error::Forbidden);
    }
    let mut stmt=db.conn.prepare("SELECT m.user_id,COALESCE(r.role,'editor') FROM memberships m LEFT JOIN membership_roles r ON r.list_id=m.list_id AND r.user_id=m.user_id WHERE m.list_id=?1 ORDER BY m.user_id")?;
    Ok(stmt
        .query_map([id], |r| {
            Ok(GrantView {
                user_id: r.get(0)?,
                role: r.get(1)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}
async fn api_sharing(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, false)?;
    Ok(Json(s.run(move|db|Ok(json!({"list":db.list_info(&actor,&id)?,"direct_grants":direct_grants(db,&actor,&id)?}))).await?))
}

#[derive(Template)]
#[template(path = "workspaces.html")]
struct Overview {
    sidebar: super::Sidebar,
    slugs: crate::slugs::EntitySlugs,
    csrf: String,
    workspaces: Vec<Workspace>,
    archived: std::collections::HashSet<String>,
    owners: std::collections::HashSet<String>,
    page: usize,
    has_next: bool,
    can_create: bool,
}
#[derive(Template)]
#[template(path = "workspace.html")]
struct WorkspacePage {
    sidebar: super::Sidebar,
    slugs: crate::slugs::EntitySlugs,
    csrf: String,
    workspace: Workspace,
    lists: Vec<WorkspaceList>,
    members: Vec<GrantView>,
    can_edit: bool,
    owner: bool,
}
#[derive(Template)]
#[template(path = "sharing.html")]
struct SharingPage {
    color: String,
    csrf: String,
    info: WorkspaceList,
    sections: Vec<Section>,
    grants: Vec<GrantView>,
    can_edit: bool,
    owner: bool,
}
fn html<T: Template>(template: T) -> Result<Response, ApiError> {
    let rendered = template.render().map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "template",
            "Render failed".into(),
        )
    })?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Html(rendered)).into_response())
}
#[derive(Deserialize)]
struct WorkspacePageQuery {
    page: Option<usize>,
}
async fn legacy_overview(Query(q): Query<WorkspacePageQuery>) -> Redirect {
    Redirect::to(&format!(
        "/spaces?page={}",
        q.page.unwrap_or(1).clamp(1, 10000)
    ))
}
async fn overview(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<WorkspacePageQuery>,
) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let csrf = s.csrf_for(&h);
    let (sidebar, slugs, workspaces, _lists, can_create) = s
        .run(move |db| {
            Ok((
                super::build_sidebar(db, &actor, "", csrf)?,
                db.entity_slugs(&actor)?,
                db.workspaces(&actor)?,
                db.lists(&actor)?,
                crate::core::actor_user(&db.conn, &actor, 3).is_ok(),
            ))
        })
        .await?;
    let mut archived = std::collections::HashSet::new();
    let mut owners = std::collections::HashSet::new();
    let actor = s.auth(&h, false)?;
    let states = s
        .run(move |db| {
            let mut q = db
                .conn
                .prepare("SELECT workspace_id,archived,deleted_at FROM workspace_lifecycle")?;
            let states = q
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, bool>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let owners = db
                .workspaces(&actor)?
                .into_iter()
                .filter(|w| db.workspace_permission(&actor, &w.id).unwrap_or(0) >= 3)
                .map(|w| w.id)
                .collect::<std::collections::HashSet<_>>();
            Ok((states, owners))
        })
        .await?;
    owners.extend(states.1);
    let deleted = states
        .0
        .iter()
        .filter(|x| x.2.is_some())
        .map(|x| x.0.clone())
        .collect::<std::collections::HashSet<_>>();
    for (id, a, _) in states.0 {
        if a {
            archived.insert(id);
        }
    }
    let visible = workspaces
        .into_iter()
        .filter(|w| !deleted.contains(&w.id))
        .collect::<Vec<_>>();
    let page = q.page.unwrap_or(1).clamp(1, 10000);
    let has_next = visible.len() > page * 20;
    let workspaces = visible.into_iter().skip((page - 1) * 20).take(20).collect();
    let mut sidebar = sidebar;
    sidebar.lists.clear();
    sidebar.space_overview = true;
    html(Overview {
        sidebar,
        slugs,
        csrf: s.csrf_for(&h),
        workspaces,
        archived,
        owners,
        page,
        has_next,
        can_create,
    })
}
async fn space_redirect(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let target = s
        .run(move |db| {
            let id = crate::defaults::resolve(&db.conn, "workspace", &id)?;
            db.workspace(&actor, &id)?;
            crate::defaults::public_id(&db.conn, "workspace", &id)
        })
        .await?;
    Ok(Redirect::to(&format!("/space/{target}")).into_response())
}
#[derive(Template)]
#[template(path = "space-overview.html")]
struct SpaceOverview {
    sidebar: super::Sidebar,
    workspace: Workspace,
    reference: String,
    lists: Vec<SpaceListCard>,
    open: i64,
    completed: i64,
    can_edit: bool,
}
struct SpaceListCard {
    id: String,
    title: String,
    description: String,
    color: String,
    open: i64,
    completed: i64,
    total: i64,
    p0: i64,
    p1: i64,
    favorite: bool,
}
async fn space_page(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let requested = id.clone();
    let csrf = s.csrf_for(&h);
    let overview = s.run(move |db| {
        let id = crate::defaults::resolve(&db.conn, "workspace", &id)?;
        let workspace = db.workspace(&actor, &id)?;
        let rank = db.workspace_permission(&actor, &id)?;
        let reference = crate::defaults::public_id(&db.conn, "workspace", &id)?;
        // Aggregate only lists visible through the live actor view. Hidden lists
        // and their task totals never contribute to this overview.
        let mut query = db.conn.prepare("SELECT l.id,l.title,l.description,coalesce(l.color_hex,l.color,'blue'),sum(CASE WHEN t.complete=0 THEN 1 ELSE 0 END),sum(CASE WHEN t.complete=1 THEN 1 ELSE 0 END),sum(CASE WHEN t.complete=0 AND json_extract(s.document,'$.priority')=0 THEN 1 ELSE 0 END),sum(CASE WHEN t.complete=0 AND json_extract(s.document,'$.priority')=1 THEN 1 ELSE 0 END) FROM lists l JOIN actor_list_access a ON a.list_id=l.id AND a.actor_id=?1 AND a.rank>=1 LEFT JOIN tasks t ON t.list_id=l.id AND t.deleted_at IS NULL LEFT JOIN task_schedule s ON s.task_id=t.id WHERE l.workspace_id=?2 AND l.archived=0 GROUP BY l.id ORDER BY l.position,l.id")?;
        let mut lists = query.query_map(rusqlite::params![actor,id], |r| {
            let description: String = r.get(2)?;
            let open: i64 = r.get(4)?; let completed: i64 = r.get(5)?;
            Ok(SpaceListCard { id:r.get(0)?, title:r.get(1)?, description: if description.chars().count()>160 { format!("{}…",description.chars().take(160).collect::<String>()) } else {description},color:r.get(3)?,open,completed,total:open+completed,p0:r.get(6)?,p1:r.get(7)?,favorite:false })
        })?.collect::<Result<Vec<_>,_>>()?;
        let mut sidebar = super::build_sidebar(db,&actor,lists.first().map(|l|l.id.as_str()).unwrap_or(""),csrf)?;
        sidebar.current_list_id.clear();
        sidebar.current_workspace_id = id;
        sidebar.current_workspace_label = format!("@{}", db.entity_slugs(&actor)?.workspaces.get(&workspace.id).unwrap_or(&workspace.title));
        for card in &mut lists { card.favorite = sidebar.favorite_ids.contains(&card.id); }
        let open = lists.iter().map(|l|l.open).sum();
        let completed = lists.iter().map(|l|l.completed).sum();
        Ok(SpaceOverview { sidebar, workspace, reference, lists, open, completed, can_edit:rank>=3 })
    }).await?;
    if requested != overview.reference {
        return Ok(Redirect::to(&format!("/space/{}", overview.reference)).into_response());
    }
    html(overview)
}
async fn workspace_page(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let csrf = s.csrf_for(&h);
    let (sidebar, slugs, workspace, lists, members, rank) = s
        .run(move |db| {
            let workspace = db.workspace(&actor, &id)?;
            let rank = db.workspace_permission(&actor, &id)?;
            let mut lists = Vec::new();
            for list in db.lists(&actor)? {
                let info = db.list_info(&actor, &list.id)?;
                if info.workspace_id.as_deref() == Some(&id) {
                    lists.push(info);
                }
            }
            lists.sort_by(|a, b| (a.position, &a.list.id).cmp(&(b.position, &b.list.id)));
            let members = if rank >= 3 {
                db.workspace_members(&actor, &id)?
                    .into_iter()
                    .map(|m| GrantView {
                        user_id: m.user_id,
                        role: match m.role {
                            WorkspaceRole::Owner => "owner",
                            WorkspaceRole::Editor => "editor",
                            WorkspaceRole::Viewer => "viewer",
                        }
                        .into(),
                    })
                    .collect()
            } else {
                vec![]
            };
            let slugs = db.entity_slugs(&actor)?;
            let mut sidebar = super::build_sidebar(
                db,
                &actor,
                lists.first().map(|l| l.list.id.as_str()).unwrap_or(""),
                csrf,
            )?;
            sidebar.current_workspace_label = format!(
                "@{}",
                slugs
                    .workspaces
                    .get(&workspace.id)
                    .unwrap_or(&workspace.title)
            );
            Ok((sidebar, slugs, workspace, lists, members, rank))
        })
        .await?;
    html(WorkspacePage {
        sidebar,
        slugs,
        csrf: s.csrf_for(&h),
        workspace,
        lists,
        members,
        can_edit: rank >= 2,
        owner: rank >= 3,
    })
}
async fn sharing_page(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let Ok(actor) = s.auth(&h, false) else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let (info, sections, grants, rank, owner, color) = s
        .run(move |db| {
            let info = db.list_info(&actor, &id)?;
            let rank = db.list_permission(&actor, &id)?;
            let owner = match &info.workspace_id {
                Some(w) => db.workspace_permission(&actor, w)? >= 3,
                None => false,
            };
            let grants = if owner {
                direct_grants(db, &actor, &id)?
            } else {
                vec![]
            };
            Ok((
                info,
                db.sections(&actor, &id)?,
                grants,
                rank,
                owner,
                db.list_color(&actor, &id)?.unwrap_or_default(),
            ))
        })
        .await?;
    html(SharingPage {
        color,
        csrf: s.csrf_for(&h),
        info,
        sections,
        grants,
        can_edit: rank >= 2,
        owner,
    })
}

struct FormError(ApiError);
impl From<ApiError> for FormError {
    fn from(value: ApiError) -> Self {
        Self(value)
    }
}
#[derive(Template)]
#[template(
    source = "<!doctype html><html lang=\"en\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Changes not saved · Todoer</title><link rel=\"stylesheet\" href=\"/assets/app.css\"><main class=\"login\"><h1>Changes not saved</h1><p role=\"alert\">{{ message }}</p><p>Use your browser’s Back button to return to your entered values. Reload the settings page before retrying a stale revision.</p><a class=\"primary\" href=\"/workspaces\">Open workspaces</a></main></html>",
    ext = "html"
)]
struct FormErrorPage {
    message: String,
}
impl IntoResponse for FormError {
    fn into_response(self) -> Response {
        let page = FormErrorPage { message: self.0.2 };
        (
            self.0.0,
            [(header::CACHE_CONTROL, "no-store")],
            Html(
                page.render()
                    .unwrap_or_else(|_| "Changes were not saved.".into()),
            ),
        )
            .into_response()
    }
}
fn form_actor(s: &AppState, h: &HeaderMap, csrf: &str) -> Result<String, ApiError> {
    if !s.origin_ok(h) || csrf != s.csrf_for(h) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "csrf",
            "Reload before saving".into(),
        ));
    }
    let mut headers = h.clone();
    headers.insert(
        "x-csrf-token",
        HeaderValue::from_str(csrf)
            .map_err(|_| ApiError(StatusCode::FORBIDDEN, "csrf", "Invalid CSRF token".into()))?,
    );
    s.auth(&headers, true)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameForm {
    csrf: String,
    title: String,
    #[serde(default)]
    description: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditForm {
    csrf: String,
    revision: i64,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    archived: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewListForm {
    csrf: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    restricted: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberForm {
    csrf: String,
    revision: i64,
    user_id: String,
    role: WorkspaceRole,
    #[serde(default)]
    acknowledge_access_change: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantForm {
    csrf: String,
    revision: i64,
    user_id: String,
    role: ListRole,
    #[serde(default)]
    acknowledge_access_change: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevokeForm {
    csrf: String,
    revision: i64,
    user_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessForm {
    csrf: String,
    revision: i64,
    restricted: bool,
    #[serde(default)]
    acknowledge_access_change: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Direction {
    Up,
    Down,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderForm {
    csrf: String,
    revision: i64,
    id: String,
    direction: Direction,
}
fn moved(ids: &mut [String], id: &str, direction: Direction) -> Result<(), Error> {
    let index = ids.iter().position(|x| x == id).ok_or(Error::NotFound)?;
    let next = match direction {
        Direction::Up => index.checked_sub(1),
        Direction::Down => (index + 1 < ids.len()).then_some(index + 1),
    };
    if let Some(next) = next {
        ids.swap(index, next);
    }
    Ok(())
}
async fn workspace_create_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Form(f): Form<NameForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let result = s
        .run(move |db| {
            db.create_workspace(
                &actor,
                WorkspaceInput {
                    title: f.title,
                    description: f.description,
                },
            )
        })
        .await?;
    Ok(Redirect::to(&format!("/workspaces/{}", result.id)))
}
async fn workspace_edit_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<EditForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let redirect = format!("/workspaces/{id}");
    s.run(move |db| {
        db.update_workspace(
            &actor,
            &id,
            WorkspaceChange {
                revision: f.revision,
                title: Some(f.title),
                description: Some(f.description),
            },
        )
    })
    .await?;
    Ok(Redirect::to(&redirect))
}
async fn list_create_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<NewListForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let list = s
        .run(move |db| {
            db.create_list(
                &actor,
                CreateList {
                    workspace_id: id,
                    title: f.title,
                    description: f.description,
                    restricted: f.restricted,
                },
            )
        })
        .await?;
    Ok(Redirect::to(&format!("/lists/{}/sharing", list.id)))
}
async fn member_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<MemberForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let redirect = format!("/workspaces/{id}");
    s.run(move |db| {
        db.set_workspace_member(
            &actor,
            &id,
            &f.user_id,
            f.role,
            f.revision,
            f.acknowledge_access_change,
        )
    })
    .await?;
    Ok(Redirect::to(&redirect))
}
async fn member_revoke_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<RevokeForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let redirect = format!("/workspaces/{id}");
    s.run(move |db| db.remove_workspace_member(&actor, &id, &f.user_id, f.revision))
        .await?;
    Ok(Redirect::to(&redirect))
}
async fn list_edit_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<EditForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let redirect = format!("/lists/{id}/sharing");
    s.run(move |db| {
        db.update_list(
            &actor,
            &id,
            ListChange {
                color: None,
                revision: f.revision,
                title: Some(f.title),
                description: Some(f.description),
                archived: f.archived,
            },
        )
    })
    .await?;
    Ok(Redirect::to(&redirect))
}
async fn access_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<AccessForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let redirect = format!("/lists/{id}/sharing");
    s.run(move |db| {
        db.set_list_restricted(
            &actor,
            &id,
            f.restricted,
            f.revision,
            f.acknowledge_access_change,
        )
    })
    .await?;
    Ok(Redirect::to(&redirect))
}
async fn grant_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<GrantForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let redirect = format!("/lists/{id}/sharing");
    s.run(move |db| {
        db.grant_list(
            &actor,
            &id,
            &f.user_id,
            f.role,
            f.revision,
            f.acknowledge_access_change,
        )
    })
    .await?;
    Ok(Redirect::to(&redirect))
}
async fn grant_revoke_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<RevokeForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let redirect = format!("/lists/{id}/sharing");
    s.run(move |db| db.revoke_list(&actor, &id, &f.user_id, f.revision))
        .await?;
    Ok(Redirect::to(&redirect))
}
async fn section_create_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<NameForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let redirect = format!("/lists/{id}/sharing");
    s.run(move |db| {
        db.create_section(
            &actor,
            &id,
            SectionInput {
                title: f.title,
                description: f.description,
            },
        )
    })
    .await?;
    Ok(Redirect::to(&redirect))
}
async fn section_edit_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<EditForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let section = s
        .run(move |db| {
            db.update_section(
                &actor,
                &id,
                SectionChange {
                    revision: f.revision,
                    title: Some(f.title),
                    description: Some(f.description),
                },
            )
        })
        .await?;
    Ok(Redirect::to(&format!("/lists/{}/sharing", section.list_id)))
}
async fn list_order_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<OrderForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let redirect = format!("/workspaces/{id}");
    s.run(move |db| {
        let _ = db.workspace(&actor, &id)?;
        if db.workspace_permission(&actor, &id)? < 3 {
            return Err(Error::Forbidden);
        }
        let mut lists = Vec::new();
        for list in db.lists(&actor)? {
            let info = db.list_info(&actor, &list.id)?;
            if info.workspace_id.as_deref() == Some(&id) {
                lists.push(info);
            }
        }
        lists.sort_by(|a, b| (a.position, &a.list.id).cmp(&(b.position, &b.list.id)));
        let mut ids: Vec<String> = lists.into_iter().map(|l| l.list.id).collect();
        moved(&mut ids, &f.id, f.direction)?;
        db.reorder_lists(&actor, &id, f.revision, &ids)
    })
    .await?;
    Ok(Redirect::to(&redirect))
}
async fn section_order_form(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<OrderForm>,
) -> Result<Redirect, FormError> {
    let actor = form_actor(&s, &h, &f.csrf)?;
    let redirect = format!("/lists/{id}/sharing");
    s.run(move |db| {
        let mut ids: Vec<String> = db
            .sections(&actor, &id)?
            .into_iter()
            .map(|x| x.id)
            .collect();
        moved(&mut ids, &f.id, f.direction)?;
        db.reorder_sections(&actor, &id, f.revision, &ids)
    })
    .await?;
    Ok(Redirect::to(&redirect))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    const TOKEN: &str = "synthetic-workspace-route-token";
    fn app() -> (Router, AppState) {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn
            .execute(
                "INSERT INTO users(id,name) VALUES('guest','Synthetic guest')",
                [],
            )
            .unwrap();
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            TOKEN.into(),
            "http://127.0.0.1:8092".into(),
        );
        (routes().with_state(state.clone()), state)
    }
    async fn json_request(
        app: &Router,
        method: &str,
        path: &str,
        value: Value,
        authorized: bool,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if authorized {
            request = request.header("authorization", format!("Bearer {TOKEN}"));
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::from(value.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&body).unwrap())
    }
    async fn page(app: &Router, path: &str) -> String {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("authorization", format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn member_email_lookup_requires_owner_and_does_not_enumerate_strangers() {
        let (app, state) = app();
        let id = state.run(|db| {
            db.conn.execute("INSERT INTO auth_identities(issuer,subject,user_id,email) VALUES('test','guest','guest','guest@example.test')",[])?;
            Ok(db.create_workspace("dev", WorkspaceInput { title:"Lookup".into(), description:String::new() })?.id)
        }).await.unwrap();
        let path = format!("/api/v1/workspaces/{id}/people?q=guest");
        assert_eq!(
            json_request(&app, "GET", &path, json!({}), false).await.0,
            StatusCode::UNAUTHORIZED
        );
        let (status, result) = json_request(&app, "GET", &path, json!({}), true).await;
        assert_eq!(status, StatusCode::OK);
        assert!(result.as_array().unwrap().is_empty());
        let (_, result) = json_request(
            &app,
            "GET",
            &format!("{path}%40example.test"),
            json!({}),
            true,
        )
        .await;
        assert_eq!(result[0]["id"], "guest");
        let (status, _) = json_request(
            &app,
            "GET",
            "/api/v1/workspaces/unknown/people?q=guest%40example.test",
            json!({}),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    #[tokio::test]
    async fn sharing_acknowledgement_and_revisions_are_enforced_through_rest() {
        let (app, _state) = app();
        assert_eq!(
            json_request(&app, "GET", "/api/v1/workspaces", json!({}), false)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        let (status, w) = json_request(
            &app,
            "POST",
            "/api/v1/workspaces",
            json!({"title":"Route workspace"}),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let id = w["id"].as_str().unwrap();
        let path = format!("/api/v1/workspaces/{id}/members");
        let mutation = json!({"revision":1,"user_id":"other","role":"viewer"});
        assert_eq!(
            json_request(&app, "POST", &path, mutation.clone(), true)
                .await
                .0,
            StatusCode::CONFLICT
        );
        let mut acknowledged = mutation;
        acknowledged["acknowledge_access_change"] = json!(true);
        assert_eq!(
            json_request(&app, "POST", &path, acknowledged, true)
                .await
                .0,
            StatusCode::OK
        );
        let (_, list) = json_request(
            &app,
            "POST",
            "/api/v1/lists",
            json!({"workspace_id":id,"title":"Restricted","restricted":true}),
            true,
        )
        .await;
        let list_id = list["id"].as_str().unwrap();
        let access = format!("/api/v1/lists/{list_id}/access");
        assert_eq!(
            json_request(
                &app,
                "POST",
                &access,
                json!({"revision":1,"restricted":false}),
                true
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let (_, info) = json_request(
            &app,
            "GET",
            &format!("/api/v1/lists/{list_id}"),
            json!({}),
            true,
        )
        .await;
        assert_eq!(info["restricted"], true);
        assert_eq!(info["revision"], 1);
        assert_eq!(
            json_request(
                &app,
                "POST",
                &access,
                json!({"revision":1,"restricted":false,"acknowledge_access_change":true}),
                true
            )
            .await
            .0,
            StatusCode::OK
        );
        let grants = format!("/api/v1/lists/{list_id}/grants");
        assert_eq!(
            json_request(
                &app,
                "POST",
                &grants,
                json!({"revision":2,"user_id":"guest","role":"editor"}),
                true
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(json_request(&app,"POST",&grants,json!({"revision":2,"user_id":"guest","role":"editor","acknowledge_access_change":true}),true).await.0,StatusCode::OK);
        assert_eq!(
            json_request(
                &app,
                "PATCH",
                &format!("/api/v1/lists/{list_id}"),
                json!({"revision":1,"title":"Stale"}),
                true
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let html = page(&app, &format!("/lists/{list_id}/sharing")).await;
        assert!(html.contains("guest"));
        assert!(html.contains("acknowledge_access_change"));
    }
    #[tokio::test]
    async fn native_forms_require_csrf_and_origin_and_work_without_javascript() {
        let (app, state) = app();
        let send = |origin: &str, csrf: &str| {
            Request::builder()
                .method("POST")
                .uri("/workspaces")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .header("origin", origin)
                .body(Body::from(format!(
                    "csrf={csrf}&title=Native+workspace&description=Saved+without+JavaScript"
                )))
                .unwrap()
        };
        let response = app
            .clone()
            .oneshot(send("http://untrusted.invalid", &state.csrf))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = app
            .clone()
            .oneshot(send("http://127.0.0.1:8092", "wrong"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(
            state
                .store
                .lock()
                .unwrap()
                .workspaces("dev")
                .unwrap()
                .is_empty()
        );
        let response = app
            .clone()
            .oneshot(send("http://127.0.0.1:8092", &state.csrf))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response.headers()[header::LOCATION].to_str().unwrap();
        let html = page(&app, &format!("{location}/settings")).await;
        assert!(html.contains("Native workspace"));
        assert!(html.contains("Saved without JavaScript"));
        assert!(html.contains("method=\"post\""));
        assert!(html.contains("Create a list"));
        assert!(html.contains("method=\"post\"")); // Native forms remain usable without JavaScript.
        let overview = page(&app, "/spaces").await;
        assert!(overview.contains("Native workspace"));
    }
    #[tokio::test]
    async fn viewer_routes_hide_mutations_and_reject_writes() {
        let (app, state) = app();
        let (workspace, list) = {
            let mut db = state.store.lock().unwrap();
            let w = db
                .create_workspace(
                    "other",
                    WorkspaceInput {
                        title: "Viewer workspace".into(),
                        description: String::new(),
                    },
                )
                .unwrap();
            let w = db
                .set_workspace_member(
                    "other",
                    &w.id,
                    "dev",
                    WorkspaceRole::Viewer,
                    w.revision,
                    true,
                )
                .unwrap();
            let list = db
                .create_list(
                    "other",
                    CreateList {
                        workspace_id: w.id.clone(),
                        title: "Read only".into(),
                        description: String::new(),
                        restricted: false,
                    },
                )
                .unwrap();
            (w, list)
        };
        let html = page(&app, &format!("/workspaces/{}/settings", workspace.id)).await;
        assert!(!html.contains(&format!("action=\"/workspaces/{}/lists\"", workspace.id)));
        assert!(!html.contains("Save member role"));
        let html = page(&app, &format!("/lists/{}/sharing", list.id)).await;
        assert!(html.contains("read-only access"));
        assert!(!html.contains("Save list details"));
        assert!(!html.contains("Save direct share"));
        assert_eq!(
            json_request(
                &app,
                "POST",
                &format!("/api/v1/lists/{}/sections", list.id),
                json!({"title":"Denied"}),
                true
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            json_request(
                &app,
                "POST",
                "/api/v1/workspaces",
                json!({"title":"Bad","owner":"other"}),
                true
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LifecycleInput {
    revision: i64,
    action: String,
    confirmation: Option<String>,
}
async fn workspace_lifecycle(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<LifecycleInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let actor = s.auth(&h, true)?;
    s.run(move |db| {
        db.change_workspace_lifecycle(
            &actor,
            &id,
            input.revision,
            &input.action,
            input.confirmation.as_deref(),
        )
    })
    .await?;
    Ok(Json(serde_json::json!({"ok":true})))
}
