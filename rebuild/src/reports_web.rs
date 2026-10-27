use super::*;
pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/reports/export", get(export))
}
async fn export(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(scope): Query<crate::reports::ReportScope>,
) -> Result<Response, ApiError> {
    let actor = s.auth(&h, false)?;
    if scope.kind == "plan" {
        super::planning_http::validate_range(
            scope.from.as_deref().unwrap_or(""),
            scope.to.as_deref().unwrap_or(""),
        )?;
    }
    let format = scope.format.clone().unwrap_or_else(|| "json".into());
    let kind = format.clone();
    let bytes = s
        .run(move |db| {
            let report = db.readable_report(&actor, &scope)?;
            match kind.as_str() {
                "txt" | "text" => Ok(crate::reports::text(&report).into_bytes()),
                "csv" => crate::reports::delimited(&report, b','),
                "tsv" => crate::reports::delimited(&report, b'\t'),
                "json" => serde_json::to_vec_pretty(&report)
                    .map_err(|_| Error::Invalid("Could not encode export".into())),
                "pdf" => crate::reports::pdf(&report),
                _ => Err(Error::Invalid("Unknown export format".into())),
            }
        })
        .await?;
    if bytes.len() > 100 * 1024 * 1024 {
        return Err(
            Error::Invalid("Export exceeds 100 MiB. Choose a smaller scope.".into()).into(),
        );
    }
    let (mime, extension) = match format.as_str() {
        "txt" | "text" => ("text/plain; charset=utf-8", "txt"),
        "csv" => ("text/csv; charset=utf-8", "csv"),
        "tsv" => ("text/tab-separated-values; charset=utf-8", "tsv"),
        "pdf" => ("application/pdf", "pdf"),
        _ => ("application/json", "json"),
    };
    Ok((
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"todoer-export.{extension}\""),
            ),
            (header::CACHE_CONTROL, "no-store".into()),
        ],
        bytes,
    )
        .into_response())
}
