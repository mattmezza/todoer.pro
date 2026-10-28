//! Authorized task taxonomy, structural ordering and persistent entity references.
use crate::core::{Error, Store, Task, access_rank, actor_identity, event, task};
use pulldown_cmark::{CowStr, Event, Parser, Tag as MarkdownTag, TagEnd};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub revision: i64,
}
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TagSelector {
    Name(String),
    Id { id: String },
}
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceChoice {
    pub prefix: &'static str,
    pub kind: String,
    pub id: String,
    pub label: String,
    pub context: String,
}
#[derive(Debug, Serialize)]
pub struct ReferenceResolution {
    pub token: String,
    pub occurrence: usize,
    pub choices: Vec<ReferenceChoice>,
}
#[derive(Debug, Serialize)]
pub struct Backlink {
    pub source_kind: String,
    pub source_id: String,
    pub source_task_id: String,
}
#[derive(Debug)]
struct Token {
    start: usize,
    end: usize,
    kind: &'static str,
    query: String,
    raw: String,
}
fn normalized(name: &str) -> Result<(String, String), Error> {
    let display = name.trim();
    if display.is_empty() || display.chars().count() > 100 || display.chars().any(char::is_control)
    {
        return Err(Error::Invalid(
            "tag name must contain 1–100 non-control characters".into(),
        ));
    }
    Ok((display.to_owned(), display.to_lowercase()))
}
fn scope(c: &Connection, list: &str) -> Result<String, Error> {
    let workspace: Option<String> =
        c.query_row("SELECT workspace_id FROM lists WHERE id=?1", [list], |r| {
            r.get(0)
        })?;
    Ok(workspace
        .map(|id| format!("workspace:{id}"))
        .unwrap_or_else(|| format!("list:{list}")))
}
fn tag_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        name: row.get(1)?,
        revision: row.get(2)?,
    })
}
fn ensure_tag(c: &Connection, home: &str, name: &str) -> Result<Tag, Error> {
    let (name, normalized) = normalized(name)?;
    c.execute("INSERT INTO tags(id,scope,name,normalized_name) VALUES(?1,?2,?3,?4) ON CONFLICT(scope,normalized_name) DO NOTHING", params![Uuid::new_v4().to_string(),home,name,normalized])?;
    Ok(c.query_row(
        "SELECT id,name,revision FROM tags WHERE scope=?1 AND normalized_name=?2",
        params![home, normalized],
        tag_row,
    )?)
}
fn subtree_ids(c: &Connection, id: &str) -> Result<Vec<String>, Error> {
    let mut stmt = c.prepare("WITH RECURSIVE tree(id) AS (SELECT id FROM tasks WHERE id=?1 AND deleted_at IS NULL UNION ALL SELECT t.id FROM tasks t JOIN tree ON t.parent_id=tree.id WHERE t.deleted_at IS NULL) SELECT id FROM tree")?;
    Ok(stmt
        .query_map([id], |r| r.get(0))?
        .collect::<Result<_, _>>()?)
}
fn cas_task(c: &Connection, old: &Task, revision: i64) -> Result<(), Error> {
    if old.revision != revision {
        return Err(Error::Conflict("stale revision".into()));
    }
    let changed = c.execute(
        "UPDATE tasks SET revision=revision+1 WHERE id=?1 AND revision=?2",
        params![old.id, revision],
    )?;
    if changed != 1 {
        return Err(Error::Conflict("stale revision".into()));
    }
    Ok(())
}
pub(crate) fn location(c: &Connection, id: &str) -> Result<Option<String>, Error> {
    Ok(c.query_row(
        "WITH RECURSIVE ancestors(id,parent_id,depth) AS (SELECT id,parent_id,0 FROM tasks WHERE id=?1 UNION ALL SELECT t.id,t.parent_id,a.depth+1 FROM tasks t JOIN ancestors a ON t.id=a.parent_id) SELECT l.section_id FROM ancestors a JOIN task_locations l ON l.task_id=a.id WHERE NOT EXISTS(SELECT 1 FROM list_defaults d WHERE d.section_id=l.section_id) ORDER BY a.depth LIMIT 1",
        [id],
        |r| r.get(0),
    )
    .optional()?)
}
fn assign_location(c: &Connection, ids: &[String], section: Option<&str>) -> Result<(), Error> {
    for id in ids {
        if let Some(section) = section {
            c.execute("INSERT INTO task_locations(task_id,section_id) VALUES(?1,?2) ON CONFLICT(task_id) DO UPDATE SET section_id=excluded.section_id",params![id,section])?;
        } else {
            c.execute("DELETE FROM task_locations WHERE task_id=?1", [id])?;
            c.execute("INSERT INTO task_locations(task_id,section_id) SELECT t.id,d.section_id FROM tasks t JOIN list_defaults d ON d.list_id=t.list_id WHERE t.id=?1 AND t.parent_id IS NULL",[id])?;
        }
    }
    Ok(())
}
impl Store {
    pub fn task_tags(&self, actor: &str, id: &str) -> Result<Vec<Tag>, Error> {
        let t = task(&self.conn, id)?;
        access_rank(&self.conn, actor, &t.list_id, 1)?;
        let mut stmt=self.conn.prepare("SELECT g.id,g.name,g.revision FROM tags g JOIN task_tags tt ON tt.tag_id=g.id WHERE tt.task_id=?1 ORDER BY g.normalized_name,g.id")?;
        Ok(stmt.query_map([t.id], tag_row)?.collect::<Result<_, _>>()?)
    }
    pub fn set_task_tags(
        &mut self,
        actor: &str,
        id: &str,
        revision: i64,
        tags: Vec<TagSelector>,
    ) -> Result<Task, Error> {
        if tags.len() > 50 {
            return Err(Error::Invalid("at most 50 tags per task".into()));
        }
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let old = task(&tx, id)?;
        access_rank(&tx, actor, &old.list_id, 2)?;
        let home = scope(&tx, &old.list_id)?;
        cas_task(&tx, &old, revision)?;
        let mut chosen = HashSet::new();
        for selector in tags {
            let tag = match selector {
                TagSelector::Name(name) => ensure_tag(&tx, &home, &name)?,
                TagSelector::Id { id } => tx
                    .query_row(
                        "SELECT id,name,revision FROM tags WHERE id=?1 AND scope=?2",
                        params![id, home],
                        tag_row,
                    )
                    .optional()?
                    .ok_or(Error::NotFound)?,
            };
            chosen.insert(tag.id);
        }
        tx.execute("DELETE FROM task_tags WHERE task_id=?1", [&old.id])?;
        for tag in chosen {
            tx.execute("INSERT INTO task_tags VALUES(?1,?2)", params![old.id, tag])?;
        }
        let updated = task(&tx, &old.id)?;
        event(&tx, &updated, "tags", actor, Some(&old))?;
        tx.commit()?;
        Ok(updated)
    }
    pub fn rename_tag(
        &mut self,
        actor: &str,
        id: &str,
        revision: i64,
        name: &str,
    ) -> Result<Tag, Error> {
        let (name, normalized) = normalized(name)?;
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let home: String = tx
            .query_row("SELECT scope FROM tags WHERE id=?1", [id], |r| r.get(0))
            .optional()?
            .ok_or(Error::NotFound)?;
        if let Some(workspace) = home.strip_prefix("workspace:") {
            workspace_visible(&tx, actor, workspace, 2)?;
        } else {
            access_rank(
                &tx,
                actor,
                home.strip_prefix("list:").ok_or(Error::NotFound)?,
                2,
            )?;
        }
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tags WHERE scope=?1 AND normalized_name=?2 AND id<>?3)",
            params![home, normalized, id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(Error::Conflict("tag name already exists".into()));
        }
        if tx.execute("UPDATE tags SET name=?1,normalized_name=?2,revision=revision+1 WHERE id=?3 AND revision=?4",params![name,normalized,id,revision])?!=1 { return Err(Error::Conflict("stale revision".into())); }
        let tag = tx.query_row(
            "SELECT id,name,revision FROM tags WHERE id=?1",
            [id],
            tag_row,
        )?;
        tx.commit()?;
        Ok(tag)
    }
    pub fn task_section(&self, actor: &str, id: &str) -> Result<Option<String>, Error> {
        let t = task(&self.conn, id)?;
        access_rank(&self.conn, actor, &t.list_id, 1)?;
        location(&self.conn, &t.id)
    }
    pub fn set_section(
        &mut self,
        actor: &str,
        id: &str,
        revision: i64,
        section: Option<String>,
    ) -> Result<Task, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let old = task(&tx, id)?;
        access_rank(&tx, actor, &old.list_id, 2)?;
        if old.parent_id.is_some() {
            return Err(Error::Invalid("assign sections on subtree roots".into()));
        }
        if let Some(section) = &section
            && !tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sections WHERE id=?1 AND list_id=?2)",
                params![section, old.list_id],
                |r| r.get::<_, bool>(0),
            )?
        {
            return Err(Error::NotFound);
        }
        cas_task(&tx, &old, revision)?;
        let ids = subtree_ids(&tx, &old.id)?;
        assign_location(&tx, &ids, section.as_deref())?;
        for child in ids.iter().filter(|id| **id != old.id) {
            let before = task(&tx, child)?;
            tx.execute("UPDATE tasks SET revision=revision+1 WHERE id=?1", [child])?;
            event(&tx, &task(&tx, child)?, "section", actor, Some(&before))?;
        }
        let updated = task(&tx, &old.id)?;
        event(&tx, &updated, "section", actor, Some(&old))?;
        tx.commit()?;
        Ok(updated)
    }
    /// Promote one level, preserving the subtree, only after confirming its snapshot.
    pub fn promote_subtree(
        &mut self,
        actor: &str,
        id: &str,
        expected: crate::subtree_query::SubtreeSnapshot,
    ) -> Result<Task, Error> {
        let mut write = crate::agent_ops::StoreWrite::new(self)?;
        let old = task(&write.conn, id)?;
        access_rank(&write.conn, actor, &old.list_id, 2)?;
        let parent = old
            .parent_id
            .as_ref()
            .ok_or_else(|| Error::Invalid("This task is already top-level".into()))?;
        let current = crate::subtree_query::SubtreeSnapshot {
            root_revision: old.revision,
            list_revision: write.conn.query_row(
                "SELECT revision FROM lists WHERE id=?1",
                [&old.list_id],
                |r| r.get(0),
            )?,
            list_sequence: write.conn.query_row(
                "SELECT coalesce(max(seq),0) FROM history WHERE list_id=?1",
                [&old.list_id],
                |r| r.get(0),
            )?,
        };
        if current != expected {
            return Err(Error::Conflict(
                "The hierarchy changed. Review the move again.".into(),
            ));
        }
        let destination = task(&write.conn, parent)?.parent_id;
        let result = write.reparent_subtree(actor, &old.id, destination, old.revision)?;
        write.commit()?;
        Ok(result)
    }

    pub fn reparent_subtree(
        &mut self,
        actor: &str,
        id: &str,
        parent: Option<String>,
        revision: i64,
    ) -> Result<Task, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let old = task(&tx, id)?;
        access_rank(&tx, actor, &old.list_id, 2)?;
        let ids = subtree_ids(&tx, &old.id)?;
        let parent = if let Some(parent) = parent {
            let p = task(&tx, &parent)?;
            if p.list_id != old.list_id {
                return Err(Error::Invalid(
                    "reparent within one list; use move for another list".into(),
                ));
            }
            if ids.contains(&p.id) {
                return Err(Error::Conflict("reparent would create a cycle".into()));
            }
            Some(p)
        } else {
            None
        };
        cas_task(&tx, &old, revision)?;
        let parent_id = parent.as_ref().map(|p| p.id.as_str());
        tx.execute("UPDATE tasks SET parent_id=?1,position=(SELECT COALESCE(MAX(position),0)+1 FROM tasks WHERE list_id=?2) WHERE id=?3",params![parent_id,old.list_id,old.id])?;
        let section = match parent {
            Some(p) => location(&tx, &p.id)?,
            None => None,
        };
        let mut changed_children = Vec::new();
        for child in ids.iter().filter(|id| **id != old.id) {
            if location(&tx, child)? != section {
                changed_children.push(task(&tx, child)?);
            }
        }
        assign_location(&tx, &ids, section.as_deref())?;
        for before in changed_children {
            tx.execute(
                "UPDATE tasks SET revision=revision+1 WHERE id=?1",
                [&before.id],
            )?;
            event(
                &tx,
                &task(&tx, &before.id)?,
                "section",
                actor,
                Some(&before),
            )?;
        }
        let updated = task(&tx, &old.id)?;
        event(&tx, &updated, "reparent", actor, Some(&old))?;
        // Reparenting an unfinished subtree must not leave a completed ancestor.
        let mut ancestor = updated.parent_id.clone();
        while let Some(id) = ancestor {
            let before = task(&tx, &id)?;
            ancestor = before.parent_id.clone();
            if before.complete && !updated.complete {
                tx.execute(
                    "UPDATE tasks SET complete=0,revision=revision+1 WHERE id=?1",
                    [&id],
                )?;
                event(&tx, &task(&tx, &id)?, "update", actor, Some(&before))?;
            }
        }
        tx.execute(
            "UPDATE lists SET revision=revision+1 WHERE id=?1",
            [&old.list_id],
        )?;
        tx.commit()?;
        Ok(updated)
    }
    pub fn reorder_siblings(
        &mut self,
        actor: &str,
        list: &str,
        parent: Option<String>,
        revision: i64,
        ids: Vec<String>,
    ) -> Result<Vec<Task>, Error> {
        if ids.len() > 10000 {
            return Err(Error::Invalid("sibling order too large".into()));
        }
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        access_rank(&tx, actor, list, 2)?;
        let parent = if let Some(id) = parent {
            let resolved = task(&tx, &id)?;
            if resolved.list_id != list {
                return Err(Error::Invalid("parent belongs to another list".into()));
            }
            Some(resolved.id)
        } else {
            None
        };
        let mut stmt = tx.prepare(
            "SELECT id FROM tasks WHERE list_id=?1 AND parent_id IS ?2 AND deleted_at IS NULL",
        )?;
        let existing: HashSet<String> = stmt
            .query_map(params![list, parent], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        drop(stmt);
        if existing.len() != ids.len() || existing != ids.iter().cloned().collect() {
            return Err(Error::Conflict(
                "order must contain every active sibling exactly once".into(),
            ));
        }
        if tx.execute(
            "UPDATE lists SET revision=revision+1 WHERE id=?1 AND revision=?2",
            params![list, revision],
        )? != 1
        {
            return Err(Error::Conflict("stale list revision".into()));
        }
        let mut result = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            let before = task(&tx, id)?;
            tx.execute(
                "UPDATE tasks SET position=?1,revision=revision+1 WHERE id=?2",
                params![index as i64 + 1, id],
            )?;
            let updated = task(&tx, id)?;
            event(&tx, &updated, "reorder", actor, Some(&before))?;
            result.push(updated);
        }
        tx.commit()?;
        Ok(result)
    }
}
/// Called inside the existing atomic cross-list move transaction, after list IDs
/// change. Both source and destination authorization must already be checked.
pub(crate) fn after_move(
    c: &Connection,
    actor: &str,
    ids: &[String],
    destination: &str,
) -> Result<(), Error> {
    assign_location(c, ids, None)?;
    remap_tags_after_move(c, actor, ids, destination)
}
/// Remap only tag scopes. Moving a whole list between workspaces must retain
/// its existing sections and task_locations, unlike a cross-list subtree move.
pub(crate) fn remap_tags_after_move(
    c: &Connection,
    actor: &str,
    ids: &[String],
    destination: &str,
) -> Result<(), Error> {
    let home = scope(c, destination)?;
    for id in ids {
        let mut stmt=c.prepare("SELECT g.id,g.name,g.scope FROM tags g JOIN task_tags tt ON tt.tag_id=g.id WHERE tt.task_id=?1")?;
        let tags = stmt
            .query_map([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (source, name, old_scope) in tags {
            if old_scope == home {
                continue;
            }
            let target = ensure_tag(c, &home, &name)?;
            c.execute(
                "DELETE FROM task_tags WHERE task_id=?1 AND tag_id=?2",
                params![id, source],
            )?;
            c.execute(
                "INSERT OR IGNORE INTO task_tags VALUES(?1,?2)",
                params![id, target.id],
            )?;
            c.execute("INSERT INTO tag_move_audit(task_id,source_tag_id,destination_tag_id,actor) VALUES(?1,?2,?3,?4)",params![id,source,target.id,actor_identity(c,actor)?.0])?;
        }
    }
    Ok(())
}

fn workspace_visible(c: &Connection, actor: &str, id: &str, rank: i64) -> Result<(), Error> {
    let permitted:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM actor_workspace_access WHERE workspace_id=?1 AND actor_id=?2 AND rank>=?3)",params![id,actor,rank],|r|r.get(0))?;
    if permitted {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}
fn visible_tag(c: &Connection, actor: &str, id: &str) -> Result<bool, Error> {
    let home: Option<String> = c
        .query_row("SELECT scope FROM tags WHERE id=?1", [id], |r| r.get(0))
        .optional()?;
    let Some(home) = home else { return Ok(false) };
    let used:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM task_tags tt JOIN tasks t ON t.id=tt.task_id JOIN actor_list_access a ON a.list_id=t.list_id WHERE tt.tag_id=?1 AND a.actor_id=?2 AND t.deleted_at IS NULL)",params![id,actor],|r|r.get(0))?;
    if used {
        return Ok(true);
    }
    Ok(if let Some(id) = home.strip_prefix("workspace:") {
        workspace_visible(c, actor, id, 1).is_ok()
    } else if let Some(id) = home.strip_prefix("list:") {
        access_rank(c, actor, id, 1).is_ok()
    } else {
        false
    })
}
fn source_task(
    c: &Connection,
    actor: &str,
    kind: &str,
    id: &str,
    rank: i64,
) -> Result<(Task, String), Error> {
    let (task_id, raw) = match kind {
        "task" => {
            let t = task(c, id)?;
            let raw = t.description.clone();
            (t.id, raw)
        }
        "comment" => c
            .query_row(
                "SELECT task_id,body FROM comments WHERE id=?1 AND deleted_at IS NULL",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or(Error::NotFound)?,
        _ => {
            return Err(Error::Invalid(
                "reference source must be task or comment".into(),
            ));
        }
    };
    let t = task(c, &task_id)?;
    access_rank(c, actor, &t.list_id, rank)?;
    Ok((t, raw))
}
fn prefix_kind(ch: char) -> Option<&'static str> {
    match ch {
        '@' => Some("workspace"),
        '#' => Some("list"),
        '$' => Some("section"),
        '%' | '^' => Some("task"),
        '+' => Some("tag"),
        _ => None,
    }
}
fn entity_prefix(kind: &str) -> &'static str {
    match kind { "workspace" => "@", "list" => "#", "section" => "$", "task" => "%", "tag" => "+", _ => "" }
}
fn matches_task_prefix(token: &str, choice: &ReferenceChoice) -> bool {
    choice.kind != "task" || token.starts_with(choice.prefix)
}
fn token_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '-')
}
fn markdown_events(raw: &str) -> Vec<(Event<'static>, std::ops::Range<usize>)> {
    let options =
        pulldown_cmark::Options::ENABLE_STRIKETHROUGH | pulldown_cmark::Options::ENABLE_TABLES;
    let mut events: Vec<(Event<'static>, std::ops::Range<usize>)> = Vec::new();
    for (event, range) in Parser::new_ext(raw, options).into_offset_iter() {
        if let Event::Text(text) = &event
            && let Some((Event::Text(previous), previous_range)) = events.last_mut()
            && previous_range.end == range.start
        {
            *previous = CowStr::from(format!("{previous}{text}"));
            previous_range.end = range.end;
            continue;
        }
        events.push((event.into_static(), range));
    }
    events
}
/// CommonMark source ranges keep escaped prefixes and original byte offsets.
/// Code, fenced blocks, existing links/images and autolinks are excluded.
fn tokens(raw: &str) -> Vec<Token> {
    let mut result = Vec::new();
    let mut excluded = 0usize;
    for (event, range) in markdown_events(raw) {
        match event {
            Event::Start(
                MarkdownTag::CodeBlock(_) | MarkdownTag::Link { .. } | MarkdownTag::Image { .. },
            ) => {
                excluded += 1;
                continue;
            }
            Event::End(TagEnd::CodeBlock | TagEnd::Link | TagEnd::Image) => {
                excluded = excluded.saturating_sub(1);
                continue;
            }
            Event::Text(_) if excluded == 0 => {}
            _ => continue,
        }
        let mut cursor = range.start;
        while cursor < range.end {
            let ch = raw[cursor..].chars().next().unwrap();
            let Some(kind) = prefix_kind(ch) else {
                cursor += ch.len_utf8();
                continue;
            };
            let previous = raw[..cursor].chars().next_back();
            let escaped = raw[..cursor]
                .chars()
                .rev()
                .take_while(|c| *c == '\\')
                .count()
                % 2
                == 1;
            if escaped
                || previous.is_some_and(|c| {
                    token_char(c) || matches!(c, '/' | '@' | '#' | '$' | '%' | '^' | '+')
                })
            {
                cursor += ch.len_utf8();
                continue;
            }
            let start = cursor;
            cursor += ch.len_utf8();
            let query_start;
            let query_end;
            if raw[cursor..range.end].starts_with('[') {
                cursor += 1;
                query_start = cursor;
                while cursor < range.end && !raw[cursor..].starts_with(']') {
                    let c = raw[cursor..].chars().next().unwrap();
                    if matches!(c, '\n' | '\r' | '[') {
                        break;
                    }
                    cursor += c.len_utf8();
                }
                if cursor >= range.end || !raw[cursor..].starts_with(']') {
                    continue;
                }
                query_end = cursor;
                cursor += 1;
            } else {
                query_start = cursor;
                while cursor < range.end {
                    let c = raw[cursor..].chars().next().unwrap();
                    if !token_char(c) {
                        break;
                    }
                    cursor += c.len_utf8();
                }
                query_end = cursor;
            }
            if query_end == query_start {
                continue;
            }
            let query = raw[query_start..query_end].to_owned();
            if query.chars().count() > 100 {
                continue;
            }
            result.push(Token {
                start,
                end: cursor,
                kind,
                query,
                raw: raw[start..cursor].to_owned(),
            });
            if result.len() >= 200 {
                return result;
            }
        }
    }
    result
}
fn list_context(c: &Connection, actor: &str, id: &str) -> Result<String, Error> {
    let workspace: Option<String> =
        c.query_row("SELECT workspace_id FROM lists WHERE id=?1", [id], |r| {
            r.get(0)
        })?;
    if let Some(workspace) = workspace
        && workspace_visible(c, actor, &workspace, 1).is_ok()
    {
        return Ok(c.query_row(
            "SELECT title FROM workspaces WHERE id=?1",
            [workspace],
            |r| r.get(0),
        )?);
    }
    Ok(format!("List {id}"))
}
fn tag_context(c: &Connection, actor: &str, id: &str) -> Result<String, Error> {
    let list:Option<String>=c.query_row("SELECT l.title FROM task_tags tt JOIN tasks t ON t.id=tt.task_id JOIN lists l ON l.id=t.list_id JOIN actor_list_access a ON a.list_id=l.id WHERE tt.tag_id=?1 AND a.actor_id=?2 AND t.deleted_at IS NULL ORDER BY l.id LIMIT 1",params![id,actor],|r|r.get(0)).optional()?;
    if let Some(list) = list {
        return Ok(list);
    }
    let home: String = c.query_row("SELECT scope FROM tags WHERE id=?1", [id], |r| r.get(0))?;
    if let Some(workspace) = home.strip_prefix("workspace:")
        && workspace_visible(c, actor, workspace, 1).is_ok()
    {
        return Ok(c.query_row(
            "SELECT title FROM workspaces WHERE id=?1",
            [workspace],
            |r| r.get(0),
        )?);
    }
    Ok(format!("Tag {id}"))
}
fn candidates(c: &Connection, actor: &str, kind: &str) -> Result<Vec<ReferenceChoice>, Error> {
    let mut result = Vec::new();
    match kind {
        "workspace" => {
            let mut stmt = c.prepare("SELECT w.id,w.title FROM workspaces w JOIN actor_workspace_access a ON a.workspace_id=w.id WHERE a.actor_id=?1 ORDER BY w.id LIMIT 1000")?;
            for row in stmt.query_map([actor], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })? {
                let (id, title) = row?;
                if workspace_visible(c, actor, &id, 1).is_ok() {
                    result.push(ReferenceChoice {
                        prefix: entity_prefix(kind),
                        kind: kind.into(),
                        id,
                        label: title,
                        context: String::new(),
                    });
                }
            }
        }
        "list" => {
            let mut stmt=c.prepare("SELECT l.id,l.title FROM lists l JOIN actor_list_access a ON a.list_id=l.id WHERE a.actor_id=?1 AND l.archived=0 AND NOT EXISTS(SELECT 1 FROM user_inboxes i WHERE i.list_id=l.id) ORDER BY l.id LIMIT 1000")?;
            for row in stmt.query_map([actor], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })? {
                let (id, label) = row?;
                let context = list_context(c, actor, &id)?;
                result.push(ReferenceChoice {
                    prefix: entity_prefix(kind),
                    kind: kind.into(),
                    id,
                    label,
                    context,
                });
            }
        }
        "section" => {
            let mut stmt=c.prepare("SELECT s.id,s.title,l.title FROM sections s JOIN lists l ON l.id=s.list_id JOIN actor_list_access a ON a.list_id=l.id WHERE a.actor_id=?1 AND l.archived=0 ORDER BY s.id LIMIT 1000")?;
            for row in stmt.query_map([actor], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))? {
                let (id, label, context) = row?;
                result.push(ReferenceChoice {
                    prefix: entity_prefix(kind),
                    kind: kind.into(),
                    id,
                    label,
                    context,
                });
            }
        }
        "task" => {
            let mut stmt=c.prepare("SELECT t.id,t.title,l.title||' · '||t.reference,t.parent_id IS NOT NULL FROM tasks t JOIN lists l ON l.id=t.list_id JOIN actor_list_access a ON a.list_id=l.id WHERE a.actor_id=?1 AND t.deleted_at IS NULL AND l.archived=0 ORDER BY t.id LIMIT 1000")?;
            for row in stmt.query_map([actor], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_,bool>(3)?)))? {
                let (id, label, context, subtask) = row?;
                result.push(ReferenceChoice {
                    prefix: if subtask { "^" } else { "%" },
                    kind: kind.into(),
                    id,
                    label,
                    context,
                });
            }
        }
        "tag" => {
            let mut stmt = c.prepare("SELECT id,name FROM tags ORDER BY id LIMIT 1000")?;
            for row in
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            {
                let (id, label) = row?;
                if visible_tag(c, actor, &id)? {
                    let context = tag_context(c, actor, &id)?;
                    result.push(ReferenceChoice {
                        prefix: entity_prefix(kind),
                        kind: kind.into(),
                        id,
                        label,
                        context,
                    });
                }
            }
        }
        _ => return Err(Error::Invalid("unknown reference kind".into())),
    }
    Ok(result)
}
fn exact_matches(
    c: &Connection,
    actor: &str,
    token: &Token,
) -> Result<Vec<ReferenceChoice>, Error> {
    if let Some((label, _)) = target(c, actor, token.kind, &token.query)? {
        let choice = ReferenceChoice {
            prefix: if token.kind == "task" { task(c, &token.query)?.reference_prefix() } else { entity_prefix(token.kind) },
            kind: token.kind.into(),
            id: if token.kind == "task" {
                task(c, &token.query)?.id
            } else {
                crate::defaults::resolve(c, token.kind, &token.query)?
            },
            label,
            context: String::new(),
        };
        return Ok(if matches_task_prefix(&token.raw, &choice) { vec![choice] } else { vec![] });
    }
    let query = token.query.to_lowercase();
    let mut choices = candidates(c, actor, token.kind)?;
    choices.retain(|choice| {
        choice.id == token.query
            || choice.label.to_lowercase() == query
            || (token.kind == "task"
                && (choice.id.starts_with(&token.query) && token.query.len() >= 8
                    || choice
                        .context
                        .rsplit(" · ")
                        .next()
                        .is_some_and(|reference| reference.to_lowercase() == query)))
    });
    if token.kind == "task" && token.query.len() >= 8 {
        let mut stmt=c.prepare("SELECT t.id,t.title,l.title||' · '||t.reference,t.parent_id IS NOT NULL FROM tasks t JOIN lists l ON l.id=t.list_id JOIN actor_list_access a ON a.list_id=l.id WHERE a.actor_id=?1 AND substr(t.id,1,length(?2))=?2 AND t.deleted_at IS NULL ORDER BY t.id LIMIT 31")?;
        for row in stmt.query_map(params![actor, token.query], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
            ))
        })? {
            let (id, label, context, subtask) = row?;
            if !choices.iter().any(|choice| choice.id == id) {
                choices.push(ReferenceChoice {
                    prefix: if subtask { "^" } else { "%" },
                    kind: "task".into(),
                    id,
                    label,
                    context,
                });
            }
        }
    }
    choices.retain(|choice| matches_task_prefix(&token.raw, choice));
    choices.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(choices)
}
fn digest(raw: &str) -> String {
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}
/// Index only content already authorized and saved by the application. An
/// unresolved/ambiguous token never gets an edge or an inaccessible candidate.
pub fn index_references(
    c: &Connection,
    actor: &str,
    source_kind: &str,
    source_id: &str,
    raw: &str,
) -> Result<Vec<ReferenceResolution>, Error> {
    let (_, current) = source_task(c, actor, source_kind, source_id, 2)?;
    if current != raw {
        return Err(Error::Conflict(
            "reference text differs from saved source".into(),
        ));
    }
    let old_hash: Option<String> = c
        .query_row(
            "SELECT raw_hash FROM reference_sources WHERE source_kind=?1 AND source_id=?2",
            params![source_kind, source_id],
            |r| r.get(0),
        )
        .optional()?;
    if old_hash.as_deref() == Some(&digest(raw)) {
        // Review unresolved occurrences without changing any binding. A renamed
        // or newly visible candidate never silently retargets an existing edge.
        return unresolved_references(c, actor, source_kind, source_id, raw);
    }
    let tokens = tokens(raw);
    let mut report = Vec::new();
    let mut edges = Vec::new();
    for (occurrence, token) in tokens.iter().enumerate() {
        let choices = exact_matches(c, actor, token)?;
        if choices.len() == 1 {
            edges.push((occurrence, choices[0].clone(), token.raw.clone()));
        } else {
            report.push(ReferenceResolution {
                token: token.raw.clone(),
                occurrence,
                choices,
            });
        }
    }
    // The caller can include this in its write transaction. Savepoint also makes
    // direct indexing atomic without taking ownership of the connection.
    c.execute_batch("SAVEPOINT reference_index")?;
    let write = (|| -> Result<(), Error> {
        c.execute("INSERT INTO reference_sources(source_kind,source_id,raw_hash) VALUES(?1,?2,?3) ON CONFLICT(source_kind,source_id) DO UPDATE SET raw_hash=excluded.raw_hash",params![source_kind,source_id,digest(raw)])?;
        c.execute(
            "DELETE FROM reference_edges WHERE source_kind=?1 AND source_id=?2",
            params![source_kind, source_id],
        )?;
        for (index, target, original) in edges {
            c.execute(
                "INSERT INTO reference_edges VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    source_kind,
                    source_id,
                    index as i64,
                    target.kind,
                    target.id,
                    original
                ],
            )?;
        }
        Ok(())
    })();
    if write.is_err() {
        c.execute_batch("ROLLBACK TO reference_index; RELEASE reference_index")?;
    } else {
        c.execute_batch("RELEASE reference_index")?;
    }
    write?;
    Ok(report)
}
pub fn unresolved_references(
    c: &Connection,
    actor: &str,
    source_kind: &str,
    source_id: &str,
    raw: &str,
) -> Result<Vec<ReferenceResolution>, Error> {
    let (_, current) = source_task(c, actor, source_kind, source_id, 1)?;
    if current != raw {
        return Err(Error::Conflict(
            "reference text differs from saved source".into(),
        ));
    }
    let mut report = Vec::new();
    for (occurrence, token) in tokens(raw).into_iter().enumerate() {
        let bound: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM reference_edges WHERE source_kind=?1 AND source_id=?2 AND occurrence=?3 AND original_token=?4)", params![source_kind,source_id,occurrence as i64,token.raw], |r|r.get(0))?;
        if !bound {
            report.push(ReferenceResolution {
                token: token.raw.clone(),
                occurrence,
                choices: exact_matches(c, actor, &token)?,
            });
        }
    }
    Ok(report)
}

