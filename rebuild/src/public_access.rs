//! Anonymous read projections are separate from authenticated APIs and ACLs.
use crate::core::{Error, Store, access_rank};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
#[derive(Serialize)]
pub struct PublicState {
    pub enabled: bool,
    pub inherited: bool,
    pub effective: bool,
    pub revision: i64,
    pub can_manage: bool,
    pub reference: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicChange {
    pub enabled: bool,
    pub revision: i64,
}
#[derive(Serialize)]
pub struct PublicItem {
    pub kind: String,
    pub reference: String,
    pub title: String,
    pub description: String,
    pub complete: bool,
    pub depth: usize,
}
#[derive(Serialize)]
pub struct PublicDocument {
    pub kind: String,
    pub reference: String,
    pub title: String,
    pub description: String,
    pub complete: bool,
    pub items: Vec<PublicItem>,
    pub has_more: bool,
}
pub(crate) fn list_public(c: &rusqlite::Connection, id: &str) -> Result<bool, Error> {
    Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM lists l LEFT JOIN workspace_lifecycle x ON x.workspace_id=l.workspace_id WHERE l.id=?1 AND l.archived=0 AND coalesce(x.archived,0)=0 AND x.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM user_inboxes i WHERE i.list_id=l.id) AND (EXISTS(SELECT 1 FROM public_lists p WHERE p.list_id=l.id AND p.enabled=1) OR (l.restricted=0 AND EXISTS(SELECT 1 FROM public_spaces p WHERE p.workspace_id=l.workspace_id AND p.enabled=1))))",[id],|r|r.get(0))?)
}
impl Store {
    pub fn is_public_list(&self, id: &str) -> Result<bool, Error> {
        list_public(&self.conn, id)
    }

