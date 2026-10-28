use super::*;
use crate::onboarding::{Finish, Setup};
#[derive(Template)]
#[template(path = "welcome.html")]
struct WelcomePage {
    account: String,
    csrf: String,
    next: String,
}
fn actor(s: &AppState, h: &HeaderMap, write: bool) -> Result<String, ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    s.auth(h, write)
}
#[derive(Deserialize)]
struct WelcomeQuery {
    next: Option<String>,
}
async fn page(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<WelcomeQuery>,
) -> Result<Response, ApiError> {
    let account = actor(&s, &h, false)?;
    Ok(Html(
        WelcomePage {
            account,
            csrf: s.csrf_for(&h),
            next: browser_auth::safe_next(q.next.as_deref()),
        }
        .render()
        .map_err(|_| Error::Invalid("Could not render welcome".into()))?,
    )
    .into_response())
}
async fn state(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let a = actor(&s, &h, false)?;
    Ok(Json(s.run(move |db| db.welcome(&a)).await?).into_response())
}
async fn setup(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<Setup>,
) -> Result<Response, ApiError> {
    let a = actor(&s, &h, true)?;
    Ok(Json(s.run(move |db| db.welcome_setup(&a, input)).await?).into_response())
}
async fn finish(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<Finish>,
) -> Result<Response, ApiError> {
    let a = actor(&s, &h, true)?;
    Ok(
        Json(json!({"redirect":s.run(move|db|db.welcome_finish(&a,input,false)).await?}))
            .into_response(),
    )
}
async fn skip(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<Finish>,
) -> Result<Response, ApiError> {
    let a = actor(&s, &h, true)?;
    Ok(
        Json(json!({"redirect":s.run(move|db|db.welcome_finish(&a,input,true)).await?}))
            .into_response(),
    )
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/welcome", get(page))
        .route("/api/v1/onboarding", get(state))
        .route("/api/v1/onboarding/setup", post(setup))
        .route("/api/v1/onboarding/finish", post(finish))
        .route("/api/v1/onboarding/skip", post(skip))
}
