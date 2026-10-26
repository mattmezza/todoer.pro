use super::*;
use crate::notifications::Preferences;
use crate::scheduling::{RecurrenceSpec, Schedule, SeriesState};
use chrono::{Datelike, Duration, Utc};
fn esc(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/tasks/{id}/schedule",
            get(schedule).put(set_schedule),
        )
        .route("/api/v1/tasks/{id}/series", post(start_series))
        .route("/api/v1/tasks/{id}/skip", post(skip))
        .route("/api/v1/series", get(series_all))
        .route("/api/v1/series/{id}", get(series))
        .route("/api/v1/series/{id}/state", post(series_state))
        .route("/api/v1/series/{id}/occurrences", get(occurrences))
        .route(
            "/api/v1/notification-preferences",
            get(preferences).put(set_preferences),
        )
        .route("/api/v1/planning", get(planning))
        .route("/api/v1/statistics", get(statistics))
        .route(
            "/notifications",
            get(notification_page).post(notification_form),
        )
        .route("/recurring/{id}/state", post(series_state_form))
        .route("/today", get(today))
        .route("/upcoming", get(upcoming))
        .route("/recurring", get(recurring))
        .route("/statistics", get(statistics_page))
}
async fn schedule(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.get_schedule(&a, &id)).await?).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleInput {
    revision: i64,
    schedule: Schedule,
}
async fn set_schedule(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<ScheduleInput>,
) -> Result<Response, ApiError> {
    let a = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.set_schedule(&a, &id, input.revision, input.schedule))
            .await?,
    )
    .into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeriesInput {
    revision: i64,
    spec: RecurrenceSpec,
}
async fn start_series(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<SeriesInput>,
) -> Result<Response, ApiError> {
    let a = s.auth(&h, true)?;
    Ok((
        StatusCode::CREATED,
        Json(
            s.run(move |db| db.start_series(&a, &id, input.revision, input.spec))
                .await?,
        ),
    )
        .into_response())
}
async fn series_all(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.recurring_series(&a)).await?).into_response())
}
async fn series(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.series(&a, &id)).await?).into_response())
}
async fn occurrences(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.occurrences(&a, &id)).await?).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateInput {
    revision: i64,
    state: SeriesState,
}
async fn series_state(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<StateInput>,
) -> Result<Response, ApiError> {
    let a = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| {
            db.set_series_state(&a, &id, input.revision, input.state, Utc::now().timestamp())
        })
        .await?,
    )
    .into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    revision: i64,
}
async fn skip(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Input(input): Input<Revision>,
) -> Result<Response, ApiError> {
    let a = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.skip_occurrence(&a, &id, input.revision, Utc::now().timestamp()))
            .await?,
    )
    .into_response())
}
async fn preferences(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }

    let a = s.auth(&h, false)?;
    Ok(Json(s.run(move |db| db.notification_preferences(&a)).await?).into_response())
}
async fn set_preferences(
    State(s): State<AppState>,
    h: HeaderMap,
    Input(input): Input<Preferences>,
) -> Result<Response, ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }

    if !s.origin_ok(&h) {
        return Err(Error::Forbidden.into());
    }
    let a = s.auth(&h, true)?;
    Ok(Json(
        s.run(move |db| db.set_notification_preferences(&a, input))
            .await?,
    )
    .into_response())
}
#[derive(Deserialize)]
struct Range {
    from: Option<String>,
    to: Option<String>,
}
async fn planning(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<Range>,
) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    let actor = a.clone();
    let prefs = s.run(move |db| db.notification_preferences(&actor)).await?;
    let tz: chrono_tz::Tz = prefs
        .timezone
        .parse()
        .map_err(|_| Error::Invalid("Invalid timezone".into()))?;
    let today = Utc::now().with_timezone(&tz).date_naive();
    let from = q.from.unwrap_or_else(|| today.to_string());
    let to =
        q.to.unwrap_or_else(|| (today + Duration::days(14)).to_string());
    Ok(Json(s.run(move |db| db.planning_tasks(&a, &from, &to)).await?).into_response())
}
async fn statistics(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<Range>,
) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    let actor = a.clone();
    let prefs = s.run(move |db| db.notification_preferences(&actor)).await?;
    let tz: chrono_tz::Tz = prefs
        .timezone
        .parse()
        .map_err(|_| Error::Invalid("Invalid timezone".into()))?;
    let today = Utc::now().with_timezone(&tz).date_naive();
    let from = q.from.unwrap_or_else(|| {
        (today - Duration::days(today.weekday().num_days_from_monday().into())).to_string()
    });
    Ok(Json(s.run(move |db| db.weekly_statistics(&a, &from)).await?).into_response())
}
fn page(title: &str, body: String, timezone: &str) -> Response {
    Html(format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{} · Todoer</title><link rel=\"stylesheet\" href=\"/assets/app.css\"><main><nav><a href=\"/\">Lists</a> · <a href=\"/today\">Today</a> · <a href=\"/upcoming\">Upcoming</a> · <a href=\"/recurring\">Recurring</a> · <a href=\"/statistics\">Weekly statistics</a> · <a href=\"/settings\">Settings</a> · <a href=\"/notifications\">Notifications</a></nav><h1>{}</h1><p>Timezone: {}</p>{}</main></html>",esc(title),esc(title),esc(timezone),body)).into_response()
}
async fn calendar(s: AppState, h: HeaderMap, upcoming: bool) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    let actor = a.clone();
    let prefs = s.run(move |db| db.notification_preferences(&actor)).await?;
    let tz: chrono_tz::Tz = prefs
        .timezone
        .parse()
        .map_err(|_| Error::Invalid("Invalid timezone".into()))?;
    let today = Utc::now().with_timezone(&tz).date_naive();
    let from = if upcoming {
        today + Duration::days(1)
    } else {
        today - Duration::days(365)
    };
    let to = if upcoming {
        today + Duration::days(14)
    } else {
        today
    };
    let rows = s
        .run(move |db| db.planning_tasks(&a, &from.to_string(), &to.to_string()))
        .await?;
    let mut body = String::from("<ul>");
    for row in rows {
        body.push_str(&format!(
            "<li><a href=\"/lists/{}\">{}</a> <small>{}</small>{}</li>",
            esc(&row.task.list_id),
            esc(&row.task.title),
            esc(&row.dates.join(", ")),
            row.schedule
                .schedule
                .priority
                .map(|p| format!(" · priority {p}"))
                .unwrap_or_default()
        ));
    }
    body.push_str("</ul><p>Date-only values stay on their entered date. Timed values are shown in your selected timezone.</p>");
    Ok(page(
        if upcoming {
            "Upcoming · next 14 days"
        } else {
            "Today and overdue · last year"
        },
        body,
        &prefs.timezone,
    ))
}
async fn today(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    calendar(s, h, false).await
}
async fn upcoming(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    calendar(s, h, true).await
}
async fn recurring(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    let rows = s.run(move |db| db.recurring_series(&a)).await?;
    let mut body = String::from("<ul>");
    for series in rows {
        let rule = match &series.spec.rule {
            crate::scheduling::Rule::Daily { interval_days } => {
                format!("Every {interval_days} calendar day(s)")
            }
            crate::scheduling::Rule::Weekdays { days } => format!(
                "Weekdays {} (Monday=1)",
                days.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            crate::scheduling::Rule::CompletionRelative { days } => {
                format!("{days} calendar day(s) after completion")
            }
        };
        body.push_str(&format!(
            "<li>{} — {:?}; {} occurrences; {}. Starts {}",
            esc(&rule),
            series.state,
            series.emitted,
            esc(&series.spec.timezone),
            esc(&series.spec.start_local)
        ));
        if series.state != SeriesState::Stopped {
            body.push_str(&format!("<form method=\"post\" action=\"/recurring/{}/state\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"revision\" value=\"{}\"><button name=\"state\" value=\"{}\">{}</button><button name=\"state\" value=\"stopped\">Stop permanently</button></form>",esc(&series.id),esc(&s.csrf_for(&h)),series.revision,if series.state==SeriesState::Active{"paused"}else{"active"},if series.state==SeriesState::Active{"Pause"}else{"Resume"}));
        }
        body.push_str("</li>");
    }
    body.push_str("</ul><p>Each series creates at most one outstanding occurrence. Stopping preserves its tasks and history.</p>");
    Ok(page("Recurring", body, "Per series"))
}
#[derive(Deserialize)]
struct SeriesForm {
    csrf: String,
    revision: i64,
    state: SeriesState,
}
async fn series_state_form(
    State(s): State<AppState>,
    mut h: HeaderMap,
    Path(id): Path<String>,
    Form(input): Form<SeriesForm>,
) -> Result<Response, ApiError> {
    if !s.origin_ok(&h) || h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    h.insert(
        "x-csrf-token",
        input.csrf.parse().map_err(|_| Error::Forbidden)?,
    );
    let a = s.auth(&h, true)?;
    s.run(move |db| {
        db.set_series_state(&a, &id, input.revision, input.state, Utc::now().timestamp())
    })
    .await?;
    Ok(Redirect::to("/recurring").into_response())
}
async fn statistics_page(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<Range>,
) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    let actor = a.clone();
    let prefs = s.run(move |db| db.notification_preferences(&actor)).await?;
    let tz: chrono_tz::Tz = prefs
        .timezone
        .parse()
        .map_err(|_| Error::Invalid("Invalid timezone".into()))?;
    let today = Utc::now().with_timezone(&tz).date_naive();
    let from = q.from.unwrap_or_else(|| {
        (today - Duration::days(today.weekday().num_days_from_monday().into())).to_string()
    });
    let stats = s.run(move |db| db.weekly_statistics(&a, &from)).await?;
    Ok(page(
        "Weekly statistics",
        format!(
            "<form><label>Week starts <input name=\"from\" type=\"date\" value=\"{}\"></label><button>View week</button></form><p>{} through {}</p><p>{} completion transitions; {} scheduled open tasks.</p><p>{}</p>",
            esc(&stats.from),
            esc(&stats.from),
            esc(&stats.to),
            stats.completed_transitions,
            stats.scheduled_open,
            esc(&stats.history_timestamp_note)
        ),
        &stats.timezone,
    ))
}

