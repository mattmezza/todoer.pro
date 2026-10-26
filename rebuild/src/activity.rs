//! Bounded entity history. Both historical and current list access are checked.
use crate::core::{Error, Store};
use rusqlite::params;
use serde::Serialize;
#[derive(Serialize)]
pub struct ActivityItem {
    pub sequence: i64,
    pub kind: String,
    pub task_id: String,
    pub title: String,
    pub actor: String,
    pub attribution: String,
    pub occurred_at: Option<i64>,
    pub undo_revision: Option<i64>,
}
#[derive(Serialize)]
pub struct ActivityPage {
    pub items: Vec<ActivityItem>,
    pub next_before: Option<i64>,
}
impl Store {
    pub fn entity_activity(
        &self,
        actor: &str,
        kind: &str,
        id: &str,
        before: Option<i64>,
    ) -> Result<ActivityPage, Error> {
        if before.is_some_and(|value| value <= 0) {
            return Err(Error::Invalid(
                "before must be a positive history sequence".into(),
            ));
        }
        let id = crate::defaults::resolve(&self.conn, kind, id)?;
        match kind {
            "task" => {
                let task = crate::core::task(&self.conn, &id)?;
                crate::core::access_rank(&self.conn, actor, &task.list_id, 1)?;
            }
            "workspace" => {
                self.workspace(actor, &id)?;
            }
            _ => return Err(Error::Invalid("unsupported activity scope".into())),
        };
        let scope = if kind == "task" {
            "json_extract(h.event,'$.task.id')=?2"
        } else {
            "EXISTS(SELECT 1 FROM lists scope WHERE scope.id=h.list_id AND scope.workspace_id=?2)"
        };
        let mut statement=self.conn.prepare(&format!("SELECT h.seq,json_extract(h.event,'$.kind'),t.id,json_extract(h.event,'$.task.title'),coalesce(u.name,'Member'),coalesce(json_extract(h.event,'$.attribution'),'human'),h.occurred_at,CASE WHEN json_extract(h.event,'$.kind')='update' AND h.before_event IS NOT NULL AND t.revision=json_extract(h.event,'$.task.revision') AND live.rank>=2 THEN t.revision END FROM history h JOIN actor_list_access old ON old.list_id=h.list_id AND old.actor_id=?1 JOIN tasks t ON t.id=json_extract(h.event,'$.task.id') JOIN actor_list_access live ON live.list_id=t.list_id AND live.actor_id=?1 LEFT JOIN users u ON u.id=h.actor WHERE {scope} AND (?3 IS NULL OR h.seq<?3) ORDER BY h.seq DESC LIMIT 51"))?;
        let mut items = statement
            .query_map(params![actor, id, before], |row| {
                Ok(ActivityItem {
                    sequence: row.get(0)?,
                    kind: row.get(1)?,
                    task_id: row.get(2)?,
                    title: row.get(3)?,
                    actor: row.get(4)?,
                    attribution: row.get(5)?,
                    occurred_at: row.get(6)?,
                    undo_revision: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > 50;
        items.truncate(50);
        let next_before = if more {
            items.last().map(|r| r.sequence)
        } else {
            None
        };
        Ok(ActivityPage { items, next_before })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Capture, Change, NewTask};
    #[test]
    fn task_activity_is_bounded_scoped_and_revocation_is_live() {
        let mut db = Store::open(":memory:").unwrap();
        db.seed_dev().unwrap();
        let mut task = db
            .capture(
                "dev",
                "activity",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![NewTask {
                        title: "Tracked".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        for i in 0..55 {
            task = db
                .update(
                    "dev",
                    &task.id,
                    Change {
                        revision: task.revision,
                        title: Some(format!("Tracked {i}")),
                        description: None,
                        complete: None,
                        complete_subtree: false,
                    },
                )
                .unwrap();
        }
        let page = db.entity_activity("dev", "task", &task.id, None).unwrap();
        assert_eq!(page.items.len(), 50);
        assert_eq!(page.items[0].undo_revision, Some(task.revision));
        let rest = db
            .entity_activity("dev", "task", &task.id, page.next_before)
            .unwrap();
        assert_eq!(rest.items.len(), 6);
        assert!(rest.next_before.is_none());
        assert!(db.entity_activity("other", "task", &task.id, None).is_err());
        db.conn
            .execute(
                "DELETE FROM memberships WHERE list_id='inbox' AND user_id='dev'",
                [],
            )
            .unwrap();
        assert!(db.entity_activity("dev", "task", &task.id, None).is_err());
    }
}
