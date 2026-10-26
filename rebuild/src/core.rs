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
pub struct Capture {
    pub list_id: String,
    pub tasks: Vec<NewTask>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct NewTask {
    pub title: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub description: String,
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Change {
    pub revision: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub complete: Option<bool>,
    #[serde(default)]
    pub complete_subtree: bool,
}
pub struct Store {
    conn: Connection,
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
fn task(c: &Connection, id: &str) -> Result<Task, Error> {
    c.query_row(
        &format!("SELECT {COLS} FROM tasks WHERE id=?1 OR reference=?1"),
        [id],
        task_row,
    )
    .optional()?
    .ok_or(Error::NotFound)
}
fn access(c: &Connection, actor: &str, list: &str) -> Result<(), Error> {
    if !c.query_row(
        "SELECT EXISTS(SELECT 1 FROM lists WHERE id=?1)",
        [list],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::NotFound);
    }
    if !c.query_row(
        "SELECT EXISTS(SELECT 1 FROM memberships WHERE list_id=?1 AND user_id=?2)",
        params![list, actor],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::Forbidden);
    }
    Ok(())
}
fn title(s: &str) -> Result<(), Error> {
    if s.trim().is_empty() || s.chars().count() > 500 {
        Err(Error::Invalid("title must contain 1–500 characters".into()))
    } else {
        Ok(())
    }
}
fn descendants(c: &Connection, id: &str) -> Result<Vec<Task>, Error> {
    let mut q=c.prepare(&format!("WITH RECURSIVE tree(id) AS (SELECT id FROM tasks WHERE id=?1 UNION ALL SELECT t.id FROM tasks t JOIN tree ON t.parent_id=tree.id) SELECT {COLS} FROM tasks WHERE id IN tree ORDER BY position,id"))?;
    let rows = q.query_map([id], task_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}
fn event(c: &Connection, t: &Task, kind: &str) -> Result<(), Error> {
    let value = serde_json::json!({"kind":kind,"task":t});
    c.execute(
        "INSERT INTO history(list_id,event) VALUES(?1,?2)",
        params![t.list_id, value.to_string()],
    )?;
    Ok(())
}
fn reopen(c: &Connection, id: &str) -> Result<(), Error> {
    let mut parent = task(c, id)?.parent_id;
    while let Some(id) = parent {
        let p = task(c, &id)?;
        parent = p.parent_id;
        if p.complete {
            c.execute(
                "UPDATE tasks SET complete=0,revision=revision+1 WHERE id=?1",
                [&id],
            )?;
            event(c, &task(c, &id)?, "update")?;
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
            "SELECT COUNT(*) FROM memberships WHERE list_id=?1",
            [list],
            |r| r.get(0),
        )?)
    };
    let added = c.query_row("SELECT COUNT(*) FROM memberships d WHERE d.list_id=?1 AND NOT EXISTS(SELECT 1 FROM memberships s WHERE s.list_id=?2 AND s.user_id=d.user_id)", params![destination, source], |r| r.get(0))?;
    Ok((count(source)?, count(destination)?, added))
}
impl Store {
    pub fn open(path: &str) -> Result<Self, Error> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let legacy: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='spaces') OR EXISTS(SELECT 1 FROM pragma_table_info('users') WHERE name='uuid')", [], |r| r.get(0))?;
        if legacy {
            return Err(Error::Invalid(
                "legacy Todoer database detected; use a separate rebuild database".into(),
            ));
        }
        if path != ":memory:" {
            conn.pragma_update(None, "journal_mode", "WAL")?;
        }
        conn.execute_batch(include_str!("../migrations/001.sql"))?;
        Ok(Self { conn })
    }
    pub fn memory() -> Result<Self, Error> {
        Self::open(":memory:")
    }
    pub fn seed_dev(&mut self) -> Result<(), Error> {
        self.conn.execute_batch("INSERT OR IGNORE INTO users VALUES('dev','alice'),('other','bob'); INSERT OR IGNORE INTO lists(id,title) VALUES('inbox','Inbox'),('groceries','Groceries'),('private','Private'); INSERT OR IGNORE INTO memberships VALUES('inbox','dev'),('groceries','dev'),('private','other');")?;
        Ok(())
    }
    pub fn lists(&self, actor: &str) -> Result<Vec<List>, Error> {
        let mut q=self.conn.prepare("SELECT l.id,l.title,l.description,l.revision FROM lists l JOIN memberships m ON m.list_id=l.id WHERE m.user_id=?1 ORDER BY l.id")?;
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
    pub fn tasks(&self, actor: &str, list: &str) -> Result<Vec<Task>, Error> {
        access(&self.conn, actor, list)?;
        let mut q = self.conn.prepare(&format!(
            "SELECT {COLS} FROM tasks WHERE list_id=?1 ORDER BY position,id"
        ))?;
        Ok(q.query_map([list], task_row)?.collect::<Result<_, _>>()?)
    }
    pub fn capture(
        &mut self,
        actor: &str,
        key: &str,
        request: Capture,
    ) -> Result<Vec<Task>, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        access(&tx, actor, &request.list_id)?;
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
                access(&tx, actor, &current.list_id)?;
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
            reopen(&tx, &id)?;
            event(&tx, &t, "create")?;
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
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let t = task(&tx, id)?;
        access(&tx, actor, &t.list_id)?;
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
                    event(&tx, &task(&tx, &child.id)?, "update")?;
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
            reopen(&tx, &t.id)?
        }
        let updated = task(&tx, &t.id)?;
        event(&tx, &updated, "update")?;
        tx.commit()?;
        Ok(updated)
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
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let t = task(&tx, id)?;
        access(&tx, actor, &t.list_id)?;
        access(&tx, actor, destination)?;
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
        for child in descendants(&tx, &t.id)? {
            pos += 1;
            tx.execute("UPDATE tasks SET list_id=?1,parent_id=CASE WHEN id=?2 THEN NULL ELSE parent_id END,position=?3,revision=revision+1 WHERE id=?4",params![destination,t.id,pos,child.id])?;
            let moved = task(&tx, &child.id)?;
            event(&tx, &child, "move_out")?;
            event(&tx, &moved, "move")?;
        }
        let moved = task(&tx, &t.id)?;
        tx.commit()?;
        Ok(moved)
    }
    pub fn changes(&self, actor: &str, after: i64) -> Result<Vec<serde_json::Value>, Error> {
        let mut q=self.conn.prepare("SELECT h.seq,h.event FROM history h JOIN memberships m ON m.list_id=h.list_id JOIN tasks t ON t.id=json_extract(h.event,'$.task.id') JOIN memberships live ON live.list_id=t.list_id AND live.user_id=m.user_id WHERE m.user_id=?1 AND h.seq>?2 ORDER BY h.seq LIMIT 1000")?;
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
}
