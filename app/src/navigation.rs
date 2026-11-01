//! Small personal favorites and restrained list color badges.
use crate::core::{Error, Store, access_rank, actor_identity, actor_user};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize)]
pub struct Favorite {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub url: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FavoriteInput {
    pub kind: String,
    pub id: String,
    pub favorite: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct FavoriteKey {
    pub kind: String,
    pub id: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FavoriteOrder {
    pub expected: Vec<FavoriteKey>,
    pub order: Vec<FavoriteKey>,
}
fn favorites_for(conn: &rusqlite::Connection, actor: &str) -> Result<Vec<Favorite>, Error> {
    let (user, _) = actor_identity(conn, actor)?;
    let mut statement=conn.prepare("SELECT f.target_kind,f.target_id,CASE f.target_kind WHEN 'list' THEN l.title ELSE w.title END FROM favorites f LEFT JOIN lists l ON f.target_kind='list' AND l.id=f.target_id LEFT JOIN workspaces w ON f.target_kind='workspace' AND w.id=f.target_id WHERE f.user_id=?1 AND (f.target_kind='list' AND l.archived=0 AND NOT EXISTS(SELECT 1 FROM workspace_lifecycle x WHERE x.workspace_id=l.workspace_id AND (x.archived=1 OR x.deleted_at IS NOT NULL)) AND EXISTS(SELECT 1 FROM actor_list_access a WHERE a.actor_id=?2 AND a.list_id=f.target_id AND a.rank>=1) OR f.target_kind='workspace' AND NOT EXISTS(SELECT 1 FROM workspace_lifecycle x WHERE x.workspace_id=w.id AND (x.archived=1 OR x.deleted_at IS NOT NULL)) AND EXISTS(SELECT 1 FROM actor_workspace_access a WHERE a.actor_id=?2 AND a.workspace_id=f.target_id AND a.rank>=1)) ORDER BY f.position,f.created_at,f.target_kind,f.target_id")?;
    Ok(statement
        .query_map(params![user, actor], |r| {
            let kind: String = r.get(0)?;
            let id: String = r.get(1)?;
            let url = format!(
                "/{}/{}",
                if kind == "list" {
                    "lists"
                } else {
                    "workspaces"
                },
                id
            );
            Ok(Favorite {
                kind,
                id,
                title: r.get(2)?,
                url,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?)
}
impl Store {
    pub fn list_reference(&self, actor: &str, id: &str) -> Result<String, Error> {
        access_rank(&self.conn, actor, id, 1)?;
        if let Some(reference) = self
            .conn
            .query_row(
                "SELECT reference FROM list_references WHERE list_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
        {
            return Ok(reference);
        }
        // Persist once, retry on a short-reference collision; names/moves never alter it.
        loop {
            let reference = crate::defaults::readable_id(7);
            if self.conn.execute(
                "INSERT OR IGNORE INTO list_references(list_id,reference) VALUES(?1,?2)",
                params![id, reference],
            )? == 1
            {
                return Ok(reference);
            }
            if let Some(reference) = self
                .conn
                .query_row(
                    "SELECT reference FROM list_references WHERE list_id=?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?
            {
                return Ok(reference);
            }
        }
    }
    pub fn favorites(&self, actor: &str) -> Result<Vec<Favorite>, Error> {
        favorites_for(&self.conn, actor)
    }
    pub fn reorder_favorites(
        &mut self,
        actor: &str,
        expected: &[FavoriteKey],
        order: &[FavoriteKey],
    ) -> Result<(), Error> {
        let user = actor_user(&self.conn, actor, 3)?;
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let current = favorites_for(&tx, actor)?
            .into_iter()
            .map(|item| FavoriteKey {
                kind: item.kind,
                id: item.id,
            })
            .collect::<Vec<_>>();
        if current != expected {
            return Err(Error::Conflict(
                "Favorites changed; reload before reordering".into(),
            ));
        }
        let mut left = current.clone();
        let mut right = order.to_vec();
        left.sort();
        right.sort();
        if left != right {
            return Err(Error::Invalid(
                "Order must include each visible favorite once".into(),
            ));
        }
        // Replace visible slots while retaining inaccessible favorites in their places.
        let all = {
            let mut statement = tx.prepare("SELECT target_kind,target_id FROM favorites WHERE user_id=?1 ORDER BY position,created_at,target_kind,target_id")?;
            statement
                .query_map([&user], |row| {
                    Ok(FavoriteKey {
                        kind: row.get(0)?,
                        id: row.get(1)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let visible = current.iter().collect::<std::collections::HashSet<_>>();
        let mut replacements = order.iter();
        for (position, original) in all.iter().enumerate() {
            let item = if visible.contains(original) {
                replacements.next().expect("validated visible order")
            } else {
                original
            };
            tx.execute("UPDATE favorites SET position=?4 WHERE user_id=?1 AND target_kind=?2 AND target_id=?3", params![user,item.kind,item.id,position as i64])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn set_favorite(&mut self, actor: &str, input: FavoriteInput) -> Result<(), Error> {
        let user = actor_user(&self.conn, actor, 3)?;
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let permitted=match input.kind.as_str(){
            "list"=>tx.query_row("SELECT EXISTS(SELECT 1 FROM actor_list_access WHERE actor_id=?1 AND list_id=?2 AND rank>=1)",params![actor,input.id],|r|r.get::<_,bool>(0))?,
            "workspace"=>tx.query_row("SELECT EXISTS(SELECT 1 FROM actor_workspace_access WHERE actor_id=?1 AND workspace_id=?2 AND rank>=1)",params![actor,input.id],|r|r.get::<_,bool>(0))?,
            _=>return Err(Error::Invalid("Favorite kind must be list or workspace".into())),
        };
        if !permitted {
            return Err(Error::Forbidden);
        }
        if input.favorite {
            tx.execute(
                "INSERT OR IGNORE INTO favorites(user_id,target_kind,target_id,position) VALUES(?1,?2,?3,CASE WHEN ?2='workspace' THEN COALESCE((SELECT min(position)-1 FROM favorites WHERE user_id=?1),0) ELSE COALESCE((SELECT max(position)+1 FROM favorites WHERE user_id=?1),0) END)",
                params![user, input.kind, input.id],
            )?;
        } else {
            tx.execute(
                "DELETE FROM favorites WHERE user_id=?1 AND target_kind=?2 AND target_id=?3",
                params![user, input.kind, input.id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn list_color(&self, actor: &str, id: &str) -> Result<Option<String>, Error> {
        access_rank(&self.conn, actor, id, 1)?;
        self.conn
            .query_row(
                "SELECT COALESCE(color_hex,color) FROM lists WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(Error::NotFound)
    }
    pub fn set_list_color(
        &mut self,
        actor: &str,
        id: &str,
        revision: i64,
        color: Option<String>,
    ) -> Result<crate::core::List, Error> {
        validate_list_color(&color)?;
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        access_rank(&tx, actor, id, 2)?;
        if tx.execute(
            "UPDATE lists SET color=CASE WHEN substr(?1,1,1)='#' THEN NULL ELSE ?1 END,color_hex=CASE WHEN substr(?1,1,1)='#' THEN ?1 ELSE NULL END,revision=revision+1 WHERE id=?2 AND revision=?3",
            params![color, id, revision],
        )? != 1
        {
            return Err(Error::Conflict(
                "List changed; reload before choosing its color".into(),
            ));
        }
        tx.commit()?;
        self.list_info(actor, id).map(|i| i.list)
    }
}
pub(crate) fn validate_list_color(color: &Option<String>) -> Result<(), Error> {
    if color.as_ref().is_some_and(|c| {
        !["blue", "green", "amber", "red", "violet", "pink", "gray"].contains(&c.as_str())
            && !(c.len() == 7
                && c.starts_with('#')
                && c[1..].bytes().all(|b| b.is_ascii_hexdigit()))
    }) {
        return Err(Error::Invalid("Unknown list color".into()));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn space_favorites_prepend_and_reordering_checks_the_visible_snapshot() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let space = db
            .create_workspace(
                "dev",
                crate::workspace::WorkspaceInput {
                    title: "Work plans".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        db.set_favorite(
            "dev",
            FavoriteInput {
                kind: "list".into(),
                id: "groceries".into(),
                favorite: true,
            },
        )
        .unwrap();
        db.set_favorite(
            "dev",
            FavoriteInput {
                kind: "workspace".into(),
                id: space.id.clone(),
                favorite: true,
            },
        )
        .unwrap();
        let expected = db
            .favorites("dev")
            .unwrap()
            .into_iter()
            .map(|item| FavoriteKey {
                kind: item.kind,
                id: item.id,
            })
            .collect::<Vec<_>>();
        assert_eq!(expected[0].id, space.id);
        let mut order = expected.clone();
        order.reverse();
        db.reorder_favorites("dev", &expected, &order).unwrap();
        assert_eq!(db.favorites("dev").unwrap()[0].id, "groceries");
        assert!(matches!(
            db.reorder_favorites("dev", &expected, &order),
            Err(Error::Conflict(_))
        ));
        assert!(db.reorder_favorites("other", &order, &order).is_err());
    }
    #[test]
    fn favorites_are_personal_and_recheck_revocation() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.set_favorite(
            "dev",
            FavoriteInput {
                kind: "list".into(),
                id: "groceries".into(),
                favorite: true,
            },
        )
        .unwrap();
        assert_eq!(db.favorites("dev").unwrap().len(), 1);
        assert!(db.favorites("other").unwrap().is_empty());
        db.conn
            .execute("UPDATE lists SET archived=1 WHERE id='groceries'", [])
            .unwrap();
        assert!(db.favorites("dev").unwrap().is_empty());
        db.conn
            .execute("UPDATE lists SET archived=0 WHERE id='groceries'", [])
            .unwrap();
        assert_eq!(db.favorites("dev").unwrap().len(), 1);
        assert!(
            db.set_favorite(
                "dev",
                FavoriteInput {
                    kind: "list".into(),
                    id: "private".into(),
                    favorite: true
                }
            )
            .is_err()
        );
        db.conn
            .execute(
                "DELETE FROM memberships WHERE list_id='groceries' AND user_id='dev'",
                [],
            )
            .unwrap();
        assert!(db.favorites("dev").unwrap().is_empty());
    }
    #[test]
    fn combined_list_edit_saves_color_atomically() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let revision = db.list_info("dev", "groceries").unwrap().list.revision;
        let change = serde_json::from_value(serde_json::json!({"revision":revision,"title":"Fresh groceries","description":"All together","color":"#ff1234"})).unwrap();
        let saved = db.update_list("dev", "groceries", change).unwrap();
        assert_eq!(saved.title, "Fresh groceries");
        assert_eq!(saved.description, "All together");
        assert_eq!(saved.revision, revision + 1);
        assert_eq!(
            db.list_color("dev", "groceries").unwrap().as_deref(),
            Some("#ff1234")
        );
        for (actor, revision, color) in [
            ("dev", saved.revision, "invalid"),
            ("dev", revision, "#000000"),
            ("other", saved.revision, "#000000"),
        ] {
            let change = serde_json::from_value(
                serde_json::json!({"revision":revision,"title":"Must not save","color":color}),
            )
            .unwrap();
            assert!(db.update_list(actor, "groceries", change).is_err());
            assert_eq!(
                db.list_info("dev", "groceries").unwrap().list.title,
                "Fresh groceries"
            );
            assert_eq!(
                db.list_color("dev", "groceries").unwrap().as_deref(),
                Some("#ff1234")
            );
        }
    }
    #[test]
    fn list_color_checks_revision_and_permissions() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let rev = db.list_info("dev", "groceries").unwrap().list.revision;
        assert!(
            db.set_list_color("other", "groceries", rev, Some("blue".into()))
                .is_err()
        );
        db.set_list_color("dev", "groceries", rev, Some("blue".into()))
            .unwrap();
        assert_eq!(
            db.list_color("dev", "groceries").unwrap().as_deref(),
            Some("blue")
        );
        assert!(db.set_list_color("dev", "groceries", rev, None).is_err());
    }
}
