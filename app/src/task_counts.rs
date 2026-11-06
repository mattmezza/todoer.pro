//! Small row indicators read together, using existing task and comment indexes.
use crate::core::{Error, Store};
use rusqlite::params;
use std::collections::HashMap;

#[derive(Clone, Copy, Default)]
pub(crate) struct TaskCounts {
    pub comments: u64,
    pub subtasks: u64,
}
impl Store {
    pub(crate) fn task_counts(
        &self,
        actor: &str,
        ids: &[&str],
    ) -> Result<HashMap<String, TaskCounts>, Error> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut query = self.conn.prepare(
            "SELECT t.id,
             (SELECT count(*) FROM comments c WHERE c.task_id=t.id AND c.deleted_at IS NULL),
             (SELECT count(*) FROM tasks child WHERE child.parent_id=t.id AND child.deleted_at IS NULL)
             FROM json_each(?2) ids JOIN tasks t ON t.id=ids.value AND t.deleted_at IS NULL
             JOIN actor_list_access a ON a.list_id=t.list_id AND a.actor_id=?1",
        )?;
        Ok(query
            .query_map(params![actor, serde_json::to_string(ids).unwrap()], |row| {
                Ok((
                    row.get(0)?,
                    TaskCounts {
                        comments: row.get::<_, i64>(1)? as u64,
                        subtasks: row.get::<_, i64>(2)? as u64,
                    },
                ))
            })?
            .collect::<Result<_, _>>()?)
    }
}

impl TaskCounts {
    pub(crate) fn html(self) -> String {
        let mut body = String::new();
        for (count, kind, icon) in [
            (self.subtasks, "subtask", "subtasks"),
            (self.comments, "comment", "comment"),
        ] {
            if count == 0 {
                continue;
            }
            let plural = if count == 1 { "" } else { "s" };
            body.push_str(&format!(r#"<span class="task-row-count task-{kind}-count" role="img" aria-label="{count} {kind}{plural}" title="{count} {kind}{plural}"><svg class="ui-icon" aria-hidden="true"><use href="/assets/icons.svg#{icon}"/></svg><span aria-hidden="true">{count}</span></span>"#));
        }
        if body.is_empty() {
            body
        } else {
            format!("<span class=\"task-indicators\">{body}</span>")
        }
    }
}
