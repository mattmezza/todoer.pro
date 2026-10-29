use super::*;
use crate::navigation::FavoriteInput;
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/favorites", get(favorites).put(set_favorite))
        .route("/favorites", get(page).post(form))
        .route("/api/v1/lists/{id}/color", get(color).put(set_color))
        .route("/lists/{id}/color", post(color_form))
}
async fn favorites(
    State(s): State<AppState>,
    h: HeaderMap,
) -> Result<Json<Vec<crate::navigation::Favorite>>, ApiError> {
    let a = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.favorites(&a)).await?))
}
async fn set_favorite(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<FavoriteInput>,
) -> Result<Json<Value>, ApiError> {
    let a = s.auth(&h, true)?;
    s.run(move |db| db.set_favorite(&a, input)).await?;
    Ok(Json(json!({"saved":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ColorInput {
    revision: i64,
    color: Option<String>,
}
async fn color(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let a = s.auth(&h, false)?;
    Ok(Json(
        json!({"color":s.run(move|db|db.list_color(&a,&id)).await?}),
    ))
}
async fn set_color(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<ColorInput>,
) -> Result<Json<List>, ApiError> {
    let a = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.set_list_color(&a, &id, input.revision, input.color))
            .await?,
    ))
}
async fn page() -> Redirect {
    Redirect::to("/today")
}
#[derive(Deserialize)]
struct FavoriteForm {
    csrf: String,
    kind: String,
    id: String,
    favorite: bool,
}
async fn form(
    State(s): State<AppState>,
    mut h: HeaderMap,
    Form(f): Form<FavoriteForm>,
) -> Result<Redirect, ApiError> {
    if !s.origin_ok(&h) {
        return Err(Error::Forbidden.into());
    }
    h.insert(
        "x-csrf-token",
        f.csrf.parse().map_err(|_| Error::Forbidden)?,
    );
    let a = s.auth(&h, true)?;
    s.run(move |db| {
        db.set_favorite(
            &a,
            FavoriteInput {
                kind: f.kind,
                id: f.id,
                favorite: f.favorite,
            },
        )
    })
    .await?;
    Ok(Redirect::to("/today"))
}
#[derive(Deserialize)]
struct ColorForm {
    csrf: String,
    revision: i64,
    color: String,
}
async fn color_form(
    State(s): State<AppState>,
    mut h: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<ColorForm>,
) -> Result<Redirect, ApiError> {
    if !s.origin_ok(&h) {
        return Err(Error::Forbidden.into());
    }
    h.insert(
        "x-csrf-token",
        f.csrf.parse().map_err(|_| Error::Forbidden)?,
    );
    let a = s.auth(&h, true)?;
    let target = id.clone();
    s.run(move |db| {
        db.set_list_color(
            &a,
            &id,
            f.revision,
            if f.color.is_empty() {
                None
            } else {
                Some(f.color)
            },
        )
    })
    .await?;
    Ok(Redirect::to(&format!("/lists/{target}/sharing")))
}
