//! A focused weekly review; all counts and observations use live authorization.
use super::*;
use crate::statistics::{ReviewRange, WeeklyReview};
use chrono::{Datelike, Duration, Utc};
const REVIEW_CSS: &str = r#"
body.review{display:block;margin:0;background:var(--bg);color:var(--text)}.review-main{max-width:980px;margin:auto;padding:24px 20px 60px;min-width:0}.review nav{display:flex;gap:18px;flex-wrap:wrap;margin-bottom:36px}.review h1{font-size:32px;margin:6px 0}.review h2{font-size:20px;margin:0 0 16px}.review .muted{color:var(--muted);line-height:1.6}.review-form{display:flex;align-items:end;gap:12px;flex-wrap:wrap;margin:24px 0}.review-form label{display:grid;gap:6px}.review-form input{max-width:100%;padding:9px;border:1px solid var(--line);border-radius:8px;background:var(--sidebar);color:var(--text);color-scheme:dark light}.review-form button{padding:10px 18px;background:var(--accent);color:#07121f;border-radius:6px;min-height:44px}.review-metrics{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:24px;margin:32px 0}.review-metrics div{border-top:1px solid var(--line);padding:20px 0}.review-panel{border-top:1px solid var(--line);padding:24px 0;background:transparent}.review-metrics dt{color:var(--muted);font-size:13px}.review-metrics dd{font-size:34px;margin:8px 0 0;font-variant-numeric:tabular-nums}.review-panel{margin-top:18px}.review-chart{width:100%;height:160px;fill:var(--accent)}.review-chart-axis{display:flex;justify-content:space-between;color:var(--muted);font-size:11px;padding:0 1.25%}.review-chart-axis span{text-align:center;flex:1}.review-chart-axis small{display:block}.review-observation-date{white-space:nowrap}.review-observation-date small{display:block;font-weight:normal;font-size:11px;color:var(--muted)}.review time{font-size:12px}.review table{width:100%;border-collapse:collapse;text-align:left;font-size:14px}.review caption{text-align:left;color:var(--muted);padding-bottom:12px}.review th,.review td{padding:10px 8px;border-bottom:1px solid var(--line);vertical-align:top}.review-scroll{overflow:auto}.review-commitments{list-style:none;padding:0}.review-commitments li{display:flex;gap:16px;justify-content:space-between;padding:13px 0;border-bottom:1px solid var(--line)}.review-commitments a{overflow-wrap:anywhere}.review-commitments span{flex-shrink:0;color:var(--muted);font-variant-numeric:tabular-nums}.review details{margin-top:18px}.review details ul{line-height:1.7;padding-left:20px}@media(max-width:600px){.review-main{padding:20px 14px 40px}.review-metrics{grid-template-columns:1fr}.review-metrics div{display:flex;align-items:center;justify-content:space-between}.review-metrics dd{margin:0}.review-panel{padding:24px 0}.review-form label{flex:1;min-width:130px}.review-form button{width:100%}}
"#;
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct QueryRange {
    from: Option<String>,
    to: Option<String>,
    list: Option<String>,
    workspace: Option<String>,
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/review.css", get(review_css))
        .route("/review", get(review_page))
        .route("/api/v1/review", get(review_json))
}
async fn review_css() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=60"),
        ],
        REVIEW_CSS,
    )
}
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
async fn review(s: &AppState, h: &HeaderMap, q: QueryRange) -> Result<WeeklyReview, ApiError> {
    let actor = s.auth(h, false)?;
    let a = actor.clone();
    let timezone = s.run(move |db| db.review_timezone(&a)).await?;
    let tz: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| Error::Invalid("Invalid review timezone".into()))?;
    let now = Utc::now();
    let today = now.with_timezone(&tz).date_naive();
    let from = q.from.unwrap_or_else(|| {
        (today - Duration::days(today.weekday().num_days_from_monday().into())).to_string()
    });
    let start = chrono::NaiveDate::parse_from_str(&from, "%Y-%m-%d")
        .map_err(|_| Error::Invalid("Use YYYY-MM-DD dates".into()))?;
    let to = q.to.unwrap_or_else(|| {
        start
            .checked_add_signed(Duration::days(6))
            .map(|d| d.to_string())
            .unwrap_or_default()
    });
    s.run(move |db| {
        db.weekly_review(
            &actor,
            ReviewRange {
                from,
                to,
                list_id: q.list,
                workspace_id: q.workspace,
            },
            now.timestamp(),
        )
    })
    .await
}
async fn review_json(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<QueryRange>,
) -> Result<Response, ApiError> {
    Ok((
        [(header::CACHE_CONTROL, "no-store, private")],
        Json(review(&s, &h, q).await?),
    )
        .into_response())
}
async fn review_page(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<QueryRange>,
) -> Result<Response, ApiError> {
    let list = q.list.clone();
    let workspace = q.workspace.clone();
    let value = review(&s, &h, q).await?;
    let mut bars = String::new();
    let max = value
        .daily
        .iter()
        .map(|d| d.completed_leaf)
        .max()
        .unwrap_or(0)
        .max(1);
    for (n, day) in value.daily.iter().enumerate() {
        let x = 10.0 + (n as f64 + 0.125) * (780.0 / value.daily.len().max(1) as f64);
        let width = (780.0 / value.daily.len().max(1) as f64 * 0.75).max(0.5);
        let height = day.completed_leaf as f64 / max as f64 * 112.0;
        bars.push_str(&format!("<rect x=\"{x:.2}\" y=\"{:.2}\" width=\"{width:.2}\" height=\"{height:.2}\"><title>{}: {} completed leaf tasks</title></rect>",130.0-height,esc(&day.date),day.completed_leaf));
    }
    let axis = if value.daily.len() <= 14 {
        value
            .daily
            .iter()
            .map(|day| {
                let parsed = chrono::NaiveDate::parse_from_str(&day.date, "%Y-%m-%d")
                    .expect("validated review date");
                format!(
                    "<span><small>{}</small>{}</span>",
                    parsed.format("%a"),
                    parsed.format("%d")
                )
            })
            .collect::<String>()
    } else {
        format!(
            "<span>{}</span><span>{}</span>",
            esc(&value.from),
            esc(&value.to)
        )
    };
    let mut history = String::new();
    for day in &value.daily {
        history.push_str(&format!(
            "<tr><th scope=\"row\">{}</th><td>{}</td></tr>",
            esc(&day.date),
            day.completed_leaf
        ));
    }
    let trend = if value.overdue_trend.is_empty() {
        "<p class=\"muted\">No overdue observations for this range yet. Daily recording starts when this version runs; earlier days are not reconstructed.</p>".into()
    } else {
        let mut rows = String::new();
        for point in &value.overdue_trend {
            let coverage = if point.complete_coverage {
                "full"
            } else {
                "partial"
            };
            let at = chrono::DateTime::from_timestamp(point.last_observed_at, 0);
            let stamp = at.map(|at| at.to_rfc3339()).unwrap_or_default();
            let label = at
                .map(|at| at.format("%d %b %H:%M").to_string())
                .unwrap_or_default();
            rows.push_str(&format!("<tr><th class=\"review-observation-date\" scope=\"row\">{}<small>{}/{} lists · {coverage}</small></th><td>{}</td><td><time datetime=\"{}\" title=\"{}\" aria-label=\"{}\">{}</time></td></tr>",esc(&point.date),point.observed_lists,point.current_lists,point.overdue_leaf,esc(&stamp),esc(&stamp),esc(&stamp),esc(&label)));
        }
        format!(
            "<div class=\"review-scroll\"><table><caption>Actual first daily overdue observations per list</caption><thead><tr><th>Date</th><th>Overdue</th><th>Observed (UTC)</th></tr></thead><tbody>{rows}</tbody></table></div>"
        )
    };
    let commitments = if value.upcoming.is_empty() {
        "<p class=\"muted\">No dated leaf commitments in the next seven days.</p>".into()
    } else {
        let mut rows = String::new();
        for item in &value.upcoming {
            rows.push_str(&format!(
                "<li><a href=\"/lists/{}?task={}\">{}</a><span>{}</span></li>",
                urlencode(&item.task.list_id),
                urlencode(&item.task.id),
                esc(&item.task.title),
                esc(&item.on)
            ));
        }
        format!(
            "<ul class=\"review-commitments\">{rows}</ul><p class=\"muted\">Showing {} of {} commitments.</p>",
            value.upcoming.len(),
            value.upcoming_total
        )
    };
    let mut notes = String::new();
    for note in &value.notes {
        notes.push_str(&format!("<li>{}</li>", esc(note)));
    }
    let hidden = match (list, workspace) {
        (Some(id), _) => format!(
            "<input type=\"hidden\" name=\"list\" value=\"{}\">",
            esc(&id)
        ),
        (_, Some(id)) => format!(
            "<input type=\"hidden\" name=\"workspace\" value=\"{}\">",
            esc(&id)
        ),
        _ => String::new(),
    };
    let html = format!(
        r##"<p class="muted">{} through {} · {}</p><form class="review-form" method="get">{hidden}<label>From<input type="date" name="from" value="{}" required></label><label>Through<input type="date" name="to" value="{}" required></label><button>Review dates</button></form><dl class="review-metrics"><div><dt>Completed leaf tasks</dt><dd>{}</dd></div><div><dt>Planned / completed</dt><dd>{} / {}</dd></div><div><dt>Overdue now</dt><dd>{}</dd></div></dl><section class="review-panel" aria-labelledby="completion-title"><h2 id="completion-title">Completed work</h2><p class="muted">Distinct recorded leaf tasks. Older history may be incomplete; repeating a completion in this range counts once.</p><svg class="review-chart" viewBox="0 0 800 150" role="img" aria-label="Completed leaf tasks by day"><title>Completed leaf tasks by day</title><line x1="10" y1="130" x2="790" y2="130" stroke="currentColor" stroke-opacity=".2"/>{bars}</svg><div class="review-chart-axis" aria-hidden="true">{axis}</div><details><summary>Daily counts</summary><div class="review-scroll"><table><thead><tr><th>Date</th><th>Completed leaf tasks</th></tr></thead><tbody>{history}</tbody></table></div></details></section><section class="review-panel" aria-labelledby="overdue-title"><h2 id="overdue-title">Overdue trend</h2><p class="muted">Daily observations keep the backlog as recorded; today’s live total may differ.</p>{trend}</section><section class="review-panel" aria-labelledby="commitment-title"><h2 id="commitment-title">Upcoming commitments</h2><p class="muted">Open leaf tasks with a planned date or due date in the next seven days.</p>{commitments}</section><details class="review-panel"><summary>How this review is counted</summary><ul>{notes}</ul></details>"##,
        esc(&value.from),
        esc(&value.to),
        esc(&value.timezone),
        esc(&value.from),
        esc(&value.to),
        value.completed_leaf,
        value.planned_leaf,
        value.planned_completed_leaf,
        value.overdue_now
    );
    let mut response = super::scheduling_http::render_feature(&s, &h, "Review", html, true).await?;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store, private".parse().unwrap());
    Ok(response)
}
fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
/// Always enabled independently of SMTP; no network transport or credentials.
pub(super) fn start_statistics_worker(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while !state.stopping.load(std::sync::atomic::Ordering::Acquire) {
            let result = state
                .run(|db| db.record_stats_observations(Utc::now().timestamp()))
                .await;
            if result.is_err() {
                eprintln!("Daily statistics observation failed; no historical point fabricated");
            }
            for _ in 0..60 {
                if state.stopping.load(std::sync::atomic::Ordering::Acquire) {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    fn state() -> AppState {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let today = Utc::now().date_naive().to_string();
        for (actor, list, title) in [
            ("dev", "inbox", "<script>synthetic()</script>"),
            ("other", "private", "Private commitment"),
        ] {
            let task = db
                .capture(
                    actor,
                    &uuid::Uuid::new_v4().to_string(),
                    crate::core::Capture {
                        list_id: list.into(),
                        tasks: vec![crate::core::NewTask {
                            title: title.into(),
                            description: String::new(),
                            parent_id: None,
                        }],
                    },
                )
                .unwrap()
                .remove(0);
            db.set_schedule(
                actor,
                &task.id,
                task.revision,
                crate::scheduling::Schedule {
                    planned: Some(crate::scheduling::Moment::Date {
                        date: today.clone(),
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        AppState::new(
            Arc::new(Mutex::new(db)),
            "synthetic-review-token".into(),
            "http://localhost".into(),
        )
    }
    #[tokio::test]
    async fn review_http_is_private_bounded_and_csp_compatible() {
        let app = routes().with_state(state());
        let no_auth = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/review")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(no_auth.status(), StatusCode::UNAUTHORIZED);
        let bad = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/review?from=2024-01-01&to=2025-01-01")
                    .header("authorization", "Bearer synthetic-review-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
        let private = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/review?list=private")
                    .header("authorization", "Bearer synthetic-review-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(private.status(), StatusCode::FORBIDDEN);
        let page = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/review")
                    .header("authorization", "Bearer synthetic-review-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
        assert_eq!(page.headers()[header::CACHE_CONTROL], "no-store, private");
        let bytes = to_bytes(page.into_body(), 100000).await.unwrap();
        let html = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(html.contains("&lt;script&gt;synthetic()&lt;/script&gt;"));
        assert!(!html.contains("Private commitment"));
        assert!(html.contains("/review.css"));
        assert!(!html.contains("<style>"));
        assert!(html.contains("role=\"img\""));
        let css = app
            .oneshot(
                Request::builder()
                    .uri("/review.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(css.status(), StatusCode::OK);
        assert_eq!(
            css.headers()[header::CONTENT_TYPE],
            "text/css; charset=utf-8"
        );
    }
}
