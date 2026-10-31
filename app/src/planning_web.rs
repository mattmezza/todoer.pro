use super::*;
use crate::planning::Placement;
use chrono::NaiveDate;
use rusqlite::params;
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/inbox", get(inbox_page))
        .route("/plan", get(plan_page))
        .route("/api/v1/inbox", get(intake))
        .route("/api/v1/tasks/{id}/inbox", axum::routing::put(settle_inbox))
        .route("/api/v1/plan", get(plan))
        .route("/api/v1/tasks/{id}/placement", axum::routing::put(place))
        .route("/api/v1/triage/destinations", get(destinations))
}
#[derive(Template)]
#[template(path = "planning-workbench.html")]
struct Workbench {
    sidebar: Sidebar,
    title: String,
    description: String,
    mode: String,
    timezone: String,
    week_start: u32,
}
async fn page(s: AppState, h: HeaderMap, mode: &str) -> Result<Response, ApiError> {
    let actor = match s.auth(&h, false) {
        Ok(v) => v,
        Err(_) => return Ok(Redirect::to("/signin").into_response()),
    };
    let a = actor.clone();
    let csrf = s.csrf_for(&h);
    let (sidebar, prefs) = s
        .run(move |db| {
            Ok((
                build_sidebar(db, &a, "", csrf)?,
                db.notification_preferences(&a)?,
            ))
        })
        .await?;
    let inbox = mode == "inbox";
    Ok(Html(
        Workbench {
            sidebar,
            title: if inbox { "Inbox" } else { "Plan" }.into(),
            description: if inbox {
                "A place for new tasks. Give each one your attention, then settle it."
            } else {
                "Make room for the things that matter."
            }
            .into(),
            mode: mode.into(),
            timezone: prefs.timezone,
            week_start: prefs.week_start,
        }
        .render()
        .map_err(|_| Error::Invalid("Could not render planning".into()))?,
    )
    .into_response())
}
async fn inbox_page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    page(s, h, "inbox").await
}
async fn plan_page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    page(s, h, "plan").await
}
async fn intake(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    let a = s.auth(&h, false)?;
    Ok(Json(
        s.run(move |db| {
            Ok(json!({"list_id":db.personal_inbox(&a)?,"entries":db.intake_tasks(&a)?}))
        })
        .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InboxSettlement {
    settled: bool,
}
async fn settle_inbox(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<InboxSettlement>,
) -> Result<Json<Value>, ApiError> {
    let actor = s.auth(&h, true)?;
    s.run(move |db| db.set_inbox_settled(&actor, &id, input.settled))
        .await?;
    Ok(Json(json!({"settled": input.settled})))
}
#[derive(Deserialize)]
struct Range {
    from: String,
    to: String,
}
async fn plan(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<Range>,
) -> Result<Json<Value>, ApiError> {
    let a = s.auth(&h, false)?;
    validate_range(&q.from, &q.to)?;
    Ok(Json(
        s.run(move |db| Ok(json!({"entries":db.plan_entries(&a,Some(&q.from),Some(&q.to),None)?})))
            .await?,
    ))
}
pub(super) fn validate_range(from: &str, to: &str) -> Result<(), ApiError> {
    let f = NaiveDate::parse_from_str(from, "%Y-%m-%d")
        .map_err(|_| Error::Invalid("Invalid start date".into()))?;
    let t = NaiveDate::parse_from_str(to, "%Y-%m-%d")
        .map_err(|_| Error::Invalid("Invalid end date".into()))?;
    if t < f || (t - f).num_days() > 366 {
        return Err(Error::Invalid("Choose a range of up to one year".into()).into());
    }
    Ok(())
}
async fn place(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<Placement>,
) -> Result<Json<crate::scheduling::ScheduleState>, ApiError> {
    let a = s.auth(&h, true)?;
    Ok(Json(s.run(move |db| db.place_task(&a, &id, input)).await?))
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct DestinationQuery {
    q: String,
    space: Option<String>,
}
async fn destinations(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<DestinationQuery>,
) -> Result<Json<Value>, ApiError> {
    let a = s.auth(&h, false)?;
    if q.q.len() > 500 {
        return Err(Error::Invalid("Search is too long".into()).into());
    }
    Ok(Json(s.run(move|db|{
 let mut stmt=db.conn.prepare("SELECT l.id,l.title,CASE WHEN wa.rank>=1 THEN w.title ELSE 'Shared with you' END,CASE WHEN wa.rank>=1 THEN l.workspace_id ELSE NULL END FROM lists l JOIN actor_list_access a ON a.list_id=l.id AND a.actor_id=?1 AND a.rank>=2 LEFT JOIN workspaces w ON w.id=l.workspace_id LEFT JOIN actor_workspace_access wa ON wa.workspace_id=w.id AND wa.actor_id=?1 WHERE l.archived=0 AND NOT EXISTS(SELECT 1 FROM workspace_lifecycle x WHERE x.workspace_id=l.workspace_id AND (x.archived=1 OR x.deleted_at IS NOT NULL)) AND NOT EXISTS(SELECT 1 FROM user_inboxes i WHERE i.list_id=l.id) AND (?3 IS NULL OR l.workspace_id=?3) AND (instr(lower(l.title),lower(?2))>0 OR (wa.rank>=1 AND instr(lower(w.title),lower(?2))>0)) ORDER BY lower(l.title),l.id LIMIT 20")?;
 let rows=stmt.query_map(params![a,q.q,q.space],|r|Ok(json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"space":r.get::<_,Option<String>>(2)?,"workspace_id":r.get::<_,Option<String>>(3)?})))?.collect::<Result<Vec<_>,_>>()?;
 Ok(json!({"choices":rows,"limit":20}))
 }).await?))
}
