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
    pub completed: bool,
    pub actions: Vec<String>,
}
#[derive(Serialize)]
pub struct ActivityPage {
    pub items: Vec<ActivityItem>,
    pub next_before: Option<i64>,
}
fn activity_actions(event: &str, before: Option<&str>) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_str(event).unwrap_or_default();
    let old: serde_json::Value = before
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let kind = value["kind"].as_str().unwrap_or("update");
    if let Some(changes) = value["changes"].as_array() {
        let actions: Vec<String> = changes
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        if !actions.is_empty() {
            return actions;
        }
    }
    let mut actions = Vec::new();
    if kind == "update" || kind == "undo" {
        let task = &value["task"];
        if !old.is_null() {
            for (field, action) in [
                (
                    "complete",
                    if task["complete"].as_bool() == Some(true) {
                        "completed"
                    } else {
                        "reopened"
                    },
                ),
                ("title", "title_changed"),
                ("description", "description_changed"),
                ("parent_id", "parent_changed"),
                ("list_id", "moved"),
            ] {
                if task[field] != old[field] {
                    actions.push(action.into());
                }
            }
        }
    }
    if actions.is_empty() {
        actions.push(kind.into());
    }
    actions
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
            "list" => {
                crate::core::access_rank(&self.conn, actor, &id, 1)?;
            }
            "workspace" => {
                self.workspace(actor, &id)?;
            }
            _ => return Err(Error::Invalid("unsupported activity scope".into())),
        };
        let scope = if kind == "task" {
            "json_extract(h.event,'$.task.id')=?2"
        } else if kind == "list" {
            "h.list_id=?2"
        } else {
            "EXISTS(SELECT 1 FROM lists scope WHERE scope.id=h.list_id AND scope.workspace_id=?2)"
        };
        let mut statement=self.conn.prepare(&format!("SELECT h.seq,json_extract(h.event,'$.kind'),t.id,json_extract(h.event,'$.task.title'),coalesce(u.name,'Member'),coalesce(json_extract(h.event,'$.attribution'),'human'),h.occurred_at,CASE WHEN json_extract(h.event,'$.kind')='update' AND h.before_event IS NOT NULL AND t.revision=json_extract(h.event,'$.task.revision') AND live.rank>=2 THEN t.revision END,coalesce(json_extract(h.event,'$.task.complete'),0)=1 AND coalesce(json_extract(h.before_event,'$.complete'),0)=0,h.event,h.before_event FROM history h JOIN actor_list_access old ON old.list_id=h.list_id AND old.actor_id=?1 JOIN tasks t ON t.id=json_extract(h.event,'$.task.id') JOIN actor_list_access live ON live.list_id=t.list_id AND live.actor_id=?1 LEFT JOIN users u ON u.id=h.actor WHERE {scope} AND (?3 IS NULL OR h.seq<?3) ORDER BY h.seq DESC LIMIT 31"))?;
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
                    completed: row.get(8)?,
                    actions: activity_actions(
                        &row.get::<_, String>(9)?,
                        row.get::<_, Option<String>>(10)?.as_deref(),
                    ),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > 30;
        items.truncate(30);
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
    fn history_describes_reopening_and_combined_edits() {
        let event =
            r#"{"kind":"update","task":{"complete":false,"title":"New","description":"Changed"}}"#;
        let before = r#"{"complete":true,"title":"Old","description":"Old description"}"#;
        assert_eq!(
            activity_actions(event, Some(before)),
            vec!["reopened", "title_changed", "description_changed"]
        );
    }
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
        task = db
            .update(
                "dev",
                &task.id,
                Change {
                    revision: task.revision,
                    title: None,
                    description: None,
                    complete: Some(true),
                    complete_subtree: false,
                },
            )
            .unwrap();
        let page = db.entity_activity("dev", "task", &task.id, None).unwrap();
        assert!(page.items[0].completed);
        assert_eq!(page.items[0].actions, vec!["completed"]);
        assert_eq!(page.items[1].actions, vec!["title_changed"]);
        assert!(!page.items[1].completed);
        assert_eq!(page.items.len(), 30);
        assert_eq!(page.items[0].undo_revision, Some(task.revision));
        let rest = db
            .entity_activity("dev", "task", &task.id, page.next_before)
            .unwrap();
        assert_eq!(rest.items.len(), 27);
        assert!(rest.next_before.is_none());
        assert!(page.items.last().unwrap().sequence > rest.items[0].sequence);
        let list_page = db.entity_activity("dev", "list", "inbox", None).unwrap();
        assert_eq!(list_page.items.len(), 30);
        assert!(list_page.next_before.is_some());
        assert!(db.entity_activity("other", "list", "inbox", None).is_err());
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
