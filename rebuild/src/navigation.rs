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
            let reference = uuid::Uuid::new_v4().simple().to_string()[..7].to_string();
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
        let (user, _) = actor_identity(&self.conn, actor)?;
        let mut statement=self.conn.prepare("SELECT f.target_kind,f.target_id,CASE f.target_kind WHEN 'list' THEN l.title ELSE w.title END FROM favorites f LEFT JOIN lists l ON f.target_kind='list' AND l.id=f.target_id LEFT JOIN workspaces w ON f.target_kind='workspace' AND w.id=f.target_id WHERE f.user_id=?1 AND (f.target_kind='list' AND EXISTS(SELECT 1 FROM actor_list_access a WHERE a.actor_id=?2 AND a.list_id=f.target_id AND a.rank>=1) OR f.target_kind='workspace' AND EXISTS(SELECT 1 FROM actor_workspace_access a WHERE a.actor_id=?2 AND a.workspace_id=f.target_id AND a.rank>=1)) ORDER BY f.created_at,f.target_kind,f.target_id")?;
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
                "INSERT OR IGNORE INTO favorites(user_id,target_kind,target_id) VALUES(?1,?2,?3)",
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
        if color.as_ref().is_some_and(|c| {
            !["blue", "green", "amber", "red", "violet", "pink", "gray"].contains(&c.as_str())
                && !(c.len() == 7
                    && c.starts_with('#')
                    && c[1..].bytes().all(|b| b.is_ascii_hexdigit()))
        }) {
            return Err(Error::Invalid("Unknown list color".into()));
        }
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
#[cfg(test)]
mod tests {
    use super::*;
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
