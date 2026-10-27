//! Explicit administrative import and live-authorized business-data snapshots.
use crate::core::{Error, Store, actor_identity};
use crate::object_storage::ObjectStorage;
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, params,
    types::{Value as SqlValue, ValueRef},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error(transparent)]
    Domain(#[from] Error),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error("transfer I/O failed")]
    Io(#[from] std::io::Error),
    #[error("invalid transfer data: {0}")]
    Invalid(String),
    #[error("transfer conflict: {0}")]
    Conflict(String),
}
type Result<T> = std::result::Result<T, TransferError>;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum ExportScope {
    Account,
    List(String),
    Workspace(String),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBundle {
    pub format: String,
    pub version: u32,
    pub exported_for: String,
    pub tables: BTreeMap<String, Vec<Value>>,
    pub notes: Vec<String>,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TransferReport {
    pub counts: BTreeMap<String, usize>,
    pub unchanged: usize,
    pub conflicts: usize,
    pub unsupported: Vec<String>,
    pub excluded_credentials: usize,
    pub applied: bool,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IdentityMap {
    pub users: BTreeMap<String, String>,
}
const BUSINESS: &[&str] = &[
    "users",
    "workspaces",
    "workspace_members",
    "workspace_lifecycle",
    "workspace_lifecycle_events",
    "lists",
    "list_references",
    "memberships",
    "membership_roles",
    "sections",
    "tasks",
    "list_defaults",
    "workspace_defaults",
    "user_defaults",
    "entity_public_ids",
    "task_public_ids",
    "list_public_ids",
    "task_locations",
    "tags",
    "task_tags",
    "comments",
    "upload_intents",
    "attachments",
    "task_schedule",
    "recurrence_series",
    "recurrence_occurrences",
    "history",
    "notification_preferences",
    "favorites",
    "agent_destination_preferences",
    "stats_observation_lists",
    "stats_overdue_items",
    "reference_sources",
    "reference_edges",
    "transfer_sources",
    "transfer_legacy_records",
    "legacy_aliases",
    "legacy_attachment_refs",
];
fn invalid(message: impl Into<String>) -> TransferError {
    TransferError::Invalid(message.into())
}
fn hash(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("JSON"))
    )
}
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn rows(
    c: &Connection,
    table: &str,
    filter: &str,
    args: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Value>> {
    let sql = format!("SELECT * FROM {} {filter}", quote(table));
    let mut q = c.prepare(&sql)?;
    let names = q.column_names().map(str::to_owned).collect::<Vec<_>>();
    let found = q.query_map(args, |r| {
        let mut object = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            let value = match r.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => n.into(),
                ValueRef::Real(n) => json!(n),
                ValueRef::Text(v) => Value::String(
                    std::str::from_utf8(v)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?
                        .into(),
                ),
                ValueRef::Blob(_) => return Err(rusqlite::Error::InvalidQuery),
            };
            object.insert(name.clone(), value);
        }
        Ok(Value::Object(object))
    })?;
    Ok(found.collect::<std::result::Result<_, _>>()?)
}
fn ids(values: &[Value], key: &str) -> BTreeSet<String> {
    values
        .iter()
        .filter_map(|v| v[key].as_str().map(str::to_owned))
        .collect()
}
fn selected(values: Vec<Value>, key: &str, allowed: &BTreeSet<String>) -> Vec<Value> {
    values
        .into_iter()
        .filter(|r| r[key].as_str().is_some_and(|s| allowed.contains(s)))
        .collect()
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| invalid("missing required text field"))
}
fn integer(value: &Value, key: &str, default: i64) -> i64 {
    value[key].as_i64().unwrap_or(default)
}
fn data(bundle: &ExportBundle, table: &str) -> Vec<Value> {
    bundle.tables.get(table).cloned().unwrap_or_default()
}
impl Store {
    pub fn export_bundle(&self, actor: &str, scope: ExportScope) -> Result<ExportBundle> {
        let tx = self.conn.unchecked_transaction()?;
        let user = actor_identity(&self.conn, actor)?.0;
        let mut lists = rows(
            &self.conn,
            "lists",
            "WHERE id IN(SELECT list_id FROM actor_list_access WHERE actor_id=?1 AND rank>=1)",
            &[&actor],
        )?;
        let owned_trash: BTreeSet<String> = if matches!(scope, ExportScope::Account)
            && !actor.starts_with("credential:")
        {
            rows(&self.conn,"workspace_lifecycle","WHERE deleted_at IS NOT NULL AND workspace_id IN(SELECT workspace_id FROM workspace_members WHERE user_id=?1 AND role='owner')",&[&user])?.into_iter().filter_map(|r|r["workspace_id"].as_str().map(str::to_owned)).collect()
        } else {
            BTreeSet::new()
        };
        if !owned_trash.is_empty() {
            for row in rows(&self.conn, "lists", "", &[])? {
                if row["workspace_id"]
                    .as_str()
                    .is_some_and(|id| owned_trash.contains(id))
                {
                    lists.push(row);
                }
            }
        }
        match &scope {
            ExportScope::Account => {}
            ExportScope::List(id) => {
                if self.list_permission(actor, id)? < 1 {
                    return Err(Error::Forbidden.into());
                }
                lists.retain(|l| l["id"] == *id);
            }
            ExportScope::Workspace(id) => {
                if self.workspace_permission(actor, id)? < 1 {
                    return Err(Error::Forbidden.into());
                }
                lists.retain(|l| l["workspace_id"] == *id);
            }
        }
        let list_ids = ids(&lists, "id");
        let mut workspaces = Vec::new();
        for w in rows(&self.conn, "workspaces", "", &[])? {
            let id = string(&w, "id")?;
            let include = match &scope {
                ExportScope::Account => true,
                ExportScope::Workspace(selected) => selected == id,
                ExportScope::List(_) => lists.iter().any(|l| l["workspace_id"] == id),
            };
            if include
                && (owned_trash.contains(id)
                    || self.workspace_permission(actor, id).unwrap_or(0) >= 1)
            {
                workspaces.push(w)
            }
        }
        let workspace_ids = ids(&workspaces, "id");
        for list in &mut lists {
            if !list["workspace_id"]
                .as_str()
                .is_some_and(|id| workspace_ids.contains(id))
            {
                list["workspace_id"] = Value::Null
            }
        }
        let tasks = selected(rows(&self.conn, "tasks", "", &[])?, "list_id", &list_ids);
        let task_ids = ids(&tasks, "id");
        let mut tables = BTreeMap::new();
        tables.insert("lists".into(), lists);
        tables.insert("workspaces".into(), workspaces);
        tables.insert("tasks".into(), tasks);
        for (table, key, allowed) in [
            ("list_references", "list_id", &list_ids),
            ("list_defaults", "list_id", &list_ids),
            ("list_public_ids", "list_id", &list_ids),
            ("workspace_lifecycle", "workspace_id", &workspace_ids),
            ("sections", "list_id", &list_ids),
            ("comments", "task_id", &task_ids),
            ("attachments", "task_id", &task_ids),
            ("task_schedule", "task_id", &task_ids),
            ("task_public_ids", "task_id", &task_ids),
            ("task_locations", "task_id", &task_ids),
            ("task_tags", "task_id", &task_ids),
            ("legacy_attachment_refs", "task_id", &task_ids),
        ] {
            tables.insert(
                table.into(),
                selected(rows(&self.conn, table, "", &[])?, key, allowed),
            );
        }
        tables.insert(
            "workspace_defaults".into(),
            rows(&self.conn, "workspace_defaults", "", &[])?
                .into_iter()
                .filter(|r| {
                    r["workspace_id"]
                        .as_str()
                        .is_some_and(|id| workspace_ids.contains(id))
                        && r["list_id"]
                            .as_str()
                            .is_some_and(|id| list_ids.contains(id))
                })
                .collect(),
        );
        tables.insert(
            "user_defaults".into(),
            rows(&self.conn, "user_defaults", "WHERE user_id=?1", &[&user])?
                .into_iter()
                .filter(|r| {
                    r["workspace_id"]
                        .as_str()
                        .is_some_and(|id| workspace_ids.contains(id))
                })
                .collect(),
        );
        let section_ids = ids(tables.get("sections").unwrap(), "id");
        tables.insert(
            "entity_public_ids".into(),
            rows(&self.conn, "entity_public_ids", "", &[])?
                .into_iter()
                .filter(|r| {
                    let allowed = match r["kind"].as_str() {
                        Some("workspace") => &workspace_ids,
                        Some("list") => &list_ids,
                        Some("section") => &section_ids,
                        Some("task") => &task_ids,
                        _ => return false,
                    };
                    r["entity_id"]
                        .as_str()
                        .is_some_and(|id| allowed.contains(id))
                })
                .collect(),
        );
        let attachment_ids = ids(tables.get("attachments").unwrap(), "intent_id");
        tables.insert(
            "upload_intents".into(),
            selected(
                rows(&self.conn, "upload_intents", "", &[])?,
                "id",
                &attachment_ids,
            ),
        );
        let tag_ids = ids(tables.get("task_tags").unwrap(), "tag_id");
        tables.insert(
            "tags".into(),
            selected(rows(&self.conn, "tags", "", &[])?, "id", &tag_ids),
        );
        let occurrences = selected(
            rows(&self.conn, "recurrence_occurrences", "", &[])?,
            "task_id",
            &task_ids,
        );
        let series_ids = ids(&occurrences, "series_id");
        tables.insert("recurrence_occurrences".into(), occurrences);
        tables.insert(
            "recurrence_series".into(),
            selected(
                rows(&self.conn, "recurrence_series", "", &[])?,
                "id",
                &series_ids,
            ),
        );
        let mut history = Vec::new();
        for row in rows(&self.conn, "history", "", &[])? {
            let event: Value = serde_json::from_str(row["event"].as_str().unwrap_or("null"))
                .map_err(|_| invalid("invalid history JSON"))?;
            if row["list_id"]
                .as_str()
                .is_some_and(|id| list_ids.contains(id))
                && event["task"]["id"]
                    .as_str()
                    .is_some_and(|id| task_ids.contains(id))
            {
                history.push(row)
            }
        }
        tables.insert("history".into(), history);
        let mut owned_workspaces = BTreeSet::new();
        for id in &workspace_ids {
            if owned_trash.contains(id) || self.workspace_permission(actor, id).unwrap_or(0) >= 3 {
                owned_workspaces.insert(id.clone());
            }
        }
        tables.insert(
            "workspace_lifecycle_events".into(),
            selected(
                rows(&self.conn, "workspace_lifecycle_events", "", &[])?,
                "workspace_id",
                &owned_workspaces,
            ),
        );
        tables.insert(
            "workspace_members".into(),
            selected(
                rows(&self.conn, "workspace_members", "", &[])?,
                "workspace_id",
                &owned_workspaces,
            ),
        );
        let mut owner_lists = BTreeSet::new();
        for row in tables.get("lists").unwrap() {
            if row["workspace_id"]
                .as_str()
                .is_some_and(|id| owned_workspaces.contains(id))
            {
                owner_lists.insert(string(row, "id")?.into());
            }
        }
        for table in ["memberships", "membership_roles"] {
            tables.insert(
                table.into(),
                selected(rows(&self.conn, table, "", &[])?, "list_id", &owner_lists),
            );
        }
        // Reference graphs are rebuilt after import; their raw descriptions/comments are preserved.
        let mut user_ids = BTreeSet::from([user.clone()]);
        for table in [
            "workspace_members",
            "memberships",
            "comments",
            "attachments",
            "recurrence_series",
            "history",
        ] {
            for row in tables.get(table).into_iter().flatten() {
                for key in ["user_id", "author_id", "creator_id", "actor", "actor_id"] {
                    if let Some(value) = row[key].as_str() {
                        user_ids.insert(value.into());
                    }
                }
            }
        }
        tables.insert(
            "users".into(),
            selected(
                rows(&self.conn, "users", "", &[])?
                    .into_iter()
                    .map(|r| json!({"id":r["id"],"name":r["name"]}))
                    .collect(),
                "id",
                &user_ids,
            ),
        );
        if matches!(scope, ExportScope::Account) && !actor.starts_with("credential:") {
            tables.insert(
                "notification_preferences".into(),
                rows(
                    &self.conn,
                    "notification_preferences",
                    "WHERE user_id=?1",
                    &[&user],
                )?,
            );
        }
        if matches!(scope, ExportScope::Account) && !actor.starts_with("credential:") {
            let favorites = rows(&self.conn, "favorites", "WHERE user_id=?1", &[&user])?
                .into_iter()
                .filter(
                    |row| match (row["target_kind"].as_str(), row["target_id"].as_str()) {
                        (Some("list"), Some(id)) => list_ids.contains(id),
                        (Some("workspace"), Some(id)) => workspace_ids.contains(id),
                        _ => false,
                    },
                )
                .collect();
            tables.insert("favorites".into(), favorites);
            tables.insert(
                "agent_destination_preferences".into(),
                selected(
                    rows(
                        &self.conn,
                        "agent_destination_preferences",
                        "WHERE user_id=?1",
                        &[&user],
                    )?,
                    "list_id",
                    &list_ids,
                ),
            );
        }
        tables.insert(
            "stats_observation_lists".into(),
            selected(
                rows(
                    &self.conn,
                    "stats_observation_lists",
                    "WHERE user_id=?1",
                    &[&user],
                )?,
                "list_id",
                &list_ids,
            ),
        );
        tables.insert(
            "stats_overdue_items".into(),
            selected(
                selected(
                    rows(
                        &self.conn,
                        "stats_overdue_items",
                        "WHERE user_id=?1",
                        &[&user],
                    )?,
                    "list_id",
                    &list_ids,
                ),
                "task_id",
                &task_ids,
            ),
        );
        let mut archives = Vec::new();
        for row in rows(&self.conn, "transfer_legacy_records", "", &[])? {
            let kind = row["destination_table"].as_str().unwrap_or("");
            let id = row["destination_id"].as_str().unwrap_or("");
            let visible = match kind {
                "tasks" => task_ids.contains(id),
                "lists" => list_ids.contains(id),
                "sections" => tables
                    .get("sections")
                    .is_some_and(|s| s.iter().any(|v| v["id"] == id)),
                "workspaces" => workspace_ids.contains(id),
                _ => {
                    matches!(scope, ExportScope::Account)
                        && row["owner_id"] == user
                        && !actor.starts_with("credential:")
                }
            };
            if visible {
                archives.push(row)
            }
        }
        let sources = ids(&archives, "source_id");
        tables.insert(
            "transfer_sources".into(),
            selected(
                rows(&self.conn, "transfer_sources", "", &[])?,
                "id",
                &sources,
            ),
        );
        tables.insert("transfer_legacy_records".into(), archives);
        tables.insert(
            "legacy_aliases".into(),
            rows(&self.conn, "legacy_aliases", "", &[])?
                .into_iter()
                .filter(|v| {
                    v["entity_id"].as_str().is_some_and(|id| {
                        task_ids.contains(id) || list_ids.contains(id) || workspace_ids.contains(id)
                    })
                })
                .collect(),
        );
        tx.commit()?;
        Ok(ExportBundle{format:"todoer-business-export".into(),version:1,exported_for:user,tables,notes:vec!["Authentication identities, sessions, tokens, password material, AI secrets, delivery jobs and upload staging not referenced by attachments are excluded.".into(),"ACLs are included only for owned workspaces; directly shared lists do not expose workspace titles. Derived reference indexes are excluded; original reference text remains in task descriptions and comments.".into()]})
    }
}
pub fn export_text(bundle: &ExportBundle) -> Result<String> {
    let mut output = String::new();
    for list in data(bundle, "lists") {
        output.push_str(&format!(
            "# {}\n{}\n",
            string(&list, "title")?,
            list["description"].as_str().unwrap_or("")
        ));
        let tasks = data(bundle, "tasks")
            .into_iter()
            .filter(|t| t["list_id"] == list["id"])
            .collect::<Vec<_>>();
        fn emit(
            out: &mut String,
            tasks: &[Value],
            parent: Option<&str>,
            depth: usize,
        ) -> Result<()> {
            if depth > 100 {
                return Err(invalid("task hierarchy exceeds text export depth"));
            }
            let mut children = tasks
                .iter()
                .filter(|t| t["parent_id"].as_str() == parent)
                .collect::<Vec<_>>();
            children.sort_by_key(|t| integer(t, "position", 0));
            for t in children {
                out.push_str(&format!(
                    "{}- [{}] {} ({})\n",
                    "  ".repeat(depth),
                    if integer(t, "complete", 0) != 0 {
                        "x"
                    } else {
                        " "
                    },
                    string(t, "title")?,
                    string(t, "id")?
                ));
                if let Some(description) = t["description"].as_str()
                    && !description.is_empty()
                {
                    out.push_str(&format!("{}{}\n", "  ".repeat(depth + 1), description));
                }
                emit(out, tasks, Some(string(t, "id")?), depth + 1)?;
            }
            Ok(())
        }
        emit(&mut output, &tasks, None, 0)?;
    }
    Ok(output)
}
fn csv_cell(value: &str) -> String {
    let trimmed = value.trim_start_matches([' ', '\t', '\r', '\n']);
    if trimmed.starts_with(['=', '+', '-', '@']) || value.starts_with(['\t', '\r', '\n']) {
        format!("'{value}")
    } else {
        value.into()
    }
}
pub fn export_csv(bundle: &ExportBundle) -> Result<Vec<u8>> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record([
            "id",
            "reference",
            "list_id",
            "parent_id",
            "title",
            "description",
            "complete",
            "position",
            "revision",
        ])
        .map_err(|_| invalid("CSV encoding failed"))?;
    for task in data(bundle, "tasks") {
        let record = [
            "id",
            "reference",
            "list_id",
            "parent_id",
            "title",
            "description",
            "complete",
            "position",
            "revision",
        ]
        .map(|key| {
            csv_cell(&task[key].as_str().map(str::to_owned).unwrap_or_else(|| {
                if task[key].is_null() {
                    String::new()
                } else {
                    task[key].to_string()
                }
            }))
        });
        writer
            .write_record(record)
            .map_err(|_| invalid("CSV encoding failed"))?;
    }
    writer
        .into_inner()
        .map_err(|_| invalid("CSV output failed"))
}
fn columns(c: &Connection, table: &str) -> Result<Vec<(String, i64)>> {
    let mut q = c.prepare(&format!("PRAGMA table_info({})", quote(table)))?;
    Ok(q.query_map([], |r| Ok((r.get(1)?, r.get(5)?)))?
        .collect::<std::result::Result<_, _>>()?)
}
fn sql_value(value: &Value) -> Result<SqlValue> {
    Ok(match value {
        Value::Null => SqlValue::Null,
        Value::String(v) => SqlValue::Text(v.clone()),
        Value::Bool(v) => SqlValue::Integer(i64::from(*v)),
        Value::Number(v) => {
            if let Some(n) = v.as_i64() {
                SqlValue::Integer(n)
            } else if let Some(n) = v.as_f64() {
                SqlValue::Real(n)
            } else {
                return Err(invalid("number outside SQLite range"));
            }
        }
        _ => return Err(invalid("table fields must be primitive values")),
    })
}
fn insert(c: &Connection, table: &str, row: &Value) -> Result<bool> {
    if !BUSINESS.contains(&table) {
        return Err(invalid("table is not importable business data"));
    }
    let object = row
        .as_object()
        .ok_or_else(|| invalid("row must be an object"))?;
    let schema = columns(c, table)?;
    if object.is_empty()
        || object
            .keys()
            .any(|name| !schema.iter().any(|(known, _)| known == name))
    {
        return Err(invalid("unknown business column"));
    }
    if table == "users"
        && object
            .keys()
            .any(|key| !matches!(key.as_str(), "id" | "name"))
    {
        return Err(invalid("user export contains unsupported identity fields"));
    }
    let mut primary = schema
        .iter()
        .filter(|(_, rank)| *rank > 0)
        .collect::<Vec<_>>();
    if object.contains_key("id")
        && let Some(id) = schema.iter().find(|(name, _)| name == "id")
    {
        primary = vec![id];
    }
    primary.sort_by_key(|(_, rank)| *rank);
    if primary.is_empty() {
        return Err(invalid("table has no import identity"));
    }
    let filter = primary
        .iter()
        .map(|(name, _)| format!("{}=?", quote(name)))
        .collect::<Vec<_>>()
        .join(" AND ");
    let values = primary
        .iter()
        .map(|(name, _)| {
            object
                .get(name)
                .ok_or_else(|| invalid("missing primary identity"))
                .and_then(sql_value)
        })
        .collect::<Result<Vec<_>>>()?;
    let refs = values
        .iter()
        .map(|v| v as &dyn rusqlite::ToSql)
        .collect::<Vec<_>>();
    let old = rows(c, table, &format!("WHERE {filter}"), &refs)?;
    if let Some(old) = old.first() {
        if object
            .iter()
            .all(|(key, value)| old.get(key) == Some(value))
        {
            return Ok(false);
        }
        return Err(TransferError::Conflict(format!(
            "existing {table} row differs; refusing overwrite"
        )));
    }
    let names = object.keys().map(|k| quote(k)).collect::<Vec<_>>();
    let values = object.values().map(sql_value).collect::<Result<Vec<_>>>()?;
    c.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES({})",
            quote(table),
            names.join(","),
            vec!["?"; names.len()].join(",")
        ),
        rusqlite::params_from_iter(values),
    )?;
    Ok(true)
}
fn remap_user(mapping: &IdentityMap, id: &str, prefix: &str) -> String {
    mapping
        .users
        .get(id)
        .cloned()
        .unwrap_or_else(|| format!("{prefix}:{id}"))
}
/// Administrative restore. Mapping never links an identity by email automatically.
pub fn import_bundle(
    store: &mut Store,
    bundle: &ExportBundle,
    mapping: &IdentityMap,
    apply: bool,
) -> Result<TransferReport> {
    if bundle.format != "todoer-business-export" || bundle.version != 1 {
        return Err(invalid("unsupported export format/version"));
    }
    if bundle
        .tables
        .keys()
        .any(|table| !BUSINESS.contains(&table.as_str()))
    {
        return Err(invalid("export includes an unsupported table"));
    }
    let tx = store
        .conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch("PRAGMA defer_foreign_keys=ON")?;
    for target in mapping.users.values() {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE id=?1)",
            [target],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(invalid("identity mapping target must already exist"));
        }
    }
    let marker = format!("bundle-{}", &hash(&json!([bundle, mapping]))[..24]);
    let repeat: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM transfer_sources WHERE id=?1)",
        [&marker],
        |r| r.get(0),
    )?;
    if !repeat {
        for table in [
            "workspaces",
            "lists",
            "sections",
            "tasks",
            "comments",
            "tags",
            "recurrence_series",
            "attachments",
        ] {
            for row in data(bundle, table) {
                if current_destination(&tx, table, string(&row, "id")?)?.is_some() {
                    return Err(TransferError::Conflict("destination contains untracked entity IDs; refusing implicit grants or overwrite".into()));
                }
            }
        }
    }
    tx.execute(
        "INSERT OR IGNORE INTO transfer_sources(id,schema_hash) VALUES(?1,?2)",
        params![marker, "todoer-business-export-v1"],
    )?;
    let mut report = TransferReport::default();
    let exported_for = remap_user(mapping, &bundle.exported_for, "import");
    let mut imported = bundle.tables.clone();
    for (table, values) in &mut imported {
        for row in values {
            if table == "users" {
                let original = string(row, "id")?.to_string();
                row["id"] = remap_user(mapping, &original, "import").into();
                if mapping.users.contains_key(&original) {
                    let mapped_name: String = tx.query_row(
                        "SELECT name FROM users WHERE id=?1",
                        [string(row, "id")?],
                        |r| r.get(0),
                    )?;
                    row["name"] = mapped_name.into();
                }
            } else {
                for key in [
                    "user_id",
                    "author_id",
                    "creator_id",
                    "owner_id",
                    "recipient",
                    "actor",
                    "actor_id",
                ] {
                    if let Some(value) = row[key].as_str()
                        && !value.starts_with("credential:")
                    {
                        row[key] = remap_user(mapping, value, "import").into();
                    }
                }
                if table == "history" {
                    for key in ["event", "before_event"] {
                        if let Some(raw) = row[key].as_str()
                            && let Ok(mut value) = serde_json::from_str::<Value>(raw)
                        {
                            if let Some(actor) = value["actor"].as_str() {
                                value["actor"] = remap_user(mapping, actor, "import").into();
                            }
                            value["client"] = Value::Null;
                            row[key] = value.to_string().into();
                        }
                    }
                }
            }
        }
    }
    // A guest export has no permission to disclose an ACL. Restore its visible lists
    // into a private workspace for the explicitly mapped export recipient.
    let needs_private = data(bundle, "lists")
        .iter()
        .any(|l| l["workspace_id"].is_null());
    if needs_private {
        let id = format!("imported-private-{}", &marker[7..]);
        insert(
            &tx,
            "workspaces",
            &json!({"id":id,"title":"Imported shared lists","description":"Restored without the original workspace ACL","revision":1}),
        )?;
        insert(
            &tx,
            "workspace_members",
            &json!({"workspace_id":id,"user_id":exported_for,"role":"owner"}),
        )?;
        for row in imported.entry("lists".into()).or_default() {
            if row["workspace_id"].is_null() {
                row["workspace_id"] = id.clone().into();
            }
        }
    }
    let all_workspace_ids = ids(
        imported.get("workspaces").map(Vec::as_slice).unwrap_or(&[]),
        "id",
    );
    for id in all_workspace_ids {
        let has_owner = imported
            .get("workspace_members")
            .into_iter()
            .flatten()
            .any(|r| r["workspace_id"] == id && r["role"] == "owner");
        if !has_owner {
            imported
                .entry("workspace_members".into())
                .or_default()
                .push(json!({"workspace_id":id,"user_id":exported_for,"role":"owner"}));
        }
    }
    for table in BUSINESS {
        let values = imported.get(*table).cloned().unwrap_or_default();
        for row in &values {
            if insert(&tx, table, row)? {
                *report.counts.entry((*table).into()).or_default() += 1
            } else {
                report.unchanged += 1
            }
        }
    }
    let issues: i64 = tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
        r.get(0)
    })?;
    if issues != 0 {
        return Err(invalid("export references missing business rows"));
    }
    crate::slugs::backfill(&tx)?;
    if apply {
        tx.commit()?;
        report.applied = true
    } else {
        tx.rollback()?;
    }
    Ok(report)
}

