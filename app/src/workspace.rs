//! Workspace policies and list/section administration. Direct list grants do not
//! imply workspace membership: external known users see only their shared list.
use crate::core::{Error, List, Store};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
#[cfg(test)]
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
    pub color: Option<String>,
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
    if title.trim().is_empty() || title.chars().count() > 500 || title.contains(['\r', '\n']) {
        return Err(Error::Invalid(
            "title must be a single line of 1–500 characters".into(),
        ));
    }
    if description.len() > 100_000 {
        return Err(Error::Invalid(
            "description must not exceed 100000 bytes".into(),
        ));
    }
    Ok(())
}
fn validate_existing_title(title: &str, description: &str, previous: &str) -> Result<(), Error> {
    if title == previous && title.contains(['\r', '\n']) {
        return validate(&title.replace(['\r', '\n'], " "), description);
    }
    validate(title, description)
}

#[cfg(test)]
#[test]
fn entity_titles_reject_line_breaks() {
    assert!(validate("first\nsecond", "").is_err());
    assert!(validate("first\rsecond", "").is_err());
    assert!(validate("single line", "line one\nline two").is_ok());
    assert!(validate_existing_title("old\ntitle", "changed description", "old\ntitle").is_ok());
    assert!(validate_existing_title("new\ntitle", "", "old\ntitle").is_err());
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
    if c.query_row("SELECT EXISTS(SELECT 1 FROM workspace_lifecycle WHERE workspace_id=?1 AND deleted_at IS NOT NULL)",[id],|r|r.get::<_,bool>(0))? {return Err(Error::NotFound);}
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
    /// Archive preserves data and access; deleting an archived workspace moves it to
    /// retained trash and revokes all inherited and direct list access atomically.
    pub fn change_workspace_lifecycle(
        &mut self,
        actor: &str,
        id: &str,
        revision: i64,
        action: &str,
        confirmation: Option<&str>,
    ) -> Result<(), Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        require_workspace(&tx, actor, id, 3)?;
        let old = workspace_raw(&tx, id)?;
        check_revision(old.revision, revision)?;
        let archived = tx
            .query_row(
                "SELECT archived FROM workspace_lifecycle WHERE workspace_id=?1",
                [id],
                |r| r.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(false);
        match action {
            "archive" | "restore" => {
                tx.execute("INSERT INTO workspace_lifecycle(workspace_id,archived) VALUES(?1,?2) ON CONFLICT(workspace_id) DO UPDATE SET archived=excluded.archived",params![id,action=="archive"])?;
            }
            "delete" => {
                if !archived {
                    return Err(Error::Invalid(
                        "Archive the workspace before deleting it".into(),
                    ));
                }
                if confirmation != Some(old.title.as_str()) {
                    return Err(Error::Invalid(
                        "Type the exact workspace name to confirm deletion".into(),
                    ));
                }
                tx.execute("UPDATE workspace_lifecycle SET deleted_at=CURRENT_TIMESTAMP WHERE workspace_id=?1",[id])?;
            }
            _ => return Err(Error::Invalid("Unknown workspace action".into())),
        }
        tx.execute(
            "UPDATE workspaces SET revision=revision+1 WHERE id=?1",
            [id],
        )?;
        tx.execute("INSERT INTO workspace_lifecycle_events(workspace_id,actor_id,action,revision) VALUES(?1,?2,?3,?4)",params![id,actor,action,revision+1])?;
        tx.commit()?;
        Ok(())
    }
    pub fn workspace_permission(&self, actor: &str, workspace: &str) -> Result<i64, Error> {
        workspace_rank(&self.conn, actor, workspace)
    }
    pub fn workspaces(&self, actor: &str) -> Result<Vec<Workspace>, Error> {
        let mut stmt=self.conn.prepare("SELECT w.id,w.title,w.description,w.revision FROM workspaces w JOIN actor_workspace_access m ON m.workspace_id=w.id WHERE m.actor_id=?1 AND NOT EXISTS(SELECT 1 FROM workspace_lifecycle x WHERE x.workspace_id=w.id AND x.deleted_at IS NOT NULL) ORDER BY w.title,w.id")?;
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let owner = crate::core::actor_user(&tx, actor, 3)?;
        let id = crate::defaults::entity_id(&tx, "workspaces", "s")?;
        tx.execute(
            "INSERT INTO workspaces(id,title,description) VALUES(?1,?2,?3)",
            params![id, input.title.trim(), input.description],
        )?;
        tx.execute(
            "INSERT INTO workspace_members(workspace_id,user_id,role) VALUES(?1,?2,'owner')",
            params![id, owner],
        )?;
        crate::slugs::workspace(&tx, &id)?;
        crate::defaults::public_id(&tx, "workspace", &id)?;
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        require_workspace(&tx, actor, id, 3)?;
        let old = workspace_raw(&tx, id)?;
        check_revision(old.revision, change.revision)?;
        let title = change.title.unwrap_or_else(|| old.title.clone());
        let description = change.description.unwrap_or(old.description);
        validate_existing_title(&title, &description, &old.title)?;
        tx.execute(
            "UPDATE workspaces SET title=?2,description=?3 WHERE id=?1",
            params![id, title.trim(), description],
        )?;
        crate::slugs::workspace(&tx, id)?;
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        require_workspace(
            &tx,
            actor,
            &input.workspace_id,
            if input.restricted { 3 } else { 2 },
        )?;
        let id = crate::defaults::entity_id(&tx, "lists", "l")?;
        tx.execute("INSERT INTO lists(id,title,description,workspace_id,restricted,position) VALUES(?1,?2,?3,?4,?5,(SELECT COALESCE(MAX(position),-1)+1 FROM lists WHERE workspace_id=?4))",params![id,input.title.trim(),input.description,input.workspace_id,input.restricted])?;
        bump_workspace(&tx, &input.workspace_id)?;
        crate::slugs::list(&tx, &id)?;
        crate::defaults::list(&tx, &id)?;
        tx.execute(
            "INSERT OR IGNORE INTO workspace_defaults(workspace_id,list_id) VALUES(?1,?2)",
            params![input.workspace_id, id],
        )?;
        crate::defaults::public_id(&tx, "list", &id)?;
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        require_list(&tx, actor, id, 2)?;
        let old = list_raw(&tx, id)?;
        check_revision(old.revision, change.revision)?;
        if change.archived == Some(true)
            && tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM user_inboxes WHERE list_id=?1)",
                [id],
                |r| r.get::<_, bool>(0),
            )?
        {
            return Err(Error::Invalid(
                "Your personal Inbox cannot be archived".into(),
            ));
        }
        if let Some(archived) = change.archived {
            let user = crate::core::actor_identity(&tx, actor)?.0;
            tx.execute("UPDATE lists SET archived_by=CASE WHEN ?2 THEN ?3 ELSE NULL END,archived_at=CASE WHEN ?2 THEN datetime('now') ELSE NULL END WHERE id=?1 AND archived!=?2",params![id,archived,user])?;
        }
        crate::navigation::validate_list_color(&change.color)?;
        let title = change.title.unwrap_or_else(|| old.title.clone());
        let description = change.description.unwrap_or(old.description);
        validate_existing_title(&title, &description, &old.title)?;
        tx.execute(
            "UPDATE lists SET title=?2,description=?3,archived=COALESCE(?4,archived) WHERE id=?1",
            params![id, title.trim(), description, change.archived],
        )?;
        if let Some(color) = change.color {
            tx.execute("UPDATE lists SET color=CASE WHEN substr(?1,1,1)='#' THEN NULL ELSE ?1 END,color_hex=CASE WHEN substr(?1,1,1)='#' THEN ?1 ELSE NULL END WHERE id=?2", params![color, id])?;
        }
        crate::slugs::list(&tx, id)?;
        let result = bump_list(&tx, id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn list_archive_note(&self, actor: &str, id: &str) -> Result<Option<String>, Error> {
        require_list(&self.conn, actor, id, 1)?;
        let user = crate::core::actor_identity(&self.conn, actor)?.0;
        self.conn.query_row("SELECT l.archived,l.archived_by,u.name,l.archived_at FROM lists l LEFT JOIN users u ON u.id=l.archived_by WHERE l.id=?1",[id],|r| {
            let archived: bool = r.get(0)?;
            let by: Option<String> = r.get(1)?;
            let name: Option<String> = r.get(2)?;
            let at: Option<String> = r.get(3)?;
            Ok(archived.then(|| match (by,at) {
                (Some(by),Some(at)) => format!("Archived by {} on {} UTC",if by == user { "you".into() } else { name.unwrap_or_else(|| "a former member".into()) },at),
                _ => "Archived · archive details unavailable".into(),
            }))
        }).map_err(Into::into)
    }
    pub fn set_list_restricted(
        &mut self,
        actor: &str,
        id: &str,
        restricted: bool,
        revision: i64,
        acknowledge_access_change: bool,
    ) -> Result<List, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
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
        let mut stmt=self.conn.prepare("SELECT id,list_id,title,description,position,revision FROM sections WHERE list_id=?1 AND NOT EXISTS(SELECT 1 FROM list_defaults d WHERE d.section_id=sections.id) ORDER BY position,id")?;
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        require_list(&tx, actor, list, 2)?;
        let id = crate::defaults::entity_id(&tx, "sections", "c")?;
        tx.execute("INSERT INTO sections(id,list_id,title,description,position) VALUES(?1,?2,?3,?4,(SELECT COALESCE(MAX(position),-1)+1 FROM sections WHERE list_id=?2))",params![id,list,input.title.trim(),input.description])?;
        bump_list(&tx, list)?;
        crate::slugs::section(&tx, &id)?;
        crate::defaults::public_id(&tx, "section", &id)?;
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let old = section_raw(&tx, id)?;
        require_list(&tx, actor, &old.list_id, 2)?;
        check_revision(old.revision, change.revision)?;
        let title = change.title.unwrap_or_else(|| old.title.clone());
        let description = change.description.unwrap_or(old.description);
        validate_existing_title(&title, &description, &old.title)?;
        tx.execute(
            "UPDATE sections SET title=?2,description=?3,revision=revision+1 WHERE id=?1",
            params![id, title.trim(), description],
        )?;
        crate::slugs::section(&tx, id)?;
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
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
    pub fn reorder_visible_lists(
        &mut self,
        actor: &str,
        workspace: &str,
        revision: i64,
        ids: &[String],
    ) -> Result<Workspace, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        require_workspace(&tx, actor, workspace, 3)?;
        check_revision(workspace_raw(&tx, workspace)?.revision, revision)?;
        let all = {
            let mut stmt = tx.prepare("SELECT l.id, l.archived=0 AND EXISTS(SELECT 1 FROM actor_list_access a WHERE a.actor_id=?2 AND a.list_id=l.id AND a.rank>=1) FROM lists l WHERE l.workspace_id=?1 ORDER BY l.position,l.id")?;
            let rows = stmt.query_map(params![workspace, actor], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let visible: Vec<_> = all
            .iter()
            .filter(|(_, visible)| *visible)
            .map(|(id, _)| id.clone())
            .collect();
        exact_order(&visible, ids)?;
        let mut reordered = ids.iter();
        for (position, (id, visible)) in all.iter().enumerate() {
            let id = if *visible {
                reordered.next().expect("validated order")
            } else {
                id
            };
            tx.execute(
                "UPDATE lists SET position=?2,revision=revision+1 WHERE id=?1 AND position<>?2",
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
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        require_list(&tx, actor, list, 2)?;
        check_revision(list_raw(&tx, list)?.revision, revision)?;
        let current = {
            let mut stmt = tx.prepare("SELECT id FROM sections WHERE list_id=?1 AND NOT EXISTS(SELECT 1 FROM list_defaults d WHERE d.section_id=sections.id)")?;
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

/// One changed effective grant, including loss (rank 0) and owner access (rank 3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListMoveAudienceChange {
    pub user_id: String,
    pub before_rank: i64,
    pub after_rank: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListMoveTagMapping {
    pub source_tag_id: String,
    pub name: String,
    pub destination_tag_id: Option<String>,
    pub destination_tag_revision: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWorkspaceMovePreview {
    pub list_id: String,
    pub source_workspace_id: String,
    pub destination_workspace_id: String,
    pub list_revision: i64,
    pub source_workspace_revision: i64,
    pub destination_workspace_revision: i64,
    pub restricted: bool,
    pub audience: Vec<ListMoveAudienceChange>,
    pub audience_broadens: bool,
    pub source_public: bool,
    pub destination_public: bool,
    pub task_count: usize,
    pub tag_mapping: Vec<ListMoveTagMapping>,
    pub policy_hash: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListWorkspaceMoveInput {
    pub destination_workspace_id: String,
    pub list_revision: i64,
    pub source_workspace_revision: i64,
    pub destination_workspace_revision: i64,
    pub policy_hash: String,
    #[serde(default)]
    pub acknowledge_access_change: bool,
}
fn move_audience(
    c: &Connection,
    list: &str,
    workspace: &str,
    restricted: bool,
) -> Result<std::collections::BTreeMap<String, i64>, Error> {
    let mut q=c.prepare("SELECT user_id,MAX(rank) FROM (SELECT m.user_id,CASE COALESCE(r.role,'editor') WHEN 'viewer' THEN 1 ELSE 2 END rank FROM memberships m LEFT JOIN membership_roles r ON r.list_id=m.list_id AND r.user_id=m.user_id WHERE m.list_id=?1 UNION ALL SELECT user_id,CASE role WHEN 'owner' THEN 3 WHEN 'editor' THEN 2 ELSE 1 END FROM workspace_members WHERE workspace_id=?2 AND (?3=0 OR role='owner')) GROUP BY user_id ORDER BY user_id")?;
    Ok(q.query_map(params![list, workspace, restricted], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?
    .collect::<Result<_, _>>()?)
}
fn list_move_preview(
    c: &Connection,
    actor: &str,
    list: &str,
    destination: &str,
) -> Result<ListWorkspaceMovePreview, Error> {
    use sha2::{Digest, Sha256};
    crate::core::actor_user(c, actor, 3)?;
    let l = list_raw(c, list)?;
    let (source, restricted): (Option<String>, bool) = c.query_row(
        "SELECT workspace_id,restricted FROM lists WHERE id=?1",
        [list],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let source = source.ok_or_else(|| {
        Error::Invalid("list has no source workspace; adoption is a separate operation".into())
    })?;
    require_workspace(c, actor, &source, 3)?;
    require_workspace(c, actor, destination, 3)?;
    if source == destination {
        return Err(Error::Invalid(
            "choose a different destination workspace".into(),
        ));
    }
    let from = workspace_raw(c, &source)?;
    let to = workspace_raw(c, destination)?;
    let before = move_audience(c, list, &source, restricted)?;
    let after = move_audience(c, list, destination, restricted)?;
    let users = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let audience = users
        .into_iter()
        .filter_map(|user_id| {
            let before_rank = before.get(&user_id).copied().unwrap_or(0);
            let after_rank = after.get(&user_id).copied().unwrap_or(0);
            (before_rank != after_rank).then_some(ListMoveAudienceChange {
                user_id,
                before_rank,
                after_rank,
            })
        })
        .collect::<Vec<_>>();
    let versions = {
        let mut q = c.prepare("SELECT id,revision FROM tasks WHERE list_id=?1 ORDER BY id")?;
        q.query_map([list], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let tag_mapping = {
        let mut q=c.prepare("SELECT DISTINCT g.id,g.name,d.id,d.revision FROM task_tags tt JOIN tasks t ON t.id=tt.task_id JOIN tags g ON g.id=tt.tag_id LEFT JOIN tags d ON d.scope=?2 AND d.normalized_name=g.normalized_name WHERE t.list_id=?1 ORDER BY g.id")?;
        q.query_map(params![list, format!("workspace:{destination}")], |r| {
            Ok(ListMoveTagMapping {
                source_tag_id: r.get(0)?,
                name: r.get(1)?,
                destination_tag_id: r.get(2)?,
                destination_tag_revision: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let grants = {
        let mut q=c.prepare("SELECT m.user_id,COALESCE(r.role,'editor') FROM memberships m LEFT JOIN membership_roles r ON r.list_id=m.list_id AND r.user_id=m.user_id WHERE m.list_id=?1 ORDER BY m.user_id")?;
        q.query_map([list], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    // Comments and files have independent revisions from their task brief.
    // Bind their state too, so a preview never silently approves later content.
    let resource_versions = {
        let mut q=c.prepare("SELECT 'section' kind,id,revision FROM sections WHERE list_id=?1 UNION ALL SELECT 'comment',c.id,c.revision FROM comments c JOIN tasks t ON t.id=c.task_id WHERE t.list_id=?1 UNION ALL SELECT 'attachment',a.id,a.revision FROM attachments a JOIN tasks t ON t.id=a.task_id WHERE t.list_id=?1 UNION ALL SELECT 'upload',u.id,u.revision FROM upload_intents u JOIN tasks t ON t.id=u.task_id WHERE t.list_id=?1 ORDER BY 1,2")?;
        q.query_map([list], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let source_public = crate::public_access::list_public(c, list)?;
    let explicit_public = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM public_lists WHERE list_id=?1 AND enabled=1)",
        [list],
        |r| r.get::<_, bool>(0),
    )?;
    let destination_public = explicit_public
        || (!restricted
            && c.query_row(
                "SELECT EXISTS(SELECT 1 FROM public_spaces WHERE workspace_id=?1 AND enabled=1)",
                [destination],
                |r| r.get::<_, bool>(0),
            )?);
    let mut result = ListWorkspaceMovePreview {
        list_id: list.into(),
        source_workspace_id: source,
        destination_workspace_id: destination.into(),
        list_revision: l.revision,
        source_workspace_revision: from.revision,
        destination_workspace_revision: to.revision,
        restricted,
        audience_broadens: audience.iter().any(|a| a.after_rank > a.before_rank)
            || (destination_public && !source_public),
        source_public,
        destination_public,
        audience,
        task_count: versions.len(),
        tag_mapping,
        policy_hash: String::new(),
    };
    let material=serde_json::to_vec(&serde_json::json!({"actor":actor,"preview":result,"tasks":versions,"direct_grants":grants,"before_audience":before,"after_audience":after,"resource_versions":resource_versions})).map_err(|_|Error::Invalid("cannot encode list move policy".into()))?;
    result.policy_hash = format!("{:x}", Sha256::digest(material));
    Ok(result)
}
impl Store {
    pub fn list_workspace_move_preview(
        &self,
        actor: &str,
        list: &str,
        destination_workspace: &str,
    ) -> Result<ListWorkspaceMovePreview, Error> {
        let tx = self.conn.unchecked_transaction()?;
        let preview = list_move_preview(&tx, actor, list, destination_workspace)?;
        tx.commit()?;
        Ok(preview)
    }
    pub fn move_list_workspace(
        &mut self,
        actor: &str,
        list: &str,
        input: ListWorkspaceMoveInput,
    ) -> Result<List, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let preview = list_move_preview(&tx, actor, list, &input.destination_workspace_id)?;
        check_revision(preview.list_revision, input.list_revision)?;
        check_revision(
            preview.source_workspace_revision,
            input.source_workspace_revision,
        )?;
        check_revision(
            preview.destination_workspace_revision,
            input.destination_workspace_revision,
        )?;
        if preview.policy_hash != input.policy_hash {
            return Err(Error::Conflict(
                "list move preview changed; review a fresh preview".into(),
            ));
        }
        if preview.audience_broadens && !input.acknowledge_access_change {
            return Err(Error::Conflict(
                "acknowledge the previewed access expansion before moving this list".into(),
            ));
        }
        let ids = {
            let mut q = tx.prepare("SELECT id FROM tasks WHERE list_id=?1 ORDER BY id")?;
            q.query_map([list], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        tx.execute("UPDATE lists SET workspace_id=?2,position=(SELECT COALESCE(MAX(position),-1)+1 FROM lists WHERE workspace_id=?2),revision=revision+1 WHERE id=?1",params![list,input.destination_workspace_id])?;
        tx.execute(
            "DELETE FROM workspace_defaults WHERE workspace_id=?1 AND list_id=?2",
            params![preview.source_workspace_id, list],
        )?;
        tx.execute("INSERT OR IGNORE INTO workspace_defaults(workspace_id,list_id) SELECT ?1,id FROM lists WHERE workspace_id=?1 AND archived=0 ORDER BY position,id LIMIT 1", [&preview.source_workspace_id])?;
        tx.execute(
            "INSERT OR IGNORE INTO workspace_defaults(workspace_id,list_id) VALUES(?1,?2)",
            params![input.destination_workspace_id, list],
        )?;
        crate::slugs::list(&tx, list)?;
        crate::taxonomy::remap_tags_after_move(&tx, actor, &ids, list)?;
        for id in ids {
            let before = crate::core::raw_task(&tx, &id)?;
            tx.execute("UPDATE tasks SET revision=revision+1 WHERE id=?1", [&id])?;
            let after = crate::core::raw_task(&tx, &id)?;
            crate::core::event(&tx, &after, "list_workspace_move", actor, Some(&before))?;
            // Task history is visible to direct list guests; never disclose either
            // workspace identifier through this public event payload.
            let detail = serde_json::json!({"policy_hash":input.policy_hash,"acknowledged":input.acknowledge_access_change});
            tx.execute(
                "UPDATE history SET event=json_set(event,'$.workspace_move',json(?2)) WHERE seq=?1",
                params![tx.last_insert_rowid(), detail.to_string()],
            )?;
        }
        bump_workspace(&tx, &preview.source_workspace_id)?;
        bump_workspace(&tx, &input.destination_workspace_id)?;
        let moved = list_raw(&tx, list)?;
        tx.commit()?;
        Ok(moved)
    }
}

/// Reviewed, bounded section move. Member identifiers remain private.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionMovePreview {
    pub section_id: String,
    pub source_list_id: String,
    pub destination_list_id: String,
    pub section_revision: i64,
    pub source_list_revision: i64,
    pub destination_list_revision: i64,
    pub assigned_task_count: usize,
    pub affected_task_count: usize,
    pub other_section_assignments_cleared: usize,
    pub source_members: i64,
    pub destination_members: i64,
    pub added_members: i64,
    pub source_public: bool,
    pub destination_public: bool,
    pub audience_broadens: bool,
    pub policy_hash: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionMoveInput {
    pub destination_list_id: String,
    pub section_revision: i64,
    pub source_list_revision: i64,
    pub destination_list_revision: i64,
    pub policy_hash: String,
    #[serde(default)]
    pub acknowledge_access_change: bool,
}
#[derive(Debug, Serialize)]
struct SectionMoveTask {
    id: String,
    parent: Option<String>,
    list: String,
    revision: i64,
    deleted: bool,
    position: i64,
    section: Option<String>,
}
struct SectionMovePlan {
    preview: SectionMovePreview,
    tasks: Vec<SectionMoveTask>,
    roots: Vec<(String, i64)>,
}
fn section_move_plan(
    c: &Connection,
    actor: &str,
    id: &str,
    destination: &str,
) -> Result<SectionMovePlan, Error> {
    use sha2::{Digest, Sha256};
    let section = section_raw(c, id)?;
    require_list(c, actor, &section.list_id, 2)?;
    require_list(c, actor, destination, 2)?;
    if section.list_id == destination {
        return Err(Error::Invalid("choose a different destination list".into()));
    }
    let source_list = list_raw(c, &section.list_id)?;
    let destination_list = list_raw(c, destination)?;
    let mut q=c.prepare("WITH RECURSIVE affected(id) AS (
        SELECT task_id FROM task_locations WHERE section_id=?1
        UNION SELECT t.id FROM tasks t INDEXED BY tasks_parent JOIN affected a ON t.parent_id=a.id
        LIMIT 501
    ) SELECT t.id,t.parent_id,t.list_id,t.revision,t.deleted_at IS NOT NULL,t.position,l.section_id
      FROM affected a JOIN tasks t ON t.id=a.id LEFT JOIN task_locations l ON l.task_id=t.id ORDER BY t.position,t.id")?;
    let tasks = q
        .query_map([id], |r| {
            Ok(SectionMoveTask {
                id: r.get(0)?,
                parent: r.get(1)?,
                list: r.get(2)?,
                revision: r.get(3)?,
                deleted: r.get(4)?,
                position: r.get(5)?,
                section: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if tasks.len() > 500 {
        return Err(Error::Invalid(
            "section move affects more than 500 tasks; move smaller groups first".into(),
        ));
    }
    if tasks.iter().any(|t| t.list != section.list_id) {
        return Err(Error::Invalid(
            "section contains invalid cross-list task ancestry".into(),
        ));
    }
    let by_id: HashMap<_, _> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    // Prove all bounded components terminate before invoking the ordinary subtree mover.
    for task in &tasks {
        let mut seen = HashSet::new();
        let mut node = Some(task);
        while let Some(current) = node {
            if !seen.insert(current.id.as_str()) {
                return Err(Error::Invalid(
                    "section contains cyclic task ancestry".into(),
                ));
            }
            node = current
                .parent
                .as_deref()
                .and_then(|p| by_id.get(p).copied());
        }
    }
    let roots = tasks
        .iter()
        .filter(|t| t.parent.as_deref().is_none_or(|p| !by_id.contains_key(p)))
        .map(|t| {
            if t.deleted {
                Err(Error::Conflict(
                    "restore trashed standalone section tasks before moving this section".into(),
                ))
            } else {
                Ok((t.id.clone(), t.revision))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let assigned_task_count = tasks
        .iter()
        .filter(|t| t.section.as_deref() == Some(id))
        .count();
    let stored_assigned: i64 = c.query_row(
        "SELECT count(*) FROM task_locations WHERE section_id=?1",
        [id],
        |r| r.get(0),
    )?;
    if stored_assigned as usize != assigned_task_count {
        return Err(Error::Invalid(
            "section contains invalid task locations".into(),
        ));
    }
    let audiences = |list: &str| -> Result<Vec<(String, i64)>, Error> {
        let mut q = c.prepare(
            "SELECT user_id,rank FROM effective_list_access WHERE list_id=?1 ORDER BY user_id",
        )?;
        Ok(q.query_map([list], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    };
    let source_audience = audiences(&section.list_id)?;
    let destination_audience = audiences(destination)?;
    let source_users: HashSet<_> = source_audience.iter().map(|(id, _)| id.as_str()).collect();
    let added_members = destination_audience
        .iter()
        .filter(|(id, _)| !source_users.contains(id.as_str()))
        .count() as i64;
    let ids =
        serde_json::to_string(&tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>()).unwrap();
    let mut q=c.prepare("SELECT tt.task_id,t.id,t.name,t.revision FROM task_tags tt JOIN tags t ON t.id=tt.tag_id WHERE tt.task_id IN(SELECT value FROM json_each(?1)) ORDER BY tt.task_id,t.id")?;
    let tags = q
        .query_map([&ids], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut q=c.prepare("SELECT id,name,revision FROM tags WHERE scope=(SELECT CASE WHEN workspace_id IS NULL THEN 'list:'||id ELSE 'workspace:'||workspace_id END FROM lists WHERE id=?2) AND normalized_name IN(SELECT t.normalized_name FROM tags t JOIN task_tags tt ON tt.tag_id=t.id WHERE tt.task_id IN(SELECT value FROM json_each(?1))) ORDER BY id")?;
    let destination_tags = q
        .query_map(params![ids, destination], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let sequences:(i64,i64)=c.query_row("SELECT (SELECT coalesce(max(seq),0) FROM history WHERE list_id=?1),(SELECT coalesce(max(seq),0) FROM history WHERE list_id=?2)",params![section.list_id,destination],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let source_public = crate::public_access::list_public(c, &section.list_id)?;
    let destination_public = crate::public_access::list_public(c, destination)?;
    let policy = serde_json::to_vec(&(
        actor,
        source_public,
        destination_public,
        &section,
        &source_list,
        &destination_list,
        &tasks,
        &source_audience,
        &destination_audience,
        &tags,
        &destination_tags,
        sequences,
    ))
    .unwrap();
    let policy_hash = format!("{:x}", Sha256::digest(policy));
    let preview = SectionMovePreview {
        section_id: id.into(),
        source_list_id: section.list_id,
        destination_list_id: destination.into(),
        section_revision: section.revision,
        source_list_revision: source_list.revision,
        destination_list_revision: destination_list.revision,
        assigned_task_count,
        affected_task_count: tasks.len(),
        other_section_assignments_cleared: tasks
            .iter()
            .filter(|t| t.section.as_deref().is_some_and(|s| s != id))
            .count(),
        source_members: source_audience.len() as i64,
        destination_members: destination_audience.len() as i64,
        added_members,
        audience_broadens: added_members > 0 || (destination_public && !source_public),
        source_public,
        destination_public,
        policy_hash,
    };
    Ok(SectionMovePlan {
        preview,
        tasks,
        roots,
    })
}
impl Store {
    pub fn section_move_preview(
        &self,
        actor: &str,
        id: &str,
        destination: &str,
    ) -> Result<SectionMovePreview, Error> {
        let tx = self.conn.unchecked_transaction()?;
        let plan = section_move_plan(&tx, actor, id, destination)?;
        tx.commit()?;
        Ok(plan.preview)
    }
    pub fn move_section(
        &mut self,
        actor: &str,
        id: &str,
        input: SectionMoveInput,
    ) -> Result<Section, Error> {
        let mut write = crate::agent_ops::StoreWrite::new(self)?;
        let plan = section_move_plan(&write.conn, actor, id, &input.destination_list_id)?;
        let p = &plan.preview;
        if p.section_revision != input.section_revision
            || p.source_list_revision != input.source_list_revision
            || p.destination_list_revision != input.destination_list_revision
            || p.policy_hash != input.policy_hash
        {
            return Err(Error::Conflict(
                "section move preview changed; review a fresh preview".into(),
            ));
        }
        if p.audience_broadens && !input.acknowledge_access_change {
            return Err(Error::Conflict("destination adds readers; review the section move preview and acknowledge_access_change".into()));
        }
        for (root, revision) in &plan.roots {
            write.move_subtree(
                actor,
                root,
                &input.destination_list_id,
                *revision,
                input.acknowledge_access_change,
            )?;
        }
        write.conn.execute("UPDATE sections SET list_id=?2,position=(SELECT coalesce(max(position),-1)+1 FROM sections WHERE list_id=?2),revision=revision+1 WHERE id=?1",params![id,input.destination_list_id])?;
        crate::slugs::section(&write.conn, id)?;
        for task in &plan.tasks {
            if task.section.as_deref() == Some(id) {
                write.conn.execute(
                    "INSERT INTO task_locations(task_id,section_id) VALUES(?1,?2) ON CONFLICT(task_id) DO UPDATE SET section_id=excluded.section_id",
                    params![task.id, id],
                )?;
            }
            let moved = crate::core::raw_task(&write.conn, &task.id)?;
            crate::core::event(&write.conn, &moved, "section_move", actor, None)?;
        }
        bump_list(&write.conn, &p.source_list_id)?;
        bump_list(&write.conn, &p.destination_list_id)?;
        let result = section_raw(&write.conn, id)?;
        write.commit()?;
        Ok(result)
    }
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
    fn list_archiving_records_actor_preserves_content_and_restores() {
        let mut db = db();
        let space = workspace(&mut db);
        let list = list(&mut db, &space.id, false);
        let change = |revision, archived| ListChange {
            revision,
            archived: Some(archived),
            title: None,
            description: None,
            color: None,
        };
        let archived = db
            .update_list("dev", &list.id, change(list.revision, true))
            .unwrap();
        assert!(
            db.list_archive_note("dev", &list.id)
                .unwrap()
                .unwrap()
                .starts_with("Archived by you on ")
        );
        assert!(db.list_archive_note("guest", &list.id).is_err());
        assert!(
            db.update_list("guest", &list.id, change(archived.revision, false))
                .is_err()
        );
        assert!(
            db.update_list("dev", &list.id, change(list.revision, false))
                .is_err()
        );
        let restored = db
            .update_list("dev", &list.id, change(archived.revision, false))
            .unwrap();
        assert!(db.list_archive_note("dev", &list.id).unwrap().is_none());
        assert_eq!(restored.title, list.title);
        assert!(!db.list_info("dev", &list.id).unwrap().archived);
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
    fn visible_list_reordering_preserves_archived_slots_and_rejects_stale_input() {
        let mut db = db();
        let w = workspace(&mut db);
        let a = list(&mut db, &w.id, false);
        let archived = list(&mut db, &w.id, false);
        let b = list(&mut db, &w.id, false);
        db.conn
            .execute("UPDATE lists SET archived=1 WHERE id=?1", [&archived.id])
            .unwrap();
        let revision = db.workspace("dev", &w.id).unwrap().revision;
        assert!(
            db.reorder_visible_lists("other", &w.id, revision, &[b.id.clone(), a.id.clone()])
                .is_err()
        );
        assert!(
            db.reorder_visible_lists("dev", &w.id, revision, std::slice::from_ref(&a.id))
                .is_err()
        );
        db.reorder_visible_lists("dev", &w.id, revision, &[b.id.clone(), a.id.clone()])
            .unwrap();
        let order: Vec<String> = {
            let mut stmt = db
                .conn
                .prepare("SELECT id FROM lists WHERE workspace_id=?1 ORDER BY position,id")
                .unwrap();
            stmt.query_map([&w.id], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(order, vec![b.id.clone(), archived.id, a.id.clone()]);
        assert!(matches!(
            db.reorder_visible_lists("dev", &w.id, revision, &[a.id, b.id]),
            Err(Error::Conflict(_))
        ));
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
    fn move_input(p: &ListWorkspaceMovePreview, ack: bool) -> ListWorkspaceMoveInput {
        ListWorkspaceMoveInput {
            destination_workspace_id: p.destination_workspace_id.clone(),
            list_revision: p.list_revision,
            source_workspace_revision: p.source_workspace_revision,
            destination_workspace_revision: p.destination_workspace_revision,
            policy_hash: p.policy_hash.clone(),
            acknowledge_access_change: ack,
        }
    }
    fn move_member(db: &mut Store, w: &str, user: &str, role: WorkspaceRole) {
        let revision = db.workspace("dev", w).unwrap().revision;
        db.set_workspace_member("dev", w, user, role, revision, true)
            .unwrap();
    }
    fn move_task(
        db: &mut Store,
        list: &str,
        parent: Option<String>,
        description: &str,
    ) -> crate::core::Task {
        db.capture(
            "dev",
            &Uuid::new_v4().to_string(),
            crate::core::Capture {
                list_id: list.into(),
                tasks: vec![crate::core::NewTask {
                    title: "Synthetic move task".into(),
                    description: description.into(),
                    parent_id: parent,
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    #[test]
    fn list_workspace_move_preserves_contents_sections_direct_guests_and_audit() {
        use crate::{
            discussion::{NewComment, UploadInput},
            taxonomy::TagSelector,
        };
        let mut db = db();
        db.conn
            .execute(
                "INSERT INTO users VALUES('destination-reader','Synthetic destination reader')",
                [],
            )
            .unwrap();
        let from = workspace(&mut db);
        let to = workspace(&mut db);
        move_member(&mut db, &from.id, "other", WorkspaceRole::Editor);
        move_member(&mut db, &to.id, "destination-reader", WorkspaceRole::Editor);
        let l = list(&mut db, &from.id, false);
        let dest_list = list(&mut db, &to.id, false);
        let direct_revision = db.list_info("dev", &l.id).unwrap().list.revision;
        db.grant_list(
            "dev",
            &l.id,
            "guest",
            ListRole::Viewer,
            direct_revision,
            true,
        )
        .unwrap();
        let section = section(&mut db, &l.id, "Preserved section");
        let root = move_task(&mut db, &l.id, None, &format!("Saved #{}", l.id));
        let child = move_task(&mut db, &l.id, Some(root.id.clone()), "Child context");
        let root = db
            .set_task_tags(
                "dev",
                &root.id,
                root.revision,
                vec![TagSelector::Name("urgent".into())],
            )
            .unwrap();
        let root = db
            .set_section("dev", &root.id, root.revision, Some(section.id.clone()))
            .unwrap();
        let old_tag = db.task_tags("dev", &root.id).unwrap().remove(0);
        let destination_tag = db.create_tag("dev", &dest_list.id, "URGENT").unwrap();
        let comment = db
            .create_comment(
                "dev",
                &root.id,
                "list-move-comment",
                NewComment {
                    body: format!("Preserve %{}", root.reference),
                },
            )
            .unwrap();
        let intent = db
            .create_upload_intent(
                "dev",
                &root.id,
                UploadInput {
                    comment_id: Some(comment.id.clone()),
                    filename: "synthetic.txt".into(),
                    size: 3,
                    sha256: "aa".repeat(32),
                },
            )
            .unwrap();
        db.conn.execute("INSERT INTO attachments(id,intent_id,task_id,comment_id,creator_id,filename,size,sha256,object_key) VALUES('synthetic-attachment',?1,?2,?3,'dev','synthetic.txt',3,?4,?5)",params![intent.id,root.id,comment.id,intent.sha256,intent.object_key]).unwrap();
        let trashed = move_task(&mut db, &l.id, None, "Trashed context");
        db.trash("dev", &trashed.id, trashed.revision).unwrap();
        let edges: Vec<(String, String)> = {
            let mut q=db.conn.prepare("SELECT target_kind,target_id FROM reference_edges ORDER BY source_id,occurrence").unwrap();
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(db.comments("other", &root.id, 0, 200).unwrap().len(), 1);
        let preview = db
            .list_workspace_move_preview("dev", &l.id, &to.id)
            .unwrap();
        assert_eq!(preview.task_count, 3);
        assert!(preview.audience_broadens);
        assert!(
            preview
                .audience
                .iter()
                .any(|a| a.user_id == "other" && a.before_rank == 2 && a.after_rank == 0)
        );
        assert!(
            preview
                .audience
                .iter()
                .any(|a| a.user_id == "destination-reader"
                    && a.before_rank == 0
                    && a.after_rank == 2)
        );
        assert!(!preview.audience.iter().any(|a| a.user_id == "guest"));
        assert_eq!(
            preview.tag_mapping[0].destination_tag_id.as_deref(),
            Some(destination_tag.id.as_str())
        );
        assert!(matches!(
            db.move_list_workspace("dev", &l.id, move_input(&preview, false)),
            Err(Error::Conflict(_))
        ));
        assert_eq!(
            db.list_info("dev", &l.id).unwrap().workspace_id.as_deref(),
            Some(from.id.as_str())
        );
        let moved = db
            .move_list_workspace("dev", &l.id, move_input(&preview, true))
            .unwrap();
        assert_eq!(moved.id, l.id);
        assert_eq!(moved.revision, preview.list_revision + 1);
        assert_eq!(
            db.list_info("dev", &l.id).unwrap().workspace_id.as_deref(),
            Some(to.id.as_str())
        );
        assert_eq!(db.subtree("dev", &root.id).unwrap().len(), 2);
        assert_eq!(
            db.task_section("dev", &root.id).unwrap(),
            Some(section.id.clone())
        );
        assert_eq!(db.task_section("dev", &child.id).unwrap(), Some(section.id));
        assert_eq!(
            db.task_tags("dev", &root.id).unwrap()[0].id,
            destination_tag.id
        );
        assert_eq!(db.conn.query_row("SELECT count(*) FROM tag_move_audit WHERE source_tag_id=?1 AND destination_tag_id=?2",params![old_tag.id,destination_tag.id],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert!(matches!(
            db.comments("other", &root.id, 0, 200),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            db.attachment("other", "synthetic-attachment"),
            Err(Error::Forbidden)
        ));
        assert!(
            db.search("other", "Synthetic", None, None, 100, None)
                .unwrap()
                .iter()
                .all(|t| t.list_id != l.id)
        );
        assert_eq!(
            db.comments("destination-reader", &root.id, 0, 200).unwrap()[0].id,
            comment.id
        );
        assert_eq!(
            db.attachment("guest", "synthetic-attachment").unwrap().id,
            "synthetic-attachment"
        );
        assert_eq!(db.list_permission("guest", &l.id).unwrap(), 1);
        assert_eq!(db.list_info("guest", &l.id).unwrap().workspace_id, None);
        assert_eq!(
            db.trash_tasks("dev")
                .unwrap()
                .iter()
                .find(|t| t.id == trashed.id)
                .unwrap()
                .revision,
            trashed.revision + 2
        );
        assert_eq!(db.conn.query_row("SELECT count(*) FROM history WHERE json_extract(event,'$.kind')='list_workspace_move' AND json_extract(event,'$.workspace_move.policy_hash')=?1",[&preview.policy_hash],|r|r.get::<_,i64>(0)).unwrap(),3);
        let after = {
            let mut q=db.conn.prepare("SELECT target_kind,target_id FROM reference_edges ORDER BY source_id,occurrence").unwrap();
            q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(edges, after);
        for event in db.changes("guest", 0).unwrap() {
            assert!(!event.to_string().contains(&from.id));
            assert!(!event.to_string().contains(&to.id));
        }

        assert!(
            db.changes("other", 0)
                .unwrap()
                .iter()
                .all(|e| e["task"]["list_id"] != l.id)
        );
    }
    #[test]
    fn list_workspace_move_restricted_policy_upgrades_and_stale_previews() {
        let mut db = db();
        let from = workspace(&mut db);
        let to = workspace(&mut db);
        move_member(&mut db, &from.id, "other", WorkspaceRole::Viewer);
        move_member(&mut db, &to.id, "other", WorkspaceRole::Editor);
        let l = list(&mut db, &from.id, false);
        let p = db
            .list_workspace_move_preview("dev", &l.id, &to.id)
            .unwrap();
        assert!(
            p.audience
                .iter()
                .any(|a| a.user_id == "other" && a.before_rank == 1 && a.after_rank == 2)
        );
        assert!(matches!(
            db.move_list_workspace("dev", &l.id, move_input(&p, false)),
            Err(Error::Conflict(_))
        ));
        let task = move_task(&mut db, &l.id, None, "Preview snapshot");
        assert!(matches!(
            db.move_list_workspace("dev", &l.id, move_input(&p, true)),
            Err(Error::Conflict(_))
        ));
        let p = db
            .list_workspace_move_preview("dev", &l.id, &to.id)
            .unwrap();
        db.set_task_tags(
            "dev",
            &task.id,
            task.revision,
            vec![crate::taxonomy::TagSelector::Name("new".into())],
        )
        .unwrap();
        assert!(matches!(
            db.move_list_workspace("dev", &l.id, move_input(&p, true)),
            Err(Error::Conflict(_))
        ));
        let p = db
            .list_workspace_move_preview("dev", &l.id, &to.id)
            .unwrap();
        let comment = db
            .create_comment(
                "dev",
                &task.id,
                "move-preview-comment",
                crate::discussion::NewComment {
                    body: "Before review".into(),
                },
            )
            .unwrap();
        let comment_preview = db
            .list_workspace_move_preview("dev", &l.id, &to.id)
            .unwrap();
        db.update_comment(
            "dev",
            &comment.id,
            crate::discussion::EditComment {
                revision: comment.revision,
                body: "Changed after review".into(),
            },
        )
        .unwrap();
        assert!(matches!(
            db.move_list_workspace("dev", &l.id, move_input(&comment_preview, true)),
            Err(Error::Conflict(_))
        ));
        move_member(&mut db, &to.id, "other", WorkspaceRole::Viewer);
        assert!(matches!(
            db.move_list_workspace("dev", &l.id, move_input(&p, true)),
            Err(Error::Conflict(_))
        ));
        let restricted = list(&mut db, &from.id, true);
        let p = db
            .list_workspace_move_preview("dev", &restricted.id, &to.id)
            .unwrap();
        assert!(!p.audience_broadens);
        db.move_list_workspace("dev", &restricted.id, move_input(&p, false))
            .unwrap();
        assert_eq!(db.list_permission("other", &restricted.id).unwrap(), 0);
        let restricted = list(&mut db, &from.id, true);
        let rev = db.list_info("dev", &restricted.id).unwrap().list.revision;
        db.grant_list("dev", &restricted.id, "guest", ListRole::Editor, rev, true)
            .unwrap();
        let p = db
            .list_workspace_move_preview("dev", &restricted.id, &to.id)
            .unwrap();
        assert!(!p.audience_broadens);
        db.move_list_workspace("dev", &restricted.id, move_input(&p, false))
            .unwrap();
        assert_eq!(db.list_permission("guest", &restricted.id).unwrap(), 2);
        assert_eq!(
            db.list_info("guest", &restricted.id).unwrap().workspace_id,
            None
        );
    }
    #[test]
    fn list_workspace_move_requires_live_owner_both_and_rejects_scoped_credentials() {
        let mut db = db();
        let from = workspace(&mut db);
        let to = workspace(&mut db);
        let l = list(&mut db, &from.id, false);
        move_member(&mut db, &from.id, "other", WorkspaceRole::Editor);
        assert!(matches!(
            db.list_workspace_move_preview("other", &l.id, &to.id),
            Err(Error::Forbidden)
        ));
        move_member(&mut db, &from.id, "other", WorkspaceRole::Owner);
        assert!(matches!(
            db.list_workspace_move_preview("other", &l.id, &to.id),
            Err(Error::Forbidden)
        ));
        db.conn.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,workspace,expires_at) VALUES('source-only','dev','synthetic-source-token','https://synthetic.invalid/api/v1','[\"admin\"]',?1,unixepoch()+3600)",[&from.id]).unwrap();
        assert!(matches!(
            db.list_workspace_move_preview("credential:source-only", &l.id, &to.id),
            Err(Error::Forbidden)
        ));
        let p = db
            .list_workspace_move_preview("dev", &l.id, &to.id)
            .unwrap();
        move_member(&mut db, &to.id, "other", WorkspaceRole::Owner);
        move_member(&mut db, &to.id, "dev", WorkspaceRole::Viewer);
        assert!(matches!(
            db.move_list_workspace("dev", &l.id, move_input(&p, true)),
            Err(Error::Forbidden)
        ));
        assert_eq!(
            db.conn
                .query_row("SELECT workspace_id FROM lists WHERE id=?1", [&l.id], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            from.id
        );
    }
}

#[cfg(test)]
mod section_move_tests {
    use super::*;
    use crate::core::{Capture, Change, NewTask, raw_task};
    fn fixture() -> (Store, List, List, Section) {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let mut make_list = |name: &str| {
            let w = db
                .create_workspace(
                    "dev",
                    WorkspaceInput {
                        title: name.into(),
                        description: String::new(),
                    },
                )
                .unwrap();
            db.create_list(
                "dev",
                CreateList {
                    workspace_id: w.id,
                    title: name.into(),
                    description: String::new(),
                    restricted: false,
                },
            )
            .unwrap()
        };
        let source = make_list("Source");
        let destination = make_list("Destination");
        let section = db
            .create_section(
                "dev",
                &source.id,
                SectionInput {
                    title: "Next Steps".into(),
                    description: "Preserved description".into(),
                },
            )
            .unwrap();
        (db, source, destination, section)
    }
    fn add(db: &mut Store, list: &str, parent: Option<String>, name: &str) -> crate::core::Task {
        db.capture(
            "dev",
            &Uuid::new_v4().to_string(),
            Capture {
                list_id: list.into(),
                tasks: vec![NewTask {
                    title: name.into(),
                    parent_id: parent,
                    description: "Synthetic content".into(),
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    fn input(p: &SectionMovePreview, ack: bool) -> SectionMoveInput {
        SectionMoveInput {
            destination_list_id: p.destination_list_id.clone(),
            section_revision: p.section_revision,
            source_list_revision: p.source_list_revision,
            destination_list_revision: p.destination_list_revision,
            policy_hash: p.policy_hash.clone(),
            acknowledge_access_change: ack,
        }
    }
    #[test]
    fn moves_nested_hidden_tasks_locations_tags_comments_and_attachment_metadata() {
        let (mut db, source, destination, section) = fixture();
        db.create_section(
            "dev",
            &destination.id,
            SectionInput {
                title: section.title.clone(),
                description: String::new(),
            },
        )
        .unwrap();
        let root = add(&mut db, &source.id, None, "Root");
        let child = add(&mut db, &source.id, Some(root.id.clone()), "Child");
        let grand = add(
            &mut db,
            &source.id,
            Some(child.id.clone()),
            "Hidden grandchild",
        );
        db.set_section("dev", &root.id, root.revision, Some(section.id.clone()))
            .unwrap();
        let current = raw_task(&db.conn, &root.id).unwrap();
        db.set_task_tags(
            "dev",
            &root.id,
            current.revision,
            vec![crate::taxonomy::TagSelector::Name("Urgent".into())],
        )
        .unwrap();
        let destination_tag = db.create_tag("dev", &destination.id, "Urgent").unwrap();
        let current = raw_task(&db.conn, &grand.id).unwrap();
        db.trash("dev", &grand.id, current.revision).unwrap();
        db.conn.execute("INSERT INTO comments(id,task_id,author_id,body) VALUES('section-move-comment',?1,'dev','Synthetic retained comment')",[&root.id]).unwrap();
        db.conn.execute("INSERT INTO upload_intents(id,task_id,creator_id,filename,size,sha256,staging_key,object_key,expires_at) VALUES('section-move-intent',?1,'dev','synthetic.txt',1,?2,'synthetic/staging','synthetic/immutable',unixepoch()+3600)",params![root.id,"0".repeat(64)]).unwrap();
        db.conn.execute("INSERT INTO attachments(id,intent_id,task_id,creator_id,filename,size,sha256,object_key) VALUES('section-move-file','section-move-intent',?1,'dev','synthetic.txt',1,?2,'synthetic/immutable')",params![root.id,"0".repeat(64)]).unwrap();
        db.conn
            .execute(
                "INSERT INTO memberships VALUES(?1,'other')",
                [&destination.id],
            )
            .unwrap();
        let preview = db
            .section_move_preview("dev", &section.id, &destination.id)
            .unwrap();
        assert_eq!(preview.affected_task_count, 3);
        assert_eq!(preview.assigned_task_count, 3);
        assert!(preview.audience_broadens);
        let before_history: i64 = db
            .conn
            .query_row("SELECT count(*) FROM history", [], |r| r.get(0))
            .unwrap();
        assert!(matches!(
            db.move_section("dev", &section.id, input(&preview, false)),
            Err(Error::Conflict(_))
        ));
        assert_eq!(
            db.conn
                .query_row("SELECT count(*) FROM history", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            before_history
        );
        let moved = db
            .move_section("dev", &section.id, input(&preview, true))
            .unwrap();
        assert_eq!(moved.id, section.id);
        assert_eq!(moved.list_id, destination.id);
        assert_eq!(moved.description, section.description);
        assert_eq!(
            db.entity_slugs("dev").unwrap().sections[&section.id],
            "next-steps-2"
        );
        for task in [&root, &child, &grand] {
            let after = raw_task(&db.conn, &task.id).unwrap();
            assert_eq!(after.list_id, destination.id);
            assert_eq!(after.reference, task.reference);
            assert_eq!(after.parent_id, task.parent_id);
            let location: String = db
                .conn
                .query_row(
                    "SELECT section_id FROM task_locations WHERE task_id=?1",
                    [&task.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(location, section.id);
        }
        assert_eq!(db.trash_tasks("dev").unwrap()[0].id, grand.id);
        assert_eq!(
            db.task_tags("dev", &root.id).unwrap()[0].id,
            destination_tag.id
        );
        let retained:(String,String)=db.conn.query_row("SELECT c.task_id,a.object_key FROM comments c JOIN attachments a ON a.task_id=c.task_id WHERE c.id='section-move-comment'",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(retained, (root.id, "synthetic/immutable".into()));
        let events: i64 = db
            .conn
            .query_row(
                "SELECT count(*) FROM history WHERE json_extract(event,'$.kind')='section_move'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(events, 3);
        assert!(
            db.conn
                .query_row(
                    "SELECT NOT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
                    [],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap()
        );
    }
    #[test]
    fn edited_descendants_acl_changes_and_lost_permission_invalidate_preview() {
        let (mut db, source, destination, section) = fixture();
        let root = add(&mut db, &source.id, None, "Root");
        let child = add(&mut db, &source.id, Some(root.id.clone()), "Child");
        db.set_section("dev", &root.id, root.revision, Some(section.id.clone()))
            .unwrap();
        let preview = db
            .section_move_preview("dev", &section.id, &destination.id)
            .unwrap();
        let current = raw_task(&db.conn, &child.id).unwrap();
        db.update(
            "dev",
            &child.id,
            Change {
                revision: current.revision,
                title: Some("Changed after preview".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(
            db.move_section("dev", &section.id, input(&preview, true)),
            Err(Error::Conflict(_))
        ));
        let preview = db
            .section_move_preview("dev", &section.id, &destination.id)
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO memberships VALUES(?1,'other')",
                [&destination.id],
            )
            .unwrap();
        assert!(matches!(
            db.move_section("dev", &section.id, input(&preview, true)),
            Err(Error::Conflict(_))
        ));
        assert!(matches!(
            db.section_move_preview("other", &section.id, &destination.id),
            Err(Error::Forbidden)
        ));
        let preview = db
            .section_move_preview("dev", &section.id, &destination.id)
            .unwrap();
        db.conn.execute("DELETE FROM workspace_members WHERE workspace_id=(SELECT workspace_id FROM lists WHERE id=?1) AND user_id='dev'",[&destination.id]).unwrap();
        assert!(matches!(
            db.move_section("dev", &section.id, input(&preview, true)),
            Err(Error::Forbidden)
        ));
        assert_eq!(db.sections("dev", &source.id).unwrap()[0].id, section.id);
    }
    #[test]
    fn later_root_failure_rolls_back_all_earlier_common_moves() {
        let (mut db, source, destination, section) = fixture();
        let a = add(&mut db, &source.id, None, "First");
        let b = add(&mut db, &source.id, None, "Second");
        for t in [&a, &b] {
            db.set_section("dev", &t.id, t.revision, Some(section.id.clone()))
                .unwrap();
        }
        let preview = db
            .section_move_preview("dev", &section.id, &destination.id)
            .unwrap();
        let before: i64 = db
            .conn
            .query_row("SELECT count(*) FROM history", [], |r| r.get(0))
            .unwrap();
        db.conn.execute_batch(&format!("CREATE TRIGGER synthetic_section_move_failure BEFORE UPDATE OF list_id ON tasks WHEN new.id='{}' AND new.list_id='{}' BEGIN SELECT RAISE(ABORT,'synthetic later-root failure'); END;",b.id,destination.id)).unwrap();
        assert!(matches!(
            db.move_section("dev", &section.id, input(&preview, false)),
            Err(Error::Database(_))
        ));
        for t in [&a, &b] {
            assert_eq!(raw_task(&db.conn, &t.id).unwrap().list_id, source.id);
        }
        assert_eq!(
            section_raw(&db.conn, &section.id).unwrap().list_id,
            source.id
        );
        assert_eq!(
            db.conn
                .query_row("SELECT count(*) FROM history", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            before
        );
        assert_eq!(
            db.entity_slugs("dev").unwrap().sections[&section.id],
            "next-steps"
        );
        assert!(db.conn.is_autocommit());
    }
    #[test]
    fn empty_section_moves_and_trashed_standalone_roots_fail_explicitly() {
        let (mut db, source, destination, section) = fixture();
        assert!(
            db.section_move_preview("dev", &section.id, &source.id)
                .is_err()
        );
        let p = db
            .section_move_preview("dev", &section.id, &destination.id)
            .unwrap();
        assert_eq!(p.affected_task_count, 0);
        db.move_section("dev", &section.id, input(&p, false))
            .unwrap();
        let root = add(&mut db, &destination.id, None, "Trashed standalone");
        db.set_section("dev", &root.id, root.revision, Some(section.id.clone()))
            .unwrap();
        let current = raw_task(&db.conn, &root.id).unwrap();
        db.trash("dev", &root.id, current.revision).unwrap();
        assert!(matches!(
            db.section_move_preview("dev", &section.id, &source.id),
            Err(Error::Conflict(_))
        ));
    }
    #[test]
    fn oversized_groups_and_list_scoped_credentials_fail_before_writing() {
        let (mut db, source, destination, section) = fixture();
        db.conn.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,list_ids,expires_at) VALUES('section-source-only','dev','synthetic-section-hash','https://synthetic.invalid/api/v1','[\"write\"]',?1,unixepoch()+3600)",[serde_json::to_string(&vec![&source.id]).unwrap()]).unwrap();
        assert!(matches!(
            db.section_move_preview(
                "credential:section-source-only",
                &section.id,
                &destination.id
            ),
            Err(Error::Forbidden)
        ));
        let tx = db.conn.transaction().unwrap();
        for i in 0..501 {
            let id = format!("section-bounded-{i}");
            tx.execute("INSERT INTO tasks(id,reference,list_id,title,position) VALUES(?1,?1,?2,'Synthetic bounded task',?3)",params![id,source.id,i]).unwrap();
            tx.execute(
                "INSERT INTO task_locations VALUES(?1,?2)",
                params![id, section.id],
            )
            .unwrap();
        }
        tx.commit().unwrap();
        assert!(matches!(
            db.section_move_preview("dev", &section.id, &destination.id),
            Err(Error::Invalid(_))
        ));
        assert_eq!(
            section_raw(&db.conn, &section.id).unwrap().list_id,
            source.id
        );
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn archived_workspace_delete_requires_owner_revision_and_exact_confirmation_and_revokes_lists()
    {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let w = db
            .create_workspace(
                "dev",
                WorkspaceInput {
                    title: "Retained project".into(),
                    description: "Keep this brief".into(),
                },
            )
            .unwrap();
        let l = db
            .create_list(
                "dev",
                CreateList {
                    workspace_id: w.id.clone(),
                    title: "Private list".into(),
                    description: String::new(),
                    restricted: false,
                },
            )
            .unwrap();
        let revision = db.workspace("dev", &w.id).unwrap().revision;
        assert!(
            db.change_workspace_lifecycle("dev", &w.id, revision, "delete", Some(&w.title))
                .is_err()
        );
        assert!(
            db.change_workspace_lifecycle("other", &w.id, revision, "archive", None)
                .is_err()
        );
        db.change_workspace_lifecycle("dev", &w.id, revision, "archive", None)
            .unwrap();
        assert!(db.list_permission("dev", &l.id).is_ok());
        assert!(
            db.change_workspace_lifecycle("dev", &w.id, revision, "restore", None)
                .is_err()
        );
        let revision = db.workspace("dev", &w.id).unwrap().revision;
        db.change_workspace_lifecycle("dev", &w.id, revision, "restore", None)
            .unwrap();
        let revision = db.workspace("dev", &w.id).unwrap().revision;
        db.change_workspace_lifecycle("dev", &w.id, revision, "archive", None)
            .unwrap();
        let revision = db.workspace("dev", &w.id).unwrap().revision;
        assert!(
            db.change_workspace_lifecycle("dev", &w.id, revision, "delete", Some("wrong"))
                .is_err()
        );
        db.change_workspace_lifecycle("dev", &w.id, revision, "delete", Some(&w.title))
            .unwrap();
        assert!(db.workspace("dev", &w.id).is_err());
        assert_eq!(db.list_permission("dev", &l.id).unwrap(), 0);
        assert!(!db.workspaces("dev").unwrap().iter().any(|x| x.id == w.id));
        assert_eq!(
            db.conn
                .query_row(
                    "SELECT description FROM workspaces WHERE id=?1",
                    [&w.id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "Keep this brief"
        );
        assert_eq!(
            db.conn
                .query_row("SELECT count(*) FROM lists WHERE id=?1", [l.id], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn native_hex_colors_and_short_references_survive_rename() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let r = db.list_reference("dev", "groceries").unwrap();
        assert_eq!(r.len(), 7);
        assert!(db.list_reference("other", "groceries").is_err());
        let rev = db.list_info("dev", "groceries").unwrap().list.revision;
        db.set_list_color("dev", "groceries", rev, Some("#1a2B3c".into()))
            .unwrap();
        assert_eq!(
            db.list_color("dev", "groceries").unwrap().as_deref(),
            Some("#1a2B3c")
        );
        let rev = db.list_info("dev", "groceries").unwrap().list.revision;
        db.update_list(
            "dev",
            "groceries",
            ListChange {
                color: None,
                revision: rev,
                title: Some("New groceries".into()),
                description: None,
                archived: None,
            },
        )
        .unwrap();
        assert_eq!(db.list_reference("dev", "groceries").unwrap(), r);
        assert_eq!(
            db.resolve_destination("dev", Some(&format!("#{r}")), None, false)
                .unwrap()
                .destination
                .unwrap()
                .id,
            "groceries"
        );
        assert_eq!(
            db.resolve_destination("other", Some(&format!("#{r}")), None, false)
                .unwrap()
                .status,
            "missing"
        );
    }
}
