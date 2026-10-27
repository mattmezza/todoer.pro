//! Transactional first-capture destinations and persistent public aliases.
use crate::core::Error;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rusqlite::{Connection, OptionalExtension, params};

pub fn short_id(prefix: &str) -> String {
    format!(
        "{prefix}_{}",
        URL_SAFE_NO_PAD.encode(&uuid::Uuid::new_v4().as_bytes()[..9])
    )
}
pub fn entity_id(c: &Connection, table: &str, prefix: &str) -> Result<String, Error> {
    // Table names are static call-site constants, never request input.
    loop {
        let id = short_id(prefix);
        if !c.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1 UNION ALL SELECT 1 FROM entity_public_ids WHERE short_id=?1)"),
            [&id],
            |r| r.get::<_, bool>(0),
        )? {
            return Ok(id);
        }
    }
}
fn prefix(kind: &str) -> Result<&'static str, Error> {
    match kind {
        "workspace" => Ok("s"),
        "list" => Ok("l"),
        "section" => Ok("c"),
        "task" => Ok("t"),
        _ => Err(Error::Invalid("unknown entity kind".into())),
    }
}
pub fn public_id(c: &Connection, kind: &str, id: &str) -> Result<String, Error> {
    if kind == "task" || kind == "list" {
        let (table, column, entities) = if kind == "task" {
            ("task_public_ids", "task_id", "tasks")
        } else {
            ("list_public_ids", "list_id", "lists")
        };
        loop {
            if let Some(value) = c
                .query_row(
                    &format!("SELECT short_id FROM {table} WHERE {column}=?1"),
                    [id],
                    |r| r.get(0),
                )
                .optional()?
            {
                return Ok(value);
            }
            let candidate =
                URL_SAFE_NO_PAD.encode(&uuid::Uuid::new_v4().as_bytes()[..6])[..7].to_owned();
            if c.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {entities} WHERE id=?1 UNION ALL SELECT 1 FROM entity_public_ids WHERE short_id=?1)"), [&candidate], |r| r.get::<_, bool>(0))? { continue; }
            c.execute(
                &format!("INSERT OR IGNORE INTO {table}({column},short_id) VALUES(?1,?2)"),
                params![id, candidate],
            )?;
        }
    }
    let prefix = prefix(kind)?;
    loop {
        if let Some(value) = c
            .query_row(
                "SELECT short_id FROM entity_public_ids WHERE kind=?1 AND entity_id=?2",
                params![kind, id],
                |r| r.get(0),
            )
            .optional()?
        {
            return Ok(value);
        }
        let candidate = if id.starts_with(&format!("{prefix}_"))
            && id.len() == 14
            && !c.query_row(
                "SELECT EXISTS(SELECT 1 FROM entity_public_ids WHERE short_id=?1)",
                [id],
                |r| r.get::<_, bool>(0),
            )? {
            id.to_owned()
        } else {
            short_id(prefix)
        };
        c.execute(
            "INSERT OR IGNORE INTO entity_public_ids(kind,entity_id,short_id) VALUES(?1,?2,?3)",
            params![kind, id, candidate],
        )?;
    }
}
pub fn resolve(c: &Connection, kind: &str, id: &str) -> Result<String, Error> {
    if kind == "task" || kind == "list" {
        let (table, column) = if kind == "task" {
            ("task_public_ids", "task_id")
        } else {
            ("list_public_ids", "list_id")
        };
        if let Some(value) = c
            .query_row(
                &format!("SELECT {column} FROM {table} WHERE short_id=?1"),
                [id],
                |r| r.get(0),
            )
            .optional()?
        {
            return Ok(value);
        }
    }
    Ok(c.query_row(
        "SELECT entity_id FROM entity_public_ids WHERE kind=?1 AND short_id=?2",
        params![kind, id],
        |r| r.get(0),
    )
    .optional()?
    .unwrap_or_else(|| id.to_owned()))
}
/// Upgrade legacy single-account Inboxes without exposing them to another account.
pub(crate) fn adopt_legacy_inboxes(c: &Connection) -> Result<(), Error> {
    let candidates = c.prepare("SELECT l.id,min(m.user_id) FROM lists l JOIN memberships m ON m.list_id=l.id LEFT JOIN membership_roles mr ON mr.list_id=m.list_id AND mr.user_id=m.user_id WHERE l.workspace_id IS NULL AND l.archived=0 AND lower(trim(l.title))='inbox' GROUP BY l.id HAVING count(*)=1 AND min(coalesce(mr.role,'editor'))='editor'")?.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<Result<Vec<_>,_>>()?;
    for (inbox, user) in candidates {
        let personal: Option<String> = c.query_row("SELECT w.id FROM workspaces w JOIN workspace_members m ON m.workspace_id=w.id LEFT JOIN workspace_lifecycle x ON x.workspace_id=w.id WHERE lower(w.title)='personal' AND m.user_id=?1 AND m.role='owner' AND x.deleted_at IS NULL AND coalesce(x.archived,0)=0 AND (SELECT count(*) FROM workspace_members m2 WHERE m2.workspace_id=w.id)=1 ORDER BY w.id LIMIT 1", [&user], |r| r.get(0)).optional()?;
        let space = if let Some(id) = personal {
            id
        } else {
            let id = entity_id(c, "workspaces", "s")?;
            c.execute(
                "INSERT INTO workspaces(id,title) VALUES(?1,'Personal')",
                [&id],
            )?;
            c.execute(
                "INSERT INTO workspace_members VALUES(?1,?2,'owner')",
                params![id, user],
            )?;
            crate::slugs::workspace(c, &id)?;
            public_id(c, "workspace", &id)?;
            id
        };
        let previous: Option<String> = c
            .query_row(
                "SELECT list_id FROM workspace_defaults WHERE workspace_id=?1",
                [&space],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(previous) = previous.as_ref() {
            // Retain, but retire, a pristine empty Inbox created by the older bootstrap.
            let pristine: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM lists l WHERE l.id=?1 AND l.title='Inbox' AND l.description='' AND l.revision=1 AND l.color IS NULL AND l.color_hex IS NULL AND NOT EXISTS(SELECT 1 FROM tasks t WHERE t.list_id=l.id) AND (SELECT count(*) FROM sections s WHERE s.list_id=l.id)=1 AND NOT EXISTS(SELECT 1 FROM memberships m WHERE m.list_id=l.id AND m.user_id<>?2))", params![previous,user], |r| r.get(0))?;
            if pristine {
                c.execute(
                    "UPDATE lists SET archived=1,revision=revision+1 WHERE id=?1",
                    [previous],
                )?;
                c.execute("UPDATE entity_slugs SET slug='inbox-bootstrap-'||substr(entity_id,1,14) WHERE kind='list' AND entity_id=?1",[previous])?;
                c.execute(
                    "UPDATE workspace_defaults SET list_id=?1 WHERE workspace_id=?2",
                    params![inbox, space],
                )?;
            }
        } else {
            c.execute(
                "INSERT INTO workspace_defaults VALUES(?1,?2)",
                params![space, inbox],
            )?;
        }
        c.execute(
            "UPDATE lists SET workspace_id=?1,restricted=1,revision=revision+1 WHERE id=?2",
            params![space, inbox],
        )?;
        c.execute("INSERT INTO user_defaults(user_id,workspace_id) VALUES(?1,?2) ON CONFLICT(user_id) DO UPDATE SET workspace_id=excluded.workspace_id",params![user,space])?;
        crate::slugs::list(c, &inbox)?;
        list(c, &inbox)?;
    }
    Ok(())
}
pub fn list(c: &Connection, id: &str) -> Result<String, Error> {
    if let Some(section) = c
        .query_row(
            "SELECT section_id FROM list_defaults WHERE list_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
    {
        return Ok(section);
    }
    let section = entity_id(c, "sections", "c")?;
    c.execute(
        "INSERT INTO sections(id,list_id,title,position) VALUES(?1,?2,'Main',-1)",
        params![section, id],
    )?;
    c.execute(
        "INSERT INTO list_defaults VALUES(?1,?2)",
        params![id, section],
    )?;
    crate::slugs::section(c, &section)?;
    public_id(c, "section", &section)?;
    Ok(section)
}
pub fn workspace(c: &Connection, id: &str) -> Result<String, Error> {
    if let Some(list) = c
        .query_row(
            "SELECT list_id FROM workspace_defaults WHERE workspace_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
    {
        return Ok(list);
    }
    let existing:Option<String>=c.query_row("SELECT id FROM lists WHERE workspace_id=?1 AND archived=0 AND restricted=0 AND lower(title)='inbox' ORDER BY position,id LIMIT 1",[id],|r|r.get(0)).optional()?;
    let list = if let Some(list) = existing {
        list
    } else {
        let list = entity_id(c, "lists", "l")?;
        c.execute(
            "INSERT INTO lists(id,title,workspace_id,position) VALUES(?1,'Inbox',?2,0)",
            params![list, id],
        )?;
        crate::slugs::list(c, &list)?;
        list
    };
    self::list(c, &list)?;
    public_id(c, "list", &list)?;
    c.execute(
        "INSERT INTO workspace_defaults VALUES(?1,?2)",
        params![id, list],
    )?;
    Ok(list)
}
pub fn user(c: &Connection, user: &str) -> Result<String, Error> {
    if let Some(workspace) = c
        .query_row(
            "SELECT workspace_id FROM user_defaults WHERE user_id=?1",
            [user],
            |r| r.get(0),
        )
        .optional()?
    {
        return Ok(workspace);
    }
    let existing:Option<String>=c.query_row("SELECT w.id FROM workspaces w JOIN workspace_members m ON m.workspace_id=w.id LEFT JOIN workspace_lifecycle x ON x.workspace_id=w.id WHERE m.user_id=?1 AND m.role='owner' AND x.deleted_at IS NULL AND coalesce(x.archived,0)=0 ORDER BY lower(w.title) IN('personal','inbox') DESC,w.id LIMIT 1",[user],|r|r.get(0)).optional()?;
    let id = if let Some(id) = existing {
        id
    } else {
        let id = entity_id(c, "workspaces", "s")?;
        c.execute(
            "INSERT INTO workspaces(id,title) VALUES(?1,'Personal')",
            [&id],
        )?;
        c.execute(
            "INSERT INTO workspace_members VALUES(?1,?2,'owner')",
            params![id, user],
        )?;
        crate::slugs::workspace(c, &id)?;
        id
    };
    let inbox = workspace(c, &id)?;
    public_id(c, "workspace", &id)?;
    c.execute("INSERT INTO user_defaults VALUES(?1,?2)", params![user, id])?;
    c.execute("INSERT OR IGNORE INTO agent_destination_preferences(user_id,preference_key,list_id) VALUES(?1,'inbox',?2)",params![user,inbox])?;
    Ok(id)
}
pub fn backfill(c: &Connection) -> Result<(), Error> {
    // Run inside the migration transaction. No tasks or existing references move.
    let ids = |table: &str| -> Result<Vec<String>, Error> {
        Ok(c.prepare(&format!("SELECT id FROM {table}"))?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?)
    };
    for id in ids("lists")? {
        list(c, &id)?;
    }
    for id in ids("workspaces")? {
        workspace(c, &id)?;
    }
    for id in ids("users")? {
        user(c, &id)?;
    }
    c.execute("INSERT OR IGNORE INTO task_locations(task_id,section_id) SELECT t.id,d.section_id FROM tasks t JOIN list_defaults d ON d.list_id=t.list_id WHERE t.parent_id IS NULL",[])?;
    for (table, kind) in [
        ("workspaces", "workspace"),
        ("lists", "list"),
        ("sections", "section"),
        ("tasks", "task"),
    ] {
        for id in ids(table)? {
            public_id(c, kind, &id)?;
        }
    }
    Ok(())
}

impl crate::core::Store {
    pub fn ensure_user_defaults(&mut self, actor: &str) -> Result<String, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let owner = crate::core::actor_user(&tx, actor, 3)?;
        let id = user(&tx, &owner)?;
        tx.commit()?;
        Ok(id)
    }
    pub fn default_list(&self, actor: &str, workspace: &str) -> Result<String, Error> {
        self.workspace(actor, workspace)?;
        let found:Option<String>=self.conn.query_row("SELECT l.id FROM lists l JOIN actor_list_access a ON a.list_id=l.id AND a.actor_id=?1 LEFT JOIN workspace_defaults d ON d.list_id=l.id AND d.workspace_id=?2 WHERE l.workspace_id=?2 AND l.archived=0 ORDER BY d.list_id IS NOT NULL DESC,l.position,l.id LIMIT 1",params![actor,workspace],|r|r.get(0)).optional()?;
        found.ok_or(Error::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::{Capture, NewTask, Store},
        workspace::{CreateList, WorkspaceInput},
    };
    #[test]
    fn legacy_inbox_adoption_preserves_private_access_and_is_repeatable() {
        let mut db = Store::open(":memory:").unwrap();
        db.seed_dev().unwrap();
        db.conn
            .execute(
                "INSERT INTO membership_roles VALUES('inbox','dev','viewer')",
                [],
            )
            .unwrap();
        adopt_legacy_inboxes(&db.conn).unwrap();
        let space: Option<String> = db
            .conn
            .query_row("SELECT workspace_id FROM lists WHERE id='inbox'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(space.is_none());
        db.conn
            .execute("DELETE FROM membership_roles WHERE list_id='inbox'", [])
            .unwrap();
        let count = db.tasks("dev", "inbox").unwrap().len();
        adopt_legacy_inboxes(&db.conn).unwrap();
        let space: String = db
            .conn
            .query_row("SELECT workspace_id FROM lists WHERE id='inbox'", [], |r| {
                r.get(0)
            })
            .unwrap();
        adopt_legacy_inboxes(&db.conn).unwrap();
        assert_eq!(db.tasks("dev", "inbox").unwrap().len(), count);
        assert!(db.tasks("other", "inbox").is_err());
        let actual: String = db
            .conn
            .query_row("SELECT workspace_id FROM lists WHERE id='inbox'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(actual, space);
    }
    #[test]
    fn workspace_and_list_creation_are_ready_for_first_capture() {
        let mut db = Store::open(":memory:").unwrap();
        db.seed_dev().unwrap();
        let w = db
            .create_workspace(
                "dev",
                WorkspaceInput {
                    title: "Ready".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(w.id.starts_with("s_"));
        assert_eq!(w.id.len(), 14);
        let inbox = db.default_list("dev", &w.id).unwrap();
        assert!(inbox.starts_with("l_"));
        assert!(db.sections("dev", &inbox).unwrap().is_empty());
        let hidden: String = db
            .conn
            .query_row(
                "SELECT section_id FROM list_defaults WHERE list_id=?1",
                [&inbox],
                |r| r.get(0),
            )
            .unwrap();
        assert!(hidden.starts_with("c_"));
        let first = db
            .capture(
                "dev",
                "first",
                Capture {
                    list_id: inbox.clone(),
                    tasks: vec![NewTask {
                        title: "First todo".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        assert!(first.id.starts_with("t_"));
        assert_eq!(first.id, first.reference);
        // Existing links and copied references keep resolving after compact URLs.
        db.conn.execute("INSERT INTO entity_public_ids(kind,entity_id,short_id) VALUES('task',?1,'t_previousAlias')", [&first.id]).unwrap();
        let compact = public_id(&db.conn, "task", &first.id).unwrap();
        assert_eq!(compact.len(), 7);
        assert_eq!(public_id(&db.conn, "task", &first.id).unwrap(), compact);
        assert_eq!(resolve(&db.conn, "task", &compact).unwrap(), first.id);
        assert_eq!(
            resolve(&db.conn, "task", "t_previousAlias").unwrap(),
            first.id
        );
        assert_eq!(crate::core::task(&db.conn, &compact).unwrap().id, first.id);
        assert_eq!(
            db.resolve_reference("dev", &format!("%{compact}"))
                .unwrap()
                .choices[0]
                .id,
            first.id
        );

        assert_eq!(
            db.conn
                .query_row(
                    "SELECT section_id FROM task_locations WHERE task_id=?1",
                    [&first.id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            hidden
        );
        assert_eq!(db.task_section("dev", &first.id).unwrap(), None);
        assert_eq!(
            db.capture(
                "dev",
                "first",
                Capture {
                    list_id: inbox.clone(),
                    tasks: vec![NewTask {
                        title: "First todo".into(),
                        description: String::new(),
                        parent_id: None
                    }]
                }
            )
            .unwrap()[0]
                .id,
            first.id
        );
        let other = db
            .create_list(
                "dev",
                CreateList {
                    workspace_id: w.id,
                    title: "Another".into(),
                    description: String::new(),
                    restricted: false,
                },
            )
            .unwrap();
        assert!(
            db.conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM list_defaults WHERE list_id=?1)",
                    [other.id],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap()
        );
    }
    #[test]
    fn user_provisioning_is_repeat_safe_and_rolls_back_the_whole_hierarchy() {
        let mut db = Store::open(":memory:").unwrap();
        db.seed_dev().unwrap();
        db.conn.execute_batch("CREATE TRIGGER reject_default_section BEFORE INSERT ON sections BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
        assert!(db.ensure_user_defaults("dev").is_err());
        assert_eq!(db.workspaces("dev").unwrap().len(), 0);
        db.conn
            .execute_batch("DROP TRIGGER reject_default_section")
            .unwrap();
        let first = db.ensure_user_defaults("dev").unwrap();
        assert_eq!(db.ensure_user_defaults("dev").unwrap(), first);
        assert_eq!(db.workspaces("dev").unwrap().len(), 1);
        let inbox = db.default_list("dev", &first).unwrap();
        assert_eq!(
            db.resolve_destination("dev", None, None, true)
                .unwrap()
                .destination
                .unwrap()
                .id,
            inbox
        );
        assert!(db.default_list("other", &first).is_err());
    }
    #[test]
    fn aliases_are_persistent_and_never_grant_access() {
        let mut db = Store::open(":memory:").unwrap();
        db.seed_dev().unwrap();
        let alias = public_id(&db.conn, "list", "private").unwrap();
        assert_eq!(alias.len(), 7);
        db.conn.execute("INSERT INTO entity_public_ids(kind,entity_id,short_id) VALUES('list','private','l_oldPrivateAlias')",[]).unwrap();
        assert_eq!(
            resolve(&db.conn, "list", "l_oldPrivateAlias").unwrap(),
            "private"
        );
        assert_eq!(public_id(&db.conn, "list", "private").unwrap(), alias);
        assert_eq!(resolve(&db.conn, "list", &alias).unwrap(), "private");
        assert!(
            db.resolve_destination("dev", Some(&alias), None, false)
                .unwrap()
                .destination
                .is_none()
        );
        assert_eq!(
            db.resolve_destination("other", Some(&alias), None, false)
                .unwrap()
                .destination
                .unwrap()
                .id,
            "private"
        );
    }
    #[test]
    fn verified_new_identity_commits_defaults_with_identity_or_rolls_back() {
        let path =
            std::env::temp_dir().join(format!("todoer-default-auth-{}.db", uuid::Uuid::new_v4()));
        let db = Store::open(path.to_str().unwrap()).unwrap();
        let mut auth = crate::auth::AuthStore::open(path.to_str().unwrap()).unwrap();
        auth.allow_email("new@example.test").unwrap();
        db.conn.execute_batch("CREATE TRIGGER reject_first_section BEFORE INSERT ON sections BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
        assert!(
            auth.accept_identity(
                "https://accounts.google.com",
                "new-sub",
                "new@example.test",
                true
            )
            .is_err()
        );
        assert_eq!(
            db.conn
                .query_row("SELECT count(*) FROM users", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        db.conn
            .execute_batch("DROP TRIGGER reject_first_section")
            .unwrap();
        let user = auth
            .accept_identity(
                "https://accounts.google.com",
                "new-sub",
                "new@example.test",
                true,
            )
            .unwrap();
        let space: String = db
            .conn
            .query_row(
                "SELECT workspace_id FROM user_defaults WHERE user_id=?1",
                [&user.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(db.default_list(&user.id, &space).unwrap().starts_with("l_"));
        assert_eq!(
            auth.accept_identity(
                "https://accounts.google.com",
                "new-sub",
                "new@example.test",
                true
            )
            .unwrap()
            .id,
            user.id
        );
        drop(auth);
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