fn target(
    c: &Connection,
    actor: &str,
    kind: &str,
    id: &str,
) -> Result<Option<(String, String)>, Error> {
    let found = match kind {
        "workspace" => {
            let resolved = crate::defaults::resolve(c, "workspace", id)?;
            let id = resolved.as_str();
            if workspace_visible(c, actor, id, 1).is_err() {
                return Ok(None);
            }
            c.query_row("SELECT title FROM workspaces WHERE id=?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .map(|title| (title, format!("/workspaces/{id}")))
        }
        "list" => {
            let resolved = crate::defaults::resolve(c, "list", id)?;
            let id = resolved.as_str();
            if access_rank(c, actor, id, 1).is_err() {
                return Ok(None);
            }
            c.query_row(
                "SELECT title FROM lists WHERE id=?1 AND archived=0",
                [id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .map(|title| (title, format!("/lists/{id}")))
        }
        "section" => {
            let row: Option<(String, String)> = c
                .query_row(
                    "SELECT title,list_id FROM sections WHERE id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some((title, list)) = row {
                if access_rank(c, actor, &list, 1).is_ok() {
                    Some((title, format!("/lists/{list}?section={id}")))
                } else {
                    None
                }
            } else {
                None
            }
        }
        "task" => match task(c, id) {
            Ok(t) if access_rank(c, actor, &t.list_id, 1).is_ok() => {
                Some((t.title, format!("/tasks/{}", t.id)))
            }
            Ok(_) | Err(Error::NotFound) => None,
            Err(error) => return Err(error),
        },
        "tag" => {
            if !visible_tag(c, actor, id)? {
                return Ok(None);
            }
            c.query_row("SELECT name FROM tags WHERE id=?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .map(|name| (name, format!("/search?tag={id}")))
        }
        _ => return Err(Error::Invalid("unknown reference kind".into())),
    };
    Ok(found)
}
/// Stored IDs survive rename/move. Missing/deleted/inaccessible targets render
/// the original token as plain text, without disclosing its current label.
pub fn render_references(
    c: &Connection,
    actor: &str,
    source_kind: &str,
    source_id: &str,
    raw: &str,
) -> Result<String, Error> {
    let (_, current) = source_task(c, actor, source_kind, source_id, 1)?;
    if current != raw {
        return Err(Error::Conflict(
            "reference text differs from saved source".into(),
        ));
    }
    let valid:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM reference_sources WHERE source_kind=?1 AND source_id=?2 AND raw_hash=?3)",params![source_kind,source_id,digest(raw)],|r|r.get(0))?;
    if !valid {
        return Ok(crate::markdown::render(raw));
    }
    let parsed = tokens(raw);
    let mut replacements = Vec::new();
    for (index, token) in parsed.iter().enumerate() {
        let edge:Option<(String,String,String)>=c.query_row("SELECT target_kind,target_id,original_token FROM reference_edges WHERE source_kind=?1 AND source_id=?2 AND occurrence=?3",params![source_kind,source_id,index as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((kind, id, original)) = edge
            && original == token.raw
            && let Some((label, url)) = target(c, actor, &kind, &id)?
        {
            replacements.push((token.start, token.end, label, url));
        }
    }
    // Inject typed CommonMark events, never raw HTML or string-built Markdown.
    let mut events = Vec::new();
    for (event, range) in markdown_events(raw) {
        if matches!(event, Event::Text(_)) {
            let local: Vec<_> = replacements
                .iter()
                .filter(|(start, end, _, _)| *start >= range.start && *end <= range.end)
                .collect();
            if !local.is_empty() {
                let mut cursor = range.start;
                for (start, end, label, url) in local {
                    if *start > cursor {
                        append_text_events(&mut events, &raw[cursor..*start]);
                    }
                    events.push(Event::Start(MarkdownTag::Link {
                        link_type: pulldown_cmark::LinkType::Inline,
                        dest_url: CowStr::from(url.clone()),
                        title: CowStr::from(""),
                        id: CowStr::from(""),
                    }));
                    events.push(Event::Text(CowStr::from(label.clone())));
                    events.push(Event::End(TagEnd::Link));
                    cursor = *end;
                }
                if cursor < range.end {
                    append_text_events(&mut events, &raw[cursor..range.end]);
                }
                continue;
            }
        }
        events.push(event.into_static());
    }
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events.into_iter());
    Ok(ammonia::Builder::default()
        .rm_tags(&["img"])
        .clean(&html)
        .to_string())
}
fn append_text_events(events: &mut Vec<Event<'static>>, raw: &str) {
    for event in Parser::new(raw) {
        if let Event::Text(text) = event {
            events.push(Event::Text(CowStr::from(text.into_string())));
        } else if let Event::Code(text) = event {
            events.push(Event::Code(CowStr::from(text.into_string())));
        }
    }
}
impl Store {
    pub fn reference_autocomplete(
        &self,
        actor: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ReferenceChoice>, Error> {
        if !(1..=30).contains(&limit) || query.chars().count() > 100 {
            return Err(Error::Invalid(
                "autocomplete limit 1–30 and query at most 100 characters".into(),
            ));
        }
        let mut chars = query.chars();
        let first = chars.next();
        let kind = first.and_then(prefix_kind);
        let needle = if kind.is_some() {
            chars.as_str()
        } else {
            query
        }
        .to_lowercase();
        let kinds = if let Some(kind) = kind {
            vec![kind]
        } else {
            vec!["workspace", "list", "section", "task", "tag"]
        };
        let mut result = Vec::new();
        for kind in kinds {
            for candidate in candidates(&self.conn, actor, kind)? {
                if kind == "task" && first.is_some_and(|ch| matches!(ch, '%' | '^')) && !matches_task_prefix(query, &candidate) { continue; }
                if candidate.label.to_lowercase().starts_with(&needle)
                    || candidate.id.starts_with(&needle)
                    || candidate
                        .context
                        .rsplit(" · ")
                        .next()
                        .is_some_and(|reference| reference.to_lowercase().starts_with(&needle))
                {
                    result.push(candidate);
                    if result.len() == limit {
                        return Ok(result);
                    }
                }
            }
        }
        Ok(result)
    }
    pub fn reference_backlinks(
        &self,
        actor: &str,
        kind: &str,
        id: &str,
    ) -> Result<Vec<Backlink>, Error> {
        if target(&self.conn, actor, kind, id)?.is_none() {
            return Err(Error::NotFound);
        }
        let mut stmt=self.conn.prepare("SELECT DISTINCT source_kind,source_id FROM reference_edges WHERE target_kind=?1 AND target_id=?2 ORDER BY source_kind,source_id LIMIT 1000")?;
        let mut result = Vec::new();
        for row in stmt.query_map(params![kind, id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (kind, id) = row?;
            if let Ok((source, raw)) = source_task(&self.conn, actor, &kind, &id, 1) {
                let current: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM reference_sources WHERE source_kind=?1 AND source_id=?2 AND raw_hash=?3)",params![kind,id,digest(&raw)],|r|r.get(0))?;
                if !current {
                    continue;
                }
                result.push(Backlink {
                    source_kind: kind,
                    source_id: id,
                    source_task_id: source.id,
                });
                if result.len() == 100 {
                    break;
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Capture, NewTask};
    fn db() -> Store {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db
    }
    fn new_task(
        db: &mut Store,
        list: &str,
        title: &str,
        parent: Option<String>,
        description: &str,
    ) -> Task {
        db.capture(
            "dev",
            &Uuid::new_v4().to_string(),
            Capture {
                list_id: list.into(),
                tasks: vec![NewTask {
                    title: title.into(),
                    description: description.into(),
                    parent_id: parent,
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    #[test]
    fn write_only_workspace_credential_can_read_references_without_exceeding_scope() {
        let mut db = db();
        let workspace = db
            .create_workspace(
                "dev",
                crate::workspace::WorkspaceInput {
                    title: "Scoped workspace".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let other = db
            .create_workspace(
                "dev",
                crate::workspace::WorkspaceInput {
                    title: "Outside workspace".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        db.conn.execute(r#"INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,workspace,expires_at) VALUES('write-only','dev','unique-test-hash','http://localhost/api/v1','["write"]',?1,unixepoch()+3600)"#,[&workspace.id]).unwrap();
        let actor = "credential:write-only";
        assert!(workspace_visible(&db.conn, actor, &workspace.id, 1).is_ok());
        assert!(workspace_visible(&db.conn, actor, &workspace.id, 2).is_ok());
        assert!(workspace_visible(&db.conn, actor, &workspace.id, 3).is_err());
        assert!(workspace_visible(&db.conn, actor, &other.id, 1).is_err());
        let choices = db.reference_autocomplete(actor, "@", 30).unwrap();
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].id, workspace.id);
    }
    #[test]
    fn unicode_tags_are_unique_and_revision_checked_without_private_leaks() {
        let mut db = db();
        let t = new_task(&mut db, "inbox", "Tagged", None, "");
        let updated = db
            .set_task_tags(
                "dev",
                &t.id,
                1,
                vec![
                    TagSelector::Name(" ÄLPHA ".into()),
                    TagSelector::Name("älpha".into()),
                ],
            )
            .unwrap();
        let tags = db.task_tags("dev", &t.id).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "ÄLPHA");
        assert!(matches!(
            db.set_task_tags("dev", &t.id, 1, vec![TagSelector::Name("new".into())]),
            Err(Error::Conflict(_))
        ));
        assert_eq!(db.task_tags("dev", &t.id).unwrap()[0].id, tags[0].id);
        assert!(
            db.reference_autocomplete("other", "+", 30)
                .unwrap()
                .is_empty()
        );
        assert!(db.task_tags("other", &t.id).is_err());
        let renamed = db
            .rename_tag("dev", &tags[0].id, 1, "Älpha renamed")
            .unwrap();
        assert_eq!(renamed.id, tags[0].id);
        assert_eq!(updated.revision, 2);
    }
    #[test]
    fn section_assignment_and_reparent_preserve_tree_invariants() {
        let mut db = db();
        let root = new_task(&mut db, "inbox", "Root", None, "");
        let child = new_task(&mut db, "inbox", "Child", Some(root.id.clone()), "");
        db.conn.execute("INSERT INTO sections(id,list_id,title,position) VALUES('section','inbox','Next',1)",[]).unwrap();
        let assigned = db
            .set_section("dev", &root.id, 1, Some("section".into()))
            .unwrap();
        assert_eq!(assigned.revision, 2);
        let later_child = new_task(&mut db, "inbox", "Later child", Some(root.id.clone()), "");
        assert_eq!(
            db.task_section("dev", &later_child.id).unwrap().as_deref(),
            Some("section")
        );
        assert_eq!(
            db.task_section("dev", &child.id).unwrap().as_deref(),
            Some("section")
        );
        assert!(db.set_section("dev", &child.id, 2, None).is_err());
        assert!(matches!(
            db.reparent_subtree("dev", &root.id, Some(child.id.clone()), 2),
            Err(Error::Conflict(_))
        ));
        let detached = db.reparent_subtree("dev", &child.id, None, 2).unwrap();
        assert!(detached.parent_id.is_none());
        assert!(db.task_section("dev", &child.id).unwrap().is_none());
        assert!(matches!(
            db.reparent_subtree("other", &root.id, None, 2),
            Err(Error::Forbidden)
        ));
    }
    #[test]
    fn promote_keeps_descendants_appends_and_rejects_stale_confirmation() {
        let mut db = db();
        let root = new_task(&mut db, "inbox", "Root", None, "");
        let child = new_task(&mut db, "inbox", "Child", Some(root.id.clone()), "");
        let nested = new_task(&mut db, "inbox", "Nested", Some(child.id.clone()), "");
        let leaf = new_task(&mut db, "inbox", "Leaf", Some(nested.id.clone()), "");
        let snapshot = db
            .read_subtree("dev", &nested.id, Default::default())
            .unwrap()
            .snapshot;
        assert!(matches!(
            db.promote_subtree("other", &nested.id, snapshot.clone()),
            Err(Error::Forbidden)
        ));
        let promoted = db
            .promote_subtree("dev", &nested.id, snapshot.clone())
            .unwrap();
        assert_eq!(promoted.parent_id.as_deref(), Some(root.id.as_str()));
        assert!(promoted.position > task(&db.conn, &child.id).unwrap().position);
        assert_eq!(
            task(&db.conn, &leaf.id).unwrap().parent_id.as_deref(),
            Some(nested.id.as_str())
        );
        assert!(matches!(
            db.promote_subtree("dev", &nested.id, snapshot),
            Err(Error::Conflict(_))
        ));
        let stale = db
            .read_subtree("dev", &child.id, Default::default())
            .unwrap()
            .snapshot;
        new_task(
            &mut db,
            "inbox",
            "New descendant",
            Some(child.id.clone()),
            "",
        );
        assert!(matches!(
            db.promote_subtree("dev", &child.id, stale),
            Err(Error::Conflict(_))
        ));
        let fresh = db
            .read_subtree("dev", &child.id, Default::default())
            .unwrap()
            .snapshot;
        assert!(
            db.promote_subtree("dev", &child.id, fresh)
                .unwrap()
                .parent_id
                .is_none()
        );
        let top = db
            .read_subtree("dev", &root.id, Default::default())
            .unwrap()
            .snapshot;
        assert!(matches!(
            db.promote_subtree("dev", &root.id, top),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn sibling_reorder_requires_complete_set_and_list_revision() {
        let mut db = db();
        let a = new_task(&mut db, "inbox", "A", None, "");
        let b = new_task(&mut db, "inbox", "B", None, "");
        let revision = db
            .lists("dev")
            .unwrap()
            .into_iter()
            .find(|l| l.id == "inbox")
            .unwrap()
            .revision;
        assert!(
            db.reorder_siblings(
                "dev",
                "inbox",
                None,
                revision,
                vec![a.id.clone(), a.id.clone()]
            )
            .is_err()
        );
        let reordered = db
            .reorder_siblings(
                "dev",
                "inbox",
                None,
                revision,
                vec![b.id.clone(), a.id.clone()],
            )
            .unwrap();
        assert_eq!(reordered[0].id, b.id);
        assert!(reordered[0].position < reordered[1].position);
        assert!(matches!(
            db.reorder_siblings("dev", "inbox", None, revision, vec![a.id, b.id]),
            Err(Error::Conflict(_))
        ));
    }
    #[test]
    fn same_parent_reparent_invalidates_sibling_order_snapshot() {
        let mut db = db();
        let a = new_task(&mut db, "inbox", "A", None, "");
        let b = new_task(&mut db, "inbox", "B", None, "");
        let revision = db
            .lists("dev")
            .unwrap()
            .into_iter()
            .find(|l| l.id == "inbox")
            .unwrap()
            .revision;
        let moved = db.reparent_subtree("dev", &a.id, None, a.revision).unwrap();
        assert!(moved.position > b.position);
        assert!(matches!(
            db.reorder_siblings(
                "dev",
                "inbox",
                None,
                revision,
                vec![a.id.clone(), b.id.clone()]
            ),
            Err(Error::Conflict(_))
        ));
        let current = db.tasks("dev", "inbox").unwrap();
        assert_eq!(
            current.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec![b.id.as_str(), a.id.as_str()]
        );
    }
    #[test]
    fn moves_remap_tag_scope_and_clear_section_without_losing_names() {
        let mut db = db();
        let t = new_task(&mut db, "inbox", "Move", None, "");
        let t = db
            .set_task_tags(
                "dev",
                &t.id,
                1,
                vec![TagSelector::Name("Shared name".into())],
            )
            .unwrap();
        let old = db.task_tags("dev", &t.id).unwrap().remove(0);
        db.conn.execute("INSERT INTO sections(id,list_id,title,position) VALUES('section','inbox','Next',1)",[]).unwrap();
        let t = db
            .set_section("dev", &t.id, t.revision, Some("section".into()))
            .unwrap();
        let tx = db.conn.transaction().unwrap();
        tx.execute("UPDATE tasks SET list_id='groceries' WHERE id=?1", [&t.id])
            .unwrap();
        after_move(&tx, "dev", std::slice::from_ref(&t.id), "groceries").unwrap();
        tx.commit().unwrap();
        let new = db.task_tags("dev", &t.id).unwrap().remove(0);
        assert_ne!(new.id, old.id);
        assert_eq!(new.name, old.name);
        assert!(db.task_section("dev", &t.id).unwrap().is_none());
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM tag_move_audit", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn reference_parser_excludes_code_links_escapes_and_literal_punctuation() {
        let parsed = tokens(
            "#Groceries, `#Code` [#Linked](/x) \\#Escaped email@example.test C++\n\n```\n#Fenced\n```\n\n%T-abc +Älpha",
        );
        let raw: Vec<_> = parsed.iter().map(|t| t.raw.as_str()).collect();
        assert_eq!(raw, vec!["#Groceries", "%T-abc", "+Älpha"]);
    }
    #[test]
    fn bracketed_names_preserve_unicode_spaces_and_punctuation() {
        let parsed =
            tokens("See @[Über Team], #[Weekly planning], $[Next steps] and +[high priority].");
        assert_eq!(
            parsed.iter().map(|t| t.query.as_str()).collect::<Vec<_>>(),
            vec![
                "Über Team",
                "Weekly planning",
                "Next steps",
                "high priority"
            ]
        );
    }
    #[test]
    fn task_and_subtask_symbols_follow_structure_at_every_depth() {
        let mut db = db();
        let root = new_task(&mut db, "inbox", "Same name", None, "");
        let child = new_task(&mut db, "inbox", "Same name", Some(root.id.clone()), "");
        let nested = new_task(&mut db, "inbox", "Nested", Some(child.id.clone()), "");
        for (task, prefix) in [(&root, "%"), (&child, "^"), (&nested, "^")] {
            let reference = crate::defaults::public_id(&db.conn, "task", &task.id).unwrap();
            let resolved = db.resolve_reference("dev", &format!("{prefix}{reference}")).unwrap();
            assert_eq!(resolved.choices.len(), 1);
            assert_eq!(resolved.choices[0].id, task.id);
            assert_eq!(resolved.choices[0].prefix, prefix);
            let wrong = if prefix == "%" { "^" } else { "%" };
            assert!(db.resolve_reference("dev", &format!("{wrong}{reference}")).unwrap().choices.is_empty());
        }
        assert_eq!(db.resolve_reference("dev", "%[Same name]").unwrap().choices[0].id, root.id);
        assert_eq!(db.resolve_reference("dev", "^[Same name]").unwrap().choices[0].id, child.id);
        assert!(db.reference_autocomplete("dev", "%", 30).unwrap().iter().all(|c| c.prefix == "%"));
        let children = db.reference_autocomplete("dev", "^", 30).unwrap();
        assert_eq!(children.len(), 2);
        assert!(children.iter().all(|c| c.prefix == "^"));
        let raw = format!("See %{} and ^{} and ^{}.", root.reference, child.reference, nested.reference);
        let source = new_task(&mut db, "inbox", "References", None, &raw);
        assert!(index_references(&db.conn, "dev", "task", &source.id, &raw).unwrap().is_empty());
        let html = render_references(&db.conn, "dev", "task", &source.id, &raw).unwrap();
        for task in [&root, &child, &nested] { assert!(html.contains(&format!("/tasks/{}", task.id))); }
        let report = db.readable_report("dev", &crate::reports::ReportScope { kind: "task".into(), id: Some(root.id.clone()), ..Default::default() }).unwrap();
        assert_eq!(report.tasks.len(), 3);
        for task in report.tasks { assert!(task.reference.starts_with(if task.parent_id.is_some() { '^' } else { '%' })); }
        let query = crate::search::Query { text: format!("^{}", nested.reference), status: "all".into(), ..Default::default() };
        let hits = db.query("dev", &query).unwrap().hits;
        assert_eq!(hits.len(), 1);
        assert!(hits[0].reference.as_ref().unwrap().starts_with('^'));
    }
    #[test]
    fn stored_targets_survive_renames_and_hide_after_access_loss_or_deletion() {
        let mut db = db();
        let referenced = new_task(&mut db, "inbox", "Original target", None, "");
        let raw = format!("See %{} and #Groceries.", referenced.reference);
        let source = new_task(&mut db, "inbox", "Source", None, &raw);
        let report = index_references(&db.conn, "dev", "task", &source.id, &raw).unwrap();
        assert!(report.is_empty());
        db.conn
            .execute(
                "UPDATE tasks SET title='<script>renamed</script>' WHERE id=?1",
                [&referenced.id],
            )
            .unwrap();
        db.conn
            .execute("UPDATE lists SET title='New name' WHERE id='groceries'", [])
            .unwrap();
        index_references(&db.conn, "dev", "task", &source.id, &raw).unwrap();
        let html = render_references(&db.conn, "dev", "task", &source.id, &raw).unwrap();
        assert!(html.contains("New name"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
        db.conn
            .execute(
                "DELETE FROM memberships WHERE list_id='groceries' AND user_id='dev'",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "UPDATE tasks SET deleted_at=unixepoch() WHERE id=?1",
                [&referenced.id],
            )
            .unwrap();
        let html = render_references(&db.conn, "dev", "task", &source.id, &raw).unwrap();
        assert!(!html.contains("New name"));
        assert!(!html.contains("renamed"));
        assert!(html.contains("#Groceries"));
        assert!(!html.contains("href="));
        assert!(render_references(&db.conn, "other", "task", &source.id, &raw).is_err());
    }
    #[test]
    fn duplicate_names_return_only_accessible_choices_and_backlinks_recheck_sources() {
        let mut db = db();
        db.conn
            .execute(
                "UPDATE lists SET title='Duplicate' WHERE id IN ('inbox','groceries','private')",
                [],
            )
            .unwrap();
        let raw = "#Duplicate";
        let source = new_task(&mut db, "inbox", "Source", None, raw);
        let unresolved = index_references(&db.conn, "dev", "task", &source.id, raw).unwrap();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].choices.len(), 2);
        assert!(unresolved[0].choices.iter().all(|c| c.id != "private"));
        assert!(
            !render_references(&db.conn, "dev", "task", &source.id, raw)
                .unwrap()
                .contains("href=")
        );
        let raw = "#groceries";
        db.conn
            .execute(
                "UPDATE tasks SET description=?1 WHERE id=?2",
                params![raw, source.id],
            )
            .unwrap();
        index_references(&db.conn, "dev", "task", &source.id, raw).unwrap();
        assert_eq!(
            db.reference_backlinks("dev", "list", "groceries")
                .unwrap()
                .len(),
            1
        );
        assert!(
            db.reference_backlinks("other", "list", "groceries")
                .is_err()
        );
        assert_eq!(
            db.reference_autocomplete("dev", "#Dup", 1).unwrap().len(),
            1
        );
    }
}

/// Explicit reference binding changes the saved source revision, even when its text is unchanged.
#[derive(Debug, Serialize)]
pub struct ReferenceBinding {
    pub source_revision: i64,
    pub unresolved: Vec<ReferenceResolution>,
}
impl Store {
    /// Unused workspace tags require workspace access. Direct list guests can
    /// introduce names through set_task_tags, which immediately associates them.
    pub fn create_tag(&mut self, actor: &str, list: &str, name: &str) -> Result<Tag, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        access_rank(&tx, actor, list, 2)?;
        let home = scope(&tx, list)?;
        if let Some(workspace) = home.strip_prefix("workspace:") {
            workspace_visible(&tx, actor, workspace, 2)?;
        }
        let tag = ensure_tag(&tx, &home, name)?;
        tx.commit()?;
        Ok(tag)
    }
    pub fn list_tags(
        &self,
        actor: &str,
        list: &str,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<Tag>, Error> {
        access_rank(&self.conn, actor, list, 1)?;
        if !(1..=100).contains(&limit) || prefix.chars().count() > 100 {
            return Err(Error::Invalid(
                "tag limit 1..100 and prefix at most 100 characters".into(),
            ));
        }
        let home = scope(&self.conn, list)?;
        let all = home
            .strip_prefix("workspace:")
            .is_none_or(|workspace| workspace_visible(&self.conn, actor, workspace, 1).is_ok());
        let mut stmt=self.conn.prepare("SELECT g.id,g.name,g.revision FROM tags g WHERE g.scope=?1 AND substr(g.normalized_name,1,length(?2))=?2 AND (?3 OR EXISTS(SELECT 1 FROM task_tags tt JOIN tasks t ON t.id=tt.task_id WHERE tt.tag_id=g.id AND t.list_id=?4 AND t.deleted_at IS NULL)) ORDER BY g.normalized_name,g.id LIMIT ?5")?;
        Ok(stmt
            .query_map(
                params![home, prefix.to_lowercase(), all, list, limit as i64],
                tag_row,
            )?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn resolve_reference(
        &self,
        actor: &str,
        input: &str,
    ) -> Result<ReferenceResolution, Error> {
        if input.chars().count() > 200 {
            return Err(Error::Invalid("reference at most 200 characters".into()));
        }
        let input = input.trim();
        let parsed = tokens(input);
        if parsed.len() != 1 || parsed[0].raw != input {
            return Err(Error::Invalid(
                "provide exactly one @space, #list, $section, %task, ^subtask or +tag reference".into(),
            ));
        }
        Ok(ReferenceResolution {
            token: parsed[0].raw.clone(),
            occurrence: 0,
            choices: exact_matches(&self.conn, actor, &parsed[0])?,
        })
    }
    pub fn source_unresolved_references(
        &self,
        actor: &str,
        kind: &str,
        id: &str,
    ) -> Result<Vec<ReferenceResolution>, Error> {
        let (source, raw) = source_task(&self.conn, actor, kind, id, 1)?;
        let id = if kind == "task" {
            source.id.as_str()
        } else {
            id
        };
        unresolved_references(&self.conn, actor, kind, id, &raw)
    }
    pub fn bind_reference(
        &mut self,
        actor: &str,
        kind: &str,
        id: &str,
        revision: i64,
        occurrence: usize,
        target: (&str, &str),
    ) -> Result<ReferenceBinding, Error> {
        let (target_kind, target_id) = target;
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let (source, raw) = source_task(&tx, actor, kind, id, 2)?;
        let source_id = if kind == "task" {
            source.id.as_str()
        } else {
            id
        };
        let actual = if kind == "task" {
            source.revision
        } else {
            let (revision, author): (i64, String) = tx.query_row(
                "SELECT revision,author_id FROM comments WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            crate::discussion::owner_or_author(&tx, actor, &source, &author)?;
            revision
        };
        if actual != revision {
            return Err(Error::Conflict("stale source revision".into()));
        }
        let parsed = tokens(&raw);
        let token = parsed
            .get(occurrence)
            .ok_or_else(|| Error::Invalid("unknown reference occurrence".into()))?;
        if token.kind != target_kind
            || !exact_matches(&tx, actor, token)?
                .iter()
                .any(|c| c.id == target_id && c.kind == target_kind)
        {
            return Err(Error::Invalid(
                "choose a currently accessible exact candidate for this occurrence".into(),
            ));
        }
        // Indexing preserves all existing bindings when the saved text hash is unchanged.
        index_references(&tx, actor, kind, source_id, &raw)?;
        tx.execute("INSERT INTO reference_edges(source_kind,source_id,occurrence,target_kind,target_id,original_token) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(source_kind,source_id,occurrence) DO UPDATE SET target_kind=excluded.target_kind,target_id=excluded.target_id,original_token=excluded.original_token",params![kind,source_id,occurrence as i64,target_kind,target_id,token.raw])?;
        if kind=="task" {cas_task(&tx,&source,revision)?;event(&tx,&task(&tx,&source.id)?,"references",actor,Some(&source))?;}else if tx.execute("UPDATE comments SET revision=revision+1,updated_at=unixepoch() WHERE id=?1 AND revision=?2",params![id,revision])?!=1{return Err(Error::Conflict("stale source revision".into()));}
        let unresolved = unresolved_references(&tx, actor, kind, source_id, &raw)?;
        tx.commit()?;
        Ok(ReferenceBinding {
            source_revision: revision + 1,
            unresolved,
        })
    }
}

#[cfg(test)]
mod adapter_contract_tests {
    use super::*;
    use crate::core::{Capture, NewTask};
    fn db() -> Store {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db
    }
    fn source(db: &mut Store, description: &str) -> Task {
        db.capture(
            "dev",
            &Uuid::new_v4().to_string(),
            Capture {
                list_id: "inbox".into(),
                tasks: vec![NewTask {
                    title: "Reference source".into(),
                    description: description.into(),
                    parent_id: None,
                }],
            },
        )
        .unwrap()
        .remove(0)
    }
    #[test]
    fn explicit_binding_is_revision_guarded_preserves_other_edges_and_survives_rename() {
        let mut db = db();
        db.conn.execute_batch("INSERT INTO lists(id,title) VALUES('twin','Inbox');INSERT INTO memberships VALUES('twin','dev');").unwrap();
        let source = source(&mut db, "#Inbox and #Groceries with `#Inbox` literal");
        let unresolved = db
            .source_unresolved_references("dev", "task", &source.reference)
            .unwrap();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].choices.len(), 2);
        let bound = db
            .bind_reference("dev", "task", &source.reference, 1, 0, ("list", "inbox"))
            .unwrap();
        assert_eq!(bound.source_revision, 2);
        assert!(bound.unresolved.is_empty());
        assert!(matches!(
            db.bind_reference("dev", "task", &source.id, 1, 0, ("list", "twin")),
            Err(Error::Conflict(_))
        ));
        db.conn
            .execute(
                "UPDATE lists SET title='Renamed inbox' WHERE id='inbox'",
                [],
            )
            .unwrap();
        assert!(
            db.source_unresolved_references("dev", "task", &source.id)
                .unwrap()
                .is_empty()
        );
        let html =
            render_references(&db.conn, "dev", "task", &source.id, &source.description).unwrap();
        assert!(html.contains("Renamed inbox"));
        assert!(html.contains("<code>#Inbox</code>"));
        assert_eq!(
            db.reference_backlinks("dev", "list", "groceries")
                .unwrap()
                .len(),
            1
        );
        assert!(
            db.bind_reference("dev", "task", &source.id, 2, 0, ("list", "private"))
                .is_err()
        );
    }
    #[test]
    fn comment_binding_updates_comment_revision_and_obeys_source_write_access() {
        let mut db = db();
        let task = source(&mut db, "");
        db.conn.execute_batch("INSERT INTO lists(id,title) VALUES('twin','Inbox');INSERT INTO memberships VALUES('twin','dev');").unwrap();
        db.conn.execute("INSERT INTO comments(id,task_id,author_id,body) VALUES('comment',?1,'dev','#Inbox')",[&task.id]).unwrap();
        assert_eq!(
            db.source_unresolved_references("dev", "comment", "comment")
                .unwrap()[0]
                .choices
                .len(),
            2
        );
        assert_eq!(
            db.bind_reference("dev", "comment", "comment", 1, 0, ("list", "inbox"))
                .unwrap()
                .source_revision,
            2
        );
        assert!(matches!(
            db.bind_reference("dev", "comment", "comment", 1, 0, ("list", "twin")),
            Err(Error::Conflict(_))
        ));
        db.conn
            .execute("INSERT INTO memberships VALUES('inbox','other')", [])
            .unwrap();
        assert!(matches!(
            db.bind_reference("other", "comment", "comment", 2, 0, ("list", "inbox")),
            Err(Error::Forbidden)
        ));
        db.conn
            .execute(
                "INSERT INTO membership_roles VALUES('inbox','dev','viewer')",
                [],
            )
            .unwrap();
        assert!(matches!(
            db.bind_reference("dev", "comment", "comment", 2, 0, ("list", "twin")),
            Err(Error::Forbidden)
        ));
    }
    #[test]
    fn direct_guest_tag_registry_never_exposes_unused_workspace_metadata() {
        let mut db = db();
        db.conn.execute_batch("INSERT INTO workspaces(id,title) VALUES('w','Hidden workspace');INSERT INTO workspace_members VALUES('w','other','owner');UPDATE lists SET workspace_id='w' WHERE id='inbox';INSERT INTO tags(id,scope,name,normalized_name) VALUES('hidden','workspace:w','Hidden tag','hidden tag');").unwrap();
        assert!(db.list_tags("dev", "inbox", "", 100).unwrap().is_empty());
        assert!(matches!(
            db.create_tag("dev", "inbox", "Standalone"),
            Err(Error::Forbidden)
        ));
        let task = source(&mut db, "");
        db.set_task_tags(
            "dev",
            &task.id,
            1,
            vec![TagSelector::Name("Assigned".into())],
        )
        .unwrap();
        let tags = db.list_tags("dev", "inbox", "", 100).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "Assigned");
        db.conn
            .execute(
                "INSERT INTO workspace_members VALUES('w','dev','editor')",
                [],
            )
            .unwrap();
        assert_eq!(db.list_tags("dev", "inbox", "", 100).unwrap().len(), 2);
        assert_eq!(
            db.create_tag("dev", "inbox", "Standalone").unwrap().name,
            "Standalone"
        );
    }
}
