//! Workspace policies and list/section administration. Direct list grants do not
//! imply workspace membership: external known users see only their shared list.
use crate::core::{Error, List, Store};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceRole {
    Owner,
    Editor,
    Viewer,
}
impl WorkspaceRole {
    fn value(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListRole {
    Editor,
    Viewer,
}
impl ListRole {
    fn value(self) -> &'static str {
        match self {
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub title: String,
    pub description: String,
    pub revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMember {
    pub user_id: String,
    pub role: WorkspaceRole,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceList {
    #[serde(flatten)]
    pub list: List,
    // Only workspace members receive its identifier. A direct guest cannot use
    // this endpoint to learn the parent workspace or enumerate its members.
    pub workspace_id: Option<String>,
    pub restricted: bool,
    pub position: i64,
    pub archived: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub list_id: String,
    pub title: String,
    pub description: String,
    pub position: i64,
    pub revision: i64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceInput {
    pub title: String,
    #[serde(default)]
    pub description: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceChange {
    pub revision: i64,
    pub title: Option<String>,
    pub description: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateList {
    pub workspace_id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub restricted: bool,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListChange {
    pub revision: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub archived: Option<bool>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionInput {
    pub title: String,
    #[serde(default)]
    pub description: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionChange {
    pub revision: i64,
    pub title: Option<String>,
    pub description: Option<String>,
}

fn validate(title: &str, description: &str) -> Result<(), Error> {
    if title.trim().is_empty() || title.chars().count() > 500 {
        return Err(Error::Invalid("title must contain 1–500 characters".into()));
    }
    if description.len() > 100_000 {
        return Err(Error::Invalid(
            "description must not exceed 100000 bytes".into(),
        ));
    }
    Ok(())
}
fn user_exists(c: &Connection, user: &str) -> Result<(), Error> {
    if c.query_row(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id=?1)",
        [user],
        |r| r.get::<_, bool>(0),
    )? {
        Ok(())
    } else {
        Err(Error::NotFound)
    }
}
fn workspace_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: r.get(0)?,
        title: r.get(1)?,
        description: r.get(2)?,
        revision: r.get(3)?,
    })
}
fn workspace_raw(c: &Connection, id: &str) -> Result<Workspace, Error> {
    c.query_row(
        "SELECT id,title,description,revision FROM workspaces WHERE id=?1",
        [id],
        workspace_row,
    )
    .optional()?
    .ok_or(Error::NotFound)
}
fn workspace_rank(c: &Connection, actor: &str, id: &str) -> Result<i64, Error> {
    workspace_raw(c, id)?;
    Ok(c.query_row(
        "SELECT rank FROM actor_workspace_access WHERE workspace_id=?1 AND actor_id=?2",
        params![id, actor],
        |r| r.get(0),
    )
    .optional()?
    .unwrap_or(0))
}
fn require_workspace(c: &Connection, actor: &str, id: &str, minimum: i64) -> Result<(), Error> {
    if workspace_rank(c, actor, id)? >= minimum {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}
fn list_raw(c: &Connection, id: &str) -> Result<List, Error> {
    c.query_row(
        "SELECT id,title,description,revision FROM lists WHERE id=?1",
        [id],
        |r| {
            Ok(List {
                id: r.get(0)?,
                title: r.get(1)?,
                description: r.get(2)?,
                revision: r.get(3)?,
            })
        },
    )
    .optional()?
    .ok_or(Error::NotFound)
}
pub(crate) fn require_list(
    c: &Connection,
    actor: &str,
    id: &str,
    minimum: i64,
) -> Result<(), Error> {
    list_raw(c, id)?;
    let rank: i64 = c
        .query_row(
            "SELECT rank FROM actor_list_access WHERE list_id=?1 AND actor_id=?2",
            params![id, actor],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    if rank >= minimum {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}
fn list_owner(c: &Connection, actor: &str, id: &str) -> Result<(), Error> {
    list_raw(c, id)?;
    let workspace: Option<String> =
        c.query_row("SELECT workspace_id FROM lists WHERE id=?1", [id], |r| {
            r.get(0)
        })?;
    require_workspace(c, actor, workspace.as_deref().ok_or(Error::Forbidden)?, 3)
}
fn check_revision(actual: i64, revision: i64) -> Result<(), Error> {
    if actual == revision {
        Ok(())
    } else {
        Err(Error::Conflict("stale revision".into()))
    }
}
fn bump_workspace(c: &Connection, id: &str) -> Result<Workspace, Error> {
    c.execute(
        "UPDATE workspaces SET revision=revision+1 WHERE id=?1",
        [id],
    )?;
    workspace_raw(c, id)
}
fn bump_list(c: &Connection, id: &str) -> Result<List, Error> {
    c.execute("UPDATE lists SET revision=revision+1 WHERE id=?1", [id])?;
    list_raw(c, id)
}
fn section_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Section> {
    Ok(Section {
        id: r.get(0)?,
        list_id: r.get(1)?,
        title: r.get(2)?,
        description: r.get(3)?,
        position: r.get(4)?,
        revision: r.get(5)?,
    })
}
fn section_raw(c: &Connection, id: &str) -> Result<Section, Error> {
    c.query_row(
        "SELECT id,list_id,title,description,position,revision FROM sections WHERE id=?1",
        [id],
        section_row,
    )
    .optional()?
    .ok_or(Error::NotFound)
}
fn exact_order(current: &[String], ordered: &[String]) -> Result<(), Error> {
    if ordered.len() > 10_000 {
        return Err(Error::Invalid(
            "reorder accepts at most 10000 identifiers".into(),
        ));
    }
    let actual: HashSet<_> = current.iter().collect();
    let provided: HashSet<_> = ordered.iter().collect();
    if ordered.len() != provided.len() || actual != provided {
        return Err(Error::Invalid(
            "reorder must contain every current identifier exactly once".into(),
        ));
    }
    Ok(())
}

fn policy_audience(
    c: &Connection,
    workspace: Option<&str>,
    list: Option<&str>,
) -> Result<HashMap<(String, String), i64>, Error> {
    let mut stmt = c.prepare("SELECT e.list_id,e.user_id,e.rank FROM effective_list_access e JOIN lists l ON l.id=e.list_id WHERE (?1 IS NULL OR l.workspace_id=?1) AND (?2 IS NULL OR e.list_id=?2)")?;
    Ok(stmt
        .query_map(params![workspace, list], |r| {
            Ok(((r.get(0)?, r.get(1)?), r.get(2)?))
        })?
        .collect::<Result<_, _>>()?)
}
fn check_audience(
    before: &HashMap<(String, String), i64>,
    after: &HashMap<(String, String), i64>,
    acknowledge: bool,
) -> Result<(), Error> {
    if !acknowledge
        && after
            .iter()
            .any(|(key, rank)| *rank > before.get(key).copied().unwrap_or(0))
    {
        return Err(Error::Conflict(
            "acknowledge access change before expanding list access".into(),
        ));
    }
    Ok(())
}

impl Store {
    pub fn list_permission(&self, actor: &str, list: &str) -> Result<i64, Error> {
        list_raw(&self.conn, list)?;
        Ok(self
            .conn
            .query_row(
                "SELECT rank FROM actor_list_access WHERE list_id=?1 AND actor_id=?2",
                params![list, actor],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }
    pub fn workspace_permission(&self, actor: &str, workspace: &str) -> Result<i64, Error> {
        workspace_rank(&self.conn, actor, workspace)
    }
    pub fn workspaces(&self, actor: &str) -> Result<Vec<Workspace>, Error> {
        let mut stmt=self.conn.prepare("SELECT w.id,w.title,w.description,w.revision FROM workspaces w JOIN actor_workspace_access m ON m.workspace_id=w.id WHERE m.actor_id=?1 ORDER BY w.title,w.id")?;
        Ok(stmt
            .query_map([actor], workspace_row)?
            .collect::<Result<_, _>>()?)
    }
    pub fn workspace(&self, actor: &str, id: &str) -> Result<Workspace, Error> {
        require_workspace(&self.conn, actor, id, 1)?;
        workspace_raw(&self.conn, id)
    }
    pub fn create_workspace(
        &mut self,
        actor: &str,
        input: WorkspaceInput,
    ) -> Result<Workspace, Error> {
        validate(&input.title, &input.description)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let owner = crate::core::actor_user(&tx, actor, 3)?;
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO workspaces(id,title,description) VALUES(?1,?2,?3)",
            params![id, input.title.trim(), input.description],
        )?;
        tx.execute(
            "INSERT INTO workspace_members(workspace_id,user_id,role) VALUES(?1,?2,'owner')",
            params![id, owner],
        )?;
        let result = workspace_raw(&tx, &id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn update_workspace(
        &mut self,
        actor: &str,
        id: &str,
        change: WorkspaceChange,
    ) -> Result<Workspace, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_workspace(&tx, actor, id, 3)?;
        let old = workspace_raw(&tx, id)?;
        check_revision(old.revision, change.revision)?;
        let title = change.title.unwrap_or(old.title);
        let description = change.description.unwrap_or(old.description);
        validate(&title, &description)?;
        tx.execute(
            "UPDATE workspaces SET title=?2,description=?3 WHERE id=?1",
            params![id, title.trim(), description],
        )?;
        let result = bump_workspace(&tx, id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn workspace_members(
        &self,
        actor: &str,
        workspace: &str,
    ) -> Result<Vec<WorkspaceMember>, Error> {
        require_workspace(&self.conn, actor, workspace, 3)?;
        let mut stmt = self.conn.prepare(
            "SELECT user_id,role FROM workspace_members WHERE workspace_id=?1 ORDER BY user_id",
        )?;
        Ok(stmt
            .query_map([workspace], |r| {
                let role: String = r.get(1)?;
                Ok(WorkspaceMember {
                    user_id: r.get(0)?,
                    role: match role.as_str() {
                        "owner" => WorkspaceRole::Owner,
                        "editor" => WorkspaceRole::Editor,
                        _ => WorkspaceRole::Viewer,
                    },
                })
            })?
            .collect::<Result<_, _>>()?)
    }
    pub fn set_workspace_member(
        &mut self,
        actor: &str,
        workspace: &str,
        user: &str,
        role: WorkspaceRole,
        revision: i64,
        acknowledge_access_change: bool,
    ) -> Result<Workspace, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_workspace(&tx, actor, workspace, 3)?;
        check_revision(workspace_raw(&tx, workspace)?.revision, revision)?;
        user_exists(&tx, user)?;
        preserve_owner(&tx, workspace, user, role == WorkspaceRole::Owner)?;
        let before = policy_audience(&tx, Some(workspace), None)?;
        let previous_role = workspace_rank(&tx, user, workspace)?;
        tx.execute("INSERT INTO workspace_members(workspace_id,user_id,role) VALUES(?1,?2,?3) ON CONFLICT(workspace_id,user_id) DO UPDATE SET role=excluded.role",params![workspace,user,role.value()])?;
        let new_role = match role {
            WorkspaceRole::Owner => 3,
            WorkspaceRole::Editor => 2,
            WorkspaceRole::Viewer => 1,
        };
        if new_role > previous_role && !acknowledge_access_change {
            return Err(Error::Conflict(
                "acknowledge access change before granting workspace access".into(),
            ));
        }
        check_audience(
            &before,
            &policy_audience(&tx, Some(workspace), None)?,
            acknowledge_access_change,
        )?;
        let result = bump_workspace(&tx, workspace)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn remove_workspace_member(
        &mut self,
        actor: &str,
        workspace: &str,
        user: &str,
        revision: i64,
    ) -> Result<Workspace, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_workspace(&tx, actor, workspace, 3)?;
        check_revision(workspace_raw(&tx, workspace)?.revision, revision)?;
        preserve_owner(&tx, workspace, user, false)?;
        if tx.execute(
            "DELETE FROM workspace_members WHERE workspace_id=?1 AND user_id=?2",
            params![workspace, user],
        )? == 0
        {
            return Err(Error::NotFound);
        }
        let result = bump_workspace(&tx, workspace)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn create_list(&mut self, actor: &str, input: CreateList) -> Result<List, Error> {
        validate(&input.title, &input.description)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_workspace(
            &tx,
            actor,
            &input.workspace_id,
            if input.restricted { 3 } else { 2 },
        )?;
        let id = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO lists(id,title,description,workspace_id,restricted,position) VALUES(?1,?2,?3,?4,?5,(SELECT COALESCE(MAX(position),-1)+1 FROM lists WHERE workspace_id=?4))",params![id,input.title.trim(),input.description,input.workspace_id,input.restricted])?;
        bump_workspace(&tx, &input.workspace_id)?;
        let result = list_raw(&tx, &id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn list_info(&self, actor: &str, id: &str) -> Result<WorkspaceList, Error> {
        require_list(&self.conn, actor, id, 1)?;
        let (workspace, restricted, position, archived): (Option<String>, bool, i64, bool) =
            self.conn.query_row(
                "SELECT workspace_id,restricted,position,archived FROM lists WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        let visible_workspace = match workspace {
            Some(w) if workspace_rank(&self.conn, actor, &w)? > 0 => Some(w),
            _ => None,
        };
        Ok(WorkspaceList {
            list: list_raw(&self.conn, id)?,
            workspace_id: visible_workspace,
            restricted,
            position,
            archived,
        })
    }
    pub fn update_list(
        &mut self,
        actor: &str,
        id: &str,
        change: ListChange,
    ) -> Result<List, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_list(&tx, actor, id, 2)?;
        let old = list_raw(&tx, id)?;
        check_revision(old.revision, change.revision)?;
        let title = change.title.unwrap_or(old.title);
        let description = change.description.unwrap_or(old.description);
        validate(&title, &description)?;
        tx.execute(
            "UPDATE lists SET title=?2,description=?3,archived=COALESCE(?4,archived) WHERE id=?1",
            params![id, title.trim(), description, change.archived],
        )?;
        let result = bump_list(&tx, id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn set_list_restricted(
        &mut self,
        actor: &str,
        id: &str,
        restricted: bool,
        revision: i64,
        acknowledge_access_change: bool,
    ) -> Result<List, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        list_owner(&tx, actor, id)?;
        check_revision(list_raw(&tx, id)?.revision, revision)?;
        let before = policy_audience(&tx, None, Some(id))?;
        tx.execute(
            "UPDATE lists SET restricted=?2 WHERE id=?1",
            params![id, restricted],
        )?;
        check_audience(
            &before,
            &policy_audience(&tx, None, Some(id))?,
            acknowledge_access_change,
        )?;
        let result = bump_list(&tx, id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn grant_list(
        &mut self,
        actor: &str,
        id: &str,
        user: &str,
        role: ListRole,
        revision: i64,
        acknowledge_access_change: bool,
    ) -> Result<List, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        list_owner(&tx, actor, id)?;
        check_revision(list_raw(&tx, id)?.revision, revision)?;
        user_exists(&tx, user)?;
        let before = policy_audience(&tx, None, Some(id))?;
        tx.execute(
            "INSERT OR IGNORE INTO memberships(list_id,user_id) VALUES(?1,?2)",
            params![id, user],
        )?;
        tx.execute("INSERT INTO membership_roles(list_id,user_id,role) VALUES(?1,?2,?3) ON CONFLICT(list_id,user_id) DO UPDATE SET role=excluded.role",params![id,user,role.value()])?;
        check_audience(
            &before,
            &policy_audience(&tx, None, Some(id))?,
            acknowledge_access_change,
        )?;
        let result = bump_list(&tx, id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn revoke_list(
        &mut self,
        actor: &str,
        id: &str,
        user: &str,
        revision: i64,
    ) -> Result<List, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        list_owner(&tx, actor, id)?;
        check_revision(list_raw(&tx, id)?.revision, revision)?;
        if tx.execute(
            "DELETE FROM memberships WHERE list_id=?1 AND user_id=?2",
            params![id, user],
        )? == 0
        {
            return Err(Error::NotFound);
        }
        let result = bump_list(&tx, id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn sections(&self, actor: &str, list: &str) -> Result<Vec<Section>, Error> {
        require_list(&self.conn, actor, list, 1)?;
        let mut stmt=self.conn.prepare("SELECT id,list_id,title,description,position,revision FROM sections WHERE list_id=?1 ORDER BY position,id")?;
        Ok(stmt
            .query_map([list], section_row)?
            .collect::<Result<_, _>>()?)
    }
    pub fn create_section(
        &mut self,
        actor: &str,
        list: &str,
        input: SectionInput,
    ) -> Result<Section, Error> {
        validate(&input.title, &input.description)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_list(&tx, actor, list, 2)?;
        let id = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO sections(id,list_id,title,description,position) VALUES(?1,?2,?3,?4,(SELECT COALESCE(MAX(position),-1)+1 FROM sections WHERE list_id=?2))",params![id,list,input.title.trim(),input.description])?;
        bump_list(&tx, list)?;
        let result = section_raw(&tx, &id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn update_section(
        &mut self,
        actor: &str,
        id: &str,
        change: SectionChange,
    ) -> Result<Section, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = section_raw(&tx, id)?;
        require_list(&tx, actor, &old.list_id, 2)?;
        check_revision(old.revision, change.revision)?;
        let title = change.title.unwrap_or(old.title);
        let description = change.description.unwrap_or(old.description);
        validate(&title, &description)?;
        tx.execute(
            "UPDATE sections SET title=?2,description=?3,revision=revision+1 WHERE id=?1",
            params![id, title.trim(), description],
        )?;
        let result = section_raw(&tx, id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn reorder_lists(
        &mut self,
        actor: &str,
        workspace: &str,
        revision: i64,
        ids: &[String],
    ) -> Result<Workspace, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_workspace(&tx, actor, workspace, 3)?;
        check_revision(workspace_raw(&tx, workspace)?.revision, revision)?;
        let current = {
            let mut stmt = tx.prepare("SELECT id FROM lists WHERE workspace_id=?1")?;
            stmt.query_map([workspace], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?
        };
        exact_order(&current, ids)?;
        for (position, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE lists SET position=?2,revision=revision+1 WHERE id=?1",
                params![id, position as i64],
            )?;
        }
        let result = bump_workspace(&tx, workspace)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn reorder_sections(
        &mut self,
        actor: &str,
        list: &str,
        revision: i64,
        ids: &[String],
    ) -> Result<List, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_list(&tx, actor, list, 2)?;
        check_revision(list_raw(&tx, list)?.revision, revision)?;
        let current = {
            let mut stmt = tx.prepare("SELECT id FROM sections WHERE list_id=?1")?;
            stmt.query_map([list], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?
        };
        exact_order(&current, ids)?;
        for (position, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE sections SET position=?2,revision=revision+1 WHERE id=?1",
                params![id, position as i64],
            )?;
        }
        let result = bump_list(&tx, list)?;
        tx.commit()?;
        Ok(result)
    }
}
fn preserve_owner(
    c: &Connection,
    workspace: &str,
    user: &str,
    will_be_owner: bool,
) -> Result<(), Error> {
    if will_be_owner {
        return Ok(());
    }
    let old: Option<String> = c
        .query_row(
            "SELECT role FROM workspace_members WHERE workspace_id=?1 AND user_id=?2",
            params![workspace, user],
            |r| r.get(0),
        )
        .optional()?;
    if old.as_deref() == Some("owner") {
        let count: i64 = c.query_row(
            "SELECT COUNT(*) FROM workspace_members WHERE workspace_id=?1 AND role='owner'",
            [workspace],
            |r| r.get(0),
        )?;
        if count <= 1 {
            return Err(Error::Conflict(
                "workspace must retain at least one owner".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Store {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn
            .execute(
                "INSERT INTO users(id,name) VALUES('guest','Synthetic guest')",
                [],
            )
            .unwrap();
        db
    }
    fn workspace(db: &mut Store) -> Workspace {
        db.create_workspace(
            "dev",
            WorkspaceInput {
                title: "Workspace".into(),
                description: "Synthetic description".into(),
            },
        )
        .unwrap()
    }
    fn list(db: &mut Store, workspace: &str, restricted: bool) -> List {
        db.create_list(
            "dev",
            CreateList {
                workspace_id: workspace.into(),
                title: "List".into(),
                description: String::new(),
                restricted,
            },
        )
        .unwrap()
    }
    fn section(db: &mut Store, list: &str, title: &str) -> Section {
        db.create_section(
            "dev",
            list,
            SectionInput {
                title: title.into(),
                description: String::new(),
            },
        )
        .unwrap()
    }
    #[test]
    fn legacy_direct_grants_retain_editor_access_without_workspace() {
        let mut db = db();
        assert_eq!(db.list_permission("dev", "inbox").unwrap(), 2);
        assert_eq!(db.list_info("dev", "inbox").unwrap().workspace_id, None);
        assert_eq!(db.list_permission("other", "inbox").unwrap(), 0);
        db.conn
            .execute(
                "INSERT INTO membership_roles(list_id,user_id,role) VALUES('inbox','dev','viewer')",
                [],
            )
            .unwrap();
        assert_eq!(db.list_permission("dev", "inbox").unwrap(), 1);
        assert!(matches!(
            db.create_section(
                "dev",
                "inbox",
                SectionInput {
                    title: "Denied".into(),
                    description: String::new()
                }
            ),
            Err(Error::Forbidden)
        ));
    }
    #[test]
    fn inherited_restricted_and_external_grants_do_not_leak_workspace() {
        let mut db = db();
        let w = workspace(&mut db);
        let w = db
            .set_workspace_member(
                "dev",
                &w.id,
                "other",
                WorkspaceRole::Viewer,
                w.revision,
                true,
            )
            .unwrap();
        let inherited = list(&mut db, &w.id, false);
        let restricted = list(&mut db, &w.id, true);
        assert_eq!(db.list_permission("dev", &restricted.id).unwrap(), 3);
        assert_eq!(db.list_permission("other", &inherited.id).unwrap(), 1);
        assert_eq!(db.list_permission("other", &restricted.id).unwrap(), 0);
        assert!(matches!(
            db.create_section(
                "other",
                &inherited.id,
                SectionInput {
                    title: "Denied".into(),
                    description: String::new()
                }
            ),
            Err(Error::Forbidden)
        ));
        let restricted = db
            .grant_list(
                "dev",
                &restricted.id,
                "guest",
                ListRole::Editor,
                restricted.revision,
                true,
            )
            .unwrap();
        assert_eq!(db.list_permission("guest", &restricted.id).unwrap(), 2);
        assert!(db.workspaces("guest").unwrap().is_empty());
        assert!(matches!(
            db.workspace("guest", &w.id),
            Err(Error::Forbidden)
        ));
        assert_eq!(
            db.list_info("guest", &restricted.id).unwrap().workspace_id,
            None
        );
        section(&mut db, &inherited.id, "Owner section");
        db.create_section(
            "guest",
            &restricted.id,
            SectionInput {
                title: "Shared section".into(),
                description: String::new(),
            },
        )
        .unwrap();
        let revision = db.list_info("dev", &restricted.id).unwrap().list.revision;
        db.revoke_list("dev", &restricted.id, "guest", revision)
            .unwrap();
        assert_eq!(db.list_permission("guest", &restricted.id).unwrap(), 0);
        assert!(matches!(
            db.sections("guest", &restricted.id),
            Err(Error::Forbidden)
        ));
    }
    #[test]
    fn last_owner_is_preserved_and_sharing_is_owner_only() {
        let mut db = db();
        let w = workspace(&mut db);
        let l = list(&mut db, &w.id, false);
        let revision = db.workspace("dev", &w.id).unwrap().revision;
        assert!(matches!(
            db.remove_workspace_member("dev", &w.id, "dev", revision),
            Err(Error::Conflict(_))
        ));
        assert!(matches!(
            db.set_workspace_member("dev", &w.id, "dev", WorkspaceRole::Viewer, revision, true),
            Err(Error::Conflict(_))
        ));
        let w = db
            .set_workspace_member("dev", &w.id, "other", WorkspaceRole::Editor, revision, true)
            .unwrap();
        assert!(matches!(
            db.grant_list("other", &l.id, "guest", ListRole::Viewer, l.revision, true),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            db.set_list_restricted("other", &l.id, true, l.revision, true),
            Err(Error::Forbidden)
        ));
        let w = db
            .set_workspace_member(
                "dev",
                &w.id,
                "other",
                WorkspaceRole::Owner,
                w.revision,
                true,
            )
            .unwrap();
        db.remove_workspace_member("other", &w.id, "dev", w.revision)
            .unwrap();
        assert_eq!(db.workspace_permission("dev", &w.id).unwrap(), 0);
        assert_eq!(db.workspace_permission("other", &w.id).unwrap(), 3);
    }
    #[test]
    fn stale_writes_and_invalid_reorders_are_atomic() {
        let mut db = db();
        let w = workspace(&mut db);
        let a = list(&mut db, &w.id, false);
        let b = list(&mut db, &w.id, false);
        let first = section(&mut db, &a.id, "First");
        let second = section(&mut db, &a.id, "Second");
        let foreign = section(&mut db, &b.id, "Foreign");
        let revision = db.list_info("dev", &a.id).unwrap().list.revision;
        assert!(matches!(
            db.reorder_sections("dev", &a.id, revision, &[second.id.clone(), foreign.id]),
            Err(Error::Invalid(_))
        ));
        assert_eq!(db.sections("dev", &a.id).unwrap()[0].id, first.id);
        assert_eq!(db.list_info("dev", &a.id).unwrap().list.revision, revision);
        let updated = db
            .reorder_sections(
                "dev",
                &a.id,
                revision,
                &[second.id.clone(), first.id.clone()],
            )
            .unwrap();
        assert_eq!(db.sections("dev", &a.id).unwrap()[0].id, second.id);
        assert!(matches!(
            db.reorder_sections(
                "dev",
                &a.id,
                revision,
                &[first.id.clone(), second.id.clone()]
            ),
            Err(Error::Conflict(_))
        ));
        assert_eq!(
            db.list_info("dev", &a.id).unwrap().list.revision,
            updated.revision
        );
        let old = db.sections("dev", &a.id).unwrap()[0].clone();
        db.update_section(
            "dev",
            &old.id,
            SectionChange {
                revision: old.revision,
                title: Some("Updated".into()),
                description: None,
            },
        )
        .unwrap();
        assert!(matches!(
            db.update_section(
                "dev",
                &old.id,
                SectionChange {
                    revision: old.revision,
                    title: Some("Stale".into()),
                    description: None
                }
            ),
            Err(Error::Conflict(_))
        ));
        let revision = db.workspace("dev", &w.id).unwrap().revision;
        db.reorder_lists("dev", &w.id, revision, &[b.id.clone(), a.id.clone()])
            .unwrap();
        assert!(
            db.list_info("dev", &b.id).unwrap().position
                < db.list_info("dev", &a.id).unwrap().position
        );
    }
    #[test]
    fn role_changes_are_live_and_direct_grants_remain_independent() {
        let mut db = db();
        let w = workspace(&mut db);
        let l = list(&mut db, &w.id, false);
        let revision = db.workspace("dev", &w.id).unwrap().revision;
        let w = db
            .set_workspace_member("dev", &w.id, "other", WorkspaceRole::Editor, revision, true)
            .unwrap();
        let l = db
            .grant_list("dev", &l.id, "other", ListRole::Viewer, l.revision, true)
            .unwrap();
        assert_eq!(db.list_permission("other", &l.id).unwrap(), 2);
        let l = db
            .set_list_restricted("dev", &l.id, true, l.revision, true)
            .unwrap();
        assert_eq!(db.list_permission("other", &l.id).unwrap(), 1);
        db.remove_workspace_member("dev", &w.id, "other", w.revision)
            .unwrap();
        assert_eq!(db.list_permission("other", &l.id).unwrap(), 1);
        assert!(
            db.workspaces("other")
                .unwrap()
                .iter()
                .all(|item| item.id != w.id)
        );
        assert!(matches!(
            db.grant_list("dev", &l.id, "missing", ListRole::Viewer, l.revision, true),
            Err(Error::NotFound)
        ));
        assert_eq!(
            db.list_info("dev", &l.id).unwrap().list.revision,
            l.revision
        );
    }
    #[test]
    fn scoped_credentials_do_not_expand_workspace_or_live_owner_rights() {
        let mut db = db();
        let w = workspace(&mut db);
        let a = list(&mut db, &w.id, false);
        let b = list(&mut db, &w.id, false);
        db.conn.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,list_ids,expires_at) VALUES('scoped','dev','synthetic-scoped-hash','synthetic','[\"admin\"]',?1,unixepoch()+3600)", [serde_json::to_string(&vec![&a.id]).unwrap()]).unwrap();
        let credential = "credential:scoped";
        assert_eq!(db.list_permission(credential, &a.id).unwrap(), 3);
        assert_eq!(db.list_permission(credential, &b.id).unwrap(), 0);
        assert!(db.workspaces(credential).unwrap().is_empty());
        assert_eq!(db.list_info(credential, &a.id).unwrap().workspace_id, None);
        assert!(matches!(
            db.grant_list(
                credential,
                &a.id,
                "guest",
                ListRole::Editor,
                a.revision,
                true
            ),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            db.create_workspace(
                credential,
                WorkspaceInput {
                    title: "Denied".into(),
                    description: String::new()
                }
            ),
            Err(Error::Forbidden)
        ));
        db.conn
            .execute(
                "UPDATE auth_agents SET list_ids=NULL,permissions='[\"read\"]' WHERE id='scoped'",
                [],
            )
            .unwrap();
        assert_eq!(db.list_permission(credential, &a.id).unwrap(), 1);
        assert_eq!(db.workspace_permission(credential, &w.id).unwrap(), 1);
        assert!(matches!(
            db.create_section(
                credential,
                &a.id,
                SectionInput {
                    title: "Denied".into(),
                    description: String::new()
                }
            ),
            Err(Error::Forbidden)
        ));
        db.conn
            .execute("UPDATE auth_agents SET revoked=1 WHERE id='scoped'", [])
            .unwrap();
        assert_eq!(db.list_permission(credential, &a.id).unwrap(), 0);
        assert!(db.workspaces(credential).unwrap().is_empty());
    }
    #[test]
    fn access_expansion_requires_consent_and_rolls_back_completely() {
        let mut db = db();
        let w = workspace(&mut db);
        let l = list(&mut db, &w.id, true);
        let revision = db.workspace("dev", &w.id).unwrap().revision;
        assert!(matches!(
            db.set_workspace_member(
                "dev",
                &w.id,
                "other",
                WorkspaceRole::Viewer,
                revision,
                false
            ),
            Err(Error::Conflict(_))
        ));
        assert_eq!(db.workspace_permission("other", &w.id).unwrap(), 0);
        assert_eq!(db.workspace("dev", &w.id).unwrap().revision, revision);
        db.set_workspace_member("dev", &w.id, "other", WorkspaceRole::Viewer, revision, true)
            .unwrap();
        assert!(matches!(
            db.set_list_restricted("dev", &l.id, false, l.revision, false),
            Err(Error::Conflict(_))
        ));
        assert!(db.list_info("dev", &l.id).unwrap().restricted);
        assert_eq!(
            db.list_info("dev", &l.id).unwrap().list.revision,
            l.revision
        );
        let l = db
            .grant_list("dev", &l.id, "guest", ListRole::Viewer, l.revision, true)
            .unwrap();
        assert!(matches!(
            db.grant_list("dev", &l.id, "guest", ListRole::Editor, l.revision, false),
            Err(Error::Conflict(_))
        ));
        assert_eq!(db.list_permission("guest", &l.id).unwrap(), 1);
        assert_eq!(
            db.list_info("dev", &l.id).unwrap().list.revision,
            l.revision
        );
        // Narrowing an existing direct editor to viewer needs no broadening consent.
        let l = db
            .grant_list("dev", &l.id, "guest", ListRole::Editor, l.revision, true)
            .unwrap();
        db.grant_list("dev", &l.id, "guest", ListRole::Viewer, l.revision, false)
            .unwrap();
        assert_eq!(db.list_permission("guest", &l.id).unwrap(), 1);
    }
    #[test]
    fn input_fields_are_strict_and_invalid_content_is_rejected() {
        assert!(
            serde_json::from_str::<WorkspaceInput>(r#"{"title":"Example","owner":"guest"}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<CreateList>(
                r#"{"title":"Example","workspace_id":"x","role":"owner"}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<WorkspaceRole>(r#""administrator""#).is_err());
        let mut db = db();
        assert!(matches!(
            db.create_workspace(
                "dev",
                WorkspaceInput {
                    title: "   ".into(),
                    description: String::new()
                }
            ),
            Err(Error::Invalid(_))
        ));
        assert!(db.workspaces("dev").unwrap().is_empty());
    }
}
