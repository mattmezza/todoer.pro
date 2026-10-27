//! Authorized literal-prefix FTS search. Cursors bind both scope and index state.
use crate::core::{Error, Store, actor_identity, actor_user};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::Value as SqlValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    #[default]
    Relevance,
    Title,
    Created,
    Updated,
    Planned,
    Due,
    Id,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Query {
    pub text: String,
    pub entities: Option<Vec<String>>,
    pub workspace_id: Option<String>,
    pub list_id: Option<String>,
    pub section_id: Option<String>,
    pub tag: Option<String>,
    pub status: String,
    pub priority: Option<u8>,
    pub planned_from: Option<String>,
    pub planned_to: Option<String>,
    pub due_from: Option<String>,
    pub due_to: Option<String>,
    pub recurrence: Option<String>,
    pub limit: usize,
    pub cursor: Option<String>,
    pub sort: Sort,
    pub fields: Option<Vec<String>>,
}
impl Default for Query {
    fn default() -> Self {
        Self {
            text: String::new(),
            entities: None,
            workspace_id: None,
            list_id: None,
            section_id: None,
            tag: None,
            status: "open".into(),
            priority: None,
            planned_from: None,
            planned_to: None,
            due_from: None,
            due_to: None,
            recurrence: None,
            limit: 50,
            cursor: None,
            sort: Sort::default(),
            fields: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    pub kind: String,
    pub id: String,
    pub title: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub kind: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<ContextItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub total: usize,
    pub next_cursor: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Cursor {
    binding: String,
    key: String,
    kind: String,
    id: String,
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
const KINDS: &[&str] = &[
    "workspace",
    "list",
    "section",
    "task",
    "comment",
    "tag",
    "attachment",
];
const FIELDS: &[&str] = &["title", "snippet", "context", "schedule", "timestamps"];
fn validate(q: &Query) -> Result<(), Error> {
    if q.text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .count()
        > 32
    {
        return Err(invalid("search text must contain at most 32 tokens"));
    }
    if q.limit == 0 || q.limit > 100 || q.text.chars().count() > 500 {
        return Err(invalid(
            "limit must be 1..100 and text at most 500 characters",
        ));
    }
    if !["all", "open", "completed"].contains(&q.status.as_str()) {
        return Err(invalid("status must be all, open or completed"));
    }
    if q.priority.is_some_and(|p| !(1..=3).contains(&p)) {
        return Err(invalid("priority must be 1..3"));
    }
    if q.entities
        .as_ref()
        .is_some_and(|v| v.is_empty() || v.iter().any(|s| !KINDS.contains(&s.as_str())))
    {
        return Err(invalid("unknown or empty entities"));
    }
    if q.fields
        .as_ref()
        .is_some_and(|v| v.iter().any(|s| !FIELDS.contains(&s.as_str())))
    {
        return Err(invalid("unknown projection field"));
    }
    if q.recurrence
        .as_ref()
        .is_some_and(|s| !["none", "any", "active", "paused", "stopped"].contains(&s.as_str()))
    {
        return Err(invalid("invalid recurrence filter"));
    }
    for date in [&q.planned_from, &q.planned_to, &q.due_from, &q.due_to]
        .into_iter()
        .flatten()
    {
        if date.len() != 10 || NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
            return Err(invalid(
                "date filters must be YYYY-MM-DD local calendar dates",
            ));
        }
    }
    for (from, to) in [(&q.planned_from, &q.planned_to), (&q.due_from, &q.due_to)] {
        if let (Some(a), Some(b)) = (from, to)
            && a > b
        {
            return Err(invalid("date range is reversed"));
        }
    }
    Ok(())
}
// Every token is quoted. User syntax is never passed to the FTS parser.
fn fts_expression(text: &str) -> String {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .take(32)
        .map(|s| format!("\"{}\"*", s.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}
fn bind(values: &mut Vec<SqlValue>, value: impl Into<SqlValue>) -> String {
    values.push(value.into());
    format!("?{}", values.len())
}
fn scope_fingerprint(c: &Connection, actor: &str) -> Result<String, Error> {
    let mut q=c.prepare("SELECT 'list',list_id,rank FROM actor_list_access WHERE actor_id=?1 UNION ALL SELECT 'workspace',workspace_id,rank FROM actor_workspace_access WHERE actor_id=?1 ORDER BY 1,2,3")?;
    let rows = q
        .query_map([actor], |r| {
            Ok(format!(
                "{}:{}:{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(digest(&rows.join("\n")))
}
impl Store {
    pub fn query(&self, actor: &str, query: &Query) -> Result<SearchPage, Error> {
        let snapshot = if self.conn.is_autocommit() {
            Some(self.conn.unchecked_transaction()?)
        } else {
            None
        };
        let (user, _) = actor_identity(&self.conn, actor)?;
        actor_user(&self.conn, &user, 1)?;
        validate(query)?;
        let mut canonical = query.clone();
        canonical.cursor = None;
        let generation: String = self.conn.query_row(
            "SELECT value FROM search_generation WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let binding = digest(&format!(
            "{}:{}:{}:{}",
            actor,
            scope_fingerprint(&self.conn, actor)?,
            generation,
            serde_json::to_string(&canonical).map_err(|_| invalid("invalid query"))?
        ));
        let cursor: Option<Cursor> = query
            .cursor
            .as_ref()
            .map(|v| {
                if v.len() > 4096 {
                    return Err(invalid("invalid cursor"));
                }
                URL_SAFE_NO_PAD
                    .decode(v)
                    .map_err(|_| invalid("invalid cursor"))
                    .and_then(|raw| {
                        serde_json::from_slice(&raw).map_err(|_| invalid("invalid cursor"))
                    })
            })
            .transpose()?;
        if cursor.as_ref().is_some_and(|v| v.binding != binding) {
            return Err(Error::Conflict(
                "resync_required: query, access or indexed data changed; restart without cursor"
                    .into(),
            ));
        }
        let mut values = vec![SqlValue::Text(actor.into())];
        let mut list_scope = String::new();
        if let Some(id) = &query.workspace_id {
            list_scope += &format!(" AND l.workspace_id={}", bind(&mut values, id.clone()));
        }
        if let Some(id) = &query.list_id {
            list_scope += &format!(" AND l.id={}", bind(&mut values, id.clone()));
        }
        // A recursive tree restricted to accessible lists gives inherited root sections.
        let mut sql=format!("WITH RECURSIVE allowed_lists AS MATERIALIZED (SELECT l.* FROM lists l JOIN actor_list_access a ON a.list_id=l.id AND a.actor_id=?1 AND a.rank>=1 WHERE l.archived=0 {list_scope}), allowed_workspaces AS MATERIALIZED (SELECT w.* FROM workspaces w JOIN actor_workspace_access a ON a.workspace_id=w.id AND a.actor_id=?1 AND a.rank>=1), tree(id,root,section_id) AS (SELECT t.id,t.id,x.section_id FROM tasks t JOIN allowed_lists l ON l.id=t.list_id LEFT JOIN task_locations x ON x.task_id=t.id WHERE t.parent_id IS NULL AND t.deleted_at IS NULL UNION ALL SELECT t.id,p.root,p.section_id FROM tasks t INDEXED BY tasks_parent JOIN tree p ON p.id=t.parent_id WHERE t.deleted_at IS NULL), sources(kind,id,list_id,workspace_id,task_id,section_id) AS MATERIALIZED (
SELECT 'workspace',w.id,NULL,w.id,NULL,NULL FROM allowed_workspaces w
UNION ALL SELECT 'list',l.id,l.id,l.workspace_id,NULL,NULL FROM allowed_lists l
UNION ALL SELECT 'section',s.id,l.id,l.workspace_id,NULL,s.id FROM sections s JOIN allowed_lists l ON l.id=s.list_id
UNION ALL SELECT 'task',t.id,l.id,l.workspace_id,t.id,tr.section_id FROM tasks t JOIN allowed_lists l ON l.id=t.list_id JOIN tree tr ON tr.id=t.id
UNION ALL SELECT 'comment',c.id,l.id,l.workspace_id,t.id,tr.section_id FROM comments c JOIN tasks t ON t.id=c.task_id JOIN allowed_lists l ON l.id=t.list_id JOIN tree tr ON tr.id=t.id WHERE c.deleted_at IS NULL
UNION ALL SELECT 'attachment',a.id,l.id,l.workspace_id,t.id,tr.section_id FROM attachments a JOIN tasks t ON t.id=a.task_id JOIN allowed_lists l ON l.id=t.list_id JOIN tree tr ON tr.id=t.id WHERE a.deleted_at IS NULL AND (a.comment_id IS NULL OR EXISTS(SELECT 1 FROM comments c WHERE c.id=a.comment_id AND c.deleted_at IS NULL))
UNION ALL SELECT 'legacy_attachment',a.id,l.id,l.workspace_id,t.id,tr.section_id FROM legacy_attachment_refs a JOIN tasks t ON t.id=a.task_id JOIN allowed_lists l ON l.id=t.list_id JOIN tree tr ON tr.id=t.id
UNION ALL SELECT 'tag',g.id,(SELECT MIN(t.list_id) FROM task_tags tt JOIN tasks t ON t.id=tt.task_id JOIN allowed_lists l ON l.id=t.list_id JOIN tree live ON live.id=t.id WHERE tt.tag_id=g.id AND t.deleted_at IS NULL),CASE WHEN g.scope LIKE 'workspace:%' THEN substr(g.scope,11) ELSE NULL END,NULL,NULL FROM tags g WHERE EXISTS(SELECT 1 FROM task_tags tt JOIN tasks t ON t.id=tt.task_id JOIN allowed_lists l ON l.id=t.list_id JOIN tree live ON live.id=t.id WHERE tt.tag_id=g.id AND t.deleted_at IS NULL) OR EXISTS(SELECT 1 FROM allowed_workspaces w WHERE g.scope='workspace:'||w.id)
), candidates AS MATERIALIZED (SELECT d.rowid,d.kind,d.id,d.title,CASE WHEN d.kind='task' THEN t.description ELSE d.body END body,s.list_id,s.workspace_id,s.task_id,s.section_id,t.complete,d.created_at,d.updated_at,json_extract(sc.document,'$.planned.date') planned,substr(COALESCE(json_extract(sc.document,'$.due.date'),json_extract(sc.document,'$.due.local')),1,10) due,json_extract(sc.document,'$.due.timezone') due_timezone,json_extract(sc.document,'$.priority') priority,rs.state recurrence FROM search_documents d JOIN sources s ON s.kind=d.kind AND s.id=d.id LEFT JOIN tasks t ON t.id=s.task_id LEFT JOIN task_schedule sc ON sc.task_id=s.task_id LEFT JOIN recurrence_occurrences ro ON ro.task_id=s.task_id LEFT JOIN recurrence_series rs ON rs.id=ro.series_id WHERE 1=1");
        let expression = fts_expression(&query.text);
        let exact_ref = query
            .text
            .trim()
            .strip_prefix('%')
            .unwrap_or(query.text.trim());
        if !query.text.trim().is_empty() {
            if expression.is_empty() {
                sql += " AND 0";
            } else {
                let p = bind(&mut values, expression);
                let reference = bind(&mut values, exact_ref.to_string());
                sql += &format!(
                    " AND (d.rowid IN (SELECT rowid FROM search_fts WHERE search_fts MATCH {p}) OR (d.kind='task' AND (t.id={reference} OR t.reference={reference} OR EXISTS(SELECT 1 FROM task_public_ids p WHERE p.task_id=t.id AND p.short_id={reference}) OR (length({reference})>=8 AND substr(t.id,1,length({reference}))={reference}))))"
                );
            }
        }
        if let Some(entities) = &query.entities {
            let mut kinds = entities.clone();
            if kinds.iter().any(|s| s == "attachment") {
                kinds.push("legacy_attachment".into());
            }
            let placeholders = kinds
                .into_iter()
                .map(|s| bind(&mut values, s))
                .collect::<Vec<_>>()
                .join(",");
            sql += &format!(" AND d.kind IN ({placeholders})");
        }
        if let Some(id) = &query.workspace_id {
            let p = bind(&mut values, id.clone());
            sql += &format!(" AND s.workspace_id={p}");
        }
        if query.list_id.is_some() {
            sql += " AND s.list_id IS NOT NULL";
        }
        if let Some(id) = &query.section_id {
            let p = bind(&mut values, id.clone());
            sql += &format!(" AND s.section_id={p}");
        }
        if query.status != "all" {
            sql += if query.status == "completed" {
                " AND t.complete=1"
            } else {
                " AND (t.complete=0 OR t.id IS NULL)"
            };
        }
        if let Some(p) = query.priority {
            let p = bind(&mut values, i64::from(p));
            sql += &format!(" AND json_extract(sc.document,'$.priority')={p}");
        }
        for (column, value, operator) in [
            ("planned", &query.planned_from, ">="),
            ("planned", &query.planned_to, "<="),
            ("due", &query.due_from, ">="),
            ("due", &query.due_to, "<="),
        ] {
            if let Some(v) = value {
                let p = bind(&mut values, v.clone());
                sql += &format!(" AND {column}{operator}{p}");
            }
        }
        if let Some(r) = &query.recurrence {
            match r.as_str() {
                "none" => sql += " AND t.id IS NOT NULL AND rs.id IS NULL",
                "any" => sql += " AND rs.id IS NOT NULL",
                _ => {
                    let p = bind(&mut values, r.clone());
                    sql += &format!(" AND rs.state={p}");
                }
            }
        }
        if let Some(tag) = &query.tag {
            let mut statement=self.conn.prepare("WITH RECURSIVE visible_tasks(id) AS (SELECT t.id FROM tasks t JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?1 WHERE t.parent_id IS NULL AND t.deleted_at IS NULL UNION ALL SELECT t.id FROM tasks t INDEXED BY tasks_parent JOIN visible_tasks p ON p.id=t.parent_id WHERE t.deleted_at IS NULL) SELECT DISTINCT g.id FROM tags g JOIN task_tags tt ON tt.tag_id=g.id JOIN tasks t ON t.id=tt.task_id JOIN visible_tasks v ON v.id=t.id JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?1 WHERE t.deleted_at IS NULL AND (g.id=?2 OR g.normalized_name=?3) AND (?4 IS NULL OR t.list_id=?4) AND (?5 IS NULL OR EXISTS(SELECT 1 FROM lists l WHERE l.id=t.list_id AND l.workspace_id=?5)) ORDER BY CASE WHEN g.id=?2 THEN 0 ELSE 1 END,g.id LIMIT 2")?;
            let mut ids = statement
                .query_map(
                    params![
                        actor,
                        tag,
                        tag.to_lowercase(),
                        query.list_id,
                        query.workspace_id
                    ],
                    |r| r.get::<_, String>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?;
            if ids.first().is_some_and(|id| id == tag) {
                ids.truncate(1);
            }
            if ids.len() > 1 {
                return Err(Error::Conflict(format!(
                    "ambiguous tag; use an accessible tag ID: {}",
                    ids.join(", ")
                )));
            }
            if let Some(id) = ids.first() {
                let p = bind(&mut values, id.clone());
                sql += &format!(
                    " AND EXISTS(SELECT 1 FROM task_tags tt WHERE tt.task_id=s.task_id AND tt.tag_id={p})"
                );
            } else {
                sql += " AND 0";
            }
        }
        sql += "), ranked AS MATERIALIZED (SELECT *,";
        let key = match query.sort {
            Sort::Id => "id".to_string(),
            Sort::Title => "lower(title)".into(),
            Sort::Created => "printf('%020d',COALESCE(created_at,0))".into(),
            Sort::Updated => "printf('%020d',COALESCE(updated_at,0))".into(),
            Sort::Planned => "COALESCE(planned,'9999-99-99')".into(),
            Sort::Due => "COALESCE(due,'9999-99-99')".into(),
            Sort::Relevance => {
                let p = bind(&mut values, query.text.to_lowercase());
                let tokens = fts_expression(&query.text);
                let title_match = if tokens.is_empty() {
                    String::new()
                } else {
                    let title_query = bind(&mut values, format!("title : ({tokens})"));
                    format!(
                        " WHEN rowid IN (SELECT rowid FROM search_fts WHERE search_fts MATCH {title_query}) THEN '2'"
                    )
                };
                format!(
                    "CASE WHEN lower(title)={p} THEN '0' WHEN substr(lower(title),1,length({p}))={p} THEN '1' WHEN instr(lower(title),{p})>0 THEN '2'{title_match} ELSE '3' END"
                )
            }
        };
        sql += &format!("{key} sort_key FROM candidates)");
        let total: i64 = self.conn.query_row(
            &(sql.clone() + " SELECT COUNT(*) FROM ranked"),
            params_from_iter(values.iter()),
            |r| r.get(0),
        )?;
        sql += " SELECT kind,id,title,body,list_id,workspace_id,task_id,section_id,complete,created_at,updated_at,planned,due,due_timezone,priority,recurrence,sort_key FROM ranked WHERE 1=1";
        if let Some(c) = cursor {
            let key = bind(&mut values, c.key);
            let kind = bind(&mut values, c.kind);
            let id = bind(&mut values, c.id);
            sql += &format!(" AND (sort_key,kind,id)>({key},{kind},{id})");
        }
        let limit = bind(&mut values, (query.limit + 1) as i64);
        sql += &format!(" ORDER BY sort_key,kind,id LIMIT {limit}");
        let wanted = |field: &str| {
            query
                .fields
                .as_ref()
                .is_none_or(|v| v.iter().any(|s| s == field))
        };
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement
            .query_map(params_from_iter(values.iter()), |r| {
                Ok((
                    SearchHit {
                        kind: r.get(0)?,
                        id: r.get(1)?,
                        title: Some(r.get(2)?),
                        snippet: Some(r.get(3)?),
                        list_id: r.get(4)?,
                        task_id: r.get(6)?,
                        complete: r.get(8)?,
                        created_at: r.get(9)?,
                        updated_at: r.get(10)?,
                        planned: r.get(11)?,
                        due: r.get(12)?,
                        due_timezone: r.get(13)?,
                        priority: r.get(14)?,
                        recurrence: r.get(15)?,
                        context: None,
                        context_truncated: None,
                    },
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, String>(16)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = rows.len() > query.limit;
        let mut hits = Vec::new();
        let mut next_cursor = None;
        for (mut hit, workspace, section, key) in rows.into_iter().take(query.limit) {
            if has_more {
                next_cursor = Some(
                    URL_SAFE_NO_PAD.encode(
                        serde_json::to_vec(&Cursor {
                            binding: binding.clone(),
                            key,
                            kind: hit.kind.clone(),
                            id: hit.id.clone(),
                        })
                        .map_err(|_| invalid("cannot encode cursor"))?,
                    ),
                );
            }
            if wanted("context") {
                let (items, truncated) = context(
                    &self.conn,
                    actor,
                    &hit,
                    workspace.as_deref(),
                    section.as_deref(),
                )?;
                hit.context = Some(items);
                hit.context_truncated = Some(truncated);
            }
            if wanted("snippet") {
                hit.snippet = hit.snippet.map(|s| excerpt(&s, &query.text));
            } else {
                hit.snippet = None;
            }
            if !wanted("title") {
                hit.title = None;
            }
            if !wanted("schedule") {
                hit.planned = None;
                hit.due = None;
                hit.due_timezone = None;
                hit.priority = None;
                hit.recurrence = None;
            }
            if !wanted("timestamps") {
                hit.created_at = None;
                hit.updated_at = None;
            }
            if hit.kind == "legacy_attachment" {
                hit.kind = "attachment".into();
            }
            hits.push(hit);
        }
        let page = SearchPage {
            hits,
            total: total as usize,
            next_cursor,
        };
        drop(statement);
        if let Some(snapshot) = snapshot {
            snapshot.commit()?;
        }
        Ok(page)
    }
}
fn excerpt(body: &str, text: &str) -> String {
    let chars: Vec<char> = body.chars().collect();
    let first = text
        .split(|c: char| !c.is_alphanumeric())
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_lowercase();
    let start = if first.is_empty() {
        0
    } else {
        body.to_lowercase()
            .find(&first)
            .map(|byte| {
                body.to_lowercase()[..byte]
                    .chars()
                    .count()
                    .saturating_sub(40)
            })
            .unwrap_or(0)
    };
    let start = start.min(chars.len());
    let end = (start + 220).min(chars.len());
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        chars[start..end].iter().collect::<String>(),
        if end < chars.len() { "…" } else { "" }
    )
}
fn context(
    c: &Connection,
    actor: &str,
    hit: &SearchHit,
    workspace: Option<&str>,
    section: Option<&str>,
) -> Result<(Vec<ContextItem>, bool), Error> {
    let mut result = Vec::new();
    let mut truncated = false;
    if let Some(id)=workspace && let Some(title)=c.query_row("SELECT w.title FROM workspaces w JOIN actor_workspace_access a ON a.workspace_id=w.id AND a.actor_id=?1 WHERE w.id=?2",params![actor,id],|r|r.get::<_,String>(0)).optional()?{result.push(ContextItem{kind:"workspace".into(),id:id.into(),title});}
    if let Some(id) = &hit.list_id {
        let title = c.query_row("SELECT title FROM lists WHERE id=?1", [id], |r| r.get(0))?;
        result.push(ContextItem {
            kind: "list".into(),
            id: id.clone(),
            title,
        });
    }
    if let Some(id) = section {
        let title = c.query_row("SELECT title FROM sections WHERE id=?1", [id], |r| r.get(0))?;
        result.push(ContextItem {
            kind: "section".into(),
            id: id.into(),
            title,
        });
    }
    if let Some(id) = &hit.task_id {
        let mut q=c.prepare("WITH RECURSIVE parents(id,parent_id,title,depth) AS (SELECT id,parent_id,title,0 FROM tasks WHERE id=?1 UNION ALL SELECT t.id,t.parent_id,t.title,p.depth+1 FROM tasks t JOIN parents p ON p.parent_id=t.id WHERE p.depth<16 AND t.deleted_at IS NULL) SELECT id,title,parent_id,depth FROM parents WHERE depth>0 OR ?2!='task' ORDER BY depth DESC")?;
        for item in q.query_map(params![id, hit.kind], |r| {
            Ok((
                ContextItem {
                    kind: "task".into(),
                    id: r.get(0)?,
                    title: r.get(1)?,
                },
                r.get::<_, Option<String>>(2)?.is_some() && r.get::<_, i64>(3)? == 16,
            ))
        })? {
            let (item, more) = item?;
            truncated |= more;
            result.push(item);
        }
    }
    for item in &mut result {
        if item.title.chars().count() > 120 {
            item.title = format!("{}…", item.title.chars().take(120).collect::<String>());
            truncated = true;
        }
    }
    Ok((result, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Store {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db
    }
    fn task(
        db: &Store,
        id: &str,
        list: &str,
        title: &str,
        description: &str,
        parent: Option<&str>,
    ) {
        db.conn.execute("INSERT INTO tasks(id,reference,list_id,parent_id,title,description,position) VALUES(?1,?2,?3,?4,?5,?6,0)",params![id,format!("T-{id}"),list,parent,title,description]).unwrap();
    }
    fn q(text: &str) -> Query {
        Query {
            text: text.into(),
            entities: Some(vec!["task".into()]),
            ..Query::default()
        }
    }
    #[test]
    fn unicode_prefix_literal_references_and_typo_boundaries() {
        let db = db();
        task(
            &db,
            "abcdefgh-opaque",
            "inbox",
            "Über café 東京",
            "A descriptive proposal",
            None,
        );
        for text in [
            "Üb",
            "cafe",
            "東",
            "descrip",
            "%T-abcdefgh-opaque",
            "%abcdefgh",
        ] {
            assert_eq!(db.query("dev", &q(text)).unwrap().total, 1, "{text}");
        }
        for text in [
            "ber",
            "cafee",
            "proposla",
            "OR NEAR",
            "\" OR *",
            "desc NOT proposal",
        ] {
            assert_eq!(db.query("dev", &q(text)).unwrap().total, 0, "{text}");
        }
        assert!(
            db.query("dev", &q("東京")).unwrap().hits[0]
                .snippet
                .as_ref()
                .unwrap()
                .contains("descriptive")
        );
    }
    #[test]
    fn authorization_precedes_counts_snippets_and_live_revocation() {
        let db = db();
        task(
            &db,
            "visible",
            "inbox",
            "needle public",
            "Allowed excerpt",
            None,
        );
        task(
            &db,
            "secret",
            "private",
            "needle secret",
            "Never return secret body",
            None,
        );
        db.conn.execute("INSERT INTO comments(id,task_id,author_id,body) VALUES('hidden-comment','secret','other','needle hidden comment')",[]).unwrap();
        let query = Query {
            text: "needle".into(),
            status: "all".into(),
            ..Query::default()
        };
        let page = db.query("dev", &query).unwrap();
        assert_eq!(page.total, 1);
        assert!(!serde_json::to_string(&page).unwrap().contains("secret"));
        db.conn
            .execute("INSERT INTO memberships VALUES('private','dev')", [])
            .unwrap();
        assert_eq!(db.query("dev", &query).unwrap().total, 3);
        db.conn
            .execute(
                "DELETE FROM memberships WHERE list_id='private' AND user_id='dev'",
                [],
            )
            .unwrap();
        assert_eq!(db.query("dev", &query).unwrap().total, 1);
    }
    #[test]
    fn independent_comment_and_filename_documents_follow_delete_restore() {
        let db = db();
        task(&db, "t", "inbox", "Root", "ordinary", None);
        db.conn.execute("INSERT INTO comments(id,task_id,author_id,body) VALUES('c1','t','dev','needle first'),('c2','t','dev','needle second')",[]).unwrap();
        db.conn
            .execute(
                "INSERT INTO transfer_sources(id,schema_hash) VALUES('source','hash')",
                [],
            )
            .unwrap();
        db.conn.execute("INSERT INTO legacy_attachment_refs(id,task_id,filename,mime,size,object_key,source_id) VALUES('a','t','needle-report.pdf','application/pdf',0,'key','source')",[]).unwrap();
        let query = Query {
            text: "needle".into(),
            ..Query::default()
        };
        let page = db.query("dev", &query).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.hits.iter().filter(|h| h.kind == "comment").count(), 2);
        assert!(page.hits.iter().any(|h| h.kind == "attachment"));
        db.conn
            .execute("UPDATE comments SET body='renamed' WHERE id='c1'", [])
            .unwrap();
        assert_eq!(db.query("dev", &query).unwrap().total, 2);
        db.conn
            .execute("UPDATE tasks SET deleted_at='now' WHERE id='t'", [])
            .unwrap();
        assert_eq!(db.query("dev", &query).unwrap().total, 0);
        db.conn
            .execute("UPDATE tasks SET deleted_at=NULL WHERE id='t'", [])
            .unwrap();
        assert_eq!(db.query("dev", &query).unwrap().total, 2);
    }
    #[test]
    fn context_inherits_root_section_without_exposing_guest_workspace() {
        let db = db();
        db.conn.execute_batch("INSERT INTO workspaces(id,title) VALUES('w','Private workspace name'); INSERT INTO workspace_members VALUES('w','other','owner'); UPDATE lists SET workspace_id='w' WHERE id='inbox'; INSERT INTO sections(id,list_id,title,position) VALUES('s','inbox','Delivery',0);").unwrap();
        task(&db, "root", "inbox", "Parent path", "", None);
        task(&db, "child", "inbox", "Child needle", "", Some("root"));
        db.conn
            .execute("INSERT INTO task_locations VALUES('root','s')", [])
            .unwrap();
        let mut query = q("needle");
        query.section_id = Some("s".into());
        let page = db.query("dev", &query).unwrap();
        assert_eq!(page.total, 1);
        let context = page.hits[0].context.as_ref().unwrap();
        assert_eq!(
            context.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(),
            vec!["Inbox", "Delivery", "Parent path"]
        );
        assert!(
            !serde_json::to_string(&page)
                .unwrap()
                .contains("Private workspace name")
        );
        db.conn
            .execute(
                "INSERT INTO workspace_members VALUES('w','dev','viewer')",
                [],
            )
            .unwrap();
        assert_eq!(
            db.query("dev", &query).unwrap().hits[0]
                .context
                .as_ref()
                .unwrap()[0]
                .title,
            "Private workspace name"
        );
    }
    #[test]
    fn status_dates_timezone_priority_and_recurrence_are_explicit() {
        let db = db();
        task(&db, "open", "inbox", "Scheduled", "", None);
        task(&db, "done", "inbox", "Scheduled done", "", None);
        db.conn
            .execute("UPDATE tasks SET complete=1 WHERE id='done'", [])
            .unwrap();
        db.conn.execute("INSERT INTO task_schedule VALUES('open',1,?1)",[r#"{"planned":{"kind":"date","date":"2026-10-26"},"due":{"kind":"timed","local":"2026-10-27T00:30:00","timezone":"Europe/Zurich"},"priority":2,"reminders":[]}"#]).unwrap();
        db.conn.execute_batch("INSERT INTO recurrence_series(id,actor,spec,state) VALUES('series','dev','{}','active');INSERT INTO recurrence_occurrences VALUES('series',1,'open','2026-10-27T00:30:00','pending');").unwrap();
        let mut query = q("Scheduled");
        assert_eq!(db.query("dev", &query).unwrap().total, 1);
        query.status = "completed".into();
        assert_eq!(db.query("dev", &query).unwrap().hits[0].id, "done");
        query.status = "all".into();
        assert_eq!(db.query("dev", &query).unwrap().total, 2);
        query.priority = Some(2);
        query.planned_from = Some("2026-10-26".into());
        query.planned_to = Some("2026-10-26".into());
        query.due_from = Some("2026-10-27".into());
        query.due_to = Some("2026-10-27".into());
        query.recurrence = Some("active".into());
        let hit = db.query("dev", &query).unwrap().hits.remove(0);
        assert_eq!(hit.due_timezone.as_deref(), Some("Europe/Zurich"));
        assert_eq!(hit.due.as_deref(), Some("2026-10-27"));
        query.due_to = Some("2026-10-26".into());
        assert!(matches!(db.query("dev", &query), Err(Error::Invalid(_))));
    }
    #[test]
    fn stable_pagination_binds_query_actor_access_and_generation() {
        let db = db();
        for id in ["a", "b", "c"] {
            task(&db, id, "inbox", "Same title", "", None);
        }
        let mut query = q("Same");
        query.limit = 1;
        query.sort = Sort::Title;
        let first = db.query("dev", &query).unwrap();
        assert_eq!(first.total, 3);
        assert_eq!(first.hits[0].id, "a");
        query.cursor = first.next_cursor;
        let second = db.query("dev", &query).unwrap();
        assert_eq!(second.hits[0].id, "b");
        query.cursor = second.next_cursor.clone();
        assert_eq!(db.query("dev", &query).unwrap().hits[0].id, "c");
        query.text = "different".into();
        assert!(matches!(db.query("dev", &query), Err(Error::Conflict(_))));
        query.text = "Same".into();
        db.conn
            .execute("UPDATE tasks SET title='Changed' WHERE id='a'", [])
            .unwrap();
        assert!(matches!(db.query("dev", &query), Err(Error::Conflict(_))));
        query.cursor = None;
        let page = db.query("dev", &query).unwrap();
        query.cursor = page.next_cursor;
        db.conn
            .execute(
                "DELETE FROM memberships WHERE list_id='inbox' AND user_id='dev'",
                [],
            )
            .unwrap();
        assert!(matches!(db.query("dev", &query), Err(Error::Conflict(_))));
    }
    #[test]
    fn projection_and_ambiguous_tag_names_do_not_disclose_private_tags() {
        let db = db();
        task(&db, "a", "inbox", "Public tagged", "long body", None);
        task(&db, "b", "groceries", "Other tagged", "", None);
        task(&db, "secret", "private", "Hidden tagged", "", None);
        db.conn.execute_batch("INSERT INTO tags(id,scope,name,normalized_name) VALUES('g1','list:inbox','Café','café'),('g2','list:groceries','Café','café'),('hidden','list:private','Café','café'); INSERT INTO task_tags VALUES('a','g1'),('b','g2'),('secret','hidden');").unwrap();
        let mut query = q("");
        query.tag = Some("Café".into());
        let error = db.query("dev", &query).unwrap_err().to_string();
        assert!(error.contains("g1") && error.contains("g2") && !error.contains("hidden"));
        query.list_id = Some("inbox".into());
        query.fields = Some(vec![]);
        let page = db.query("dev", &query).unwrap();
        assert_eq!(page.total, 1);
        let value = serde_json::to_value(&page.hits[0]).unwrap();
        assert!(value.get("title").is_none());
        assert!(value.get("snippet").is_none());
        assert!(value.get("context").is_none());
        assert_eq!(value["id"], "a");
    }
    #[test]
    fn scoped_credential_and_title_weighted_relevance() {
        let db = db();
        task(&db, "body", "inbox", "Unrelated title", "needle body", None);
        task(&db, "title", "inbox", "needle", "", None);
        task(&db, "other-list", "groceries", "needle", "", None);
        db.conn.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,list_ids,expires_at) VALUES('scoped','dev','hash','http://localhost/api/v1','[\"read\"]','[\"inbox\"]',unixepoch()+3600)",[]).unwrap();
        let page = db.query("credential:scoped", &q("needle")).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.hits[0].id, "title");
        db.conn
            .execute("UPDATE auth_agents SET revoked=1 WHERE id='scoped'", [])
            .unwrap();
        assert!(db.query("credential:scoped", &q("needle")).is_err());
    }
    #[test]
    #[ignore = "bounded synthetic 10k document performance observation"]
    fn ten_thousand_tasks_and_comments() {
        let db = db();
        db.conn.execute_batch("BEGIN").unwrap();
        for n in 0..10_000 {
            let id = format!("perf-{n:05}");
            task(
                &db,
                &id,
                "inbox",
                &format!("Task {n}"),
                "Synthetic description",
                None,
            );
            db.conn.execute("INSERT INTO comments(id,task_id,author_id,body) VALUES(?1,?2,'dev','searchable perfneedle comment')",params![format!("c{n}"),id]).unwrap();
        }
        db.conn.execute_batch("COMMIT").unwrap();
        let start = std::time::Instant::now();
        let page = db
            .query(
                "dev",
                &Query {
                    text: "perfneedle".into(),
                    entities: Some(vec!["comment".into()]),
                    limit: 100,
                    ..Query::default()
                },
            )
            .unwrap();
        assert_eq!(page.total, 10_000);
        assert_eq!(page.hits.len(), 100);
        eprintln!(
            "10k tasks + 10k comments authorized query: {:?}",
            start.elapsed()
        );
    }
}
