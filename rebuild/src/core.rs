use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct List {
    pub id: String,
    pub title: String,
    pub description: String,
    pub revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub reference: String,
    pub list_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: String,
    pub complete: bool,
    pub revision: i64,
    pub position: i64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub list_id: String,
    pub tasks: Vec<NewTask>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewTask {
    pub title: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub description: String,
}
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub revision: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub complete: Option<bool>,
    #[serde(default)]
    pub complete_subtree: bool,
}
pub struct Store {
    pub(crate) conn: Connection,
}
fn task_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        reference: r.get(1)?,
        list_id: r.get(2)?,
        parent_id: r.get(3)?,
        title: r.get(4)?,
        description: r.get(5)?,
        complete: r.get(6)?,
        revision: r.get(7)?,
        position: r.get(8)?,
    })
}
const COLS: &str = "id,reference,list_id,parent_id,title,description,complete,revision,position";
pub(crate) fn raw_task(c: &Connection, id: &str) -> Result<Task, Error> {
    c.query_row(
        &format!("SELECT {COLS} FROM tasks WHERE id=?1 OR reference=?1"),
        [id],
        task_row,
    )
    .optional()?
    .ok_or(Error::NotFound)
}
pub(crate) fn task(c: &Connection, id: &str) -> Result<Task, Error> {
    let t = raw_task(c, id)?;
    let deleted: bool = c.query_row(
        "SELECT deleted_at IS NOT NULL FROM tasks WHERE id=?1",
        [&t.id],
        |r| r.get(0),
    )?;
    if deleted { Err(Error::NotFound) } else { Ok(t) }
}
fn access(c: &Connection, actor: &str, list: &str) -> Result<(), Error> {
    access_rank(c, actor, list, 1)
}
pub(crate) fn access_rank(c: &Connection, actor: &str, list: &str, rank: i64) -> Result<(), Error> {
    if !c.query_row(
        "SELECT EXISTS(SELECT 1 FROM lists WHERE id=?1)",
        [list],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::NotFound);
    }
    if !c.query_row(
        "SELECT EXISTS(SELECT 1 FROM actor_list_access WHERE list_id=?1 AND actor_id=?2 AND rank>=?3)",
        params![list, actor, rank],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::Forbidden);
    }
    Ok(())
}
pub(crate) fn actor_user(c: &Connection, actor: &str, required: i64) -> Result<String, Error> {
    if let Some(id) = actor.strip_prefix("credential:") {
        let user = c.query_row("SELECT user_id FROM auth_agents a WHERE id=?1 AND revoked=0 AND expires_at>unixepoch() AND list_ids IS NULL AND workspace IS NULL AND EXISTS(SELECT 1 FROM json_each(a.permissions) WHERE CASE value WHEN 'admin' THEN 3 WHEN 'write' THEN 2 WHEN 'read' THEN 1 ELSE 0 END >= ?2)", params![id,required], |r|r.get(0)).optional()?;
        return user.ok_or(Error::Forbidden);
    }
    c.query_row("SELECT id FROM users WHERE id=?1", [actor], |r| r.get(0))
        .optional()?
        .ok_or(Error::Forbidden)
}
pub(crate) fn actor_identity(
    c: &Connection,
    actor: &str,
) -> Result<(String, Option<String>), Error> {
    if let Some(id) = actor.strip_prefix("credential:") {
        let user=c.query_row("SELECT user_id FROM auth_agents WHERE id=?1 AND revoked=0 AND expires_at>unixepoch()",[id],|r|r.get(0)).optional()?.ok_or(Error::Forbidden)?;
        Ok((user, Some(id.to_owned())))
    } else {
        Ok((actor.to_owned(), None))
    }
}
fn title(s: &str) -> Result<(), Error> {
    if s.trim().is_empty() || s.chars().count() > 500 {
        Err(Error::Invalid("title must contain 1–500 characters".into()))
    } else {
        Ok(())
    }
}
fn descendants(c: &Connection, id: &str) -> Result<Vec<Task>, Error> {
    // The deleted_at index otherwise makes SQLite scan every live task for
    // each recursive step. Parent lookup keeps traversal bounded to children.
    let mut q=c.prepare(&format!("WITH RECURSIVE tree(id) AS (SELECT id FROM tasks WHERE id=?1 AND deleted_at IS NULL UNION ALL SELECT t.id FROM tasks t INDEXED BY tasks_parent JOIN tree ON t.parent_id=tree.id WHERE t.deleted_at IS NULL) SELECT {COLS} FROM tasks WHERE id IN tree ORDER BY position,id"))?;
    let rows = q.query_map([id], task_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}
pub(crate) fn event(
    c: &Connection,
    t: &Task,
    kind: &str,
    actor: &str,
    before: Option<&Task>,
) -> Result<(), Error> {
    if let Some(previous) = before {
        crate::scheduling::after_task_update(
            c,
            actor,
            previous,
            t,
            chrono::Utc::now().timestamp(),
        )?;
    }
    if matches!(kind, "create" | "update" | "undo" | "recurrence-create") {
        crate::taxonomy::index_references(c, actor, "task", &t.id, &t.description)?;
    }
    let (user, credential) = actor_identity(c, actor)?;
    let leaf: bool = c.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM tasks WHERE parent_id=?1 AND deleted_at IS NULL)",
        [&t.id],
        |row| row.get(0),
    )?;
    let value = serde_json::json!({"kind":kind,"task":t,"actor":user,"client":credential,"countable_leaf":leaf,"request_id":crate::audit::request_id(),"attribution":if credential.is_some(){"agent"}else{"human"}});
    c.execute(
        "INSERT INTO history(list_id,event,actor,before_event) VALUES(?1,?2,?3,?4)",
        params![
            t.list_id,
            value.to_string(),
            user,
            before.map(|t| serde_json::to_string(t).expect("serialize task"))
        ],
    )?;
    Ok(())
}
fn reopen(c: &Connection, id: &str, actor: &str) -> Result<(), Error> {
    let mut parent = task(c, id)?.parent_id;
    while let Some(id) = parent {
        let p = task(c, &id)?;
        parent = p.parent_id.clone();
        if p.complete {
            c.execute(
                "UPDATE tasks SET complete=0,revision=revision+1 WHERE id=?1",
                [&id],
            )?;
            event(c, &task(c, &id)?, "update", actor, Some(&p))?;
        }
    }
    Ok(())
}
fn audience_change(
    c: &Connection,
    source: &str,
    destination: &str,
) -> Result<(i64, i64, i64), Error> {
    let count = |list: &str| -> Result<i64, Error> {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM effective_list_access WHERE list_id=?1",
            [list],
            |r| r.get(0),
        )?)
    };
    let added = c.query_row("SELECT COUNT(*) FROM effective_list_access d WHERE d.list_id=?1 AND NOT EXISTS(SELECT 1 FROM effective_list_access s WHERE s.list_id=?2 AND s.user_id=d.user_id)", params![destination, source], |r| r.get(0))?;
    Ok((count(source)?, count(destination)?, added))
}
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/001.sql")),
    (2, include_str!("../migrations/002.sql")),
    (3, include_str!("../migrations/003.sql")),
    (4, include_str!("../migrations/004_auth.sql")),
    (5, include_str!("../migrations/005_workspace.sql")),
    (6, include_str!("../migrations/006_credentials.sql")),
    (7, include_str!("../migrations/007_auth_oauth.sql")),
    (8, include_str!("../migrations/008_scheduling.sql")),
    (9, include_str!("../migrations/009_discussion.sql")),
    (10, include_str!("../migrations/010_notifications.sql")),
    (11, include_str!("../migrations/011_taxonomy.sql")),
    (12, include_str!("../migrations/012_transfer.sql")),
    (13, include_str!("../migrations/013_invitations.sql")),
    (14, include_str!("../migrations/014_agent_ops.sql")),
    (15, include_str!("../migrations/015_push.sql")),
    (16, include_str!("../migrations/016_search.sql")),
    (17, include_str!("../migrations/017_navigation.sql")),
    (18, include_str!("../migrations/018_stats.sql")),
    (19, include_str!("../migrations/019_assist.sql")),
    (20, include_str!("../migrations/020_subtree.sql")),
    (21, include_str!("../migrations/021_push_digests.sql")),
    (22, include_str!("../migrations/022_slugs.sql")),
];
impl Store {
    pub fn open(path: &str) -> Result<Self, Error> {
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let legacy: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='spaces') OR EXISTS(SELECT 1 FROM pragma_table_info('users') WHERE name='uuid')", [], |r| r.get(0))?;
        if legacy {
            return Err(Error::Invalid(
                "legacy Todoer database detected; use a separate rebuild database".into(),
            ));
        }
        let versioned:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",[],|r|r.get(0))?;
        if versioned {
            let newest: i64 = conn.query_row(
                "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
                [],
                |r| r.get(0),
            )?;
            if newest > MIGRATIONS.last().unwrap().0 {
                return Err(Error::Invalid("Database schema is newer than this executable; restore a compatible recovery point or use the newer application".into()));
            }
        }
        if path != ":memory:" {
            conn.pragma_update(None, "journal_mode", "WAL")?;
        }
        conn.pragma_update(None, "foreign_keys", true)?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)")?;
        for &(version, sql) in MIGRATIONS {
            let applied: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=?1)",
                [version],
                |r| r.get(0),
            )?;
            if !applied {
                tx.execute_batch(sql)?;
                if version == 1 {
                    tx.execute("INSERT INTO tasks_fts(tasks_fts) VALUES('rebuild')", [])?;
                }
                tx.execute(
                    "INSERT INTO schema_migrations(version) VALUES(?1)",
                    [version],
                )?;
            }
        }
        crate::slugs::backfill(&tx)?;
        tx.commit()?;
        Ok(Self { conn })
    }
    pub fn memory() -> Result<Self, Error> {
        Self::open(":memory:")
    }
    pub fn seed_dev(&mut self) -> Result<(), Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        tx.execute_batch("INSERT OR IGNORE INTO users VALUES('dev','alice'),('other','bob'); INSERT OR IGNORE INTO lists(id,title) VALUES('inbox','Inbox'),('groceries','Groceries'),('private','Private'); INSERT OR IGNORE INTO memberships VALUES('inbox','dev'),('groceries','dev'),('private','other');")?;
        crate::slugs::backfill(&tx)?;
        tx.commit()?;
        Ok(())
    }
    pub fn lists(&self, actor: &str) -> Result<Vec<List>, Error> {
        let mut q=self.conn.prepare("SELECT l.id,l.title,l.description,l.revision FROM lists l JOIN actor_list_access m ON m.list_id=l.id WHERE m.actor_id=?1 ORDER BY l.position,l.id")?;
        Ok(q.query_map([actor], |r| {
            Ok(List {
                id: r.get(0)?,
                title: r.get(1)?,
                description: r.get(2)?,
                revision: r.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?)
    }
    /// Search literal whitespace-separated prefixes; punctuation never becomes FTS syntax.
    pub fn search(
        &self,
        actor: &str,
        query: &str,
        list: Option<&str>,
        complete: Option<bool>,
        limit: i64,
        after: Option<&str>,
    ) -> Result<Vec<Task>, Error> {
        if !(1..=100).contains(&limit) {
            return Err(Error::Invalid("limit must be between 1 and 100".into()));
        }
        if query.len() > 4000 {
            return Err(Error::Invalid("search query too long".into()));
        }
        if let Some(list) = list {
            access(&self.conn, actor, list)?;
        }
        let expression = query
            .split_whitespace()
            .map(|token| format!("\"{}\"*", token.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        let columns = COLS
            .split(',')
            .map(|col| format!("t.{col}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT {columns} FROM tasks t JOIN actor_list_access m ON m.list_id=t.list_id WHERE m.actor_id=?1 AND t.deleted_at IS NULL AND (?2 IS NULL OR t.list_id=?2) AND (?3 IS NULL OR t.complete=?3) AND (?4 IS NULL OR t.id>?4) {} ORDER BY t.id LIMIT ?6",
            if expression.is_empty() {
                ""
            } else {
                "AND t.rowid IN (SELECT rowid FROM tasks_fts WHERE tasks_fts MATCH ?5)"
            }
        );
        let mut stmt = self.conn.prepare(&sql)?;
        // Bind ?5 even for an empty query: numbered ?6 reserves its slot.
        Ok(stmt
            .query_map(
                params![actor, list, complete, after, expression, limit],
                task_row,
            )?
            .collect::<Result<_, _>>()?)
    }
    pub fn tasks(&self, actor: &str, list: &str) -> Result<Vec<Task>, Error> {
        access(&self.conn, actor, list)?;
        let mut q = self.conn.prepare(&format!(
            "SELECT {COLS} FROM tasks WHERE list_id=?1 AND deleted_at IS NULL ORDER BY position,id"
        ))?;
        Ok(q.query_map([list], task_row)?.collect::<Result<_, _>>()?)
    }
    pub fn capture(
        &mut self,
        actor: &str,
        key: &str,
        request: Capture,
    ) -> Result<Vec<Task>, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        access_rank(&tx, actor, &request.list_id, 2)?;
        if key.trim().is_empty() || key.len() > 200 {
            return Err(Error::Invalid(
                "idempotency key must contain 1–200 bytes".into(),
            ));
        }
        let payload = serde_json::to_string(&request).expect("serialize capture");
        if let Some((old, response)) = tx
            .query_row(
                "SELECT payload,response FROM idempotency WHERE actor=?1 AND key=?2",
                params![actor, key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            if old != payload {
                return Err(Error::Conflict(
                    "idempotency key reused with different payload".into(),
                ));
            }
            let result: Vec<Task> =
                serde_json::from_str(&response).map_err(|e| Error::Invalid(e.to_string()))?;
            for original in &result {
                let current = task(&tx, &original.id)?;
                access_rank(&tx, actor, &current.list_id, 2)?;
            }
            return Ok(result);
        }
        if request.tasks.is_empty() || request.tasks.len() > 100 {
            return Err(Error::Invalid("capture requires 1–100 tasks".into()));
        }
        let mut position: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position),0) FROM tasks WHERE list_id=?1",
            [&request.list_id],
            |r| r.get(0),
        )?;
        let mut result = Vec::new();
        for mut n in request.tasks {
            title(&n.title)?;
            if n.description.len() > 100_000 {
                return Err(Error::Invalid("description too long".into()));
            }
            if let Some(ref p) = n.parent_id {
                let p = task(&tx, p)?;
                if p.list_id != request.list_id {
                    return Err(Error::Invalid("parent must belong to the same list".into()));
                }
                n.parent_id = Some(p.id);
            }
            position += 1;
            let (id, reference) = loop {
                let id = Uuid::new_v4().to_string();
                let reference = format!("T-{}", &id[..8]);
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1 OR reference=?2)",
                    params![id, reference],
                    |r| r.get(0),
                )?;
                if !exists {
                    break (id, reference);
                }
            };
            tx.execute("INSERT INTO tasks(id,reference,list_id,parent_id,title,description,position) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,reference,request.list_id,n.parent_id,n.title.trim(),n.description,position])?;
            let t = task(&tx, &id)?;
            reopen(&tx, &id, actor)?;
            event(&tx, &t, "create", actor, None)?;
            result.push(t);
        }
        tx.execute(
            "INSERT INTO idempotency VALUES(?1,?2,?3,?4)",
            params![actor, key, payload, serde_json::to_string(&result).unwrap()],
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn subtree(&self, actor: &str, id: &str) -> Result<Vec<Task>, Error> {
        let t = task(&self.conn, id)?;
        access(&self.conn, actor, &t.list_id)?;
        descendants(&self.conn, &t.id)
    }
    pub fn update(&mut self, actor: &str, id: &str, change: Change) -> Result<Task, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let t = task(&tx, id)?;
        access_rank(&tx, actor, &t.list_id, 2)?;
        if t.revision != change.revision {
            return Err(Error::Conflict("stale revision".into()));
        }
        if let Some(ref s) = change.title {
            title(s)?
        }
        if change
            .description
            .as_ref()
            .is_some_and(|s| s.len() > 100_000)
        {
            return Err(Error::Invalid("description too long".into()));
        }
        if change.complete == Some(true) {
            crate::scheduling::validate_recurrence_completion(&tx, &t.id, change.complete_subtree)?;
            let children = descendants(&tx, &t.id)?;
            if !change.complete_subtree && children.iter().any(|c| c.id != t.id && !c.complete) {
                return Err(Error::Conflict(
                    "complete unfinished descendants first or set complete_subtree".into(),
                ));
            }
            if change.complete_subtree {
                for child in children.into_iter().filter(|c| c.id != t.id && !c.complete) {
                    tx.execute(
                        "UPDATE tasks SET complete=1,revision=revision+1 WHERE id=?1",
                        [&child.id],
                    )?;
                    event(&tx, &task(&tx, &child.id)?, "update", actor, Some(&child))?;
                }
            }
        }
        tx.execute(
            "UPDATE tasks SET title=?1,description=?2,complete=?3,revision=revision+1 WHERE id=?4",
            params![
                change.title.as_deref().map(str::trim).unwrap_or(&t.title),
                change.description.as_deref().unwrap_or(&t.description),
                change.complete.unwrap_or(t.complete),
                t.id
            ],
        )?;
        if change.complete == Some(false) {
            reopen(&tx, &t.id, actor)?
        }
        let updated = task(&tx, &t.id)?;
        event(&tx, &updated, "update", actor, Some(&t))?;
        tx.commit()?;
        Ok(updated)
    }
    /// Compensate one update event, never a whole cascade or a newer edit.
    pub fn undo_update(&mut self, actor: &str, seq: i64, revision: i64) -> Result<Task, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let (encoded, original_actor, before): (String, String, Option<String>) = tx
            .query_row(
                "SELECT event,actor,before_event FROM history WHERE seq=?1",
                [seq],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or(Error::NotFound)?;
        let value: serde_json::Value =
            serde_json::from_str(&encoded).map_err(|e| Error::Invalid(e.to_string()))?;
        let recorded: Task = serde_json::from_value(value["task"].clone())
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let current = task(&tx, &recorded.id)?;
        access_rank(&tx, actor, &current.list_id, 2)?;
        if original_actor != actor_identity(&tx, actor)?.0 {
            return Err(Error::Forbidden);
        }
        if value["kind"] != "update" || before.is_none() {
            return Err(Error::Invalid(
                "only individual task update events can be undone".into(),
            ));
        }
        if current.revision != revision || current.revision != recorded.revision {
            return Err(Error::Conflict(
                "task changed after this event; undo would overwrite newer work".into(),
            ));
        }
        let previous: Task =
            serde_json::from_str(&before.unwrap()).map_err(|e| Error::Invalid(e.to_string()))?;
        if previous.complete
            && descendants(&tx, &current.id)?
                .iter()
                .any(|t| t.id != current.id && !t.complete)
        {
            return Err(Error::Conflict(
                "undo would complete a task with unfinished descendants".into(),
            ));
        }
        tx.execute(
            "UPDATE tasks SET title=?1,description=?2,complete=?3,revision=revision+1 WHERE id=?4",
            params![
                previous.title,
                previous.description,
                previous.complete,
                current.id
            ],
        )?;
        if !previous.complete {
            reopen(&tx, &current.id, actor)?;
        }
        let result = task(&tx, &current.id)?;
        event(&tx, &result, "undo", actor, Some(&current))?;
        tx.commit()?;
        Ok(result)
    }
    pub fn trash_tasks(&self, actor: &str) -> Result<Vec<Task>, Error> {
        let cols = COLS
            .split(',')
            .map(|col| format!("t.{col}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut q = self.conn.prepare(&format!("SELECT {cols} FROM tasks t JOIN actor_list_access m ON m.list_id=t.list_id WHERE m.actor_id=?1 AND t.deleted_at IS NOT NULL ORDER BY t.id"))?;
        Ok(q.query_map([actor], task_row)?.collect::<Result<_, _>>()?)
    }
    pub fn trash(&mut self, actor: &str, id: &str, revision: i64) -> Result<Task, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let root = task(&tx, id)?;
        access_rank(&tx, actor, &root.list_id, 2)?;
        if root.revision != revision {
            return Err(Error::Conflict("stale revision".into()));
        }
        for t in descendants(&tx, &root.id)? {
            tx.execute("UPDATE tasks SET deleted_at=CURRENT_TIMESTAMP,delete_group=?1,revision=revision+1 WHERE id=?2",params![root.id,t.id])?;
            event(&tx, &raw_task(&tx, &t.id)?, "trash", actor, Some(&t))?;
        }
        let result = raw_task(&tx, &root.id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn restore(&mut self, actor: &str, id: &str, revision: i64) -> Result<Task, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let root = raw_task(&tx, id)?;
        access_rank(&tx, actor, &root.list_id, 2)?;
        if root.revision != revision {
            return Err(Error::Conflict("stale revision".into()));
        }
        let group: Option<String> = tx
            .query_row(
                "SELECT delete_group FROM tasks WHERE id=?1 AND deleted_at IS NOT NULL",
                [&root.id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if group.as_deref() != Some(root.id.as_str()) {
            return Err(Error::Conflict(
                "restore the root of this trash operation".into(),
            ));
        }
        if let Some(ref parent) = root.parent_id
            && task(&tx, parent).is_err()
        {
            return Err(Error::Conflict("restore the parent first".into()));
        }
        let members = {
            let mut q = tx.prepare(&format!(
                "SELECT {COLS} FROM tasks WHERE delete_group=?1 AND deleted_at IS NOT NULL"
            ))?;
            q.query_map([&root.id], task_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        for t in members {
            access_rank(&tx, actor, &t.list_id, 2)?;
            tx.execute("UPDATE tasks SET deleted_at=NULL,delete_group=NULL,revision=revision+1 WHERE id=?1",[&t.id])?;
            event(&tx, &raw_task(&tx, &t.id)?, "restore", actor, Some(&t))?;
        }
        // Restoring an unfinished child must reopen completed ancestors.
        if !root.complete {
            reopen(&tx, &root.id, actor)?;
        }
        let result = task(&tx, &root.id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn move_preview(
        &self,
        actor: &str,
        id: &str,
        destination: &str,
    ) -> Result<serde_json::Value, Error> {
        let t = task(&self.conn, id)?;
        access(&self.conn, actor, &t.list_id)?;
        access(&self.conn, actor, destination)?;
        let (source_members, destination_members, added_members) =
            audience_change(&self.conn, &t.list_id, destination)?;
        Ok(
            serde_json::json!({"destination": destination, "audience_broadens": added_members > 0, "source_members": source_members, "destination_members": destination_members, "added_members": added_members}),
        )
    }
    pub fn move_subtree(
        &mut self,
        actor: &str,
        id: &str,
        destination: &str,
        revision: i64,
        acknowledge_access_change: bool,
    ) -> Result<Task, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let t = task(&tx, id)?;
        access_rank(&tx, actor, &t.list_id, 2)?;
        access_rank(&tx, actor, destination, 2)?;
        if t.revision != revision {
            return Err(Error::Conflict("stale revision".into()));
        }
        if t.list_id == destination {
            return Ok(t);
        }
        if audience_change(&tx, &t.list_id, destination)?.2 > 0 && !acknowledge_access_change {
            return Err(Error::Conflict("destination adds members who can access this subtree; inspect move_preview and set acknowledge_access_change=true to confirm".into()));
        }
        let mut pos: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position),0) FROM tasks WHERE list_id=?1",
            [destination],
            |r| r.get(0),
        )?;
        // Hidden descendants travel too, preserving parent/list invariants for a later restore.
        let moving = {
            let mut q = tx.prepare(&format!("WITH RECURSIVE tree(id) AS (SELECT id FROM tasks WHERE id=?1 UNION ALL SELECT child.id FROM tasks child JOIN tree ON child.parent_id=tree.id) SELECT {COLS} FROM tasks WHERE id IN tree ORDER BY position,id"))?;
            q.query_map([&t.id], task_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        let moving_ids = moving
            .iter()
            .map(|child| child.id.clone())
            .collect::<Vec<_>>();
        for child in moving {
            pos += 1;
            tx.execute("UPDATE tasks SET list_id=?1,parent_id=CASE WHEN id=?2 THEN NULL ELSE parent_id END,position=?3,revision=revision+1 WHERE id=?4",params![destination,t.id,pos,child.id])?;
            let moved = raw_task(&tx, &child.id)?;
            event(&tx, &child, "move_out", actor, None)?;
            event(&tx, &moved, "move", actor, Some(&child))?;
        }
        crate::taxonomy::after_move(&tx, actor, &moving_ids, destination)?;
        let moved = task(&tx, &t.id)?;
        tx.commit()?;
        Ok(moved)
    }
    pub fn changes(&self, actor: &str, after: i64) -> Result<Vec<serde_json::Value>, Error> {
        let mut q=self.conn.prepare("SELECT h.seq,h.event FROM history h JOIN actor_list_access m ON m.list_id=h.list_id JOIN tasks t ON t.id=json_extract(h.event,'$.task.id') JOIN actor_list_access live ON live.list_id=t.list_id AND live.actor_id=m.actor_id WHERE m.actor_id=?1 AND h.seq>?2 ORDER BY h.seq LIMIT 1000")?;
        let rows = q.query_map(params![actor, after], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|r| {
            let (seq, s) = r?;
            let mut v: serde_json::Value =
                serde_json::from_str(&s).map_err(|e| Error::Invalid(e.to_string()))?;
            v["seq"] = seq.into();
            Ok(v)
        })
        .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Store {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        s
    }
    fn capture(s: &mut Store, key: &str, parent: Option<String>) -> Task {
        s.capture(
            "dev",
            key,
            Capture {
                list_id: "inbox".into(),
                tasks: vec![NewTask {
                    title: "Task".into(),
                    parent_id: parent,
                    description: String::new(),
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    #[test]
    fn permissions_and_atomic_capture() {
        let mut s = db();
        assert!(matches!(s.tasks("dev", "private"), Err(Error::Forbidden)));
        let r = s.capture(
            "dev",
            "batch",
            Capture {
                list_id: "inbox".into(),
                tasks: vec![
                    NewTask {
                        title: "valid".into(),
                        parent_id: None,
                        description: String::new(),
                    },
                    NewTask {
                        title: " ".into(),
                        parent_id: None,
                        description: String::new(),
                    },
                ],
            },
        );
        assert!(r.is_err());
        assert!(s.tasks("dev", "inbox").unwrap().is_empty());
        assert!(s.changes("dev", 0).unwrap().is_empty());
    }
    #[test]
    fn idempotency_and_revision() {
        let mut s = db();
        let a = capture(&mut s, "one", None);
        let b = capture(&mut s, "one", None);
        assert_eq!(a.id, b.id);
        assert_eq!(s.tasks("dev", "inbox").unwrap().len(), 1);
        assert!(matches!(
            s.capture(
                "dev",
                "one",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![]
                }
            ),
            Err(Error::Conflict(_))
        ));
        let b = s
            .update(
                "dev",
                &a.id,
                Change {
                    revision: 1,
                    title: Some("renamed".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(b.revision, 2);
        assert!(matches!(
            s.update(
                "dev",
                &a.id,
                Change {
                    revision: 1,
                    ..Default::default()
                }
            ),
            Err(Error::Conflict(_))
        ));
    }
    #[test]
    fn completion_and_move() {
        let mut s = db();
        let root = capture(&mut s, "root", None);
        let child = capture(&mut s, "child", Some(root.id.clone()));
        assert!(matches!(
            s.update(
                "dev",
                &root.id,
                Change {
                    revision: 1,
                    complete: Some(true),
                    ..Default::default()
                }
            ),
            Err(Error::Conflict(_))
        ));
        let root = s
            .update(
                "dev",
                &root.id,
                Change {
                    revision: 1,
                    complete: Some(true),
                    complete_subtree: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(
            s.subtree("dev", &root.id)
                .unwrap()
                .iter()
                .all(|t| t.complete)
        );
        s.update(
            "dev",
            &child.id,
            Change {
                revision: 2,
                complete: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        let root = s
            .subtree("dev", &root.id)
            .unwrap()
            .into_iter()
            .find(|t| t.id == root.id)
            .unwrap();
        assert!(!root.complete);
        assert!(matches!(
            s.move_subtree("dev", &root.id, "private", root.revision, false),
            Err(Error::Forbidden)
        ));
        let moved = s
            .move_subtree("dev", &root.id, "groceries", root.revision, false)
            .unwrap();
        assert_eq!(moved.list_id, "groceries");
        let tree = s.subtree("dev", &root.id).unwrap();
        assert_eq!(tree.len(), 2);
        assert!(tree.iter().all(|t| t.list_id == "groceries"));
        assert_eq!(
            tree.iter()
                .find(|t| t.id == child.id)
                .unwrap()
                .parent_id
                .as_deref(),
            Some(root.id.as_str())
        );
        assert!(s.tasks("dev", "inbox").unwrap().is_empty());
    }
    #[test]
    fn live_membership_controls_history() {
        let mut s = db();
        capture(&mut s, "one", None);
        assert!(!s.changes("dev", 0).unwrap().is_empty());
        s.conn
            .execute(
                "DELETE FROM memberships WHERE user_id='dev' AND list_id='inbox'",
                [],
            )
            .unwrap();
        assert!(s.changes("dev", 0).unwrap().is_empty());
        assert!(matches!(s.tasks("dev", "inbox"), Err(Error::Forbidden)));
    }
    #[test]
    fn capture_reopens_completed_ancestors_and_checks_parent_list() {
        let mut s = db();
        let root = capture(&mut s, "root", None);
        s.update(
            "dev",
            &root.id,
            Change {
                revision: 1,
                complete: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let child = capture(&mut s, "child", Some(root.reference.clone()));
        assert_eq!(child.parent_id.as_deref(), Some(root.id.as_str()));
        assert!(
            !s.subtree("dev", &root.id)
                .unwrap()
                .iter()
                .find(|t| t.id == root.id)
                .unwrap()
                .complete
        );
        assert!(matches!(
            s.update(
                "other",
                &child.id,
                Change {
                    revision: 1,
                    ..Default::default()
                }
            ),
            Err(Error::Forbidden)
        ));
        assert!(
            s.capture(
                "dev",
                "wrong-list",
                Capture {
                    list_id: "groceries".into(),
                    tasks: vec![NewTask {
                        title: "child".into(),
                        parent_id: Some(root.id),
                        description: String::new()
                    }]
                }
            )
            .is_err()
        );
        assert!(s.tasks("dev", "groceries").unwrap().is_empty());
    }
    #[test]
    fn broadened_move_requires_explicit_acknowledgement() {
        let mut s = db();
        let root = capture(&mut s, "root", None);
        s.conn
            .execute("INSERT INTO memberships VALUES('groceries','other')", [])
            .unwrap();
        let preview = s.move_preview("dev", &root.id, "groceries").unwrap();
        assert_eq!(preview["audience_broadens"], true);
        assert_eq!(preview["added_members"], 1);
        let before = s.changes("dev", 0).unwrap().len();
        assert!(matches!(
            s.move_subtree("dev", &root.id, "groceries", root.revision, false),
            Err(Error::Conflict(_))
        ));
        assert_eq!(s.tasks("dev", "inbox").unwrap().len(), 1);
        assert_eq!(s.changes("dev", 0).unwrap().len(), before);
        assert!(matches!(
            s.move_preview("dev", &root.id, "private"),
            Err(Error::Forbidden)
        ));
        s.move_subtree("dev", &root.id, "groceries", root.revision, true)
            .unwrap();
        assert_eq!(s.tasks("other", "groceries").unwrap().len(), 1);
    }
    #[test]
    fn moved_tasks_recheck_history_and_idempotency_permissions() {
        let mut s = db();
        let root = capture(&mut s, "root", None);
        s.conn
            .execute("INSERT INTO memberships VALUES('private','dev')", [])
            .unwrap();
        s.move_subtree("dev", &root.id, "private", 1, true).unwrap();
        s.conn
            .execute(
                "DELETE FROM memberships WHERE list_id='private' AND user_id='dev'",
                [],
            )
            .unwrap();
        assert!(s.changes("dev", 0).unwrap().is_empty());
        let result = s.capture(
            "dev",
            "root",
            Capture {
                list_id: "inbox".into(),
                tasks: vec![NewTask {
                    title: "Task".into(),
                    parent_id: None,
                    description: String::new(),
                }],
            },
        );
        assert!(matches!(result, Err(Error::Forbidden)));
    }
    #[test]
    fn newer_schema_is_rejected_before_any_database_mutation() {
        let path = std::env::temp_dir().join(format!("todoer-future-{}.db", Uuid::new_v4()));
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY);INSERT INTO schema_migrations VALUES(9999);").unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        assert!(
            matches!(Store::open(path.to_str().unwrap()),Err(Error::Invalid(message)) if message.contains("newer"))
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!std::path::Path::new(&format!("{}-wal", path.display())).exists());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn legacy_database_is_rejected_without_schema_changes() {
        let path = std::env::temp_dir().join(format!("todoer-legacy-{}.db", Uuid::new_v4()));
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("CREATE TABLE users(uuid TEXT PRIMARY KEY);")
                .unwrap();
        }
        assert!(matches!(
            Store::open(path.to_str().unwrap()),
            Err(Error::Invalid(_))
        ));
        {
            let c = Connection::open(&path).unwrap();
            let count: i64 = c
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1);
        }
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn search_is_literal_authorized_filtered_and_paginated() {
        let mut s = db();
        for (actor, list, key, title) in [
            ("dev", "inbox", "a", "Überraschung 東京"),
            ("dev", "inbox", "b", "Überblick"),
            ("other", "private", "c", "Überraschung secret"),
        ] {
            s.capture(
                actor,
                key,
                Capture {
                    list_id: list.into(),
                    tasks: vec![NewTask {
                        title: title.into(),
                        parent_id: None,
                        description: "café description".into(),
                    }],
                },
            )
            .unwrap();
        }
        assert_eq!(
            s.search("dev", "Über", None, None, 100, None)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            s.search("dev", "東", None, None, 100, None).unwrap().len(),
            1
        );
        assert_eq!(
            s.search("dev", "café", None, None, 100, None)
                .unwrap()
                .len(),
            2
        );
        assert!(matches!(
            s.search("dev", "", Some("private"), None, 100, None),
            Err(Error::Forbidden)
        ));
        for query in [
            "OR",
            "NEAR(foo)",
            "\"",
            "*",
            "foo:bar",
            "-",
            "()",
            "foo OR bar",
        ] {
            assert!(
                s.search("dev", query, None, None, 100, None).is_ok(),
                "literal query {query}"
            );
        }
        let first = s.search("dev", "", None, None, 1, None).unwrap().remove(0);
        let second = s
            .search("dev", "", None, None, 1, Some(&first.id))
            .unwrap()
            .remove(0);
        assert!(first.id < second.id);
        assert!(
            s.search("dev", "", None, None, 1, Some(&second.id))
                .unwrap()
                .is_empty()
        );
        s.update(
            "dev",
            &first.id,
            Change {
                revision: 1,
                complete: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            s.search("dev", "Über", None, Some(true), 100, None)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            s.search("dev", "Über", None, Some(false), 100, None)
                .unwrap()
                .len(),
            1
        );
        s.update(
            "dev",
            &second.id,
            Change {
                revision: 1,
                title: Some("Renamed".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            s.search("dev", "Über", None, None, 100, None)
                .unwrap()
                .len(),
            1
        );
        assert!(s.search("dev", "", None, None, 101, None).is_err());
    }
    #[test]
    fn trash_restore_groups_are_atomic_and_authorized() {
        let mut s = db();
        let root = capture(&mut s, "root", None);
        let child = capture(&mut s, "child", Some(root.id.clone()));
        let independent = capture(&mut s, "independent", Some(root.id.clone()));
        let independent = s.trash("dev", &independent.id, 1).unwrap();
        assert!(matches!(
            s.trash("other", &root.id, 1),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            s.trash("dev", &root.id, 9),
            Err(Error::Conflict(_))
        ));
        let root = s.trash("dev", &root.id, 1).unwrap();
        assert!(s.tasks("dev", "inbox").unwrap().is_empty());
        assert!(
            s.search("dev", "Task", None, None, 100, None)
                .unwrap()
                .is_empty()
        );
        assert!(matches!(s.subtree("dev", &root.id), Err(Error::NotFound)));
        assert_eq!(s.trash_tasks("dev").unwrap().len(), 3);
        assert!(s.trash_tasks("other").unwrap().is_empty());
        assert!(
            s.restore("dev", &independent.id, independent.revision)
                .is_err()
        );
        assert!(matches!(
            s.restore("other", &root.id, root.revision),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            s.restore("dev", &root.id, 1),
            Err(Error::Conflict(_))
        ));
        s.restore("dev", &root.id, root.revision).unwrap();
        assert_eq!(s.subtree("dev", &root.id).unwrap().len(), 2);
        assert_eq!(s.trash_tasks("dev").unwrap().len(), 1);
        s.restore("dev", &independent.id, independent.revision)
            .unwrap();
        assert_eq!(s.subtree("dev", &root.id).unwrap().len(), 3);
        assert!(
            s.tasks("dev", "inbox")
                .unwrap()
                .iter()
                .any(|t| t.id == child.id)
        );
    }
    #[test]
    fn migrations_run_once_and_upgrade_synthetic_database() {
        let path = std::env::temp_dir().join(format!("todoer-migrate-{}.db", Uuid::new_v4()));
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(include_str!("../migrations/001.sql"))
                .unwrap();
            c.execute_batch("INSERT INTO users VALUES('dev','alice');INSERT INTO lists(id,title) VALUES('inbox','Inbox');INSERT INTO memberships VALUES('inbox','dev');INSERT INTO tasks(id,reference,list_id,title,position) VALUES('one','T-one','inbox','Existing task',1);").unwrap();
        }
        {
            let s = Store::open(path.to_str().unwrap()).unwrap();
            assert_eq!(
                s.search("dev", "Existing", None, None, 100, None)
                    .unwrap()
                    .len(),
                1
            );
            let count: i64 = s
                .conn
                .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, MIGRATIONS.len() as i64);
        }
        {
            let s = Store::open(path.to_str().unwrap()).unwrap();
            assert_eq!(s.tasks("dev", "inbox").unwrap().len(), 1);
            let mode: String = s
                .conn
                .pragma_query_value(None, "journal_mode", |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
        }
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn hidden_descendants_move_with_parent_for_safe_restore() {
        let mut s = db();
        let root = capture(&mut s, "root", None);
        let child = capture(&mut s, "child", Some(root.id.clone()));
        let child = s.trash("dev", &child.id, 1).unwrap();
        s.move_subtree("dev", &root.id, "groceries", 1, false)
            .unwrap();
        let child = s
            .trash_tasks("dev")
            .unwrap()
            .into_iter()
            .find(|t| t.id == child.id)
            .unwrap();
        assert_eq!(child.list_id, "groceries");
        s.restore("dev", &child.id, child.revision).unwrap();
        assert_eq!(s.subtree("dev", &root.id).unwrap().len(), 2);
    }
    fn update_seq(s: &Store, id: &str) -> i64 {
        s.changes("dev", 0)
            .unwrap()
            .into_iter()
            .rev()
            .find(|v| v["kind"] == "update" && v["task"]["id"] == id)
            .unwrap()["seq"]
            .as_i64()
            .unwrap()
    }
    #[test]
    fn undo_compensates_own_edit_and_refuses_newer_edits_or_access_loss() {
        let mut s = db();
        let root = capture(&mut s, "root", None);
        let edited = s
            .update(
                "dev",
                &root.id,
                Change {
                    revision: 1,
                    title: Some("Changed".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let seq = update_seq(&s, &root.id);
        s.conn
            .execute("INSERT INTO memberships VALUES('inbox','other')", [])
            .unwrap();
        assert!(matches!(
            s.undo_update("other", seq, edited.revision),
            Err(Error::Forbidden)
        ));
        let restored = s.undo_update("dev", seq, edited.revision).unwrap();
        assert_eq!(restored.title, root.title);
        assert_eq!(restored.revision, 3);
        assert!(matches!(
            s.undo_update("dev", seq, 3),
            Err(Error::Conflict(_))
        ));
        let edited = s
            .update(
                "dev",
                &root.id,
                Change {
                    revision: 3,
                    title: Some("Newer".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(matches!(
            s.undo_update("dev", seq, edited.revision),
            Err(Error::Conflict(_))
        ));
        assert_eq!(s.tasks("dev", "inbox").unwrap()[0].title, "Newer");
        let seq = update_seq(&s, &root.id);
        s.conn
            .execute(
                "DELETE FROM memberships WHERE user_id='dev' AND list_id='inbox'",
                [],
            )
            .unwrap();
        assert!(matches!(
            s.undo_update("dev", seq, edited.revision),
            Err(Error::Forbidden)
        ));
    }
    #[test]
    fn undo_completion_reopens_ancestors_and_guards_unfinished_children() {
        let mut s = db();
        let root = capture(&mut s, "root", None);
        let child = capture(&mut s, "child", Some(root.id.clone()));
        s.update(
            "dev",
            &root.id,
            Change {
                revision: 1,
                complete: Some(true),
                complete_subtree: true,
                ..Default::default()
            },
        )
        .unwrap();
        let child_seq = update_seq(&s, &child.id);
        let child = s.undo_update("dev", child_seq, 2).unwrap();
        assert!(!child.complete);
        let root = s
            .subtree("dev", &root.id)
            .unwrap()
            .into_iter()
            .find(|t| t.id == root.id)
            .unwrap();
        assert!(!root.complete);
        let reopened_seq = update_seq(&s, &root.id);
        assert!(matches!(
            s.undo_update("dev", reopened_seq, root.revision),
            Err(Error::Conflict(_))
        ));
        assert!(
            !s.tasks("dev", "inbox")
                .unwrap()
                .into_iter()
                .find(|t| t.id == root.id)
                .unwrap()
                .complete
        );
    }
}