pub struct LegacySnapshot {
    pub source_id: String,
    pub schema_hash: String,
    pub tables: BTreeMap<String, Vec<Value>>,
    pub unsupported: Vec<String>,
    pub excluded_credentials: usize,
}
const LEGACY_KNOWN: &[&str] = &[
    "users",
    "spaces",
    "lists",
    "sections",
    "tasks",
    "subtasks",
    "comments",
    "attachments",
    "bookmarks",
    "task_audit_log",
    "notification_reads",
    "user_settings",
    "ai_usage",
    "_lists_20240428",
    "_sections_20240428",
];
fn legacy_safe(table: &str, mut value: Value) -> (Value, usize) {
    let mut removed = 0;
    if table == "users" {
        let keep = [
            "uuid", "sid", "name", "email", "disabled", "created", "updated",
        ];
        if let Some(object) = value.as_object_mut() {
            let keys = object
                .keys()
                .filter(|key| !keep.contains(&key.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            for key in keys {
                if object.remove(&key).is_some() {
                    removed += 1;
                }
            }
        }
    }
    if table == "user_settings" {
        let key = value["key"].as_str().unwrap_or("");
        if !matches!(
            key,
            "ai_model" | "ai_max_tokens" | "ai_temperature" | "keybindings"
        ) {
            value["value"] = Value::Null;
            removed += 1;
        }
    }
    (value, removed)
}
/// Uses SQLite's online backup API on a read-only connection for a consistent
/// source snapshot. The source path, rows and identity values never enter reports.
pub fn read_legacy(path: &Path) -> Result<LegacySnapshot> {
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(std::time::Duration::from_secs(5))?;
    let check: String = source.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(invalid("legacy database integrity check failed"));
    }
    let mut snapshot = Connection::open_in_memory()?;
    rusqlite::backup::Backup::new(&source, &mut snapshot)?.run_to_completion(
        100,
        std::time::Duration::from_millis(1),
        None,
    )?;
    let schema = rows(
        &snapshot,
        "sqlite_master",
        "WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        &[],
    )?;
    let schema_hash = hash(&json!(
        schema
            .iter()
            .map(|v| (&v["name"], &v["sql"]))
            .collect::<Vec<_>>()
    ));
    let mut tables = BTreeMap::new();
    let mut unsupported = Vec::new();
    let mut excluded = 0;
    for table in schema {
        let name = string(&table, "name")?;
        if !LEGACY_KNOWN.contains(&name) {
            unsupported.push(format!("unrecognized table {name}"));
            continue;
        }
        let mut values = Vec::new();
        for row in rows(&snapshot, name, "", &[])? {
            let (row, removed) = legacy_safe(name, row);
            values.push(row);
            excluded += removed;
        }
        tables.insert(name.into(), values);
    }
    for required in ["users", "spaces", "lists", "sections", "tasks"] {
        if !tables.contains_key(required) {
            return Err(invalid("legacy schema is missing required entity tables"));
        }
    }
    let users = ids(tables.get("users").unwrap(), "uuid");
    let source_id = format!(
        "legacy-{}",
        &hash(&json!({"users":users,"schema":schema_hash}))[..24]
    );
    Ok(LegacySnapshot {
        source_id,
        schema_hash,
        tables,
        unsupported,
        excluded_credentials: excluded,
    })
}
fn legacy_key(table: &str, row: &Value) -> Result<String> {
    if let Some(id) = row["uuid"].as_str() {
        Ok(id.into())
    } else if let Some(id) = row["id"].as_i64() {
        Ok(format!("{table}:{id}"))
    } else {
        Err(invalid("legacy row has no stable primary key"))
    }
}
fn current_destination(c: &Connection, table: &str, id: &str) -> Result<Option<Value>> {
    Ok(rows(c, table, "WHERE id=?1", &[&id])?.into_iter().next())
}
fn archived_record(
    c: &Connection,
    source: &LegacySnapshot,
    table: &str,
    row: &Value,
    owner: &str,
    destination: Option<(&str, &str)>,
) -> Result<bool> {
    let key = legacy_key(table, row)?;
    type ArchivedFingerprint = (String, Option<String>, Option<String>, Option<String>);
    let old:Option<ArchivedFingerprint>=c.query_row("SELECT source_hash,destination_table,destination_id,destination_hash FROM transfer_legacy_records WHERE source_id=?1 AND table_name=?2 AND source_key=?3",params![source.source_id,table,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let source_hash = hash(row);
    if let Some((old, table, id, destination_hash)) = old {
        if old != source_hash {
            return Err(TransferError::Conflict(format!(
                "legacy {table:?} source row changed; no overwrite applied"
            )));
        }
        if let (Some(table), Some(id), Some(expected)) = (table, id, destination_hash) {
            let current = current_destination(c, &table, &id)?.ok_or_else(|| {
                TransferError::Conflict("imported destination entity was removed".into())
            })?;
            if hash(&current) != expected {
                return Err(TransferError::Conflict(
                    "imported destination entity has newer edits".into(),
                ));
            }
        }
        return Ok(false);
    }
    let destination_hash = if let Some((table, id)) = destination {
        current_destination(c, table, id)?.map(|v| hash(&v))
    } else {
        None
    };
    c.execute("INSERT INTO transfer_legacy_records(source_id,table_name,source_key,owner_id,payload,source_hash,destination_table,destination_id,destination_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![source.source_id,table,key,owner,row.to_string(),source_hash,destination.map(|v|v.0),destination.map(|v|v.1),destination_hash])?;
    Ok(true)
}
fn legacy_time(value: &Value, key: &str) -> Option<i64> {
    value[key]
        .as_str()
        .and_then(|v| chrono::NaiveDateTime::parse_from_str(v, "%Y-%m-%d %H:%M:%S").ok())
        .map(|v| v.and_utc().timestamp())
}
/// Materializes legacy entities while retaining sanitized source metadata verbatim.
pub fn import_legacy(
    store: &mut Store,
    source: &LegacySnapshot,
    mapping: &IdentityMap,
    apply: bool,
    ack_unsupported: bool,
) -> Result<TransferReport> {
    let mut report = TransferReport {
        unsupported: source.unsupported.clone(),
        excluded_credentials: source.excluded_credentials,
        ..Default::default()
    };
    for (table, values) in &source.tables {
        report.counts.insert(table.clone(), values.len());
    }
    if apply && !report.unsupported.is_empty() && !ack_unsupported {
        return Err(invalid(
            "unsupported legacy tables require explicit acknowledgement; inspect dry-run counts",
        ));
    }
    let tx = store
        .conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch("PRAGMA defer_foreign_keys=ON")?;
    for target in mapping.users.values() {
        if !tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE id=?1)",
            [target],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(invalid("identity mapping target must already exist"));
        }
    }
    let policy_hash = hash(&json!([source.schema_hash, mapping]));
    let previous: Option<String> = tx
        .query_row(
            "SELECT schema_hash FROM transfer_sources WHERE id=?1",
            [&source.source_id],
            |r| r.get(0),
        )
        .optional()?;
    if previous.as_ref().is_some_and(|value| value != &policy_hash) {
        return Err(TransferError::Conflict(
            "source schema or identity mapping differs from the previous import".into(),
        ));
    }
    tx.execute(
        "INSERT OR IGNORE INTO transfer_sources(id,schema_hash) VALUES(?1,?2)",
        params![source.source_id, policy_hash],
    )?;
    let table = |name: &str| source.tables.get(name).cloned().unwrap_or_default();
    let users = table("users");
    let mapuser = |id: &str| remap_user(mapping, id, "legacy");
    for user in &users {
        let id = string(user, "uuid")?;
        let target = mapuser(id);
        if !mapping.users.contains_key(id) {
            insert(
                &tx,
                "users",
                &json!({"id":target,"name":user["name"].as_str().filter(|s|!s.is_empty()).unwrap_or("Imported account")}),
            )?;
        }
        if !archived_record(
            &tx,
            source,
            "users",
            user,
            &target,
            Some(("users", &target)),
        )? {
            report.unchanged += 1
        }
    }
    // Legacy SQLite files can retain ownership references after deleting users.
    // Preserve these identities as disabled local placeholders, never email matches.
    let mut missing_users = BTreeSet::new();
    for values in source.tables.values() {
        for row in values {
            for key in ["creator", "user", "author", "actor"] {
                if let Some(id) = row[key].as_str()
                    && !users.iter().any(|u| u["uuid"] == id)
                {
                    if uuid::Uuid::parse_str(id).is_err() {
                        return Err(invalid("legacy owner reference is not a UUID"));
                    }
                    missing_users.insert(id.to_owned());
                }
            }
        }
    }
    if !missing_users.is_empty() {
        report
            .counts
            .insert("recovered_deleted_users".into(), missing_users.len());
        report
            .unsupported
            .push("Deleted source owners preserved as disabled local accounts".into());
        if apply && !ack_unsupported {
            return Err(invalid("deleted owner recovery requires --ack-unsupported"));
        }
        for id in &missing_users {
            if !mapping.users.contains_key(id) {
                insert(
                    &tx,
                    "users",
                    &json!({"id":mapuser(id),"name":"Deleted legacy account"}),
                )?;
            }
        }
    }
    let spaces = table("spaces");
    let mut space_owners = spaces
        .iter()
        .map(|s| {
            Ok((
                string(s, "uuid")?.to_string(),
                mapuser(string(s, "creator")?),
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    for row in &spaces {
        let id = string(row, "uuid")?;
        let owner = space_owners
            .get(id)
            .ok_or_else(|| invalid("missing legacy space owner"))?;
        insert(
            &tx,
            "workspaces",
            &json!({"id":id,"title":string(row,"title")?,"description":row["descr"].as_str().unwrap_or(""),"revision":1}),
        )?;
        insert(
            &tx,
            "workspace_members",
            &json!({"workspace_id":id,"user_id":owner,"role":"owner"}),
        )?;
        archived_record(&tx, source, "spaces", row, owner, Some(("workspaces", id)))?;
        if let Some(sid) = row["sid"].as_str() {
            tx.execute(
                "INSERT OR IGNORE INTO legacy_aliases VALUES(?1,'workspace',?2,?3)",
                params![source.source_id, sid, id],
            )?;
        }
    }
    // Old migration shadow tables are archives, not live entities. Materialize
    // only shadow rows still referenced by live tasks; retain every other row below.
    let required_sections = table("tasks")
        .iter()
        .filter_map(|t| t["section"].as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let mut sections = table("sections");
    for row in table("_sections_20240428") {
        if row["uuid"]
            .as_str()
            .is_some_and(|id| required_sections.contains(id))
            && !sections.iter().any(|s| s["uuid"] == row["uuid"])
        {
            sections.push(row);
        }
    }
    let required_lists = sections
        .iter()
        .filter_map(|s| s["list"].as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let mut lists = table("lists");
    for row in table("_lists_20240428") {
        if row["uuid"]
            .as_str()
            .is_some_and(|id| required_lists.contains(id))
            && !lists.iter().any(|l| l["uuid"] == row["uuid"])
        {
            lists.push(row);
        }
    }

    // Recover missing hierarchy only under the original known creator, never an
    // inferred shared account. Original parent IDs remain in the source archive.
    for section in &sections {
        let list = string(section, "list")?;
        if !lists.iter().any(|row| row["uuid"] == list) {
            lists.push(json!({"uuid":list,"title":"Recovered legacy list","creator":string(section,"creator")?,"space":"missing","descr":"Recovered missing legacy parent"}));
            *report
                .counts
                .entry("recovered_missing_lists".into())
                .or_default() += 1;
        }
    }
    for row in &mut lists {
        let old_workspace = string(row, "space")?.to_owned();
        if !space_owners.contains_key(&old_workspace) {
            let creator = string(row, "creator")?;
            if !users.iter().any(|u| u["uuid"] == creator) && !missing_users.contains(creator) {
                return Err(invalid(
                    "orphan hierarchy has unknown creator; cannot safely recover",
                ));
            }
            let owner = mapuser(creator);
            let recovery = format!(
                "legacy-recovery-{}",
                hash(&json!([source.source_id, creator]))
            );
            insert(
                &tx,
                "workspaces",
                &json!({"id":recovery,"title":"Recovered legacy items","description":"Original parent workspace was missing; source metadata retained in archive.","revision":1}),
            )?;
            insert(
                &tx,
                "workspace_members",
                &json!({"workspace_id":recovery,"user_id":owner,"role":"owner"}),
            )?;
            space_owners.insert(recovery.clone(), owner);
            row["space"] = recovery.into();
            *report
                .counts
                .entry("recovered_orphan_lists".into())
                .or_default() += 1;
        }
    }
    if report.counts.contains_key("recovered_orphan_lists") {
        report.unsupported.push("Orphan hierarchy recovered into private creator-owned workspaces; inspect recovery counts".into());
        if apply && !ack_unsupported {
            return Err(invalid(
                "hierarchy recovery requires --ack-unsupported after reviewing dry-run counts",
            ));
        }
    }
    let mut list_workspace = BTreeMap::new();
    for row in &lists {
        let id = string(row, "uuid")?;
        let workspace = string(row, "space")?;
        let owner = space_owners
            .get(workspace)
            .ok_or_else(|| invalid("legacy list has missing space"))?;
        let parent_archived = spaces
            .iter()
            .find(|s| s["uuid"] == workspace)
            .is_some_and(|s| integer(s, "archived", 0) != 0);
        insert(
            &tx,
            "lists",
            &json!({"id":id,"title":string(row,"title")?,"description":row["descr"].as_str().unwrap_or(""),"revision":1,"workspace_id":workspace,"restricted":0,"position":0,"archived":i64::from(parent_archived||integer(row,"archived",0)!=0)}),
        )?;
        list_workspace.insert(id.to_string(), workspace.to_string());
        if source
            .tables
            .get("lists")
            .is_some_and(|v| v.iter().any(|l| l["uuid"] == id))
        {
            let original = source.tables["lists"]
                .iter()
                .find(|r| r["uuid"] == id)
                .unwrap();
            archived_record(&tx, source, "lists", original, owner, Some(("lists", id)))?;
        }
        if let Some(sid) = row["sid"].as_str() {
            tx.execute(
                "INSERT OR IGNORE INTO legacy_aliases VALUES(?1,'list',?2,?3)",
                params![source.source_id, sid, id],
            )?;
        }
    }
    let mut section_list = BTreeMap::new();
    for row in &sections {
        let id = string(row, "uuid")?;
        let list = string(row, "list")?;
        let workspace = list_workspace
            .get(list)
            .ok_or_else(|| invalid("legacy section has missing list"))?;
        let owner = space_owners.get(workspace).unwrap();
        insert(
            &tx,
            "sections",
            &json!({"id":id,"list_id":list,"title":string(row,"title")?,"description":row["descr"].as_str().unwrap_or(""),"position":integer(row,"order",0),"revision":1}),
        )?;
        section_list.insert(id.to_string(), list.to_string());
        if source
            .tables
            .get("sections")
            .is_some_and(|v| v.iter().any(|s| s["uuid"] == id))
        {
            archived_record(&tx, source, "sections", row, owner, Some(("sections", id)))?;
        }
        if let Some(sid) = row["sid"].as_str() {
            tx.execute(
                "INSERT OR IGNORE INTO legacy_aliases VALUES(?1,'section',?2,?3)",
                params![source.source_id, sid, id],
            )?;
        }
    }
    let roots = table("tasks");
    let mut normalized = BTreeMap::new();
    let mut owners = BTreeMap::new();
    for row in roots.iter().chain(table("subtasks").iter()) {
        let id = string(row, "uuid")?;
        let root = if row.get("section").is_some() {
            row
        } else {
            roots
                .iter()
                .find(|root| root["uuid"] == row["task"])
                .ok_or_else(|| invalid("legacy subtask has missing task"))?
        };
        let list = section_list
            .get(string(root, "section")?)
            .ok_or_else(|| invalid("legacy task has missing section"))?;
        let owner = space_owners.get(list_workspace.get(list).unwrap()).unwrap();
        let parent = if row.get("section").is_some() {
            None
        } else {
            row["parent"].as_str().or_else(|| row["task"].as_str())
        };
        let archived = integer(root, "archived", 0) != 0;
        let reference = string(row, "sid")?;
        let collision: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE reference=?1 AND id!=?2)",
            params![reference, id],
            |r| r.get(0),
        )?;
        if collision {
            return Err(TransferError::Conflict(
                "legacy short reference collides with a destination reference".into(),
            ));
        }
        let task = json!({"id":id,"reference":reference,"list_id":list,"parent_id":parent,"title":string(row,"title")?,"description":row["descr"].as_str().unwrap_or(""),"complete":integer(row,"complete",0),"revision":1,"position":integer(row,"order",0),"deleted_at":if archived{Some(root["created"].as_str().unwrap_or("legacy-archived"))}else{None},"delete_group":if archived{root["uuid"].as_str()}else{None}});
        owners.insert(id.to_string(), owner.clone());
        normalized.insert(id.to_string(), task);
    }
    for (id, t) in &normalized {
        let mut seen = BTreeSet::new();
        let mut current = Some(id.as_str());
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(invalid("legacy task hierarchy contains a cycle"));
            }
            let row = normalized
                .get(id)
                .ok_or_else(|| invalid("legacy task parent is missing"))?;
            if row["list_id"] != t["list_id"] {
                return Err(invalid("legacy child crosses list boundary"));
            }
            current = row["parent_id"].as_str();
        }
        insert(&tx, "tasks", t)?;
    }
    for (kind, values) in [("tasks", roots), ("subtasks", table("subtasks"))] {
        for row in values {
            let id = string(&row, "uuid")?;
            let owner = owners.get(id).unwrap();
            if kind == "tasks" {
                insert(
                    &tx,
                    "task_locations",
                    &json!({"task_id":id,"section_id":string(&row,"section")?}),
                )?;
            }
            let due = row["due_date"].as_str().filter(|v| !v.is_empty());
            if let Some(value) = due {
                chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
                    invalid("legacy due date requires explicit conversion; no date was guessed")
                })?;
            }
            let priority = row["priority"].as_i64();
            if priority.is_some_and(|p| !(1..=3).contains(&p)) {
                return Err(invalid("legacy priority outside supported 1–3 range"));
            }
            if due.is_some() || priority.is_some() {
                insert(
                    &tx,
                    "task_schedule",
                    &json!({"task_id":id,"revision":1,"document":json!({"planned":null,"due":due.map(|v|json!({"kind":"date","date":v})),"priority":priority,"reminders":[]}).to_string()}),
                )?;
            }
            let list = string(normalized.get(id).unwrap(), "list_id")?;
            let home = format!("workspace:{}", list_workspace.get(list).unwrap());
            for name in row["tags"]
                .as_str()
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if name.chars().count() > 100 || name.chars().any(char::is_control) {
                    return Err(invalid(
                        "legacy tag cannot be represented without modification",
                    ));
                }
                let normalized_name = name.to_lowercase();
                let tag_id = format!(
                    "legacy-tag-{}",
                    &hash(&json!([home, normalized_name]))[..24]
                );
                let existing: Option<String> = tx
                    .query_row(
                        "SELECT id FROM tags WHERE scope=?1 AND normalized_name=?2",
                        params![home, normalized_name],
                        |r| r.get(0),
                    )
                    .optional()?;
                let tag_id = existing.unwrap_or(tag_id);
                if !tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM tags WHERE id=?1)",
                    [&tag_id],
                    |r| r.get::<_, bool>(0),
                )? {
                    insert(
                        &tx,
                        "tags",
                        &json!({"id":tag_id,"scope":home,"name":name,"normalized_name":normalized_name,"revision":1}),
                    )?;
                }
                insert(&tx, "task_tags", &json!({"task_id":id,"tag_id":tag_id}))?;
            }
            if !archived_record(&tx, source, kind, &row, owner, Some(("tasks", id)))? {
                report.unchanged += 1;
            }
            tx.execute(
                "INSERT OR IGNORE INTO legacy_aliases VALUES(?1,'task',?2,?3)",
                params![source.source_id, string(&row, "sid")?, id],
            )?;
        }
    }
    for row in table("comments") {
        let id = string(&row, "uuid")?;
        let task = string(&row, "task")?;
        let owner = owners
            .get(task)
            .ok_or_else(|| invalid("legacy comment has missing task"))?;
        insert(
            &tx,
            "comments",
            &json!({"id":id,"task_id":task,"author_id":mapuser(string(&row,"author")?),"body":row["content"].as_str().unwrap_or(""),"revision":1,"created_at":legacy_time(&row,"created").ok_or_else(||invalid("legacy comment timestamp cannot be parsed"))?,"updated_at":legacy_time(&row,"updated")}),
        )?;
        archived_record(&tx, source, "comments", &row, owner, Some(("comments", id)))?;
    }
    for row in table("attachments") {
        let id = string(&row, "uuid")?;
        let task = string(&row, "task")?;
        let owner = owners
            .get(task)
            .ok_or_else(|| invalid("legacy attachment has missing task"))?;
        let metadata = json!({"id":id,"task_id":task,"filename":string(&row,"fname")?,"mime":string(&row,"mime")?,"size":integer(&row,"size",0),"object_key":id,"created_at":row["created"],"source_id":source.source_id});
        if integer(&row, "size", 0) < 0 {
            return Err(invalid("legacy attachment size is negative"));
        }
        insert_legacy_ref(&tx, &metadata)?;
        archived_record(&tx, source, "attachments", &row, owner, None)?;
    }
    for name in [
        "bookmarks",
        "task_audit_log",
        "notification_reads",
        "user_settings",
        "ai_usage",
        "_lists_20240428",
        "_sections_20240428",
    ] {
        for row in table(name) {
            let owner = if let Some(id) = row["user"]
                .as_str()
                .or_else(|| row["creator"].as_str())
                .or_else(|| row["actor"].as_str())
            {
                mapuser(id)
            } else if let Some(task) = row["task"].as_str().or_else(|| row["task_uuid"].as_str()) {
                owners
                    .get(task)
                    .cloned()
                    .ok_or_else(|| invalid("legacy auxiliary record has missing task"))?
            } else {
                return Err(invalid("legacy auxiliary record has no ownership mapping"));
            };
            archived_record(&tx, source, name, &row, &owner, None)?;
        }
    }
    let issues: i64 = tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
        r.get(0)
    })?;
    if issues != 0 {
        let tables: Vec<String> = tx
            .prepare("SELECT DISTINCT \"table\" FROM pragma_foreign_key_check")?
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        return Err(invalid(format!(
            "legacy import foreign key reconciliation failed in {}",
            tables.join(",")
        )));
    }
    let imported: i64 = tx.query_row(
        "SELECT COUNT(*) FROM transfer_legacy_records WHERE source_id=?1",
        [&source.source_id],
        |r| r.get(0),
    )?;
    let expected = source.tables.values().map(Vec::len).sum::<usize>();
    if imported as usize != expected {
        return Err(invalid("legacy sanitized row reconciliation failed"));
    }
    crate::slugs::backfill(&tx)?;
    if apply {
        tx.commit()?;
        report.applied = true
    } else {
        tx.rollback()?;
    }
    Ok(report)
}
fn insert_legacy_ref(c: &Connection, row: &Value) -> Result<bool> {
    insert(c, "legacy_attachment_refs", row)
}
/// Read-only administrative snapshot; no schema migrations are run against input.
pub fn copy_destination_for_dry_run(source: &Connection, target: &mut Store) -> Result<()> {
    rusqlite::backup::Backup::new(source, &mut target.conn)?.run_to_completion(
        100,
        std::time::Duration::from_millis(1),
        None,
    )?;
    let exists: bool = target.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='transfer_sources')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        let tx = target.conn.transaction()?;
        tx.execute_batch(include_str!("../migrations/012_transfer.sql"))?;
        tx.execute(
            "INSERT OR IGNORE INTO schema_migrations(version) VALUES(12)",
            [],
        )?;
        tx.commit()?;
    }
    // Older Rust destinations copied for a dry run have not passed Store::open
    // migrations. Upgrade only this private copy before import backfill hooks.
    let has_slugs: bool = target.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='entity_slugs')",
        [],
        |r| r.get(0),
    )?;
    if !has_slugs {
        let tx = target.conn.transaction()?;
        tx.execute_batch(include_str!("../migrations/022_slugs.sql"))?;
        crate::slugs::backfill(&tx)?;
        tx.execute(
            "INSERT OR IGNORE INTO schema_migrations(version) VALUES(22)",
            [],
        )?;
        tx.commit()?;
    }
    Ok(())
}
pub fn open_readonly_snapshot(path: &Path) -> Result<Store> {
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut target = Store::memory()?;
    copy_destination_for_dry_run(&source, &mut target)?;
    let valid: bool = target.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name='id')",
        [],
        |r| r.get(0),
    )?;
    if !valid {
        return Err(invalid("expected a versioned Rust business database"));
    }
    Ok(target)
}
fn private_file(path: &Path) -> Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).read(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
pub fn write_private_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = private_file(path)?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub mime: String,
    pub verification: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipManifest {
    pub format: String,
    pub version: u32,
    pub entries: Vec<ManifestEntry>,
}
fn bytes_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct OutputGuard<'a> {
    path: &'a Path,
    committed: bool,
}
impl Drop for OutputGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(self.path);
        }
    }
}
/// Writes each attachment independently, never an aggregate in-memory archive.
/// Errors remove the incomplete archive; existing paths are never overwritten.
pub async fn export_zip(
    store: Arc<Mutex<Store>>,
    actor: String,
    scope: ExportScope,
    storage: Option<&ObjectStorage>,
    legacy: Option<&crate::object_storage::LegacyBlobSource>,
    path: &Path,
) -> Result<ZipManifest> {
    let bundle = store
        .lock()
        .map_err(|_| invalid("database lock failed"))?
        .export_bundle(&actor, scope)?;
    let file = private_file(path)?;
    let mut guard = OutputGuard {
        path,
        committed: false,
    };
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o600);
    let json = serde_json::to_vec_pretty(&bundle).map_err(|_| invalid("JSON encoding failed"))?;
    if json.len() > 100 * 1024 * 1024 {
        return Err(invalid("business JSON exceeds 100 MiB export bound"));
    }
    let mut manifest = ZipManifest {
        format: "todoer-zip-manifest".into(),
        version: 1,
        entries: vec![ManifestEntry {
            path: "business.json".into(),
            size: json.len() as u64,
            sha256: bytes_hash(&json),
            mime: "application/json".into(),
            verification: "export-snapshot".into(),
        }],
    };
    writer
        .start_file("business.json", options)
        .map_err(|_| invalid("ZIP entry failed"))?;
    writer.write_all(&json)?;
    for (table, prefix) in [
        ("attachments", "attachments"),
        ("legacy_attachment_refs", "legacy-attachments"),
    ] {
        for row in data(&bundle, table) {
            let id = string(&row, "id")?;
            if table == "legacy_attachment_refs"
                && data(&bundle, "attachments").iter().any(|a| a["id"] == id)
            {
                continue;
            }
            let canonical = uuid::Uuid::parse_str(id)
                .map_err(|_| invalid("attachment ID is not a safe UUID"))?
                .to_string();
            if canonical != id {
                return Err(invalid("attachment ID is not canonical"));
            }
            let task_id = string(&row, "task_id")?;
            {
                let db = store.lock().map_err(|_| invalid("database lock failed"))?;
                let list: String =
                    db.conn
                        .query_row("SELECT list_id FROM tasks WHERE id=?1", [task_id], |r| {
                            r.get(0)
                        })?;
                if db.list_permission(&actor, &list)? < 1 {
                    return Err(Error::Forbidden.into());
                }
            }
            let (bytes, digest, verification, mime) = if table == "attachments" {
                let provider = storage.ok_or_else(|| {
                    invalid("private attachment storage is required for complete ZIP export")
                })?;
                let attachment = crate::discussion::Attachment {
                    id: id.into(),
                    intent_id: string(&row, "intent_id")?.into(),
                    task_id: task_id.into(),
                    comment_id: row["comment_id"].as_str().map(str::to_owned),
                    creator_id: string(&row, "creator_id")?.into(),
                    filename: string(&row, "filename")?.into(),
                    size: row["size"]
                        .as_u64()
                        .ok_or_else(|| invalid("invalid attachment size"))?,
                    sha256: string(&row, "sha256")?.into(),
                    revision: integer(&row, "revision", 1),
                    created_at: integer(&row, "created_at", 0),
                    deleted: !row["deleted_at"].is_null(),
                    object_key: string(&row, "object_key")?.into(),
                };
                let bytes = provider.get(&attachment).await.map_err(|_| {
                    invalid("private attachment download or checksum verification failed")
                })?;
                let digest = bytes_hash(&bytes);
                (
                    bytes,
                    digest,
                    "stored-checksum-verified",
                    "application/octet-stream".to_string(),
                )
            } else {
                let provider=legacy.ok_or_else(||invalid("separately configured legacy blob source is required for complete ZIP export"))?;
                let blob = provider
                    .get(string(&row, "object_key")?, row["size"].as_u64())
                    .await
                    .map_err(|_| {
                        invalid(
                            "legacy blob download/size verification failed; archive not published",
                        )
                    })?;
                (
                    blob.bytes,
                    blob.sha256,
                    "legacy-first-capture-checksum",
                    row["mime"]
                        .as_str()
                        .unwrap_or("application/octet-stream")
                        .into(),
                )
            };
            let filename = format!("{prefix}/{id}.bin");
            writer
                .start_file(&filename, options)
                .map_err(|_| invalid("ZIP attachment entry failed"))?;
            writer.write_all(&bytes)?;
            manifest.entries.push(ManifestEntry {
                path: filename,
                size: bytes.len() as u64,
                sha256: digest,
                mime,
                verification: verification.into(),
            });
        }
    }
    writer
        .start_file("manifest.json", options)
        .map_err(|_| invalid("ZIP manifest entry failed"))?;
    writer.write_all(
        &serde_json::to_vec_pretty(&manifest).map_err(|_| invalid("manifest encoding failed"))?,
    )?;
    let file = writer
        .finish()
        .map_err(|_| invalid("ZIP finalization failed"))?;
    file.sync_all()?;
    guard.committed = true;
    Ok(manifest)
}
/// Integrity check without extraction (no archive paths are written to disk).
pub fn verify_zip(path: &Path) -> Result<ZipManifest> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| invalid("invalid ZIP"))?;
    let manifest: ZipManifest = {
        let mut reader = archive
            .by_name("manifest.json")
            .map_err(|_| invalid("missing ZIP manifest"))?;
        if reader.size() > 1024 * 1024 {
            return Err(invalid("manifest exceeds 1 MiB"));
        }
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        serde_json::from_slice(&bytes).map_err(|_| invalid("invalid ZIP manifest"))?
    };
    if manifest.version != 1 || manifest.format != "todoer-zip-manifest" {
        return Err(invalid("unsupported ZIP manifest"));
    }
    if manifest.entries.len() > 10000
        || manifest
            .entries
            .iter()
            .try_fold(0u64, |sum, e| sum.checked_add(e.size))
            .is_none_or(|sum| sum > 512 * 1024 * 1024)
    {
        return Err(invalid("archive exceeds total restore bounds"));
    }
    let mut seen = BTreeSet::new();
    for entry in &manifest.entries {
        if entry.path.starts_with('/')
            || entry
                .path
                .split('/')
                .any(|part| part == ".." || part.is_empty())
            || !seen.insert(entry.path.clone())
        {
            return Err(invalid("unsafe or duplicate ZIP path"));
        }
        let mut reader = archive
            .by_name(&entry.path)
            .map_err(|_| invalid("missing ZIP entry"))?;
        if reader.size() != entry.size || entry.size > 100 * 1024 * 1024 {
            return Err(invalid("ZIP entry size mismatch or bound exceeded"));
        }
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        if format!("{:x}", digest.finalize()) != entry.sha256 {
            return Err(invalid("ZIP checksum mismatch"));
        }
    }
    if archive.len() != manifest.entries.len() + 1 {
        return Err(invalid("ZIP contains undeclared entries"));
    }
    Ok(manifest)
}
/// Fully validated archive metadata and immutable upload plans. No object keys
/// supplied by an archive are used as destination storage paths.
fn zip_restore_plan(
    path: &Path,
) -> Result<(ExportBundle, Vec<(String, crate::discussion::UploadIntent)>)> {
    let manifest = verify_zip(path)?;
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(path)?).map_err(|_| invalid("invalid ZIP"))?;
    let mut business = Vec::new();
    archive
        .by_name("business.json")
        .map_err(|_| invalid("missing business JSON"))?
        .take(100 * 1024 * 1024 + 1)
        .read_to_end(&mut business)?;
    if business.len() > 100 * 1024 * 1024 {
        return Err(invalid("business JSON too large"));
    }
    let mut bundle: ExportBundle =
        serde_json::from_slice(&business).map_err(|_| invalid("invalid business JSON"))?;
    let namespace = hash(&json!(&bundle));
    let mut plans = Vec::new();
    let mut expected = BTreeSet::from(["business.json".to_string()]);
    for (table, prefix) in [
        ("attachments", "attachments"),
        ("legacy_attachment_refs", "legacy-attachments"),
    ] {
        for mut row in data(&bundle, table) {
            let id = string(&row, "id")?.to_owned();
            if table == "legacy_attachment_refs"
                && let Some(native) = data(&bundle, "attachments")
                    .into_iter()
                    .find(|a| a["id"] == id)
            {
                // Legacy metadata remains an archive after promotion, while the
                // single native object supplies the bytes on subsequent exports.
                if native["task_id"] != row["task_id"] || native["size"] != row["size"] {
                    return Err(invalid("legacy/native attachment metadata mismatch"));
                }
                continue;
            }
            if uuid::Uuid::parse_str(&id)
                .map_err(|_| invalid("invalid attachment UUID"))?
                .to_string()
                != id
            {
                return Err(invalid("noncanonical attachment UUID"));
            }
            let entry_path = format!("{prefix}/{id}.bin");
            if !expected.insert(entry_path.clone()) {
                return Err(invalid("duplicate attachment"));
            }
            let entry = manifest
                .entries
                .iter()
                .find(|e| e.path == entry_path)
                .ok_or_else(|| invalid("attachment bytes missing"))?;
            if entry.size > crate::discussion::MAX_ATTACHMENT_BYTES
                || row["size"].as_u64() != Some(entry.size)
            {
                return Err(invalid("attachment size mismatch or limit exceeded"));
            }
            if table == "attachments" && row["sha256"] != entry.sha256 {
                return Err(invalid("attachment digest differs from metadata"));
            }
            let digest = Sha256::digest(format!("{namespace}:{id}"));
            let mut uuid_bytes = [0u8; 16];
            uuid_bytes.copy_from_slice(&digest[..16]);
            uuid_bytes[6] = (uuid_bytes[6] & 15) | 64;
            uuid_bytes[8] = (uuid_bytes[8] & 63) | 128;
            let intent_id = if table == "attachments" {
                string(&row, "intent_id")?.to_owned()
            } else {
                uuid::Uuid::from_bytes(uuid_bytes).to_string()
            };
            let staging = format!("staging/restore-{namespace}/{id}");
            let object = format!("objects/restore-{namespace}/{id}");
            let creator = if table == "attachments" {
                string(&row, "creator_id")?.to_owned()
            } else {
                bundle.exported_for.clone()
            };
            let filename = string(&row, "filename")?.to_owned();
            let task = string(&row, "task_id")?.to_owned();
            let comment = row["comment_id"].as_str().map(str::to_owned);
            let intent = crate::discussion::UploadIntent {
                id: intent_id.clone(),
                task_id: task.clone(),
                comment_id: comment.clone(),
                filename: filename.clone(),
                size: entry.size,
                sha256: entry.sha256.clone(),
                revision: 1,
                expires_at: chrono::Utc::now().timestamp() + 3600,
                finalized: false,
                staging_key: staging.clone(),
                object_key: object.clone(),
                creator_id: creator.clone(),
            };
            if table == "attachments" {
                row["object_key"] = object.clone().into();
                let target = bundle
                    .tables
                    .get_mut("attachments")
                    .unwrap()
                    .iter_mut()
                    .find(|r| r["id"] == id)
                    .unwrap();
                *target = row;
                let target = bundle
                    .tables
                    .get_mut("upload_intents")
                    .and_then(|v| v.iter_mut().find(|r| r["id"] == intent_id))
                    .ok_or_else(|| invalid("attachment upload metadata missing"))?;
                target["object_key"] = object.into();
                target["staging_key"] = staging.into();
            } else {
                bundle.tables.entry("upload_intents".into()).or_default().push(json!({"id":intent_id,"task_id":task,"comment_id":comment,"creator_id":creator,"client_id":null,"filename":filename,"size":entry.size,"sha256":entry.sha256,"staging_key":staging,"object_key":object,"revision":1,"created_at":0,"expires_at":0,"finalized_at":0}));
                bundle.tables.entry("attachments".into()).or_default().push(json!({"id":id,"intent_id":intent.id,"task_id":task,"comment_id":comment,"creator_id":creator,"filename":filename,"size":entry.size,"sha256":entry.sha256,"object_key":intent.object_key,"revision":1,"created_at":0,"deleted_at":null}));
            }
            plans.push((entry_path, intent));
        }
    }
    if manifest.entries.len() != expected.len()
        || manifest.entries.iter().any(|e| !expected.contains(&e.path))
    {
        return Err(invalid("archive contains unattached object data"));
    }
    Ok((bundle, plans))
}
pub fn validated_zip_bundle(path: &Path) -> Result<ExportBundle> {
    Ok(zip_restore_plan(path)?.0)
}
/// Verify all archive bytes and dry-run metadata before uploading. Every S3 write
/// is conditional and checksummed. The database commits only after all promotion
/// checks succeed; interrupted uploads may leave unreachable immutable objects.
pub async fn restore_zip(
    store: Arc<Mutex<Store>>,
    path: &Path,
    mapping: &IdentityMap,
    storage: Option<&ObjectStorage>,
    apply: bool,
) -> Result<TransferReport> {
    let (bundle, plans) = zip_restore_plan(path)?;
    let dry = import_bundle(
        &mut *store.lock().map_err(|_| invalid("database lock failed"))?,
        &bundle,
        mapping,
        false,
    )?;
    if !apply {
        return Ok(dry);
    }
    let provider = if plans.is_empty() {
        None
    } else {
        Some(storage.ok_or_else(|| invalid("private object storage is required for ZIP restore"))?)
    };
    for (entry, intent) in plans {
        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(path)?).map_err(|_| invalid("invalid ZIP"))?;
        let mut bytes = Vec::new();
        archive
            .by_name(&entry)
            .map_err(|_| invalid("missing attachment bytes"))?
            .take(intent.size + 1)
            .read_to_end(&mut bytes)?;
        let provider = provider.unwrap();
        provider
            .put_intent(&intent, bytes)
            .await
            .map_err(|_| invalid("immutable attachment staging failed; metadata not committed"))?;
        provider.verify_and_promote(&intent).await.map_err(|_| {
            invalid("attachment promotion verification failed; metadata not committed")
        })?;
    }
    import_bundle(
        &mut *store.lock().map_err(|_| invalid("database lock failed"))?,
        &bundle,
        mapping,
        true,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    const USER: &str = "00000000-0000-4000-8000-000000000001";
    const TASK: &str = "00000000-0000-4000-8000-000000000005";
    fn fixture() -> (std::path::PathBuf, LegacySnapshot) {
        let path =
            std::env::temp_dir().join(format!("todoer-synthetic-{}.db", uuid::Uuid::new_v4()));
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(include_str!("../tests/fixtures/legacy.sql"))
                .unwrap();
        }
        (path.clone(), read_legacy(&path).unwrap())
    }
    #[test]
    fn account_export_preserves_empty_workspaces_preferences_and_real_observations() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let empty = db
            .create_workspace(
                "dev",
                crate::workspace::WorkspaceInput {
                    title: "Empty workspace".into(),
                    description: "Preserve me".into(),
                },
            )
            .unwrap();
        db.set_favorite(
            "dev",
            crate::navigation::FavoriteInput {
                kind: "workspace".into(),
                id: empty.id.clone(),
                favorite: true,
            },
        )
        .unwrap();
        db.set_destination_preference("dev", "inbox", "inbox")
            .unwrap();
        db.record_stats_observations(chrono::Utc::now().timestamp())
            .unwrap();
        let bundle = db.export_bundle("dev", ExportScope::Account).unwrap();
        assert!(
            bundle.tables["workspaces"]
                .iter()
                .any(|w| w["id"] == empty.id)
        );
        assert_eq!(bundle.tables["favorites"].len(), 1);
        assert_eq!(bundle.tables["agent_destination_preferences"].len(), 1);
        assert!(!bundle.tables["stats_observation_lists"].is_empty());
        let mut target = Store::memory().unwrap();
        target
            .conn
            .execute(
                "INSERT INTO users(id,name) VALUES('restored','Restored synthetic user')",
                [],
            )
            .unwrap();
        let mapping = IdentityMap {
            users: std::collections::BTreeMap::from([("dev".into(), "restored".into())]),
        };
        import_bundle(&mut target, &bundle, &mapping, true).unwrap();
        assert_eq!(target.favorites("restored").unwrap()[0].id, empty.id);
        assert_eq!(
            target.destination_preferences("restored").unwrap()[0].key,
            "inbox"
        );
        let observations: i64 = target
            .conn
            .query_row(
                "SELECT count(*) FROM stats_observation_lists WHERE user_id='restored'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            observations as usize,
            bundle.tables["stats_observation_lists"].len()
        );
    }
    #[test]
    fn exports_filter_inaccessible_tasks_before_rendering() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn.execute("INSERT INTO tasks(id,reference,list_id,title,position) VALUES('visible','vis','inbox','Public synthetic',0),('hidden','hid','private','Hidden synthetic',0)",[]).unwrap();
        let bundle = db.export_bundle("dev", ExportScope::Account).unwrap();
        assert_eq!(data(&bundle, "tasks").len(), 1);
        assert!(
            !serde_json::to_string(&bundle)
                .unwrap()
                .contains("Hidden synthetic")
        );
        assert!(!export_text(&bundle).unwrap().contains("Hidden synthetic"));
        assert!(
            !String::from_utf8(export_csv(&bundle).unwrap())
                .unwrap()
                .contains("Hidden synthetic")
        );
        assert!(
            db.export_bundle("dev", ExportScope::List("private".into()))
                .is_err()
        );
    }
    #[test]
    fn orphan_recovery_is_private_explicit_and_repeatable() {
        let (path, _) = fixture();
        Connection::open(&path)
            .unwrap()
            .execute("DELETE FROM spaces", [])
            .unwrap();
        let source = read_legacy(&path).unwrap();
        let mut store = Store::memory().unwrap();
        let report =
            import_legacy(&mut store, &source, &IdentityMap::default(), false, false).unwrap();
        assert_eq!(report.counts["recovered_orphan_lists"], 1);
        assert!(import_legacy(&mut store, &source, &IdentityMap::default(), true, false).is_err());
        import_legacy(&mut store, &source, &IdentityMap::default(), true, true).unwrap();
        import_legacy(&mut store, &source, &IdentityMap::default(), true, true).unwrap();
        assert_eq!(
            store
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM workspace_members WHERE role='owner'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn legacy_dry_run_repeat_nested_metadata_and_drift_are_safe() {
        let (path, source) = fixture();
        let before = std::fs::read(&path).unwrap();
        let mut db = Store::memory().unwrap();
        let map = IdentityMap::default();
        let report = import_legacy(&mut db, &source, &map, false, false).unwrap();
        assert!(!report.applied);
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        import_legacy(&mut db, &source, &map, true, false).unwrap();
        let again = import_legacy(&mut db, &source, &map, true, false).unwrap();
        assert!(again.unchanged >= 4);
        let actor = format!("legacy:{USER}");
        assert_eq!(db.subtree(&actor, TASK).unwrap().len(), 3);
        assert_eq!(
            db.get_schedule(&actor, TASK).unwrap().schedule.priority,
            Some(2)
        );
        let export = db.export_bundle(&actor, ExportScope::Account).unwrap();
        let json = serde_json::to_string(&export).unwrap();
        for secret in ["SECRET-PASSWORD", "SECRET-CODE", "SECRET-TOTP", "SECRET-AI"] {
            assert!(!json.contains(secret));
        }
        assert!(
            export_text(&export)
                .unwrap()
                .contains("Synthetic grandchild")
        );
        assert!(
            String::from_utf8(export_csv(&export).unwrap())
                .unwrap()
                .contains("'=CSV")
        );
        db.update(
            &actor,
            TASK,
            crate::core::Change {
                revision: 1,
                title: Some("New edit".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(
            import_legacy(&mut db, &source, &map, true, false),
            Err(TransferError::Conflict(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn json_roundtrip_is_explicit_and_does_not_link_emails_or_overwrite() {
        let (path, source) = fixture();
        let mut original = Store::memory().unwrap();
        import_legacy(&mut original, &source, &IdentityMap::default(), true, false).unwrap();
        let bundle = original
            .export_bundle(&format!("legacy:{USER}"), ExportScope::Account)
            .unwrap();
        let mut target = Store::memory().unwrap();
        target.seed_dev().unwrap();
        let map = IdentityMap {
            users: BTreeMap::from([(format!("legacy:{USER}"), "dev".into())]),
        };
        assert!(
            !import_bundle(&mut target, &bundle, &map, false)
                .unwrap()
                .applied
        );
        import_bundle(&mut target, &bundle, &map, true).unwrap();
        assert_eq!(target.subtree("dev", TASK).unwrap().len(), 3);
        import_bundle(&mut target, &bundle, &map, true).unwrap();
        target
            .update(
                "dev",
                TASK,
                crate::core::Change {
                    revision: 1,
                    description: Some("Concurrent edit".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(matches!(
            import_bundle(&mut target, &bundle, &map, true),
            Err(TransferError::Conflict(_))
        ));
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn zip_roundtrip_promotes_immutable_objects_before_metadata() {
        use axum::{
            Router,
            body::Bytes,
            extract::{Path as AxumPath, State},
            http::{HeaderMap, Method, StatusCode},
            response::IntoResponse,
            routing::any,
        };
        type Objects = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;
        async fn object(
            State(objects): State<Objects>,
            AxumPath(path): AxumPath<String>,
            method: Method,
            headers: HeaderMap,
            bytes: Bytes,
        ) -> axum::response::Response {
            let mut objects = objects.lock().unwrap();
            if method == Method::PUT {
                assert_eq!(headers.get("if-none-match").unwrap(), "*");
                if objects.contains_key(&path) {
                    return StatusCode::PRECONDITION_FAILED.into_response();
                }
                objects.insert(path, bytes.to_vec());
                return StatusCode::OK.into_response();
            }
            let Some(bytes) = objects.get(&path) else {
                return StatusCode::NOT_FOUND.into_response();
            };
            (
                [("content-length", bytes.len().to_string())],
                if method == Method::HEAD {
                    Vec::new()
                } else {
                    bytes.clone()
                },
            )
                .into_response()
        }
        let id = "00000000-0000-4000-8000-000000000010";
        let bytes = b"synthetic restored attachment".to_vec();
        let objects = Arc::new(Mutex::new(BTreeMap::from([(
            format!("files/{id}"),
            bytes.clone(),
        )])));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/{*path}", any(object))
            .with_state(objects.clone());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (source_path, _) = fixture();
        Connection::open(&source_path).unwrap().execute("INSERT INTO attachments(uuid,sid,task,fname,mime,size,created) VALUES(?1,'blobzip',?2,'synthetic.txt','text/plain',?3,'2026-10-01 00:00:00')",params![id,TASK,bytes.len() as i64]).unwrap();
        let source = read_legacy(&source_path).unwrap();
        let mut original = Store::memory().unwrap();
        import_legacy(&mut original, &source, &IdentityMap::default(), true, false).unwrap();
        let legacy = crate::object_storage::LegacyBlobSource::new(
            crate::object_storage::LegacyStorageConfig {
                endpoint: format!("http://{address}"),
                bucket: "files".into(),
                region: "us-east-1".into(),
                prefix: String::new(),
                access_key_id: "synthetic".into(),
                secret_access_key: "synthetic".into(),
            },
        )
        .unwrap();
        let storage = ObjectStorage::new(crate::object_storage::S3Config {
            endpoint: format!("http://{address}"),
            bucket: "files".into(),
            region: "us-east-1".into(),
            prefix: "destination".into(),
            access_key_id: "synthetic".into(),
            secret_access_key: "synthetic".into(),
        })
        .unwrap();
        let zip =
            std::env::temp_dir().join(format!("todoer-roundtrip-{}.zip", uuid::Uuid::new_v4()));
        export_zip(
            Arc::new(Mutex::new(original)),
            format!("legacy:{USER}"),
            ExportScope::Account,
            None,
            Some(&legacy),
            &zip,
        )
        .await
        .unwrap();
        let mut destination = Store::memory().unwrap();
        destination.seed_dev().unwrap();
        let destination = Arc::new(Mutex::new(destination));
        let mapping = IdentityMap {
            users: BTreeMap::from([(format!("legacy:{USER}"), "dev".into())]),
        };
        restore_zip(destination.clone(), &zip, &mapping, None, false)
            .await
            .unwrap();
        assert!(
            restore_zip(destination.clone(), &zip, &mapping, None, true)
                .await
                .is_err()
        );
        assert_eq!(
            destination
                .lock()
                .unwrap()
                .conn
                .query_row("SELECT COUNT(*) FROM attachments", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        restore_zip(destination.clone(), &zip, &mapping, Some(&storage), true)
            .await
            .unwrap();
        let attachment = destination.lock().unwrap().attachment("dev", id).unwrap();
        assert_eq!(storage.get(&attachment).await.unwrap(), bytes);
        let again =
            std::env::temp_dir().join(format!("todoer-reexport-{}.zip", uuid::Uuid::new_v4()));
        let reexport = export_zip(
            destination.clone(),
            "dev".into(),
            ExportScope::Account,
            Some(&storage),
            None,
            &again,
        )
        .await
        .unwrap();
        assert_eq!(reexport.entries.len(), 2);
        let checked = validated_zip_bundle(&again).unwrap();
        assert_eq!(
            data(&checked, "legacy_attachment_refs")[0]["mime"],
            "text/plain"
        );
        std::fs::remove_file(again).unwrap();
        let object_count = objects.lock().unwrap().len();
        restore_zip(destination.clone(), &zip, &mapping, Some(&storage), true)
            .await
            .unwrap();
        assert_eq!(objects.lock().unwrap().len(), object_count);
        assert!(attachment.object_key.starts_with("objects/restore-"));
        let final_key = format!("files/destination/{}", attachment.object_key);
        objects
            .lock()
            .unwrap()
            .insert(final_key.clone(), b"corrupt".to_vec());
        let mut clean = Store::memory().unwrap();
        clean.seed_dev().unwrap();
        let clean = Arc::new(Mutex::new(clean));
        assert!(
            restore_zip(clean.clone(), &zip, &mapping, Some(&storage), true)
                .await
                .is_err()
        );
        assert_eq!(
            clean
                .lock()
                .unwrap()
                .conn
                .query_row("SELECT COUNT(*) FROM attachments", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(objects.lock().unwrap()[&final_key], b"corrupt");
        std::fs::remove_file(zip).unwrap();
        std::fs::remove_file(source_path).unwrap();
        server.abort();
    }
    #[tokio::test]
    async fn empty_attachment_zip_is_private_complete_and_checksummed() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let path = std::env::temp_dir().join(format!("todoer-{}.zip", uuid::Uuid::new_v4()));
        let manifest = export_zip(
            Arc::new(Mutex::new(db)),
            "dev".into(),
            ExportScope::Account,
            None,
            None,
            &path,
        )
        .await
        .unwrap();
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(verify_zip(&path).unwrap().entries[0].path, "business.json");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(write_private_new(&path, b"must not replace").is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    #[ignore = "explicit local legacy-copy validation; prints no source rows"]
    fn actual_legacy_readonly_dry_run() {
        let path = Path::new("../.local/rebuild/legacy.db");
        let before = Sha256::digest(std::fs::read(path).unwrap());
        let source = read_legacy(path).unwrap();
        let mut db = Store::memory().unwrap();
        let result = import_legacy(&mut db, &source, &IdentityMap::default(), false, false);
        if let Err(error) = result {
            panic!("legacy dry-run static validation: {error}")
        }
        assert_eq!(before, Sha256::digest(std::fs::read(path).unwrap()));
    }
    #[tokio::test]
    async fn zip_legacy_bytes_use_safe_paths_and_fresh_checksums() {
        use axum::{Router, http::header, routing::any};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/{*path}",
            any(|| async { ([(header::CONTENT_LENGTH, "0")], Vec::<u8>::new()) }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (source_path, _) = fixture();
        {
            let c = Connection::open(&source_path).unwrap();
            c.execute("INSERT INTO attachments VALUES('00000000-0000-4000-8000-000000000010','att0001','text/plain','../../unsafe.txt',0,'2026-01-01 10:00:00',?1)",[TASK]).unwrap();
        }
        let source = read_legacy(&source_path).unwrap();
        let mut db = Store::memory().unwrap();
        import_legacy(&mut db, &source, &IdentityMap::default(), true, false).unwrap();
        let storage = crate::object_storage::LegacyBlobSource::new(
            crate::object_storage::LegacyStorageConfig {
                endpoint: format!("http://{address}"),
                bucket: "source".into(),
                region: "us-east-1".into(),
                prefix: String::new(),
                access_key_id: "synthetic".into(),
                secret_access_key: "synthetic-secret".into(),
            },
        )
        .unwrap();
        let path = std::env::temp_dir().join(format!("todoer-legacy-{}.zip", uuid::Uuid::new_v4()));
        let manifest = export_zip(
            Arc::new(Mutex::new(db)),
            format!("legacy:{USER}"),
            ExportScope::Account,
            None,
            Some(&storage),
            &path,
        )
        .await
        .unwrap();
        assert_eq!(manifest.entries.len(), 2);
        let entry = manifest
            .entries
            .iter()
            .find(|e| e.path.starts_with("legacy-attachments/"))
            .unwrap();
        assert_eq!(entry.size, 0);
        assert_eq!(entry.sha256, bytes_hash(&[]));
        assert!(!entry.path.contains("unsafe"));
        verify_zip(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(source_path).unwrap();
        server.abort();
    }
}

#[cfg(test)]
mod lifecycle_export_tests {
    use super::*;
    #[test]
    fn account_archive_roundtrip_preserves_owned_workspace_trash_colors_and_short_references() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let w = db
            .create_workspace(
                "dev",
                crate::workspace::WorkspaceInput {
                    title: "Archived project".into(),
                    description: "Retained brief".into(),
                },
            )
            .unwrap();
        let list = db
            .create_list_with_color(
                "dev",
                crate::workspace::CreateList {
                    workspace_id: w.id.clone(),
                    title: "Retained list".into(),
                    description: String::new(),
                    restricted: false,
                },
                Some("#123456".into()),
            )
            .unwrap();
        let reference = db.list_reference("dev", &list.id).unwrap();
        let revision = db.workspace("dev", &w.id).unwrap().revision;
        db.change_workspace_lifecycle("dev", &w.id, revision, "archive", None)
            .unwrap();
        let revision = db.workspace("dev", &w.id).unwrap().revision;
        db.change_workspace_lifecycle("dev", &w.id, revision, "delete", Some(&w.title))
            .unwrap();
        let bundle = db.export_bundle("dev", ExportScope::Account).unwrap();
        assert!(bundle.tables["lists"].iter().any(|r| r["id"] == list.id));
        assert!(
            bundle.tables["workspace_lifecycle"]
                .iter()
                .any(|r| r["workspace_id"] == w.id && !r["deleted_at"].is_null())
        );
        let other = db.export_bundle("other", ExportScope::Account).unwrap();
        assert!(!other.tables["workspaces"].iter().any(|r| r["id"] == w.id));
        let mut target = Store::memory().unwrap();
        import_bundle(&mut target, &bundle, &IdentityMap::default(), true).unwrap();
        assert_eq!(
            target
                .conn
                .query_row(
                    "SELECT reference FROM list_references WHERE list_id=?1",
                    [&list.id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            reference
        );
        assert_eq!(
            target
                .conn
                .query_row("SELECT color_hex FROM lists WHERE id=?1", [&list.id], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "#123456"
        );
        assert!(
            target
                .conn
                .query_row(
                    "SELECT deleted_at IS NOT NULL FROM workspace_lifecycle WHERE workspace_id=?1",
                    [w.id],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap()
        );
    }
}
