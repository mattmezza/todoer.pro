//! Bounded, actor-bound bulk plans over the existing mutation methods.
use crate::core::{Error, Store, Task, access_rank, actor_identity, raw_task};
use rusqlite::{
    Connection, OptionalExtension, Savepoint, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

/// A normal write is an immediate transaction; inside an atomic operation it is
/// a savepoint. Native Drop implementations roll back every uncommitted branch.
pub enum AtomicWrite<'a> {
    Transaction(Transaction<'a>),
    Savepoint(Savepoint<'a>),
}
pub fn begin_atomic(connection: &mut Connection) -> rusqlite::Result<AtomicWrite<'_>> {
    if connection.is_autocommit() {
        Ok(AtomicWrite::Transaction(
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?,
        ))
    } else {
        Ok(AtomicWrite::Savepoint(connection.savepoint()?))
    }
}
impl Deref for AtomicWrite<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        match self {
            Self::Transaction(tx) => tx,
            Self::Savepoint(tx) => tx,
        }
    }
}
impl AtomicWrite<'_> {
    pub fn commit(self) -> rusqlite::Result<()> {
        match self {
            Self::Transaction(tx) => tx.commit(),
            Self::Savepoint(tx) => tx.commit(),
        }
    }
}
pub(crate) struct StoreWrite<'a> {
    store: &'a mut Store,
    finished: bool,
    savepoint: Option<String>,
}
impl<'a> StoreWrite<'a> {
    pub(crate) fn new(store: &'a mut Store) -> Result<Self, Error> {
        if !store.conn.is_autocommit() {
            return Err(Error::Invalid(
                "bulk operations require an outer connection".into(),
            ));
        }
        store.conn.execute_batch("BEGIN IMMEDIATE")?;
        Ok(Self {
            store,
            finished: false,
            savepoint: None,
        })
    }
    pub(crate) fn nested(store: &'a mut Store) -> Result<Self, Error> {
        if store.conn.is_autocommit() {
            return Self::new(store);
        }
        let name = format!("atomic_{}", Uuid::new_v4().simple());
        store.conn.execute_batch(&format!("SAVEPOINT {name}"))?;
        Ok(Self {
            store,
            finished: false,
            savepoint: Some(name),
        })
    }
    pub(crate) fn commit(mut self) -> Result<(), Error> {
        if let Some(name) = &self.savepoint {
            self.store.conn.execute_batch(&format!("RELEASE {name}"))?;
        } else {
            self.store.conn.execute_batch("COMMIT")?;
        }
        self.finished = true;
        Ok(())
    }
}
impl Deref for StoreWrite<'_> {
    type Target = Store;
    fn deref(&self) -> &Store {
        self.store
    }
}
impl DerefMut for StoreWrite<'_> {
    fn deref_mut(&mut self) -> &mut Store {
        self.store
    }
}
impl Drop for StoreWrite<'_> {
    fn drop(&mut self) {
        if !self.finished {
            if let Some(name) = &self.savepoint {
                let _ = self
                    .store
                    .conn
                    .execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name}"));
            } else {
                let _ = self.store.conn.execute_batch("ROLLBACK");
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum BulkTag {
    Name(String),
    Id { id: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum BulkOperation {
    Update {
        id: String,
        revision: i64,
        title: Option<String>,
        description: Option<String>,
    },
    Move {
        id: String,
        revision: i64,
        destination: String,
        #[serde(default)]
        acknowledge_access_change: bool,
    },
    Trash {
        id: String,
        revision: i64,
    },
    Restore {
        id: String,
        revision: i64,
    },
    Tags {
        id: String,
        revision: i64,
        tags: Vec<BulkTag>,
    },
    Section {
        id: String,
        revision: i64,
        section_id: Option<String>,
    },
    Reparent {
        id: String,
        revision: i64,
        parent_id: Option<String>,
    },
}
impl BulkOperation {
    fn id(&self) -> &str {
        match self {
            Self::Update { id, .. }
            | Self::Move { id, .. }
            | Self::Trash { id, .. }
            | Self::Restore { id, .. }
            | Self::Tags { id, .. }
            | Self::Section { id, .. }
            | Self::Reparent { id, .. } => id,
        }
    }
    fn set_id(&mut self, value: String) {
        match self {
            Self::Update { id, .. }
            | Self::Move { id, .. }
            | Self::Trash { id, .. }
            | Self::Restore { id, .. }
            | Self::Tags { id, .. }
            | Self::Section { id, .. }
            | Self::Reparent { id, .. } => *id = value,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskState {
    pub id: String,
    pub list_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub description_excerpt: String,
    pub description_hash: String,
    pub complete: bool,
    pub revision: i64,
    pub position: i64,
    pub deleted: bool,
    pub tags: Vec<String>,
    pub section_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkStep {
    pub operation: BulkOperation,
    pub before: Vec<TaskState>,
    pub after: Vec<TaskState>,
    pub access_change: Option<serde_json::Value>,
    policy_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkPreview {
    pub preview_id: String,
    pub expected_hash: String,
    pub expires_at: i64,
    pub steps: Vec<BulkStep>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkResult {
    pub preview_id: String,
    pub expected_hash: String,
    pub steps: Vec<BulkStep>,
}
fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn encode<T: Serialize>(value: &T) -> Result<String, Error> {
    serde_json::to_string(value).map_err(|_| Error::Invalid("cannot encode bulk plan".into()))
}
fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Error> {
    serde_json::from_str(value).map_err(|_| Error::Invalid("invalid stored bulk plan".into()))
}
fn state(c: &Connection, actor: &str, id: &str) -> Result<TaskState, Error> {
    let task = raw_task(c, id)?;
    access_rank(c, actor, &task.list_id, 1)?;
    let deleted = c.query_row(
        "SELECT deleted_at IS NOT NULL FROM tasks WHERE id=?1",
        [&task.id],
        |r| r.get(0),
    )?;
    let mut stmt=c.prepare("SELECT g.name FROM tags g JOIN task_tags tt ON tt.tag_id=g.id WHERE tt.task_id=?1 ORDER BY g.normalized_name,g.id")?;
    let tags = stmt
        .query_map([&task.id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let section_id = crate::taxonomy::location(c, &task.id)?;
    Ok(TaskState {
        id: task.id,
        list_id: task.list_id,
        parent_id: task.parent_id,
        title: task.title,
        description_excerpt: task.description.chars().take(120).collect(),
        description_hash: hash(&task.description),
        complete: task.complete,
        revision: task.revision,
        position: task.position,
        deleted,
        tags,
        section_id,
    })
}
fn subtree(c: &Connection, actor: &str, id: &str) -> Result<Vec<TaskState>, Error> {
    let task = raw_task(c, id)?;
    access_rank(c, actor, &task.list_id, 1)?;
    let mut stmt=c.prepare("WITH RECURSIVE tree(id) AS (SELECT id FROM tasks WHERE id=?1 UNION ALL SELECT t.id FROM tasks t JOIN tree ON t.parent_id=tree.id) SELECT id FROM tree LIMIT 501")?;
    let ids = stmt
        .query_map([task.id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if ids.len() > 500 {
        return Err(Error::Invalid("bulk subtree exceeds 500 tasks".into()));
    }
    let mut states = ids
        .into_iter()
        .map(|id| state(c, actor, &id))
        .collect::<Result<Vec<_>, _>>()?;
    states.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(states)
}
fn affected_states(
    store: &Store,
    actor: &str,
    operation: &BulkOperation,
) -> Result<Vec<TaskState>, Error> {
    let mut states = subtree(&store.conn, actor, operation.id())?;
    if let BulkOperation::Reparent {
        parent_id: Some(parent),
        ..
    } = operation
    {
        let mut next = Some(parent.clone());
        let mut seen = std::collections::HashSet::new();
        while let Some(id) = next {
            let parent = raw_task(&store.conn, &id)?;
            if !seen.insert(parent.id.clone()) {
                return Err(Error::Conflict("parent cycle".into()));
            }
            next = parent.parent_id.clone();
            if !states.iter().any(|state| state.id == parent.id) {
                states.push(state(&store.conn, actor, &parent.id)?);
            }
            if states.len() > 500 {
                return Err(Error::Invalid(
                    "bulk operation exceeds 500 affected tasks".into(),
                ));
            }
        }
    }
    states.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(states)
}
fn policy_hash(
    c: &Connection,
    actor: &str,
    operation: &BulkOperation,
    before: &[TaskState],
) -> Result<String, Error> {
    let mut lists: Vec<String> = before.iter().map(|t| t.list_id.clone()).collect();
    if let BulkOperation::Move { destination, .. } = operation {
        access_rank(c, actor, destination, 2)?;
        lists.push(destination.clone());
    }
    lists.sort();
    lists.dedup();
    let mut policy = Vec::new();
    for list in lists {
        access_rank(c, actor, &list, 2)?;
        let revision: i64 =
            c.query_row("SELECT revision FROM lists WHERE id=?1", [&list], |r| {
                r.get(0)
            })?;
        let mut stmt = c.prepare(
            "SELECT user_id,rank FROM effective_list_access WHERE list_id=?1 ORDER BY user_id",
        )?;
        let members = stmt
            .query_map([&list], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        policy.push((list, revision, members));
    }
    let mut targets: Vec<(String, String, i64)> = Vec::new();
    if let BulkOperation::Tags { tags, .. } = operation {
        for tag in tags {
            if let BulkTag::Id { id } = tag {
                let row = c
                    .query_row("SELECT id,name,revision FROM tags WHERE id=?1", [id], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })
                    .optional()?
                    .ok_or_else(|| Error::Conflict("preview tag is no longer available".into()))?;
                targets.push(row);
            }
        }
    }
    if let BulkOperation::Section {
        section_id: Some(id),
        ..
    } = operation
    {
        targets.push(
            c.query_row(
                "SELECT id,title,revision FROM sections WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| Error::Conflict("preview section is no longer available".into()))?,
        );
    }
    Ok(hash(&encode(&(policy, targets))?))
}
fn execute(store: &mut Store, actor: &str, operation: &BulkOperation) -> Result<Task, Error> {
    match operation {
        BulkOperation::Update {
            id,
            revision,
            title,
            description,
        } => {
            if title.is_none() && description.is_none() {
                return Err(Error::Invalid(
                    "bulk update requires title or description".into(),
                ));
            }
            store.update(
                actor,
                id,
                crate::core::Change {
                    revision: *revision,
                    title: title.clone(),
                    description: description.clone(),
                    complete: None,
                    complete_subtree: false,
                },
            )
        }
        BulkOperation::Move {
            id,
            revision,
            destination,
            acknowledge_access_change,
        } => store.move_subtree(
            actor,
            id,
            destination,
            *revision,
            *acknowledge_access_change,
        ),
        BulkOperation::Trash { id, revision } => store.trash(actor, id, *revision),
        BulkOperation::Restore { id, revision } => store.restore(actor, id, *revision),
        BulkOperation::Tags { id, revision, tags } => store.set_task_tags(
            actor,
            id,
            *revision,
            tags.iter()
                .map(|tag| match tag {
                    BulkTag::Name(name) => crate::taxonomy::TagSelector::Name(name.clone()),
                    BulkTag::Id { id } => crate::taxonomy::TagSelector::Id { id: id.clone() },
                })
                .collect(),
        ),
        BulkOperation::Section {
            id,
            revision,
            section_id,
        } => store.set_section(actor, id, *revision, section_id.clone()),
        BulkOperation::Reparent {
            id,
            revision,
            parent_id,
        } => store.reparent_subtree(actor, id, parent_id.clone(), *revision),
    }
}
fn access_change(
    store: &Store,
    actor: &str,
    operation: &BulkOperation,
) -> Result<Option<serde_json::Value>, Error> {
    if let BulkOperation::Move {
        id, destination, ..
    } = operation
    {
        Ok(Some(store.move_preview(actor, id, destination)?))
    } else {
        Ok(None)
    }
}
fn make_step(
    store: &mut Store,
    actor: &str,
    mut operation: BulkOperation,
) -> Result<BulkStep, Error> {
    operation.set_id(raw_task(&store.conn, operation.id())?.id);
    if let BulkOperation::Tags { id, tags, .. } = &mut operation {
        let target = raw_task(&store.conn, id)?;
        access_rank(&store.conn, actor, &target.list_id, 2)?;
        for selector in tags {
            if let BulkTag::Name(name) = selector {
                let existing:Option<String>=store.conn.query_row("SELECT g.id FROM tags g JOIN lists l ON g.scope=COALESCE('workspace:'||l.workspace_id,'list:'||l.id) WHERE l.id=?1 AND g.normalized_name=?2",params![target.list_id,name.trim().to_lowercase()],|r|r.get(0)).optional()?;
                *selector=BulkTag::Id{id:existing.ok_or_else(||Error::Invalid("bulk tags must already exist in the destination home; create new tags with the individual task-tag endpoint first".into()))?};
            }
        }
    }
    let before = affected_states(store, actor, &operation)?;
    let policy_hash = policy_hash(&store.conn, actor, &operation, &before)?;
    let access_change = access_change(store, actor, &operation)?;
    execute(store, actor, &operation)?;
    let after = affected_states(store, actor, &operation)?;
    Ok(BulkStep {
        operation,
        before,
        after,
        access_change,
        policy_hash,
    })
}
impl Store {
    pub fn bulk_preview(
        &mut self,
        actor: &str,
        operations: Vec<BulkOperation>,
    ) -> Result<BulkPreview, Error> {
        if operations.is_empty() || operations.len() > 50 {
            return Err(Error::Invalid(
                "bulk preview requires 1–50 operations".into(),
            ));
        }
        let steps = {
            let mut transaction = StoreWrite::new(self)?;
            let mut steps = Vec::new();
            let mut affected = 0;
            for operation in operations {
                let step = make_step(&mut transaction, actor, operation)?;
                affected += step.before.len();
                if affected > 500 {
                    return Err(Error::Invalid(
                        "bulk preview exceeds 500 task states".into(),
                    ));
                }
                steps.push(step);
            }
            steps // The guard drops here and rolls back every business mutation.
        };
        let id = Uuid::new_v4().to_string();
        let expires_at = now() + 600;
        let plan_hash = hash(&encode(&(actor, &id, expires_at, &steps))?);
        let plan = BulkPreview {
            preview_id: id.clone(),
            expected_hash: plan_hash.clone(),
            expires_at,
            steps,
        };
        let encoded = encode(&plan)?;
        if encoded.len() > 1_048_576 {
            return Err(Error::Invalid("bulk preview exceeds 1 MiB".into()));
        }
        self.conn.execute(
            "DELETE FROM agent_bulk_previews WHERE expires_at<=?1",
            [now()],
        )?;
        let active: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM agent_bulk_previews WHERE actor=?1 AND expires_at>?2",
            params![actor, now()],
            |r| r.get(0),
        )?;
        if active >= 100 {
            return Err(Error::Invalid("too many active bulk previews".into()));
        }
        self.conn.execute(
            "INSERT INTO agent_bulk_previews VALUES(?1,?2,?3,?4,?5)",
            params![id, actor, plan_hash, encoded, expires_at],
        )?;
        Ok(plan)
    }
    pub fn bulk_apply(
        &mut self,
        actor: &str,
        preview_id: &str,
        expected_hash: &str,
        idempotency_key: &str,
    ) -> Result<BulkResult, Error> {
        if idempotency_key.is_empty() || idempotency_key.len() > 200 {
            return Err(Error::Invalid(
                "idempotency key required (1–200 bytes)".into(),
            ));
        }
        let mut tx = StoreWrite::new(self)?;
        let retry:Option<(String,String,String)>=tx.conn.query_row("SELECT preview_id,plan_hash,response FROM agent_bulk_applies WHERE actor=?1 AND idempotency_key=?2",params![actor,idempotency_key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((prior, prior_hash, response)) = retry {
            if prior != preview_id || prior_hash != expected_hash {
                return Err(Error::Conflict("idempotency key payload mismatch".into()));
            }
            let result: BulkResult = decode(&response)?;
            for step in &result.steps {
                for state in step.before.iter().chain(&step.after) {
                    access_rank(&tx.conn, actor, &state.list_id, 1)?;
                    let live = raw_task(&tx.conn, &state.id)?;
                    access_rank(&tx.conn, actor, &live.list_id, 1)?;
                }
            }
            tx.commit()?;
            return Ok(result);
        }
        let encoded:Option<String>=tx.conn.query_row("SELECT plan FROM agent_bulk_previews WHERE id=?1 AND actor=?2 AND plan_hash=?3 AND expires_at>?4",params![preview_id,actor,expected_hash,now()],|r|r.get(0)).optional()?;
        let encoded = encoded.ok_or_else(|| {
            Error::Conflict("bulk preview missing, expired, or hash mismatched".into())
        })?;
        let plan: BulkPreview = decode(&encoded)?;
        let mut steps = Vec::new();
        for expected in plan.steps {
            let before = affected_states(&tx, actor, &expected.operation)?;
            if before != expected.before
                || policy_hash(&tx.conn, actor, &expected.operation, &before)?
                    != expected.policy_hash
                || access_change(&tx, actor, &expected.operation)? != expected.access_change
            {
                return Err(Error::Conflict(
                    "bulk preview is stale; create a new preview".into(),
                ));
            }
            steps.push(make_step(&mut tx, actor, expected.operation)?);
        }
        let result = BulkResult {
            preview_id: preview_id.into(),
            expected_hash: expected_hash.into(),
            steps,
        };
        let response = encode(&result)?;
        tx.conn.execute(
            "INSERT INTO agent_bulk_applies VALUES(?1,?2,?3,?4,?5)",
            params![actor, idempotency_key, preview_id, expected_hash, response],
        )?;
        tx.conn
            .execute("DELETE FROM agent_bulk_previews WHERE id=?1", [preview_id])?;
        tx.commit()?;
        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Destination {
    pub id: String,
    pub title: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestinationPreference {
    pub key: String,
    pub destination: Destination,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestinationResolution {
    pub status: String,
    pub destination: Option<Destination>,
    pub choices: Vec<Destination>,
    pub has_more: bool,
}
fn preference_key(key: &str) -> Result<(), Error> {
    if key.is_empty()
        || key.len() > 40
        || !key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
    {
        return Err(Error::Invalid("preference key must contain 1–40 lowercase ASCII letters, digits, underscores or hyphens".into()));
    }
    Ok(())
}
fn destination(c: &Connection, actor: &str, id: &str) -> Result<Destination, Error> {
    if id.starts_with("intake:")
        && !c.query_row(
            "SELECT EXISTS(SELECT 1 FROM lists WHERE id=?1)",
            [id],
            |r| r.get::<_, bool>(0),
        )?
    {
        let user = crate::core::actor_user(c, actor, 1)?;
        if id == format!("intake:{user}") {
            return Ok(Destination {
                id: id.into(),
                title: "Inbox".into(),
            });
        }
        return Err(Error::Forbidden);
    }
    let reference_id: Option<String> = c
        .query_row(
            "SELECT list_id FROM list_references WHERE reference=?1",
            [id.strip_prefix('#').unwrap_or(id)],
            |r| r.get(0),
        )
        .optional()?;
    let resolved = crate::defaults::resolve(c, "list", reference_id.as_deref().unwrap_or(id))?;
    let id = resolved.as_str();
    access_rank(c, actor, id, 1)?;
    c.query_row(
        "SELECT id,title FROM lists WHERE id=?1 AND archived=0",
        [id],
        |r| {
            Ok(Destination {
                id: r.get(0)?,
                title: r.get(1)?,
            })
        },
    )
    .optional()?
    .ok_or(Error::NotFound)
}
fn resolved(choices: Vec<Destination>, has_more: bool) -> DestinationResolution {
    DestinationResolution {
        status: if choices.is_empty() {
            "missing"
        } else if choices.len() == 1 && !has_more {
            "resolved"
        } else {
            "ambiguous"
        }
        .into(),
        destination: if choices.len() == 1 && !has_more {
            choices.first().cloned()
        } else {
            None
        },
        choices,
        has_more,
    }
}
impl Store {
    pub fn destination_preferences(
        &self,
        actor: &str,
    ) -> Result<Vec<DestinationPreference>, Error> {
        let user = actor_identity(&self.conn, actor)?.0;
        let mut stmt=self.conn.prepare("SELECT preference_key,list_id FROM agent_destination_preferences WHERE user_id=?1 ORDER BY preference_key LIMIT 100")?;
        let mut preferences = Vec::new();
        for row in stmt.query_map([user], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (key, id) = row?;
            match destination(&self.conn, actor, &id) {
                Ok(destination) => preferences.push(DestinationPreference { key, destination }),
                Err(Error::Forbidden | Error::NotFound) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(preferences)
    }
    pub fn set_destination_preference(
        &mut self,
        actor: &str,
        key: &str,
        id: &str,
    ) -> Result<DestinationPreference, Error> {
        preference_key(key)?;
        let user = actor_identity(&self.conn, actor)?.0;
        access_rank(&self.conn, actor, id, 2)?;
        let destination = destination(&self.conn, actor, id)?;
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM agent_destination_preferences WHERE user_id=?1",
            [&user],
            |r| r.get(0),
        )?;
        let exists:bool=self.conn.query_row("SELECT EXISTS(SELECT 1 FROM agent_destination_preferences WHERE user_id=?1 AND preference_key=?2)",params![user,key],|r|r.get(0))?;
        if count >= 100 && !exists {
            return Err(Error::Invalid("at most 100 destination preferences".into()));
        }
        self.conn.execute("INSERT INTO agent_destination_preferences VALUES(?1,?2,?3) ON CONFLICT(user_id,preference_key) DO UPDATE SET list_id=excluded.list_id",params![user,key,id])?;
        Ok(DestinationPreference {
            key: key.into(),
            destination,
        })
    }
    pub fn resolve_destination(
        &self,
        actor: &str,
        explicit: Option<&str>,
        default_key: Option<&str>,
        fallback_inbox: bool,
    ) -> Result<DestinationResolution, Error> {
        if let Some(explicit) = explicit {
            if explicit.is_empty() || explicit.chars().count() > 500 {
                return Err(Error::Invalid(
                    "destination must contain 1–500 characters".into(),
                ));
            }
            match destination(&self.conn, actor, explicit) {
                Ok(found) => return Ok(resolved(vec![found], false)),
                Err(Error::NotFound | Error::Forbidden) => {}
                Err(error) => return Err(error),
            }
            let mut stmt=self.conn.prepare("SELECT l.id,l.title FROM lists l JOIN actor_list_access a ON a.list_id=l.id WHERE a.actor_id=?1 AND l.title=?2 AND l.archived=0 ORDER BY l.id LIMIT 31")?;
            let mut choices = stmt
                .query_map(params![actor, explicit], |r| {
                    Ok(Destination {
                        id: r.get(0)?,
                        title: r.get(1)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let more = choices.len() > 30;
            choices.truncate(30);
            return Ok(resolved(choices, more));
        }
        let user = actor_identity(&self.conn, actor)?.0;
        if let Some(key) = default_key {
            preference_key(key)?;
            let id:Option<String>=self.conn.query_row("SELECT list_id FROM agent_destination_preferences WHERE user_id=?1 AND preference_key=?2",params![user,key],|r|r.get(0)).optional()?;
            if let Some(id) = id {
                match destination(&self.conn, actor, &id) {
                    Ok(found) => return Ok(resolved(vec![found], false)),
                    Err(Error::NotFound | Error::Forbidden) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        if fallback_inbox {
            return Ok(resolved(
                vec![destination(
                    &self.conn,
                    actor,
                    &self.personal_inbox(actor)?,
                )?],
                false,
            ));
        }
        Ok(resolved(Vec::new(), false))
    }
    pub fn search_destinations(
        &self,
        actor: &str,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<Destination>, Error> {
        if !(1..=30).contains(&limit) || prefix.chars().count() > 500 {
            return Err(Error::Invalid(
                "destination search limit 1–30 and prefix at most 500 characters".into(),
            ));
        }
        let escaped = prefix
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let mut stmt=self.conn.prepare("SELECT l.id,l.title FROM lists l JOIN actor_list_access a ON a.list_id=l.id WHERE a.actor_id=?1 AND l.archived=0 AND NOT EXISTS(SELECT 1 FROM workspace_lifecycle x WHERE x.workspace_id=l.workspace_id AND (x.archived=1 OR x.deleted_at IS NOT NULL)) AND (l.title LIKE ?2 ESCAPE '\\' OR l.id=?3) ORDER BY l.title,l.id LIMIT ?4")?;
        Ok(stmt
            .query_map(
                params![actor, format!("{escaped}%"), prefix, limit as i64],
                |r| {
                    Ok(Destination {
                        id: r.get(0)?,
                        title: r.get(1)?,
                    })
                },
            )?
            .collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Capture, NewTask};
    fn db() -> Store {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db
    }
    fn task(db: &mut Store, list: &str, title: &str) -> Task {
        db.capture(
            "dev",
            &Uuid::new_v4().to_string(),
            Capture {
                list_id: list.into(),
                tasks: vec![NewTask {
                    title: title.into(),
                    description: String::new(),
                    parent_id: None,
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    fn edit(t: &Task, title: &str) -> BulkOperation {
        BulkOperation::Update {
            id: t.id.clone(),
            revision: t.revision,
            title: Some(title.into()),
            description: None,
        }
    }
    #[test]
    fn atomic_write_supports_nested_commit_and_rolls_back_uncommitted_savepoints() {
        let mut c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE values_test(value INTEGER); BEGIN IMMEDIATE")
            .unwrap();
        {
            let nested = begin_atomic(&mut c).unwrap();
            nested
                .execute("INSERT INTO values_test VALUES(1)", [])
                .unwrap();
        }
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM values_test", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        {
            let nested = begin_atomic(&mut c).unwrap();
            nested
                .execute("INSERT INTO values_test VALUES(2)", [])
                .unwrap();
            nested.commit().unwrap();
        }
        c.execute_batch("ROLLBACK").unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM values_test", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn preview_rolls_back_real_writes_and_apply_retries_are_payload_bound() {
        let mut db = db();
        let t = task(&mut db, "inbox", "Before");
        let events = db.changes("dev", 0).unwrap().len();
        let plan = db.bulk_preview("dev", vec![edit(&t, "After")]).unwrap();
        assert_eq!(raw_task(&db.conn, &t.id).unwrap().title, "Before");
        assert_eq!(db.changes("dev", 0).unwrap().len(), events);
        assert!(matches!(
            db.bulk_apply("dev", &plan.preview_id, "wrong", "key"),
            Err(Error::Conflict(_))
        ));
        let applied = db
            .bulk_apply("dev", &plan.preview_id, &plan.expected_hash, "key")
            .unwrap();
        assert_eq!(raw_task(&db.conn, &t.id).unwrap().title, "After");
        let retried = db
            .bulk_apply("dev", &plan.preview_id, &plan.expected_hash, "key")
            .unwrap();
        assert_eq!(encode(&applied).unwrap(), encode(&retried).unwrap());
        assert!(matches!(
            db.bulk_apply("dev", &plan.preview_id, "different", "key"),
            Err(Error::Conflict(_))
        ));
        assert_eq!(raw_task(&db.conn, &t.id).unwrap().revision, 2);
    }
    #[test]
    fn stale_later_operation_rolls_back_all_earlier_operations() {
        let mut db = db();
        let first = task(&mut db, "inbox", "First");
        let second = task(&mut db, "inbox", "Second");
        let plan = db
            .bulk_preview(
                "dev",
                vec![
                    edit(&first, "Changed first"),
                    edit(&second, "Changed second"),
                ],
            )
            .unwrap();
        db.update(
            "dev",
            &second.id,
            crate::core::Change {
                revision: 1,
                title: Some("Concurrent work".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(
            db.bulk_apply("dev", &plan.preview_id, &plan.expected_hash, "atomic"),
            Err(Error::Conflict(_))
        ));
        assert_eq!(raw_task(&db.conn, &first.id).unwrap().title, "First");
        assert_eq!(raw_task(&db.conn, &first.id).unwrap().revision, 1);
        assert_eq!(
            raw_task(&db.conn, &second.id).unwrap().title,
            "Concurrent work"
        );
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM agent_bulk_applies", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn preview_failure_permission_loss_expiry_and_cross_actor_replay_fail_closed() {
        let mut db = db();
        let first = task(&mut db, "inbox", "First");
        let second = task(&mut db, "groceries", "Second");
        let mut stale = edit(&second, "Never");
        if let BulkOperation::Update { revision, .. } = &mut stale {
            *revision = 99;
        }
        assert!(
            db.bulk_preview("dev", vec![edit(&first, "Never"), stale])
                .is_err()
        );
        assert_eq!(raw_task(&db.conn, &first.id).unwrap().title, "First");
        let plan = db
            .bulk_preview(
                "dev",
                vec![edit(&first, "New first"), edit(&second, "New second")],
            )
            .unwrap();
        assert!(
            db.bulk_apply("other", &plan.preview_id, &plan.expected_hash, "foreign")
                .is_err()
        );
        db.conn
            .execute(
                "DELETE FROM memberships WHERE list_id='groceries' AND user_id='dev'",
                [],
            )
            .unwrap();
        assert!(
            db.bulk_apply("dev", &plan.preview_id, &plan.expected_hash, "lost-access")
                .is_err()
        );
        assert_eq!(raw_task(&db.conn, &first.id).unwrap().title, "First");
        let plan = db
            .bulk_preview("dev", vec![edit(&first, "Expired")])
            .unwrap();
        db.conn
            .execute(
                "UPDATE agent_bulk_previews SET expires_at=0 WHERE id=?1",
                [&plan.preview_id],
            )
            .unwrap();
        assert!(
            db.bulk_apply("dev", &plan.preview_id, &plan.expected_hash, "expired")
                .is_err()
        );
    }
    #[test]
    fn descendant_edits_and_membership_changes_invalidate_reviewed_moves() {
        let mut db = db();
        let root = task(&mut db, "inbox", "Root");
        let child = db
            .capture(
                "dev",
                "child",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![NewTask {
                        title: "Child".into(),
                        description: String::new(),
                        parent_id: Some(root.id.clone()),
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let op = BulkOperation::Move {
            id: root.id.clone(),
            revision: 1,
            destination: "groceries".into(),
            acknowledge_access_change: false,
        };
        let plan = db.bulk_preview("dev", vec![op]).unwrap();
        db.update(
            "dev",
            &child.id,
            crate::core::Change {
                revision: 1,
                title: Some("New child".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            db.bulk_apply(
                "dev",
                &plan.preview_id,
                &plan.expected_hash,
                "child-changed"
            )
            .is_err()
        );
        assert_eq!(raw_task(&db.conn, &root.id).unwrap().list_id, "inbox");
    }
    #[test]
    fn bulk_tags_require_existing_entities_and_bind_names_to_ids() {
        let mut db = db();
        let t = task(&mut db, "inbox", "Tagged");
        let operation = BulkOperation::Tags {
            id: t.id.clone(),
            revision: 1,
            tags: vec![BulkTag::Name("new-tag".into())],
        };
        assert!(matches!(
            db.bulk_preview("dev", vec![operation]),
            Err(Error::Invalid(_))
        ));
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM tags", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        let t = db
            .set_task_tags(
                "dev",
                &t.id,
                1,
                vec![crate::taxonomy::TagSelector::Name("existing".into())],
            )
            .unwrap();
        let plan = db
            .bulk_preview(
                "dev",
                vec![BulkOperation::Tags {
                    id: t.id.clone(),
                    revision: t.revision,
                    tags: vec![BulkTag::Name("existing".into())],
                }],
            )
            .unwrap();
        if let BulkOperation::Tags { tags, .. } = &plan.steps[0].operation {
            assert!(matches!(&tags[0], BulkTag::Id { .. }));
        } else {
            panic!("expected tags")
        }
        db.bulk_apply("dev", &plan.preview_id, &plan.expected_hash, "existing-tag")
            .unwrap();
        assert_eq!(db.task_tags("dev", &t.id).unwrap()[0].name, "existing");
    }
    #[test]
    fn destination_resolution_is_explicit_bounded_and_live_authorized() {
        let mut db = db();
        assert_eq!(
            db.resolve_destination("dev", None, None, false)
                .unwrap()
                .status,
            "missing"
        );
        db.set_destination_preference("dev", "inbox", "inbox")
            .unwrap();
        assert_eq!(
            db.resolve_destination("dev", None, None, true)
                .unwrap()
                .destination
                .unwrap()
                .id,
            db.personal_inbox("dev").unwrap()
        );
        assert_eq!(
            db.resolve_destination("dev", None, Some("inbox"), false)
                .unwrap()
                .destination
                .unwrap()
                .id,
            "inbox"
        );
        assert_eq!(
            db.resolve_destination("dev", Some("private"), None, false)
                .unwrap()
                .status,
            "missing"
        );
        db.conn
            .execute(
                "UPDATE lists SET title='Same' WHERE id IN ('inbox','groceries','private')",
                [],
            )
            .unwrap();
        let ambiguous = db
            .resolve_destination("dev", Some("Same"), None, false)
            .unwrap();
        assert_eq!(ambiguous.status, "ambiguous");
        assert_eq!(ambiguous.choices.len(), 2);
        assert_eq!(db.search_destinations("dev", "S", 1).unwrap().len(), 1);
        assert!(
            db.set_destination_preference("dev", "private", "private")
                .is_err()
        );
        db.conn
            .execute(
                "DELETE FROM memberships WHERE list_id='inbox' AND user_id='dev'",
                [],
            )
            .unwrap();
        assert!(db.destination_preferences("dev").unwrap().is_empty());
        assert_eq!(
            db.resolve_destination("dev", None, Some("inbox"), false)
                .unwrap()
                .status,
            "missing"
        );
    }
}
