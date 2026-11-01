//! Persisted display slugs. Stable entity IDs remain canonical URL identifiers.
use crate::core::{Error, Store};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct EntitySlugs {
    pub workspaces: HashMap<String, String>,
    pub lists: HashMap<String, String>,
    pub sections: HashMap<String, String>,
}
/// Unicode lowercase letters/numbers separated by one ASCII hyphen. No transliteration.
pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for c in title.to_lowercase().chars() {
        if c.is_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(c);
            separator = false;
        } else {
            separator = true;
        }
    }
    let trimmed: String = slug.chars().take(64).collect();
    let trimmed = trimmed.trim_end_matches('-');
    if trimmed.is_empty() {
        "untitled".into()
    } else {
        trimmed.into()
    }
}
fn persist(
    c: &Connection,
    kind: &str,
    id: &str,
    scope: &str,
    title: &str,
) -> Result<String, Error> {
    let old: Option<(String, String, String)> = c
        .query_row(
            "SELECT scope,slug,source_title FROM entity_slugs WHERE kind=?1 AND entity_id=?2",
            params![kind, id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((old_scope, slug, old_title)) = &old
        && old_scope == scope
        && old_title == title
    {
        return Ok(slug.clone());
    }
    let base = slugify(title);
    for suffix in 1..=100_000 {
        let slug = if suffix == 1 {
            base.clone()
        } else {
            format!("{base}-{suffix}")
        };
        let occupied:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM entity_slugs WHERE kind=?1 AND scope=?2 AND slug=?3 AND entity_id!=?4)",params![kind,scope,slug,id],|r|r.get(0))?;
        if !occupied {
            c.execute("INSERT INTO entity_slugs(kind,entity_id,scope,slug,source_title) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(kind,entity_id) DO UPDATE SET scope=excluded.scope,slug=excluded.slug,source_title=excluded.source_title",params![kind,id,scope,slug,title])?;
            return Ok(slug);
        }
    }
    Err(Error::Conflict(
        "too many identical names in this scope; choose a different title".into(),
    ))
}
/// Caller owns the transaction, including membership insertion on initial workspace creation.
pub(crate) fn workspace(c: &Connection, id: &str) -> Result<String, Error> {
    let (title,owner):(String,String)=c.query_row("SELECT w.title,coalesce((SELECT substr(scope,9) FROM entity_slugs WHERE kind='workspace' AND entity_id=w.id),(SELECT user_id FROM workspace_members WHERE workspace_id=w.id AND role='owner' ORDER BY user_id LIMIT 1),w.id) FROM workspaces w WHERE w.id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
    persist(c, "workspace", id, &format!("account:{owner}"), &title)
}
pub(crate) fn list(c: &Connection, id: &str) -> Result<String, Error> {
    let (title,scope):(String,String)=c.query_row("SELECT l.title,CASE WHEN workspace_id IS NOT NULL THEN 'workspace:'||workspace_id ELSE coalesce((SELECT scope FROM entity_slugs WHERE kind='list' AND entity_id=l.id AND scope LIKE 'account:%'),'account:'||coalesce((SELECT user_id FROM memberships WHERE list_id=l.id ORDER BY user_id LIMIT 1),l.id)) END FROM lists l WHERE l.id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
    persist(c, "list", id, &scope, &title)
}
pub(crate) fn section(c: &Connection, id: &str) -> Result<String, Error> {
    let (title, list): (String, String) = c.query_row(
        "SELECT title,list_id FROM sections WHERE id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    persist(c, "section", id, &format!("list:{list}"), &title)
}
/// Incremental deterministic backfill for migration/import/seed transactions only.
/// This is never invoked by a read operation and never changes raw titles or revisions.
pub(crate) fn backfill(c: &Connection) -> Result<(), Error> {
    for (table, kind) in [
        ("workspaces", "workspace"),
        ("lists", "list"),
        ("sections", "section"),
    ] {
        let mut stmt=c.prepare(&format!("SELECT t.id FROM {table} t LEFT JOIN entity_slugs s ON s.kind=?1 AND s.entity_id=t.id WHERE s.entity_id IS NULL OR s.source_title!=t.title {} ORDER BY t.id",if kind=="list" {"OR (t.workspace_id IS NOT NULL AND s.scope!='workspace:'||t.workspace_id) OR (t.workspace_id IS NULL AND s.scope NOT LIKE 'account:%')"} else if kind=="section" {"OR s.scope!='list:'||t.list_id"} else {""}))?;
        let ids = stmt
            .query_map([kind], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for id in ids {
            match kind {
                "workspace" => {
                    workspace(c, &id)?;
                }
                "list" => {
                    list(c, &id)?;
                }
                _ => {
                    section(c, &id)?;
                }
            }
        }
    }
    Ok(())
}
impl Store {
    /// Only currently visible entities; direct list guests learn no workspace names/IDs.
    pub fn entity_slugs(&self, actor: &str) -> Result<EntitySlugs, Error> {
        let tx = self.conn.unchecked_transaction()?;
        let mut result = EntitySlugs::default();
        for (kind, sql) in [
            (
                "workspace",
                "SELECT s.entity_id,s.slug FROM entity_slugs s JOIN actor_workspace_access a ON a.workspace_id=s.entity_id WHERE s.kind='workspace' AND a.actor_id=?1",
            ),
            (
                "list",
                "SELECT s.entity_id,s.slug FROM entity_slugs s JOIN actor_list_access a ON a.list_id=s.entity_id WHERE s.kind='list' AND a.actor_id=?1",
            ),
            (
                "section",
                "SELECT s.entity_id,s.slug FROM entity_slugs s JOIN sections x ON x.id=s.entity_id JOIN actor_list_access a ON a.list_id=x.list_id WHERE s.kind='section' AND a.actor_id=?1 AND x.archived=0",
            ),
        ] {
            let mut q = tx.prepare(sql)?;
            let pairs = q
                .query_map([actor], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<Result<HashMap<_, _>, _>>()?;
            match kind {
                "workspace" => result.workspaces = pairs,
                "list" => result.lists = pairs,
                _ => result.sections = pairs,
            }
        }
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{
        CreateList, ListChange, ListWorkspaceMoveInput, SectionChange, SectionInput,
        WorkspaceChange, WorkspaceInput,
    };
    fn store() -> Store {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        s
    }
    fn workspace(s: &mut Store, title: &str) -> crate::workspace::Workspace {
        s.create_workspace(
            "dev",
            WorkspaceInput {
                title: title.into(),
                description: String::new(),
            },
        )
        .unwrap()
    }
    fn list(s: &mut Store, w: &str, title: &str) -> crate::core::List {
        s.create_list(
            "dev",
            CreateList {
                workspace_id: w.into(),
                title: title.into(),
                description: String::new(),
                restricted: false,
            },
        )
        .unwrap()
    }
    #[test]
    fn canonical_unicode_and_bounded_fallback() {
        assert_eq!(slugify("  #My  NEXT_Project! "), "my-next-project");
        assert_eq!(slugify("CAFÉ 東京 ÉTÉ"), "café-東京-été");
        assert_eq!(slugify("💡 / ___"), "untitled");
        assert_eq!(slugify(&"X".repeat(500)).chars().count(), 64);
    }
    #[test]
    fn scoped_collisions_and_renames_preserve_titles_ids_and_revisions() {
        let mut s = store();
        let w = workspace(&mut s, "My Projects");
        let w2 = workspace(&mut s, "My Projects");
        let l = list(&mut s, &w.id, "Next Steps");
        let l2 = list(&mut s, &w.id, "Next Steps");
        let l3 = list(&mut s, &w2.id, "Next Steps");
        let sec = s
            .create_section(
                "dev",
                &l.id,
                SectionInput {
                    title: "Important Things".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let sec2 = s
            .create_section(
                "dev",
                &l.id,
                SectionInput {
                    title: "Important Things".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let slugs = s.entity_slugs("dev").unwrap();
        assert_eq!(slugs.workspaces[&w.id], "my-projects");
        assert_eq!(slugs.workspaces[&w2.id], "my-projects-2");
        assert_eq!(slugs.lists[&l.id], "next-steps");
        assert_eq!(slugs.lists[&l2.id], "next-steps-2");
        assert_eq!(slugs.lists[&l3.id], "next-steps");
        assert_eq!(slugs.sections[&sec.id], "important-things");
        assert_eq!(slugs.sections[&sec2.id], "important-things-2");
        let l = s
            .update_list(
                "dev",
                &l.id,
                ListChange {
                    color: None,
                    revision: l.revision + 2,
                    title: Some("Café Reading".into()),
                    description: None,
                    archived: None,
                },
            )
            .unwrap();
        assert_eq!(l.title, "Café Reading");
        assert_eq!(s.entity_slugs("dev").unwrap().lists[&l.id], "café-reading");
        assert!(
            s.update_list(
                "dev",
                &l.id,
                ListChange {
                    color: None,
                    revision: 1,
                    title: Some("bad stale name".into()),
                    description: None,
                    archived: None
                }
            )
            .is_err()
        );
        assert_eq!(s.entity_slugs("dev").unwrap().lists[&l.id], "café-reading");
        let changed = s
            .update_section(
                "dev",
                &sec.id,
                SectionChange {
                    revision: sec.revision,
                    title: Some("Read Now".into()),
                    description: None,
                },
            )
            .unwrap();
        assert_eq!(changed.id, sec.id);
        assert_eq!(s.entity_slugs("dev").unwrap().sections[&sec.id], "read-now");
        let current = s.workspace("dev", &w.id).unwrap();
        s.update_workspace(
            "dev",
            &w.id,
            WorkspaceChange {
                revision: current.revision,
                title: Some("Personal Space".into()),
                description: None,
            },
        )
        .unwrap();
        assert_eq!(
            s.entity_slugs("dev").unwrap().workspaces[&w.id],
            "personal-space"
        );
    }
    #[test]
    fn list_move_reallocates_destination_slug_atomically() {
        let mut s = store();
        let from = workspace(&mut s, "From");
        let to = workspace(&mut s, "To");
        let a = list(&mut s, &from.id, "Next");
        let b = list(&mut s, &to.id, "Next");
        let p = s.list_workspace_move_preview("dev", &a.id, &to.id).unwrap();
        s.move_list_workspace(
            "dev",
            &a.id,
            ListWorkspaceMoveInput {
                destination_workspace_id: to.id.clone(),
                list_revision: p.list_revision,
                source_workspace_revision: p.source_workspace_revision,
                destination_workspace_revision: p.destination_workspace_revision,
                policy_hash: p.policy_hash,
                acknowledge_access_change: false,
            },
        )
        .unwrap();
        let map = s.entity_slugs("dev").unwrap();
        assert_eq!(map.lists[&a.id], "next-2");
        assert_eq!(map.lists[&b.id], "next");
        let scope: String = s
            .conn
            .query_row(
                "SELECT scope FROM entity_slugs WHERE kind='list' AND entity_id=?1",
                [&a.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(scope, format!("workspace:{}", to.id));
    }
    #[test]
    fn direct_guest_and_revoked_access_never_expose_workspace_slugs() {
        let mut s = store();
        let w = workspace(&mut s, "Secret Workspace");
        let l = list(&mut s, &w.id, "Shared Reading");
        let section = s
            .create_section(
                "dev",
                &l.id,
                SectionInput {
                    title: "Public To Guest".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        s.conn
            .execute("INSERT INTO memberships VALUES(?1,'other')", [&l.id])
            .unwrap();
        let map = s.entity_slugs("other").unwrap();
        assert!(map.workspaces.is_empty());
        assert_eq!(map.lists[&l.id], "shared-reading");
        assert_eq!(map.sections[&section.id], "public-to-guest");
        s.conn
            .execute(
                "DELETE FROM memberships WHERE list_id=?1 AND user_id='other'",
                [&l.id],
            )
            .unwrap();
        let map = s.entity_slugs("other").unwrap();
        assert!(!map.lists.contains_key(&l.id));
        assert!(!map.sections.contains_key(&section.id));
    }
    #[test]
    fn migration_backfills_deterministically_and_reopens_without_renaming() {
        let dir = std::env::temp_dir().join(format!("todoer-slugs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("synthetic.db");
        {
            let mut s = Store::open(path.to_str().unwrap()).unwrap();
            s.seed_dev().unwrap();
            s.conn.execute_batch("INSERT INTO lists(id,title) VALUES('backfill-a','Same Name'),('backfill-b','Same Name');INSERT INTO memberships VALUES('backfill-a','dev'),('backfill-b','dev');DROP TRIGGER workspace_slug_delete;DROP TRIGGER list_slug_delete;DROP TRIGGER section_slug_delete;DROP TABLE entity_slugs;DELETE FROM schema_migrations WHERE version=22;").unwrap();
            let mut copied = Store::memory().unwrap();
            crate::transfer::copy_destination_for_dry_run(&s.conn, &mut copied).unwrap();
            assert_eq!(
                copied.entity_slugs("dev").unwrap().lists["backfill-b"],
                "same-name-2"
            );
            let original_unchanged: bool = s
                .conn
                .query_row(
                    "SELECT NOT EXISTS(SELECT 1 FROM sqlite_master WHERE name='entity_slugs')",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(original_unchanged);
        }
        for _ in 0..2 {
            let s = Store::open(path.to_str().unwrap()).unwrap();
            let map = s.entity_slugs("dev").unwrap();
            assert_eq!(map.lists["backfill-a"], "same-name");
            assert_eq!(map.lists["backfill-b"], "same-name-2");
            let title: String = s
                .conn
                .query_row("SELECT title FROM lists WHERE id='backfill-b'", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(title, "Same Name");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