    fn public_scope(
        &self,
        kind: &str,
        id: &str,
    ) -> Result<(&'static str, &'static str, String), Error> {
        let (table, column, entity) = match kind {
            "space" => ("public_spaces", "workspace_id", "workspace"),
            "list" => ("public_lists", "list_id", "list"),
            _ => return Err(Error::Invalid("Choose space or list".into())),
        };
        Ok((
            table,
            column,
            crate::defaults::resolve(&self.conn, entity, id)?,
        ))
    }
    pub fn public_state(&self, actor: &str, kind: &str, id: &str) -> Result<PublicState, Error> {
        let (table, column, id) = self.public_scope(kind, id)?;
        let rank = if kind == "space" {
            self.workspace_permission(actor, &id)?
        } else {
            access_rank(&self.conn, actor, &id, 1)?;
            self.list_permission(actor, &id)?
        };
        if rank < 1 { return Err(Error::Forbidden); }
        let state: Option<(bool, i64)> = self
            .conn
            .query_row(
                &format!("SELECT enabled,revision FROM {table} WHERE {column}=?1"),
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (enabled, revision) = state.unwrap_or((false, 0));
        let inherited=kind=="list" && self.conn.query_row("SELECT EXISTS(SELECT 1 FROM lists l JOIN public_spaces p ON p.workspace_id=l.workspace_id AND p.enabled=1 WHERE l.id=?1 AND l.restricted=0)",[&id],|r|r.get::<_,bool>(0))?;
        let reference = crate::defaults::public_id(
            &self.conn,
            if kind == "space" { "workspace" } else { "list" },
            &id,
        )?;
        Ok(PublicState {
            enabled,
            inherited,
            effective: if kind == "space" {
                enabled
            } else {
                self.is_public_list(&id)?
            },
            revision,
            can_manage: rank >= 3,
            reference,
        })
    }
    pub fn set_public_access(
        &mut self,
        actor: &str,
        kind: &str,
        id: &str,
        input: PublicChange,
    ) -> Result<PublicState, Error> {
        let mut write = crate::agent_ops::StoreWrite::new(self)?;
        let state = write.public_state(actor, kind, id)?;
        if !state.can_manage {
            return Err(Error::Forbidden);
        }
        if input.revision != state.revision {
            return Err(Error::Conflict(
                "Public sharing changed. Reload before trying again.".into(),
            ));
        }
        let (table, column, id) = write.public_scope(kind, id)?;
        write.conn.execute(&format!("INSERT INTO {table}({column},enabled,revision) VALUES(?1,?2,1) ON CONFLICT({column}) DO UPDATE SET enabled=excluded.enabled,revision=revision+1"),params![id,input.enabled])?;
        let user = crate::core::actor_identity(&write.conn, actor)?.0;
        write.conn.execute(
            "INSERT INTO public_access_events(kind,entity_id,actor_id,enabled) VALUES(?1,?2,?3,?4)",
            params![kind, id, user, input.enabled],
        )?;
        let result = write.public_state(actor, kind, &id)?;
        write.commit()?;
        Ok(result)
    }
    pub fn public_document(
        &self,
        kind: &str,
        raw: &str,
        page: usize,
    ) -> Result<PublicDocument, Error> {
        let entity = if kind == "space" { "workspace" } else { kind };
        let id = crate::defaults::resolve(&self.conn, entity, raw)?;
        let (title,description,complete)=match kind {
            "space"=>self.conn.query_row("SELECT w.title,w.description,0 FROM workspaces w JOIN public_spaces p ON p.workspace_id=w.id AND p.enabled=1 LEFT JOIN workspace_lifecycle x ON x.workspace_id=w.id WHERE w.id=?1 AND coalesce(x.archived,0)=0 AND x.deleted_at IS NULL",[&id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?))).optional()?.ok_or(Error::NotFound)?,
            "list"=>{if !self.is_public_list(&id)?{return Err(Error::NotFound)}self.conn.query_row("SELECT title,description,0 FROM lists WHERE id=?1",[&id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?},
            "task"=>{let t=crate::core::task(&self.conn,&id)?;if !self.is_public_list(&t.list_id)?{return Err(Error::NotFound)}(t.title,t.description,t.complete)},
            _=>return Err(Error::NotFound)
        };
        let reference = crate::defaults::public_id(&self.conn, entity, &id)?;
        let mut items = Vec::new();
        let offset = ((page.clamp(1, 10000) - 1) * 50) as i64;
        if kind == "space" {
            let mut q=self.conn.prepare("SELECT id,title,description FROM lists WHERE workspace_id=?1 AND archived=0 AND restricted=0 ORDER BY position,id LIMIT 51 OFFSET ?2")?;
            for r in q.query_map(params![id, offset], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })? {
                let (id, title, description) = r?;
                if self.is_public_list(&id)? {
                    items.push(PublicItem {
                        kind: "list".into(),
                        reference: crate::defaults::public_id(&self.conn, "list", &id)?,
                        title,
                        description,
                        complete: false,
                        depth: 0,
                    });
                }
            }
        } else {
            let sql = if kind == "task" {
                "WITH RECURSIVE tree(id,depth,path) AS (SELECT id,0,printf('%012d',position)||id FROM tasks WHERE id=?1 AND deleted_at IS NULL UNION ALL SELECT t.id,tree.depth+1,tree.path||'/'||printf('%012d',t.position)||t.id FROM tasks t JOIN tree ON t.parent_id=tree.id WHERE t.deleted_at IS NULL AND tree.depth<256) SELECT t.id,t.title,t.description,t.complete,tree.depth FROM tree JOIN tasks t ON t.id=tree.id WHERE tree.depth>0 ORDER BY tree.path LIMIT 51 OFFSET ?2"
            } else {
                "WITH RECURSIVE tree(id,depth,path) AS (SELECT id,0,printf('%012d',position)||id FROM tasks WHERE list_id=?1 AND parent_id IS NULL AND deleted_at IS NULL UNION ALL SELECT t.id,tree.depth+1,tree.path||'/'||printf('%012d',t.position)||t.id FROM tasks t JOIN tree ON t.parent_id=tree.id WHERE t.deleted_at IS NULL AND tree.depth<256) SELECT t.id,t.title,t.description,t.complete,tree.depth FROM tree JOIN tasks t ON t.id=tree.id ORDER BY tree.path LIMIT 51 OFFSET ?2"
            };
            let mut q = self.conn.prepare(sql)?;
            for r in q.query_map(params![id, offset], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, i64>(4)? as usize,
                ))
            })? {
                let (id, title, description, complete, depth) = r?;
                items.push(PublicItem {
                    kind: "task".into(),
                    reference: crate::defaults::public_id(&self.conn, "task", &id)?,
                    title,
                    description,
                    complete,
                    depth,
                });
            }
        }
        let has_more = items.len() > 50;
        items.truncate(50);
        Ok(PublicDocument {
            kind: kind.into(),
            reference,
            title,
            description,
            complete,
            items,
            has_more,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Capture, NewTask};
    use crate::workspace::WorkspaceInput;
    fn fixture() -> (Store, String, String, String) {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let space = db
            .create_workspace(
                "dev",
                WorkspaceInput {
                    title: "Shared garden".into(),
                    description: "A small shared plan".into(),
                },
            )
            .unwrap();
        let list = db.default_list("dev", &space.id).unwrap();
        let t = db
            .capture(
                "dev",
                "public-fixture",
                Capture {
                    list_id: list.clone(),
                    tasks: vec![NewTask {
                        title: "Plant a tiny forest".into(),
                        description: "Bring **friends** and a shovel.".into(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        (db, space.id, list, t.id)
    }
    #[test]
    fn public_access_is_explicit_read_only_revocable_and_respects_restrictions() {
        let (mut db, space, list, task) = fixture();
        assert!(db.public_document("task", &task, 1).is_err());
        assert!(
            db.set_public_access(
                "other",
                "space",
                &space,
                PublicChange {
                    enabled: true,
                    revision: 0
                }
            )
            .is_err()
        );
        db.set_public_access(
            "dev",
            "space",
            &space,
            PublicChange {
                enabled: true,
                revision: 0,
            },
        )
        .unwrap();
        assert!(db.public_document("task", &task, 1).is_ok());
        assert!(!db.public_state("dev", "list", &list).unwrap().enabled);
        assert!(db.public_state("dev", "list", &list).unwrap().inherited);
        db.conn
            .execute("UPDATE lists SET restricted=1 WHERE id=?1", [&list])
            .unwrap();
        assert!(db.public_document("task", &task, 1).is_err());
        assert!(
            db.public_document("space", &space, 1)
                .unwrap()
                .items
                .is_empty()
        );
        db.set_public_access(
            "dev",
            "list",
            &list,
            PublicChange {
                enabled: true,
                revision: 0,
            },
        )
        .unwrap();
        assert!(db.public_document("task", &task, 1).is_ok());
        assert!(
            db.set_public_access(
                "dev",
                "list",
                &list,
                PublicChange {
                    enabled: false,
                    revision: 0
                }
            )
            .is_err()
        );
        db.set_public_access(
            "dev",
            "list",
            &list,
            PublicChange {
                enabled: false,
                revision: 1,
            },
        )
        .unwrap();
        assert!(db.public_document("task", &task, 1).is_err());
        db.conn
            .execute("UPDATE lists SET restricted=0 WHERE id=?1", [&list])
            .unwrap();
        db.set_public_access(
            "dev",
            "space",
            &space,
            PublicChange {
                enabled: false,
                revision: 1,
            },
        )
        .unwrap();
        assert!(db.public_document("task", &task, 1).is_err());
    }
    #[test]
    fn archived_and_deleted_entities_never_have_public_projections() {
        let (mut db, space, list, task) = fixture();
        db.set_public_access(
            "dev",
            "space",
            &space,
            PublicChange {
                enabled: true,
                revision: 0,
            },
        )
        .unwrap();
        db.conn
            .execute("UPDATE lists SET archived=1 WHERE id=?1", [&list])
            .unwrap();
        assert!(db.public_document("task", &task, 1).is_err());
        db.conn
            .execute("UPDATE lists SET archived=0 WHERE id=?1", [&list])
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO workspace_lifecycle(workspace_id,archived) VALUES(?1,1)",
                [&space],
            )
            .unwrap();
        assert!(db.public_document("space", &space, 1).is_err());
        assert!(db.public_document("task", &task, 1).is_err());
    }
    #[test]
    fn moving_into_public_list_requires_audience_confirmation() {
        let (mut db, space, list, task) = fixture();
        db.set_public_access(
            "dev",
            "space",
            &space,
            PublicChange {
                enabled: true,
                revision: 0,
            },
        )
        .unwrap();
        let inbox = db.personal_inbox("dev").unwrap();
        let source = db
            .capture(
                "dev",
                "private-move",
                Capture {
                    list_id: inbox,
                    tasks: vec![NewTask {
                        title: "Private before confirmation".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let p = db.move_preview("dev", &source.id, &list).unwrap();
        assert_eq!(p["destination_public"], true);
        assert_eq!(p["audience_broadens"], true);
        assert!(
            db.move_subtree("dev", &source.id, &list, source.revision, false)
                .is_err()
        );
        db.move_subtree("dev", &source.id, &list, source.revision, true)
            .unwrap();
        assert!(db.public_document("task", &source.id, 1).is_ok());
        assert!(db.public_document("task", &task, 1).is_ok());
    }
}
