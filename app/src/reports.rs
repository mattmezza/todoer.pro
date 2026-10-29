//! Human-readable, scoped exports. Authentication material is never included.
use crate::core::{Error, Store};
use crate::planning::PlanEntry;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ReportScope {
    pub kind: String,
    pub id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub format: Option<String>,
}
#[derive(Serialize)]
pub struct Report {
    pub title: String,
    pub generated_at: String,
    pub tasks: Vec<ReportTask>,
}
#[derive(Serialize)]
pub struct ReportTask {
    pub id: String,
    pub reference: String,
    pub parent_id: Option<String>,
    pub depth: usize,
    pub title: String,
    pub description: String,
    pub list: String,
    pub space: Option<String>,
    pub complete: bool,
    pub planned: Option<String>,
    pub time: Option<String>,
    pub due: Option<String>,
    pub priority: Option<u8>,
}
impl Store {
    pub fn readable_report(&self, actor: &str, scope: &ReportScope) -> Result<Report, Error> {
        let raw_id = scope.id.as_deref().unwrap_or("");
        let (title, mut entries) = match scope.kind.as_str() {
            "task" => {
                let tasks = self.subtree(actor, raw_id)?;
                let ids: HashSet<_> = tasks.iter().map(|t| t.id.clone()).collect();
                let t = tasks
                    .iter()
                    .find(|t| {
                        t.id == crate::defaults::resolve(&self.conn, "task", raw_id)
                            .unwrap_or_default()
                    })
                    .ok_or(Error::NotFound)?;
                (
                    t.title.clone(),
                    self.plan_entries(actor, None, None, Some(&t.list_id))?
                        .into_iter()
                        .filter(|e| ids.contains(&e.task.id))
                        .collect(),
                )
            }
            "list" => {
                let id = crate::defaults::resolve(&self.conn, "list", raw_id)?;
                let list = self.list_info(actor, &id)?;
                (
                    list.list.title,
                    self.plan_entries(actor, None, None, Some(&id))?,
                )
            }
            "workspace" | "space" => {
                let id = crate::defaults::resolve(&self.conn, "workspace", raw_id)?;
                let w = self.workspace(actor, &id)?;
                (
                    w.title,
                    self.plan_entries(actor, None, None, None)?
                        .into_iter()
                        .filter(|e| e.workspace_id.as_deref() == Some(id.as_str()))
                        .collect(),
                )
            }
            "inbox" => ("Inbox · needs planning".into(), self.intake_tasks(actor)?),
            "plan" => {
                let from = scope
                    .from
                    .as_deref()
                    .ok_or_else(|| Error::Invalid("Start date required".into()))?;
                let to = scope
                    .to
                    .as_deref()
                    .ok_or_else(|| Error::Invalid("End date required".into()))?;
                (
                    format!("Plan · {from} to {to}"),
                    self.plan_entries(actor, Some(from), Some(to), None)?,
                )
            }
            _ => {
                return Err(Error::Invalid(
                    "Choose space, list, task, inbox or plan".into(),
                ));
            }
        };
        let included: HashSet<_> = entries.iter().map(|e| e.task.id.clone()).collect();
        let mut parents: HashMap<Option<String>, Vec<PlanEntry>> = HashMap::new();
        for e in entries.drain(..) {
            let parent = e
                .task
                .parent_id
                .as_ref()
                .filter(|p| included.contains(*p))
                .cloned();
            parents.entry(parent).or_default().push(e);
        }
        let mut ordered = Vec::new();
        fn emit(
            parents: &mut HashMap<Option<String>, Vec<PlanEntry>>,
            parent: Option<String>,
            depth: usize,
            out: &mut Vec<(PlanEntry, usize)>,
        ) {
            if let Some(mut entries) = parents.remove(&parent) {
                entries.sort_by_key(|e| {
                    (
                        e.space.clone(),
                        e.list.clone(),
                        e.task.position,
                        e.task.id.clone(),
                    )
                });
                for e in entries {
                    let id = e.task.id.clone();
                    out.push((e, depth));
                    emit(parents, Some(id), depth + 1, out);
                }
            }
        }
        emit(&mut parents, None, 0, &mut ordered);
        if !parents.is_empty() {
            return Err(Error::Invalid("Invalid task hierarchy".into()));
        }
        let prefs =
            self.notification_preferences(&crate::core::actor_identity(&self.conn, actor)?.0)?;
        let tz: chrono_tz::Tz = prefs
            .timezone
            .parse()
            .map_err(|_| Error::Invalid("Invalid timezone".into()))?;
        let tasks = ordered
            .into_iter()
            .map(|(e, depth)| {
                Ok(ReportTask {
                    id: e.task.id.clone(),
                    reference: format!(
                        "{}{}",
                        e.task.reference_prefix(),
                        crate::defaults::public_id(&self.conn, "task", &e.task.id)?
                    ),
                    parent_id: e.task.parent_id,
                    depth,
                    title: e.task.title,
                    description: e.task.description,
                    list: e.list,
                    space: e.space,
                    complete: e.task.complete,
                    planned: e.schedule.planned.as_ref().map(|m| moment(m, tz)),
                    time: e.local_time,
                    due: e.schedule.due.as_ref().map(|m| moment(m, tz)),
                    priority: e.schedule.priority,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let prefs =
            self.notification_preferences(&crate::core::actor_identity(&self.conn, actor)?.0)?;
        let tz: chrono_tz::Tz = prefs
            .timezone
            .parse()
            .map_err(|_| Error::Invalid("Invalid timezone".into()))?;
        Ok(Report {
            title,
            generated_at: chrono::Utc::now()
                .with_timezone(&tz)
                .format("%b %d, %Y, %H:%M")
                .to_string(),
            tasks,
        })
    }
}
fn moment(m: &crate::scheduling::Moment, zone: chrono_tz::Tz) -> String {
    use chrono::TimeZone;
    match m {
        crate::scheduling::Moment::Date { date } => date.clone(),
        crate::scheduling::Moment::Timed { local, timezone } => timezone
            .parse::<chrono_tz::Tz>()
            .ok()
            .and_then(|source| {
                chrono::NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M")
                    .ok()
                    .and_then(|date| source.from_local_datetime(&date).earliest())
            })
            .map(|date| {
                date.with_timezone(&zone)
                    .format("%b %d, %Y, %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| local.replace('T', " ")),
    }
}
pub fn text(report: &Report) -> String {
    let done = report.tasks.iter().filter(|t| t.complete).count();
    let mut s = format!(
        "TODOER\n{}\n{}\n{}\n\n{} / {} tasks complete\n\n",
        report.title,
        "=".repeat(report.title.chars().count().min(70)),
        report.generated_at,
        done,
        report.tasks.len()
    );
    let mut group = String::new();
    for t in &report.tasks {
        let current = format!(
            "{} / #{}",
            t.space
                .as_ref()
                .map(|s| format!("@{s}"))
                .unwrap_or_else(|| "Personal".into()),
            t.list
        );
        if current != group {
            s.push_str(&format!(
                "{current}\n{}\n",
                "-".repeat(current.chars().count().min(70))
            ));
            group = current;
        }
        let indent = "  ".repeat(t.depth);
        s.push_str(&format!(
            "{indent}- [{}] {} ({})\n",
            if t.complete { "x" } else { " " },
            t.title,
            t.reference
        ));
        for line in t.description.lines() {
            s.push_str(&format!("{indent}  {line}\n"));
        }
        let meta = [
            t.planned.as_ref().map(|d| {
                format!(
                    "Planned: {d}{}",
                    t.time.as_ref().map(|v| format!(" {v}")).unwrap_or_default()
                )
            }),
            t.due.as_ref().map(|d| format!("Due: {d}")),
            t.priority.map(|p| format!("P{p}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        if !meta.is_empty() {
            s.push_str(&format!("{indent}  {meta}\n"));
        }
        s.push('\n');
    }
    s
}
fn safe_cell(v: &str) -> String {
    if v.trim_start().starts_with(['=', '+', '-', '@']) || v.starts_with(['\t', '\n', '\r']) {
        format!("'{v}")
    } else {
        v.into()
    }
}
pub fn delimited(report: &Report, delimiter: u8) -> Result<Vec<u8>, Error> {
    let mut w = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());
    w.write_record([
        "reference",
        "parent_id",
        "depth",
        "space",
        "list",
        "title",
        "description",
        "complete",
        "planned",
        "time",
        "due",
        "priority",
    ])
    .map_err(|_| Error::Invalid("Could not encode export".into()))?;
    for t in &report.tasks {
        w.write_record(
            [
                t.reference.clone(),
                t.parent_id.clone().unwrap_or_default(),
                t.depth.to_string(),
                t.space.clone().unwrap_or_default(),
                t.list.clone(),
                t.title.clone(),
                t.description.clone(),
                t.complete.to_string(),
                t.planned.clone().unwrap_or_default(),
                t.time.clone().unwrap_or_default(),
                t.due.clone().unwrap_or_default(),
                t.priority.map(|p| format!("P{p}")).unwrap_or_default(),
            ]
            .map(|s| safe_cell(&s)),
        )
        .map_err(|_| Error::Invalid("Could not encode export".into()))?;
    }
    w.into_inner()
        .map_err(|_| Error::Invalid("Could not encode export".into()))
}
pub fn pdf(report: &Report) -> Result<Vec<u8>, Error> {
    use genpdf::{
        Element,
        elements::Paragraph,
        style::{Color, Style},
    };
    let fail = |e: genpdf::error::Error| Error::Invalid(format!("Could not render PDF: {e}"));
    let regular = genpdf::fonts::FontData::new(
        include_bytes!("../assets/fonts/ReportSans.ttf").to_vec(),
        None,
    )
    .map_err(fail)?;
    let bold = genpdf::fonts::FontData::new(
        include_bytes!("../assets/fonts/ReportSans-Bold.ttf").to_vec(),
        None,
    )
    .map_err(fail)?;
    let family = genpdf::fonts::FontFamily {
        regular: regular.clone(),
        bold: bold.clone(),
        italic: regular,
        bold_italic: bold,
    };
    let mut doc = genpdf::Document::new(family);
    doc.set_title(&report.title);
    doc.set_font_size(10);
    doc.set_line_spacing(1.4);
    doc.set_minimal_conformance();
    let mut decorator = genpdf::SimplePageDecorator::new();
    decorator.set_margins(20);
    decorator.set_header(|page| {
        Paragraph::new(format!(
            "TODOER                                                    {page:02}"
        ))
        .styled(
            Style::new()
                .with_font_size(8)
                .with_color(Color::Rgb(110, 120, 130)),
        )
        .padded((0, 0, 8, 0))
    });
    doc.set_page_decorator(decorator);
    doc.push(
        Paragraph::new(&report.title).styled(
            Style::new()
                .bold()
                .with_font_size(26)
                .with_color(Color::Rgb(28, 45, 62)),
        ),
    );
    doc.push(
        Paragraph::new(&report.generated_at).styled(
            Style::new()
                .with_font_size(9)
                .with_color(Color::Rgb(110, 120, 130)),
        ),
    );
    let completed = report.tasks.iter().filter(|t| t.complete).count();
    doc.push(
        Paragraph::new(format!(
            "{completed} of {} tasks complete",
            report.tasks.len()
        ))
        .padded((5, 0, 8, 0)),
    );
    let mut group = String::new();
    for t in &report.tasks {
        let current = format!(
            "{}  /  {}",
            t.space.as_deref().unwrap_or("Personal"),
            t.list
        );
        if current != group {
            doc.push(
                Paragraph::new(&current)
                    .styled(
                        Style::new()
                            .bold()
                            .with_font_size(13)
                            .with_color(Color::Rgb(40, 80, 110)),
                    )
                    .padded((5, 0, 3, 0)),
            );
            group = current;
        }
        let indent = (t.depth.min(12) * 5) as u32;
        doc.push(
            Paragraph::new(format!(
                "[{}] {}",
                if t.complete { "x" } else { " " },
                t.title
            ))
            .styled(Style::new().bold().with_font_size(11))
            .padded((2, 0, 1, indent)),
        );
        doc.push(
            Paragraph::new(format!(
                "{}{}{}",
                t.reference,
                t.planned
                    .as_ref()
                    .map(|v| format!(
                        "   ·   {v}{}",
                        t.time.as_ref().map(|v| format!(" {v}")).unwrap_or_default()
                    ))
                    .unwrap_or_default(),
                t.priority
                    .map(|p| format!("   ·   P{p}"))
                    .unwrap_or_default()
            ))
            .styled(
                Style::new()
                    .with_font_size(8)
                    .with_color(Color::Rgb(110, 120, 130)),
            )
            .padded((0, 0, 2, indent + 6)),
        );
        if !t.description.is_empty() {
            doc.push(
                Paragraph::new(&t.description)
                    .styled(Style::new().with_color(Color::Rgb(80, 90, 100)))
                    .padded((0, 0, 4, indent + 6)),
            );
        }
    }
    let mut bytes = Vec::new();
    doc.render(&mut bytes).map_err(fail)?;
    Ok(bytes)
}