async fn notification_page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    let a = s.auth(&h, false)?;
    let actor = a.clone();
    let prefs = s.run(move |db| db.notification_preferences(&actor)).await?;
    let lists = s.run(move |db| db.lists(&a)).await?;
    let mut mutes = String::new();
    for list in lists {
        mutes.push_str(&format!(
            "<label><input type=\"checkbox\" name=\"mute_{}\" value=\"yes\" {}>{}</label><br>",
            esc(&list.id),
            if prefs.list_opt_out.contains(&list.id) {
                "checked"
            } else {
                ""
            },
            esc(&list.title)
        ));
    }
    let body = format!(
        "<form method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><label>IANA timezone <input name=\"timezone\" value=\"{}\" required></label><p><label><input type=\"checkbox\" name=\"email_opt_in\" {}>Email reminders and digests to my verified sign-in email</label></p><label>Quiet hours start <input type=\"time\" name=\"quiet_start\" value=\"{}\"></label><label>Quiet hours end <input type=\"time\" name=\"quiet_end\" value=\"{}\"></label><p>Equal quiet-hour times mute all hours. Leave both blank to disable quiet hours.</p><p><label><input type=\"checkbox\" name=\"daily_opt_in\" {}>Daily summary</label><input type=\"time\" name=\"daily_time\" value=\"{}\"></p><p><label><input type=\"checkbox\" name=\"weekly_opt_in\" {}>Weekly summary</label><label>Weekday (Monday=1)<input type=\"number\" name=\"weekly_day\" min=\"1\" max=\"7\" value=\"{}\"></label><input type=\"time\" name=\"weekly_time\" value=\"{}\"></p><fieldset><legend>Mute notifications from these lists</legend>{}</fieldset><button>Save notification preferences</button></form><p>Delivery requires the server's SMTP worker to be configured. Push delivery is not available.</p>",
        esc(&s.csrf_for(&h)),
        esc(&prefs.timezone),
        if prefs.email_opt_in { "checked" } else { "" },
        esc(prefs.quiet_start.as_deref().unwrap_or("")),
        esc(prefs.quiet_end.as_deref().unwrap_or("")),
        if prefs.daily_opt_in { "checked" } else { "" },
        esc(&prefs.daily_time),
        if prefs.weekly_opt_in { "checked" } else { "" },
        prefs.weekly_day,
        esc(&prefs.weekly_time),
        mutes
    );
    Ok(page("Notification preferences", body, &prefs.timezone))
}
async fn notification_form(
    State(s): State<AppState>,
    mut h: HeaderMap,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Result<Response, ApiError> {
    if !s.origin_ok(&h) || h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    h.insert(
        "x-csrf-token",
        form.get("csrf")
            .ok_or(Error::Forbidden)?
            .parse()
            .map_err(|_| Error::Forbidden)?,
    );
    let a = s.auth(&h, true)?;
    let field = |key: &str| form.get(key).cloned().unwrap_or_default();
    let nonempty = |key: &str| {
        let v = field(key);
        (!v.is_empty()).then_some(v)
    };
    let prefs = Preferences {
        timezone: field("timezone"),
        email_opt_in: form.contains_key("email_opt_in"),
        quiet_start: nonempty("quiet_start"),
        quiet_end: nonempty("quiet_end"),
        daily_opt_in: form.contains_key("daily_opt_in"),
        daily_time: field("daily_time"),
        weekly_opt_in: form.contains_key("weekly_opt_in"),
        weekly_day: field("weekly_day")
            .parse()
            .map_err(|_| Error::Invalid("Invalid weekday".into()))?,
        weekly_time: field("weekly_time"),
        list_opt_out: form
            .iter()
            .filter_map(|(key, value)| {
                key.strip_prefix("mute_")
                    .filter(|_| value.as_str() == "yes")
                    .map(str::to_owned)
            })
            .collect(),
    };
    s.run(move |db| db.set_notification_preferences(&a, prefs))
        .await?;
    Ok(Redirect::to("/notifications").into_response())
}

/// Startup owns the returned task and awaits it during graceful shutdown.
pub(super) fn spawn_worker(
    state: AppState,
    config: crate::notifications::Notifications,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        while !state.stopping.load(std::sync::atomic::Ordering::SeqCst) {
            interval.tick().await;
            if state.stopping.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            if let Some(signer) = &state.invitations
                && crate::invitations::deliver_invitation_mail(
                    &state.store,
                    &config,
                    signer,
                    &state.stopping,
                )
                .await
                .is_err()
            {
                eprintln!("invitation delivery state unavailable; durable jobs retained");
            }
            if crate::notifications::worker_tick_until(
                &state.store,
                &config,
                Some(state.stopping.as_ref()),
            )
            .await
            .is_err()
            {
                eprintln!(
                    "notification worker iteration failed; durable jobs remain available for retry"
                );
            }
        }
    })
}
impl AppState {
    pub fn start_notification_worker(
        &self,
        config: crate::notifications::Notifications,
    ) -> tokio::task::JoinHandle<()> {
        spawn_worker(self.clone(), config)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    #[tokio::test]
    async fn schedule_http_conflicts_and_browser_preferences_are_protected() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let task = db
            .capture(
                "dev",
                "http",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![crate::core::NewTask {
                        title: "Planning <task>".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let state = AppState::new(
            Arc::new(Mutex::new(db)),
            "agent".into(),
            "http://127.0.0.1:8000".into(),
        );
        state
            .sessions
            .lock()
            .unwrap()
            .insert(Sha256::digest(b"browser").into(), std::time::Instant::now());
        let app = routes().with_state(state.clone());
        let today = Utc::now().date_naive().to_string();
        let body=serde_json::json!({"revision":1,"schedule":{"planned":{"kind":"date","date":today},"reminders":[]}}).to_string();
        let request = || {
            axum::http::Request::builder()
                .uri(format!("/api/v1/tasks/{}/schedule", task.id))
                .method("PUT")
                .header("authorization", "Bearer agent")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body.clone()))
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            StatusCode::CONFLICT
        );
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/today")
                    .header("authorization", "Bearer agent")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let text = String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(text.contains("Planning &lt;task&gt;"));
        let request = |csrf: &str| {
            axum::http::Request::builder()
                .uri("/api/v1/notification-preferences")
                .method("PUT")
                .header("cookie", "todoer_dev=browser")
                .header("content-type", "application/json")
                .header("x-csrf-token", csrf)
                .body(axum::body::Body::from("{\"timezone\":\"Europe/Zurich\"}"))
                .unwrap()
        };
        assert!(
            app.clone()
                .oneshot(request("wrong"))
                .await
                .unwrap()
                .status()
                .is_client_error()
        );
        assert_eq!(
            app.clone()
                .oneshot(request(&state.csrf))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let mut bearer = request(&state.csrf);
        bearer
            .headers_mut()
            .insert("authorization", "Bearer agent".parse().unwrap());
        assert_eq!(
            app.oneshot(bearer).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
}
