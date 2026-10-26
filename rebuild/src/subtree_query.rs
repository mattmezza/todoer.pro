//! Bounded, revision-bound projected subtree reads. See docs/rebuild/subtree-query.md.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::core::{Error, Store, access_rank};

pub const MAX_PAGE_BYTES: usize = 256 * 1024;
const MAX_DEPTH: usize = 256;
const MAX_CONTEXT_BYTES: usize = 32 * 1024;
const MAX_PATH_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskField {
    Reference,
    ListId,
    Title,
    Description,
    Complete,
    Revision,
    Position,
}
impl TaskField {
    fn column(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::ListId => "list_id",
            Self::Title => "title",
            Self::Description => "description",
            Self::Complete => "complete",
            Self::Revision => "revision",
            Self::Position => "position",
        }
    }
}
fn default_limit() -> usize {
    50
}
fn default_fields() -> Vec<TaskField> {
    vec![
        TaskField::Reference,
        TaskField::Title,
        TaskField::Complete,
        TaskField::Revision,
        TaskField::Position,
    ]
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubtreeRead {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_fields")]
    pub fields: Vec<TaskField>,
    #[serde(default)]
    pub cursor: Option<String>,
}
impl Default for SubtreeRead {
    fn default() -> Self {
        Self {
            limit: default_limit(),
            fields: default_fields(),
            cursor: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubtreeSnapshot {
    pub root_revision: i64,
    pub list_revision: i64,
    pub list_sequence: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breadcrumb {
    pub id: String,
    pub parent_id: Option<String>,
    pub reference: String,
    pub revision: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub title_omitted: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtreeContext {
    /// Oldest available ancestor first, selected root last. No workspace metadata.
    pub breadcrumbs: Vec<Breadcrumb>,
    pub truncated: bool,
    pub before_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtreePage {
    pub root_id: String,
    pub list_id: String,
    pub snapshot: SubtreeSnapshot,
    pub fields: Vec<TaskField>,
    pub context: SubtreeContext,
    /// Each item always contains id, parent_id and depth (root = 0).
    pub items: Vec<Value>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    root: String,
    list: String,
    snapshot: SubtreeSnapshot,
    fields: Vec<TaskField>,
    last: String,
    path: String,
}
#[derive(Debug)]
struct Node {
    id: String,
    parent: Option<String>,
    position: i64,
    list: String,
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn resync() -> Error {
    Error::Conflict("subtree changed; restart without cursor".into())
}
fn segment(position: i64, id: &str) -> String {
    let hex: String = id
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect();
    format!(
        "{}{:016x}:{hex}/",
        if position < 0 { '0' } else { '1' },
        position as u64
    )
}
const SQL_SEGMENT: &str = "(CASE WHEN n.position<0 THEN '0' ELSE '1' END || printf('%016llx',n.position) || ':' || hex(CAST(n.id AS BLOB)) || '/')";

fn ancestors(
    c: &Connection,
    id: &str,
    stop: Option<&str>,
    limit: usize,
) -> Result<Vec<Node>, Error> {
    let mut q = c.prepare(
        "WITH RECURSIVE ancestors(id,parent_id,position,list_id,depth) AS (
        SELECT id,parent_id,position,list_id,0 FROM tasks WHERE id=?1 AND deleted_at IS NULL
        UNION ALL SELECT t.id,t.parent_id,t.position,t.list_id,a.depth+1 FROM ancestors a
        JOIN tasks t ON t.id=a.parent_id AND t.deleted_at IS NULL
        WHERE a.id!=coalesce(?2,'') AND a.depth<?3
    ) SELECT id,parent_id,position,list_id FROM ancestors LIMIT ?4",
    )?;
    Ok(
        q.query_map(params![id, stop, limit as i64, (limit + 1) as i64], |r| {
            Ok(Node {
                id: r.get(0)?,
                parent: r.get(1)?,
                position: r.get(2)?,
                list: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?,
    )
}
fn context(c: &Connection, root: &str, list: &str, titles: bool) -> Result<SubtreeContext, Error> {
    let nodes = ancestors(c, root, None, 64)?;
    if nodes.iter().any(|n| n.list != list) {
        return Err(invalid("invalid cross-list ancestry"));
    }
    // One query retrieves bounded metadata for all context nodes, never descriptions.
    let ids: Vec<&str> = nodes.iter().take(64).map(|n| n.id.as_str()).collect();
    let mut q = c.prepare(
        "SELECT id,parent_id,reference,revision,
        CASE WHEN ?2 AND octet_length(title)<=4096 THEN title END,
        ?2 AND octet_length(title)>4096 FROM tasks WHERE id IN (SELECT value FROM json_each(?1))",
    )?;
    let rows = q
        .query_map(params![serde_json::to_string(&ids).unwrap(), titles], |r| {
            Ok(Breadcrumb {
                id: r.get(0)?,
                parent_id: r.get(1)?,
                reference: r.get(2)?,
                revision: r.get(3)?,
                title: r.get(4)?,
                title_omitted: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut by_id: std::collections::HashMap<_, _> =
        rows.into_iter().map(|b| (b.id.clone(), b)).collect();
    let mut breadcrumbs: Vec<_> = ids
        .iter()
        .rev()
        .filter_map(|id| by_id.remove(*id))
        .collect();
    let mut truncated = nodes.len() > 64;
    while serde_json::to_vec(&breadcrumbs).unwrap().len() > MAX_CONTEXT_BYTES
        && breadcrumbs.len() > 1
    {
        breadcrumbs.remove(0);
        truncated = true;
    }
    let before_id = breadcrumbs.first().and_then(|b| b.parent_id.clone());
    truncated |= before_id.is_some();
    Ok(SubtreeContext {
        breadcrumbs,
        truncated,
        before_id,
    })
}
impl Store {
    pub fn read_subtree(
        &self,
        actor: &str,
        root: &str,
        mut read: SubtreeRead,
    ) -> Result<SubtreePage, Error> {
        if !(1..=100).contains(&read.limit) {
            return Err(invalid("limit must be between 1 and 100"));
        }
        read.fields.sort();
        read.fields.dedup();
        let tx = self.conn.unchecked_transaction()?;
        let (root_id,list_id,root_revision): (String,String,i64) = tx.query_row(
            "SELECT id,list_id,revision FROM tasks WHERE (id=?1 OR reference=?1) AND deleted_at IS NULL",[root],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or(Error::NotFound)?;
        access_rank(&tx, actor, &list_id, 1)?;
        let snapshot = SubtreeSnapshot {
            root_revision,
            list_revision: tx.query_row(
                "SELECT revision FROM lists WHERE id=?1",
                [&list_id],
                |r| r.get(0),
            )?,
            list_sequence: tx.query_row(
                "SELECT coalesce(max(seq),0) FROM history WHERE list_id=?1",
                [&list_id],
                |r| r.get(0),
            )?,
        };
        let mut seeds = vec![json!({"kind":"root","id":root_id,"depth":0,"prefix":0})];
        let mut last_path = String::new();
        if let Some(encoded) = &read.cursor {
            if encoded.len() > 128 * 1024 {
                return Err(invalid("cursor too large"));
            }
            let cursor: Cursor = URL_SAFE_NO_PAD
                .decode(encoded)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .ok_or_else(|| invalid("invalid subtree cursor"))?;
            if cursor.version != 1 || cursor.root != root_id || cursor.fields != read.fields {
                return Err(invalid(
                    "cursor belongs to a different subtree or projection",
                ));
            }
            if cursor.list != list_id || cursor.snapshot != snapshot {
                return Err(resync());
            }
            if cursor.path.len() > MAX_PATH_BYTES {
                return Err(invalid("cursor path too large"));
            }
            let mut chain = ancestors(&tx, &cursor.last, Some(&root_id), MAX_DEPTH)?;
            if chain.last().is_none_or(|n| n.id != root_id)
                || chain.len() > MAX_DEPTH + 1
                || chain.iter().any(|n| n.list != list_id)
            {
                return Err(invalid("cursor node is outside the bounded subtree"));
            }
            chain.reverse();
            let mut path = String::new();
            seeds.clear();
            for (depth, n) in chain.iter().enumerate().skip(1) {
                seeds.push(json!({"kind":"sibling","id":n.id,"parent":n.parent,"position":n.position,"depth":depth,"prefix":path.len()}));
                path.push_str(&segment(n.position, &n.id));
            }
            if path != cursor.path {
                return Err(invalid("invalid cursor ordering path"));
            }
            seeds.push(
                json!({"kind":"child","id":cursor.last,"depth":chain.len(),"prefix":path.len()}),
            );
            last_path = path;
        }
        let context = context(
            &tx,
            &root_id,
            &list_id,
            read.fields.contains(&TaskField::Title),
        )?;
        let mut page = SubtreePage {
            root_id: root_id.clone(),
            list_id: list_id.clone(),
            snapshot: snapshot.clone(),
            fields: read.fields.clone(),
            context,
            items: vec![],
            has_more: false,
            next_cursor: None,
        };
        let mut projection = String::from("'id',p.id,'parent_id',p.parent_id,'depth',w.depth");
        let mut sizes = String::from("octet_length(p.id)+coalesce(octet_length(p.parent_id),0)");
        for field in &read.fields {
            let col = field.column();
            projection.push_str(&format!(
                ",'{col}',{}",
                if *field == TaskField::Complete {
                    "json(CASE WHEN p.complete THEN 'true' ELSE 'false' END)".into()
                } else {
                    format!("p.{col}")
                }
            ));
            sizes.push_str(&format!("+coalesce(octet_length(p.{col}),0)"));
        }
        let sql=format!("WITH RECURSIVE walk(id,parent_id,position,list_id,path,parent_path,depth) AS (
          SELECT n.id,n.parent_id,n.position,n.list_id,
            CASE WHEN json_extract(s.value,'$.kind')='root' THEN '' ELSE substr(?2,1,json_extract(s.value,'$.prefix'))||{SQL_SEGMENT} END,
            substr(?2,1,json_extract(s.value,'$.prefix')),json_extract(s.value,'$.depth')
          FROM json_each(?1) s JOIN tasks n ON n.id=CASE json_extract(s.value,'$.kind')
            WHEN 'root' THEN json_extract(s.value,'$.id')
            WHEN 'child' THEN (SELECT x.id FROM tasks x INDEXED BY tasks_subtree_live_order WHERE x.parent_id=json_extract(s.value,'$.id') AND x.deleted_at IS NULL ORDER BY x.position,x.id LIMIT 1)
            ELSE (SELECT x.id FROM tasks x INDEXED BY tasks_subtree_live_order WHERE x.parent_id=json_extract(s.value,'$.parent') AND x.deleted_at IS NULL AND (x.position,x.id)>(json_extract(s.value,'$.position'),json_extract(s.value,'$.id')) ORDER BY x.position,x.id LIMIT 1) END
          UNION ALL
          SELECT n.id,n.parent_id,n.position,n.list_id,
            CASE WHEN b.kind=0 THEN w.path ELSE w.parent_path END||{SQL_SEGMENT},
            CASE WHEN b.kind=0 THEN w.path ELSE w.parent_path END,w.depth+CASE WHEN b.kind=0 THEN 1 ELSE 0 END
          FROM walk w CROSS JOIN (SELECT 0 kind UNION ALL SELECT 1) b
          JOIN tasks n ON n.id=CASE b.kind WHEN 0 THEN
            (SELECT x.id FROM tasks x INDEXED BY tasks_subtree_live_order WHERE x.parent_id=w.id AND x.deleted_at IS NULL ORDER BY x.position,x.id LIMIT 1)
            ELSE CASE WHEN w.id=?3 THEN NULL ELSE
            (SELECT x.id FROM tasks x INDEXED BY tasks_subtree_live_order WHERE x.parent_id=w.parent_id AND x.deleted_at IS NULL AND (x.position,x.id)>(w.position,w.id) ORDER BY x.position,x.id LIMIT 1) END END
          WHERE w.depth<=?4 ORDER BY 5 LIMIT ?5
        ) SELECT w.id,w.list_id,w.path,w.depth,
          (SELECT CASE WHEN p.list_id=?6 AND ({sizes})<=?7 THEN json_object({projection}) END FROM tasks p WHERE p.id=w.id)
          FROM walk w");
        let mut q = tx.prepare(&sql)?;
        let mut rows = q.query(params![
            serde_json::to_string(&seeds).unwrap(),
            last_path,
            root_id,
            MAX_DEPTH as i64,
            (read.limit + 1) as i64,
            list_id,
            MAX_PAGE_BYTES as i64
        ])?;
        while let Some(row) = rows.next()? {
            if page.items.len() == read.limit {
                page.has_more = true;
                break;
            }
            let id: String = row.get(0)?;
            let list: String = row.get(1)?;
            let path: String = row.get(2)?;
            let depth: i64 = row.get(3)?;
            if list != list_id {
                return Err(invalid("invalid cross-list subtree"));
            }
            if depth > MAX_DEPTH as i64 || path.len() > MAX_PATH_BYTES {
                return Err(invalid(
                    "subtree read depth/path limit exceeded; select a deeper root",
                ));
            }
            if id == root_id && depth != 0 {
                return Err(invalid("cyclic subtree"));
            }
            let item: Option<String> = row.get(4)?;
            let item = item.ok_or_else(|| {
                invalid("projected item exceeds response budget; omit large fields")
            })?;
            let item: Value = serde_json::from_str(&item)
                .map_err(|_| invalid("invalid stored task projection"))?;
            let cursor = URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&Cursor {
                    version: 1,
                    root: root_id.clone(),
                    list: list_id.clone(),
                    snapshot: snapshot.clone(),
                    fields: read.fields.clone(),
                    last: id,
                    path,
                })
                .unwrap(),
            );
            let previous = page.next_cursor.clone();
            page.items.push(item);
            page.next_cursor = Some(cursor);
            page.has_more = true;
            if serde_json::to_vec(&page).unwrap().len() > MAX_PAGE_BYTES {
                page.items.pop();
                page.next_cursor = previous;
                if page.items.is_empty() {
                    return Err(invalid(
                        "projected item exceeds response budget; omit large fields",
                    ));
                }
                break;
            }
            page.has_more = false;
        }
        if !page.has_more {
            page.next_cursor = None;
        }
        drop(rows);
        drop(q);
        tx.commit()?;
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Capture, Change, NewTask};
    fn store() -> Store {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        s
    }
    fn add(
        s: &mut Store,
        title: &str,
        parent: Option<&str>,
        description: &str,
    ) -> crate::core::Task {
        s.capture(
            "dev",
            &uuid::Uuid::new_v4().to_string(),
            Capture {
                list_id: "inbox".into(),
                tasks: vec![NewTask {
                    title: title.into(),
                    parent_id: parent.map(str::to_string),
                    description: description.into(),
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    fn read(s: &Store, root: &str, limit: usize, cursor: Option<String>) -> SubtreePage {
        s.read_subtree(
            "dev",
            root,
            SubtreeRead {
                limit,
                cursor,
                ..Default::default()
            },
        )
        .unwrap()
    }
    #[test]
    fn preorder_pages_projection_and_stable_context() {
        let mut s = store();
        let r = add(&mut s, "root", None, "");
        let a = add(&mut s, "a", Some(&r.id), "secret description");
        let b = add(&mut s, "b", Some(&r.id), "");
        let aa = add(&mut s, "aa", Some(&a.id), "");
        let ab = add(&mut s, "ab", Some(&a.id), "");
        let mut all = vec![];
        let mut cursor = None;
        loop {
            let p = read(&s, &r.reference, 2, cursor);
            assert_eq!(p.context.breadcrumbs.last().unwrap().id, r.id);
            for item in &p.items {
                assert!(item.get("description").is_none());
                all.push(item["id"].as_str().unwrap().to_string());
            }
            assert_eq!(p.has_more, p.next_cursor.is_some());
            if !p.has_more {
                break;
            }
            cursor = p.next_cursor;
        }
        assert_eq!(all, vec![r.id, a.id, aa.id, ab.id, b.id]);
    }
    #[test]
    fn cursor_rechecks_permissions_and_list_mutations() {
        let mut s = store();
        let r = add(&mut s, "root", None, "");
        let a = add(&mut s, "a", Some(&r.id), "");
        let p = read(&s, &r.id, 1, None);
        let cursor = p.next_cursor.unwrap();
        assert!(matches!(
            s.read_subtree(
                "other",
                &r.id,
                SubtreeRead {
                    cursor: Some(cursor.clone()),
                    ..Default::default()
                }
            ),
            Err(Error::Forbidden)
        ));
        s.update(
            "dev",
            &a.id,
            Change {
                revision: a.revision,
                title: Some("edited".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(
            s.read_subtree(
                "dev",
                &r.id,
                SubtreeRead {
                    cursor: Some(cursor),
                    ..Default::default()
                }
            ),
            Err(Error::Conflict(_))
        ));
    }
    #[test]
    fn payload_budget_never_skips_large_items() {
        let mut s = store();
        let r = add(&mut s, "root", None, &"x".repeat(99_000));
        for i in 0..5 {
            add(
                &mut s,
                &format!("child{i}"),
                Some(&r.id),
                &"x".repeat(99_000),
            );
        }
        let mut cursor = None;
        let mut count = 0;
        loop {
            let p = s
                .read_subtree(
                    "dev",
                    &r.id,
                    SubtreeRead {
                        limit: 100,
                        fields: vec![TaskField::Description],
                        cursor,
                    },
                )
                .unwrap();
            assert!(serde_json::to_vec(&p).unwrap().len() <= MAX_PAGE_BYTES);
            assert!(!p.items.is_empty());
            count += p.items.len();
            if !p.has_more {
                break;
            }
            cursor = p.next_cursor;
        }
        assert_eq!(count, 6);
        s.conn
            .execute(
                "UPDATE tasks SET description=?1 WHERE id=?2",
                params!["x".repeat(2_000_000), r.id],
            )
            .unwrap();
        assert!(
            s.read_subtree(
                "dev",
                &r.id,
                SubtreeRead {
                    fields: vec![TaskField::Description],
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            s.read_subtree(
                "dev",
                &r.id,
                SubtreeRead {
                    fields: vec![],
                    ..Default::default()
                }
            )
            .is_ok()
        );
    }
    #[test]
    fn depth_guard_can_resume_at_deeper_root() {
        let mut s = store();
        let r = add(&mut s, "root", None, "");
        let mut parent = r.id.clone();
        let tx = s.conn.transaction().unwrap();
        for i in 0..258 {
            let id = format!("deep{i}");
            tx.execute("INSERT INTO tasks(id,reference,list_id,parent_id,title,description,position) VALUES(?1,?1,'inbox',?2,'deep','',0)",params![id,parent]).unwrap();
            parent = id;
        }
        tx.commit().unwrap();
        let mut cursor = None;
        let mut errored = false;
        for _ in 0..4 {
            match s.read_subtree(
                "dev",
                &r.id,
                SubtreeRead {
                    limit: 100,
                    fields: vec![],
                    cursor: cursor.clone(),
                },
            ) {
                Ok(p) => cursor = p.next_cursor,
                Err(Error::Invalid(_)) => {
                    errored = true;
                    break;
                }
                Err(e) => panic!("{e}"),
            }
        }
        assert!(errored);
        assert!(read(&s, "deep250", 10, None).items.len() > 1);
    }
    #[test]
    fn malformed_cursors_and_changed_projection_are_rejected() {
        let mut s = store();
        let r = add(&mut s, "root", None, "");
        add(&mut s, "child", Some(&r.id), "");
        let p = read(&s, &r.id, 1, None);
        assert!(
            s.read_subtree(
                "dev",
                &r.id,
                SubtreeRead {
                    fields: vec![],
                    cursor: p.next_cursor,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            s.read_subtree(
                "dev",
                &r.id,
                SubtreeRead {
                    cursor: Some("nonsense".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            s.read_subtree(
                "dev",
                &r.id,
                SubtreeRead {
                    limit: 101,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn wide_tree_signed_order_and_late_resume_use_indexed_seeks() {
        let mut s = store();
        let r = add(&mut s, "root", None, "");
        let tx = s.conn.transaction().unwrap();
        for i in 0..3000 {
            let id = format!("wide{i:04}");
            tx.execute("INSERT INTO tasks(id,reference,list_id,parent_id,title,description,position) VALUES(?1,?1,'inbox',?2,?1,?3,?4)",params![id,r.id,"x".repeat(4096),i as i64-1500]).unwrap();
        }
        tx.commit().unwrap();
        let mut cursor = None;
        let mut ids = vec![];
        loop {
            let p = s
                .read_subtree(
                    "dev",
                    &r.id,
                    SubtreeRead {
                        limit: 100,
                        fields: vec![],
                        cursor,
                    },
                )
                .unwrap();
            ids.extend(p.items.iter().map(|v| v["id"].as_str().unwrap().to_owned()));
            if !p.has_more {
                break;
            }
            cursor = p.next_cursor;
        }
        assert_eq!(ids.len(), 3001);
        assert_eq!(ids[0], r.id);
        assert_eq!(
            &ids[1..],
            (0..3000).map(|i| format!("wide{i:04}")).collect::<Vec<_>>()
        );
        let plan:String=s.conn.query_row("EXPLAIN QUERY PLAN SELECT id FROM tasks INDEXED BY tasks_subtree_live_order WHERE parent_id=?1 AND deleted_at IS NULL AND (position,id)>(?2,?3) ORDER BY position,id LIMIT 1",params![r.id,1490,"wide2990"],|r|r.get(3)).unwrap();
        assert!(
            plan.contains("SEARCH tasks USING INDEX tasks_subtree_live_order"),
            "{plan}"
        );
    }
    #[test]
    fn unrelated_list_changes_do_not_invalidate_and_context_has_no_descriptions() {
        let mut s = store();
        let ancestor = add(&mut s, "ancestor", None, "private description");
        let root = add(&mut s, "root", Some(&ancestor.id), "");
        add(&mut s, "child", Some(&root.id), "");
        let p = read(&s, &root.id, 1, None);
        assert_eq!(
            p.context
                .breadcrumbs
                .iter()
                .map(|b| b.id.as_str())
                .collect::<Vec<_>>(),
            vec![ancestor.id.as_str(), root.id.as_str()]
        );
        assert!(
            !serde_json::to_string(&p.context)
                .unwrap()
                .contains("description")
        );
        s.capture(
            "dev",
            "unrelated",
            Capture {
                list_id: "groceries".into(),
                tasks: vec![NewTask {
                    title: "unrelated".into(),
                    description: String::new(),
                    parent_id: None,
                }],
            },
        )
        .unwrap();
        assert!(
            s.read_subtree(
                "dev",
                &root.id,
                SubtreeRead {
                    cursor: p.next_cursor,
                    ..Default::default()
                }
            )
            .is_ok()
        );
    }
    #[test]
    fn forged_cursor_cannot_reach_another_subtree_or_private_list() {
        let mut s = store();
        let root = add(&mut s, "root", None, "");
        add(&mut s, "child", Some(&root.id), "");
        let unrelated = add(&mut s, "unrelated", None, "");
        let private = s
            .capture(
                "other",
                "private",
                Capture {
                    list_id: "private".into(),
                    tasks: vec![NewTask {
                        title: "private secret".into(),
                        description: "private contents".into(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let page = read(&s, &root.id, 1, None);
        let encoded = page.next_cursor.unwrap();
        for id in [unrelated.id, private.id] {
            let mut cursor: Cursor =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&encoded).unwrap()).unwrap();
            cursor.last = id;
            cursor.path = String::new();
            let forged = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).unwrap());
            assert!(matches!(
                s.read_subtree(
                    "dev",
                    &root.id,
                    SubtreeRead {
                        cursor: Some(forged),
                        ..Default::default()
                    }
                ),
                Err(Error::Invalid(_))
            ));
        }
    }
}
