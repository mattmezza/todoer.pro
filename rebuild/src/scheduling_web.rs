use super::*;
use crate::notifications::Preferences;
use crate::scheduling::{RecurrenceSpec, Schedule, SeriesState};
use chrono::{Datelike, Duration, Utc};
pub(super) fn esc(value: &str) -> String {
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
            "/settings/notifications",
            get(notification_page).post(notification_form),
        )
        .route(
            "/notifications",
            post(notification_form),
        )
        .route("/recurring/{id}/state", post(series_state_form))
        .route("/settings", get(general_settings))
        .route("/settings/account", get(account_settings))
        .route("/settings/general", get(general_settings))
        .route("/today", get(today))
        .route("/upcoming", get(upcoming))
        .route("/recurring", get(recurring))
        .route("/statistics", get(|| async { Redirect::to("/review") }))
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
#[derive(Template)]
#[template(path = "planning.html")]
struct PlanningPage {
    sidebar: super::Sidebar,
    title: String,
    timezone: String,
    account_timezone: String,
    range: String,
    body: String,
    tabs: String,
    space_filters: String,
    selected_spaces: String,
    today: String,
    week_start: u32,
}
#[derive(Deserialize, Default)]
struct CalendarQuery {
    range: Option<String>,
    tz: Option<String>,
    spaces: Option<String>,
}
fn calendar_range(
    today: chrono::NaiveDate,
    key: &str,
    week_start: u32,
) -> Result<(chrono::NaiveDate, chrono::NaiveDate, &'static str), Error> {
    let monday = today
        - Duration::days(((today.weekday().number_from_monday() + 7 - week_start) % 7).into());
    Ok(match key {
        "past" => (
            chrono::NaiveDate::from_ymd_opt(1, 1, 1).unwrap(),
            monday - Duration::days(8),
            "Past",
        ),
        "previous" => (
            monday - Duration::days(7),
            monday - Duration::days(1),
            "Prev week",
        ),
        "week" => (monday, monday + Duration::days(6), "This week"),
        "wtn" => (monday, today, "Week so far"),
        "yesterday" => (
            today - Duration::days(1),
            today - Duration::days(1),
            "Yesterday",
        ),
        "today" => (today, today, "Today"),
        "tomorrow" => (
            today + Duration::days(1),
            today + Duration::days(1),
            "Tomorrow",
        ),
        "wte" => (today, monday + Duration::days(6), "Rest of week"),
        "next" => (
            monday + Duration::days(7),
            monday + Duration::days(13),
            "Next week",
        ),
        "upcoming" => (
            monday + Duration::days(14),
            chrono::NaiveDate::from_ymd_opt(9999, 12, 31).unwrap(),
            "Upcoming",
        ),
        _ => return Err(Error::Invalid("Unknown planning range".into())),
    })
}
async fn calendar(
    s: AppState,
    h: HeaderMap,
    key: String,
    timezone: Option<String>,
    selected_spaces: Option<String>,
) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    let actor = a.clone();
    let prefs = s.run(move |db| db.notification_preferences(&actor)).await?;
    let timezone = timezone.unwrap_or_else(|| prefs.timezone.clone());
    let tz: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| Error::Invalid("Invalid timezone".into()))?;
    let today = Utc::now().with_timezone(&tz).date_naive();
    let (from, to, label) = calendar_range(today, &key, prefs.week_start)?;
    let sidebar_actor = a.clone();
    let requested = selected_spaces.unwrap_or_default();
    if requested.len() > 4096 {
        return Err(Error::Invalid("Too many space filters".into()).into());
    }
    let (rows, spaces, selected) = s
        .run(move |db| {
            let mut available = db.workspaces(&a)?;
            let inactive = db.conn.prepare("SELECT workspace_id FROM workspace_lifecycle WHERE archived=1 OR deleted_at IS NOT NULL")?.query_map([], |r| r.get::<_,String>(0))?.collect::<Result<std::collections::HashSet<_>,_>>()?;
            available.retain(|space| !inactive.contains(&space.id));
            let mut selected = std::collections::BTreeSet::new();
            for value in requested.split(',').filter(|v| !v.is_empty()) {
                let id = crate::defaults::resolve(&db.conn, "workspace", value)?;
                if !available.iter().any(|w| w.id == id) {
                    return Err(Error::Invalid("Space filter is unavailable".into()));
                }
                selected.insert(id);
            }
            let mut rows = db.planning_timeline(&a, &from.to_string(), &to.to_string())?;
            if !selected.is_empty() {
                let mut stmt = db
                    .conn
                    .prepare("SELECT id,workspace_id FROM lists WHERE workspace_id IS NOT NULL")?;
                let lists = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                    .collect::<Result<std::collections::HashMap<_, _>, _>>()?;
                rows.retain(|row| {
                    lists
                        .get(&row.task.list_id)
                        .is_some_and(|id| selected.contains(id))
                });
            }
            let spaces = available
                .into_iter()
                .map(|w| {
                    let alias = crate::defaults::public_id(&db.conn, "workspace", &w.id)?;
                    Ok((alias, w.title, selected.contains(&w.id)))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let selected = spaces
                .iter()
                .filter(|v| v.2)
                .map(|v| v.0.clone())
                .collect::<Vec<_>>()
                .join(",");
            Ok((rows, spaces, selected))
        })
        .await?;
    let mut body = String::new();
    for row in rows {
        let priority = row.schedule.schedule.priority.unwrap_or(3);
        let priority_label = match priority {
            0 => "P0 · Highest",
            1 => "P1 · High",
            2 => "P2 · Medium",
            3 => "P3 · Low",
            _ => "P2 · Medium",
        };
        let description = if row.task.description.trim().is_empty() { String::new() } else { format!("<div class=\"planning-task-description markdown\">{}</div>", crate::markdown::render(&row.task.description)) };
        body.push_str(&format!(r#"<article class="task planning-task{}" data-planning-task="{}"><a class="planning-completion" href="/tasks/{}" aria-label="Open task">{}</a><div class="task-content"><a class="task-title" href="/tasks/{}">{}</a>{}</div><span class="planning-row-meta"><span class="task-priority-summary" data-priority="{}"><svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#flag"/></svg>{}</span><time class="planning-date" datetime="{}"><svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#calendar"/></svg>{}</time></span></article>"#,
          if row.task.complete { " is-complete" } else { "" }, esc(&row.task.id), esc(&row.task.id), if row.task.complete { "✓" } else { "" }, esc(&row.task.id), esc(&row.task.title), description, priority, priority_label, esc(&row.dates[0]), esc(&row.dates[0])));
    }
    if body.is_empty() {
        body.push_str("<div class=\"planning-empty\"><h2>A little breathing room.</h2><p>Nothing planned here yet. Enjoy the space, or capture something you want to make time for.</p><a href=\"/capture\" class=\"primary\">Add a task</a></div>");
    }
    let mut tabs = String::new();
    for tab in [
        "past",
        "previous",
        "wtn",
        "yesterday",
        "today",
        "tomorrow",
        "wte",
        "next",
        "upcoming",
    ] {
        let (_, _, caption) = calendar_range(today, tab, prefs.week_start)?;
        tabs.push_str(&format!(
            "<a href=\"/today?range={tab}&amp;tz={timezone}&amp;spaces={selected}\" {}>{caption}</a>",
            if tab == key {
                "aria-current=\"page\""
            } else {
                ""
            }
        ));
    }
    let mut space_filters = format!(
        "<a href=\"/today?range={key}&amp;tz={timezone}\" {}>All spaces</a>",
        if selected.is_empty() {
            "aria-current=\"true\""
        } else {
            ""
        }
    );
    let selected_ids = selected
        .split(',')
        .filter(|v| !v.is_empty())
        .collect::<std::collections::BTreeSet<_>>();
    for (id, title, active) in spaces {
        let mut next = selected_ids.clone();
        if active {
            next.remove(id.as_str());
        } else {
            next.insert(id.as_str());
        }
        let next = next.into_iter().collect::<Vec<_>>().join(",");
        let display_title = title
            .rsplit_once(' ')
            .map_or(title.as_str(), |(name, suffix)| {
                if suffix.len() == 8 && suffix.bytes().all(|c| c.is_ascii_hexdigit()) {
                    name
                } else {
                    title.as_str()
                }
            })
            .trim_start_matches('@');
        space_filters.push_str(&format!(
            "<a href=\"/today?range={key}&amp;tz={timezone}&amp;spaces={next}\" {}>@{}</a>",
            if active { "aria-current=\"true\"" } else { "" },
            esc(display_title)
        ));
    }
    let csrf = s.csrf_for(&h);
    let sidebar = s
        .run(move |db| super::build_sidebar(db, &sidebar_actor, "", csrf))
        .await?;
    let template = PlanningPage {
        sidebar,
        title: label.into(),
        tabs,
        space_filters,
        selected_spaces: selected,
        today: today.to_string(),
        week_start: prefs.week_start,
        timezone,
        account_timezone: prefs.timezone,
        range: key,
        body,
    };
    let rendered = template
        .render()
        .map_err(|_| Error::Invalid("Could not render planning page".into()))?;
    Ok(Html(rendered).into_response())
}
async fn today(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<CalendarQuery>,
) -> Result<Response, ApiError> {
    calendar(
        s,
        h,
        q.range.unwrap_or_else(|| "today".into()),
        q.tz,
        q.spaces,
    )
    .await
}
async fn upcoming(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    calendar(s, h, "upcoming".into(), None, None).await
}
async fn recurring(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let a = s.auth(&h, false)?;
    let rows = s.run(move |db| db.recurring_series(&a)).await?;
    if rows.is_empty() {
        return render_feature(&s, &h, "Recurring", String::from("<div class=\"planning-empty\"><svg class=\"ui-icon\" aria-hidden=\"true\"><use href=\"/assets/icons.svg#repeat\"/></svg><h2>Make room for your routines.</h2><p>No repeating tasks yet. Open a task and choose ‘Repeat this task’ in its Planning panel to start a routine.</p></div>"), false).await;
    }
    let mut body = String::from("<ul class=\"recurring-series-list\">");
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
    body.push_str("</ul><p class=\"muted\">Each series creates at most one outstanding occurrence. Stopping preserves its tasks and history.</p>");
    render_feature(&s, &h, "Recurring", body, false).await
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
async fn notification_page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    let a = s.auth(&h, false)?;
    let actor = a.clone();
    let prefs = s.run(move |db| db.notification_preferences(&actor)).await?;
    let lists = s.run(move |db| {
        let mut query = db.conn.prepare("SELECT l.id,l.title,CASE WHEN l.workspace_id IS NULL THEN 'Personal lists' WHEN wa.rank>=1 THEN w.title ELSE 'Shared with you' END FROM lists l JOIN actor_list_access a ON a.list_id=l.id AND a.actor_id=?1 AND a.rank>=1 LEFT JOIN workspaces w ON w.id=l.workspace_id LEFT JOIN actor_workspace_access wa ON wa.workspace_id=w.id AND wa.actor_id=?1 WHERE l.archived=0 ORDER BY lower(l.title),l.id")?;
        Ok(query.query_map([&a], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<Result<Vec<_>,_>>()?)
    }).await?;
    let mut mutes = String::new();
    for (id, title, space) in lists {
        mutes.push_str(&format!("<label data-mute-label data-space=\"{}\"><input type=\"checkbox\" name=\"mute_{}\" value=\"yes\" {}><span data-mute-title>{}</span><small>{}</small></label>",esc(&space),esc(&id),if prefs.list_opt_out.contains(&id) { "checked" } else { "" },esc(&title),esc(&space)));
    }
    let check = |v:bool| if v { "checked" } else { "" };
    let body = format!(r#"<form method="post" class="notification-preferences"><input type="hidden" name="csrf" value="{csrf}">
    <section class="security-card"><h2>In-app notifications</h2><p>Your bell and notification center work even when push is off.</p>
    <label class="notification-check"><input type="checkbox" name="inbox_due" {due}>Overdue tasks</label>
    <label class="notification-check"><input type="checkbox" name="inbox_reminders" {reminders}>Scheduled reminders</label>
    <label class="notification-check"><input type="checkbox" name="inbox_updates" {updates}>Changes by others in shared lists and spaces</label></section>
    <section class="security-card"><h2>Push notifications</h2><p>Receive reminders, overdue alerts and shared updates on your devices. Permission is requested only when you enable a browser.</p><a class="secondary" href="/settings/devices">Manage push devices…</a></section>
    <section class="security-card"><h2>Email &amp; summaries</h2><label class="notification-check"><input type="checkbox" name="email_opt_in" {email}>Email reminders and summaries</label>
    <div class="notification-fields"><label>Daily summary time<input type="time" name="daily_time" value="{daily_time}" required></label><label>Weekly summary time<input type="time" name="weekly_time" value="{weekly_time}" required></label></div>
    <label class="notification-check"><input type="checkbox" name="daily_opt_in" {daily}>Daily email summary</label><label class="notification-check"><input type="checkbox" name="push_daily_opt_in" {push_daily}>Daily push summary</label>
    <label class="notification-check"><input type="checkbox" name="weekly_opt_in" {weekly}>Weekly email summary</label><label class="notification-check"><input type="checkbox" name="push_weekly_opt_in" {push_weekly}>Weekly push summary</label>
    <label>Weekly summary day (Monday = 1)<input type="number" name="weekly_day" min="1" max="7" value="{weekday}" required></label></section>
    <section class="security-card"><h2>Timing &amp; quiet hours</h2><p>Times use your account timezone: <strong>{timezone}</strong>. <a href="/settings/general">Change in General settings…</a></p><div class="notification-fields"><label>Quiet from<input type="time" name="quiet_start" value="{quiet_start}"></label><label>Until<input type="time" name="quiet_end" value="{quiet_end}"></label></div><p>Quiet hours delay email and push. Notifications still arrive silently in your inbox. Leave both times empty to turn quiet hours off; equal times mute delivery all day.</p></section>
    <section class="security-card"><h2>Muted lists</h2><p>Skip new notifications and delivery from these lists.</p><fieldset data-muted-lists><legend class="sr-only">Muted lists</legend><div data-mute-choices>{mutes}</div></fieldset></section><button class="primary">Save notification preferences</button></form>"#,
        csrf=esc(&s.csrf_for(&h)), due=check(prefs.inbox_due), reminders=check(prefs.inbox_reminders), updates=check(prefs.inbox_updates), email=check(prefs.email_opt_in), daily=check(prefs.daily_opt_in), weekly=check(prefs.weekly_opt_in), push_daily=check(prefs.push_daily_opt_in), push_weekly=check(prefs.push_weekly_opt_in), daily_time=esc(&prefs.daily_time), weekly_time=esc(&prefs.weekly_time), weekday=prefs.weekly_day,timezone=esc(&prefs.timezone),quiet_start=esc(prefs.quiet_start.as_deref().unwrap_or("")),quiet_end=esc(prefs.quiet_end.as_deref().unwrap_or("")));
    render_settings(&s, &h, "Notifications", body, "notifications").await
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
    let current_actor = a.clone();
    let current = s
        .run(move |db| db.notification_preferences(&current_actor))
        .await?;
    let prefs = Preferences {
        inbox_due: form.contains_key("inbox_due"),
        inbox_reminders: form.contains_key("inbox_reminders"),
        inbox_updates: form.contains_key("inbox_updates"),
        timezone: current.timezone,
        week_start: current.week_start,
        fun_animations: current.fun_animations,
        fun_animation: current.fun_animation,
        email_opt_in: form.contains_key("email_opt_in"),
        quiet_start: nonempty("quiet_start"),
        quiet_end: nonempty("quiet_end"),
        daily_opt_in: form.contains_key("daily_opt_in"),
        push_daily_opt_in: form.contains_key("push_daily_opt_in"),
        push_weekly_opt_in: form.contains_key("push_weekly_opt_in"),
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
    Ok(Redirect::to("/settings/notifications").into_response())
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
    #[test]
    fn planning_windows_respect_week_start_and_cross_year_boundaries() {
        let day = chrono::NaiveDate::from_ymd_opt(2027, 1, 3).unwrap();
        let (from, to, _) = calendar_range(day, "previous", 1).unwrap();
        assert_eq!(
            (from.to_string(), to.to_string()),
            ("2026-12-21".into(), "2026-12-27".into())
        );
        let (from, to, _) = calendar_range(day, "next", 7).unwrap();
        assert_eq!(
            (from.to_string(), to.to_string()),
            ("2027-01-10".into(), "2027-01-16".into())
        );
        let (_, past, _) = calendar_range(day, "past", 7).unwrap();
        let (previous, _, _) = calendar_range(day, "previous", 7).unwrap();
        assert_eq!(past + Duration::days(1), previous);
        assert!(calendar_range(day, "invalid", 1).is_err());
    }
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
        for path in ["/settings", "/settings/general", "/settings/notifications"] {
            let response = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .uri(path)
                        .header("cookie", "todoer_dev=browser")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let html = String::from_utf8(
                response
                    .into_body()
                    .collect()
                    .await
                    .unwrap()
                    .to_bytes()
                    .to_vec(),
            )
            .unwrap();
            assert!(html.contains("aria-label=\"Settings\""));
            assert!(!html.contains("OAuth is not configured"));
        }
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

async fn account_settings(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    render_settings(&s, &h, "Account", String::new(), "account").await
}

#[derive(Template)]
#[template(path = "general-settings.html")]
struct GeneralSettings {
    sidebar: super::Sidebar,
}
async fn general_settings(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let actor = s.auth(&h, false)?;
    let csrf = s.csrf_for(&h);
    let mut sidebar = s
        .run(move |db| super::build_sidebar(db, &actor, "", csrf))
        .await?;
    sidebar.settings_section = "general".into();
    Ok(Html(
        GeneralSettings { sidebar }
            .render()
            .map_err(|_| Error::Invalid("Could not render settings".into()))?,
    )
    .into_response())
}

#[derive(Template)]
#[template(path = "feature-page.html")]
struct FeaturePage {
    sidebar: super::Sidebar,
    title: String,
    body: String,
    review: bool,
}
pub(super) async fn render_feature(
    s: &AppState,
    h: &HeaderMap,
    title: &str,
    body: String,
    review: bool,
) -> Result<Response, ApiError> {
    let actor = s.auth(h, false)?;
    let csrf = s.csrf_for(h);
    let sidebar = s
        .run(move |db| super::build_sidebar(db, &actor, "", csrf))
        .await?;
    Ok(Html(
        FeaturePage {
            sidebar,
            title: title.into(),
            body,
            review,
        }
        .render()
        .map_err(|_| Error::Invalid("Could not render page".into()))?,
    )
    .into_response())
}

#[derive(Template)]
#[template(path = "settings-page.html")]
struct SettingsPage {
    sidebar: super::Sidebar,
    title: String,
    body: String,
}
pub(super) async fn render_settings(
    s: &AppState,
    h: &HeaderMap,
    title: &str,
    html: String,
    selected: &str,
) -> Result<Response, ApiError> {
    let actor = s.auth(h, false)?;
    let csrf = s.csrf_for(h);
    let mut sidebar = s
        .run(move |db| super::build_sidebar(db, &actor, "", csrf))
        .await?;
    sidebar.settings_section = selected.into();
    let mut body = if let Some(main) = html.find("<main") {
        let begin = html[main..].find('>').unwrap() + main + 1;
        html[begin..]
            .split("</main>")
            .next()
            .unwrap_or("")
            .to_owned()
    } else {
        html
    };
    if let Some(begin) = body.find("<h1") {
        if let Some(end) = body[begin..].find("</h1>") {
            body.replace_range(begin..begin + end + 5, "");
        }
    }
    Ok(Html(
        SettingsPage {
            sidebar,
            title: title.into(),
            body,
        }
        .render()
        .map_err(|_| Error::Invalid("Could not render settings".into()))?,
    )
    .into_response())
}
