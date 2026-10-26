//! Task discussion and private attachment metadata. Storage I/O happens outside
//! SQLite transactions; finalization rechecks current permissions and revision.
use crate::core::{self, Error, Store, Task};
use crate::object_storage::VerifiedObject;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
pub const MAX_ATTACHMENT_BYTES: u64 = 25 * 1024 * 1024;
pub const INTENT_TTL_SECONDS: i64 = 15 * 60;
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs() as i64
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub task_id: String,
    pub author_id: String,
    pub body: String,
    pub revision: i64,
    pub seq: i64,
    pub created_at: i64,
    pub updated_at: Option<i64>,
    pub deleted: bool,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewComment {
    pub body: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditComment {
    pub revision: i64,
    pub body: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadInput {
    pub comment_id: Option<String>,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct UploadIntent {
    pub id: String,
    pub task_id: String,
    pub comment_id: Option<String>,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    pub revision: i64,
    pub expires_at: i64,
    pub finalized: bool,
    #[serde(skip_serializing)]
    pub(crate) staging_key: String,
    #[serde(skip_serializing)]
    pub(crate) object_key: String,
    #[serde(skip_serializing)]
    creator_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub intent_id: String,
    pub task_id: String,
    pub comment_id: Option<String>,
    pub creator_id: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    pub revision: i64,
    pub created_at: i64,
    pub deleted: bool,
    #[serde(skip)]
    pub(crate) object_key: String,
}
fn body_valid(body: &str) -> Result<(), Error> {
    if body.trim().is_empty() || body.len() > 100_000 {
        Err(Error::Invalid("comment must contain 1–100000 bytes".into()))
    } else {
        Ok(())
    }
}
/// Keep only a display basename; it never determines an object key or MIME type.
pub fn safe_filename(filename: &str) -> Result<String, Error> {
    let basename = filename.rsplit(['/', '\\']).next().unwrap_or("");
    let safe: String = basename
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(*c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
        })
        .collect();
    let safe = safe.trim().trim_matches('.').trim();
    if safe.is_empty() || safe.len() > 255 {
        return Err(Error::Invalid(
            "filename must have a basename of 1–255 bytes".into(),
        ));
    }
    Ok(safe.into())
}
fn hash_valid(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn revision(actual: i64, expected: i64) -> Result<(), Error> {
    if actual == expected {
        Ok(())
    } else {
        Err(Error::Conflict("stale revision".into()))
    }
}
fn task_access(c: &Connection, actor: &str, id: &str, rank: i64) -> Result<Task, Error> {
    let task = core::task(c, id)?;
    core::access_rank(c, actor, &task.list_id, rank)?;
    Ok(task)
}
fn owner_or_author(c: &Connection, actor: &str, task: &Task, author: &str) -> Result<(), Error> {
    let (user, _) = core::actor_identity(c, actor)?;
    if user == author {
        return Ok(());
    }
    core::access_rank(c, actor, &task.list_id, 3)
}
const COMMENT_COLUMNS: &str =
    "id,task_id,author_id,body,revision,seq,created_at,updated_at,deleted_at IS NOT NULL";
fn comment_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Comment> {
    Ok(Comment {
        id: r.get(0)?,
        task_id: r.get(1)?,
        author_id: r.get(2)?,
        body: r.get(3)?,
        revision: r.get(4)?,
        seq: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
        deleted: r.get(8)?,
    })
}
fn comment_raw(c: &Connection, id: &str) -> Result<Comment, Error> {
    c.query_row(
        &format!("SELECT {COMMENT_COLUMNS} FROM comments WHERE id=?1 AND deleted_at IS NULL"),
        [id],
        comment_row,
    )
    .optional()?
    .ok_or(Error::NotFound)
}
fn comment_belongs(c: &Connection, comment: Option<&str>, task: &str) -> Result<(), Error> {
    if let Some(id) = comment
        && comment_raw(c, id)?.task_id != task
    {
        return Err(Error::Invalid(
            "comment must belong to the attachment task".into(),
        ));
    }
    Ok(())
}
fn discussion_event(
    c: &Connection,
    task: &Task,
    actor: &str,
    kind: &str,
    entity: serde_json::Value,
    before: Option<String>,
) -> Result<(), Error> {
    let (user, client) = core::actor_identity(c, actor)?;
    // The task snapshot supports the existing live history visibility join; its
    // revision is unchanged. The event kind/entity are separate from task edits.
    let event = serde_json::json!({"kind":kind,"task":task,"actor":user,"client":client,"attribution":if client.is_some(){"agent"}else{"human"},"discussion":entity});
    c.execute(
        "INSERT INTO history(list_id,event,actor,before_event) VALUES(?1,?2,?3,?4)",
        params![task.list_id, event.to_string(), user, before],
    )?;
    Ok(())
}
const INTENT_COLUMNS: &str = "id,task_id,comment_id,filename,size,sha256,revision,expires_at,finalized_at IS NOT NULL,staging_key,object_key,creator_id";
fn intent_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<UploadIntent> {
    Ok(UploadIntent {
        id: r.get(0)?,
        task_id: r.get(1)?,
        comment_id: r.get(2)?,
        filename: r.get(3)?,
        size: r.get::<_, i64>(4)? as u64,
        sha256: r.get(5)?,
        revision: r.get(6)?,
        expires_at: r.get(7)?,
        finalized: r.get(8)?,
        staging_key: r.get(9)?,
        object_key: r.get(10)?,
        creator_id: r.get(11)?,
    })
}
fn intent_raw(c: &Connection, id: &str) -> Result<UploadIntent, Error> {
    c.query_row(
        &format!("SELECT {INTENT_COLUMNS} FROM upload_intents WHERE id=?1"),
        [id],
        intent_row,
    )
    .optional()?
    .ok_or(Error::NotFound)
}
fn intent_access(c: &Connection, actor: &str, id: &str) -> Result<(UploadIntent, Task), Error> {
    let intent = intent_raw(c, id)?;
    let task = task_access(c, actor, &intent.task_id, 2)?;
    owner_or_author(c, actor, &task, &intent.creator_id)?;
    comment_belongs(c, intent.comment_id.as_deref(), &intent.task_id)?;
    Ok((intent, task))
}
const ATTACHMENT_COLUMNS: &str = "a.id,a.intent_id,a.task_id,a.comment_id,a.creator_id,a.filename,a.size,a.sha256,a.revision,a.created_at,a.deleted_at IS NOT NULL,a.object_key";
fn attachment_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Attachment> {
    Ok(Attachment {
        id: r.get(0)?,
        intent_id: r.get(1)?,
        task_id: r.get(2)?,
        comment_id: r.get(3)?,
        creator_id: r.get(4)?,
        filename: r.get(5)?,
        size: r.get::<_, i64>(6)? as u64,
        sha256: r.get(7)?,
        revision: r.get(8)?,
        created_at: r.get(9)?,
        deleted: r.get(10)?,
        object_key: r.get(11)?,
    })
}
fn attachment_raw(c: &Connection, id: &str) -> Result<Attachment, Error> {
    c.query_row(
        &format!(
            "SELECT {ATTACHMENT_COLUMNS} FROM attachments a WHERE a.id=?1 AND a.deleted_at IS NULL"
        ),
        [id],
        attachment_row,
    )
    .optional()?
    .ok_or(Error::NotFound)
}
impl Store {
    pub fn comments(
        &self,
        actor: &str,
        task: &str,
        after_seq: i64,
        limit: i64,
    ) -> Result<Vec<Comment>, Error> {
        task_access(&self.conn, actor, task, 1)?;
        if after_seq < 0 || !(1..=200).contains(&limit) {
            return Err(Error::Invalid(
                "comment cursor must be nonnegative and limit 1–200".into(),
            ));
        }
        let mut stmt=self.conn.prepare(&format!("SELECT {COMMENT_COLUMNS} FROM comments WHERE task_id=?1 AND deleted_at IS NULL AND seq>?2 ORDER BY seq LIMIT ?3"))?;
        Ok(stmt
            .query_map(params![task, after_seq, limit], comment_row)?
            .collect::<Result<_, _>>()?)
    }
    pub fn comment(&self, actor: &str, id: &str) -> Result<Comment, Error> {
        let result = comment_raw(&self.conn, id)?;
        task_access(&self.conn, actor, &result.task_id, 1)?;
        Ok(result)
    }
    pub fn create_comment(
        &mut self,
        actor: &str,
        task: &str,
        key: &str,
        input: NewComment,
    ) -> Result<Comment, Error> {
        body_valid(&input.body)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let task = task_access(&tx, actor, task, 2)?;
        if key.trim().is_empty() || key.len() > 200 {
            return Err(Error::Invalid(
                "Idempotency-Key must contain 1–200 bytes".into(),
            ));
        }
        let key = format!("comment:{key}");
        let payload =
            serde_json::json!({"operation":"comment.create","task_id":task.id,"body":input.body})
                .to_string();
        if let Some((previous, response)) = tx
            .query_row(
                "SELECT payload,response FROM idempotency WHERE actor=?1 AND key=?2",
                params![actor, key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            if previous != payload {
                return Err(Error::Conflict(
                    "idempotency key reused with different comment payload".into(),
                ));
            }
            let original: Comment = serde_json::from_str(&response)
                .map_err(|_| Error::Invalid("invalid stored comment response".into()))?;
            let current = comment_raw(&tx, &original.id)?;
            task_access(&tx, actor, &current.task_id, 2)?;
            tx.commit()?;
            return Ok(original);
        }
        let (user, _) = core::actor_identity(&tx, actor)?;
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO comments(id,task_id,author_id,body) VALUES(?1,?2,?3,?4)",
            params![id, task.id, user, input.body],
        )?;
        let result = comment_raw(&tx, &id)?;
        crate::taxonomy::index_references(&tx, actor, "comment", &id, &result.body)?;
        tx.execute(
            "INSERT INTO idempotency(actor,key,payload,response) VALUES(?1,?2,?3,?4)",
            params![actor, key, payload, serde_json::to_string(&result).unwrap()],
        )?;
        discussion_event(
            &tx,
            &task,
            actor,
            "comment.create",
            serde_json::json!({"id":id,"revision":result.revision}),
            None,
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn update_comment(
        &mut self,
        actor: &str,
        id: &str,
        input: EditComment,
    ) -> Result<Comment, Error> {
        body_valid(&input.body)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = comment_raw(&tx, id)?;
        let task = task_access(&tx, actor, &old.task_id, 2)?;
        owner_or_author(&tx, actor, &task, &old.author_id)?;
        revision(old.revision, input.revision)?;
        tx.execute(
            "UPDATE comments SET body=?2,revision=revision+1,updated_at=unixepoch() WHERE id=?1",
            params![id, input.body],
        )?;
        let result = comment_raw(&tx, id)?;
        crate::taxonomy::index_references(&tx, actor, "comment", id, &result.body)?;
        discussion_event(
            &tx,
            &task,
            actor,
            "comment.update",
            serde_json::json!({"id":id,"revision":result.revision}),
            Some(serde_json::to_string(&old).unwrap()),
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn delete_comment(
        &mut self,
        actor: &str,
        id: &str,
        expected_revision: i64,
    ) -> Result<Comment, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = comment_raw(&tx, id)?;
        let task = task_access(&tx, actor, &old.task_id, 2)?;
        owner_or_author(&tx, actor, &task, &old.author_id)?;
        revision(old.revision, expected_revision)?;
        tx.execute("UPDATE comments SET deleted_at=unixepoch(),updated_at=unixepoch(),revision=revision+1 WHERE id=?1",[id])?;
        let result = Comment {
            body: String::new(),
            revision: old.revision + 1,
            updated_at: Some(now()),
            deleted: true,
            ..old.clone()
        };
        discussion_event(
            &tx,
            &task,
            actor,
            "comment.delete",
            serde_json::json!({"id":id,"revision":result.revision}),
            Some(serde_json::to_string(&old).unwrap()),
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn create_upload_intent(
        &mut self,
        actor: &str,
        task: &str,
        input: UploadInput,
    ) -> Result<UploadIntent, Error> {
        let filename = safe_filename(&input.filename)?;
        if input.size > MAX_ATTACHMENT_BYTES {
            return Err(Error::Invalid("attachment exceeds 25 MiB".into()));
        }
        if !hash_valid(&input.sha256) {
            return Err(Error::Invalid(
                "SHA-256 must be 64 hexadecimal characters".into(),
            ));
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let task = task_access(&tx, actor, task, 2)?;
        comment_belongs(&tx, input.comment_id.as_deref(), &task.id)?;
        let (user, client) = core::actor_identity(&tx, actor)?;
        let id = Uuid::new_v4().to_string();
        let staging = format!("staging/{}", Uuid::new_v4());
        let object = format!("objects/{}", Uuid::new_v4());
        tx.execute("INSERT INTO upload_intents(id,task_id,comment_id,creator_id,client_id,filename,size,sha256,staging_key,object_key,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![id,task.id,input.comment_id,user,client,filename,input.size as i64,input.sha256.to_ascii_lowercase(),staging,object,now()+INTENT_TTL_SECONDS])?;
        let result = intent_raw(&tx, &id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn upload_intent(&self, actor: &str, id: &str) -> Result<UploadIntent, Error> {
        let (intent, _) = intent_access(&self.conn, actor, id)?;
        if !intent.finalized && intent.expires_at <= now() {
            return Err(Error::Conflict("upload intent expired".into()));
        }
        Ok(intent)
    }
    pub fn finalize_attachment(
        &mut self,
        actor: &str,
        verified: VerifiedObject,
    ) -> Result<Attachment, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (intent, task) = intent_access(&tx, actor, &verified.intent_id)?;
        if intent.object_key != verified.object_key
            || intent.size != verified.size
            || intent.sha256 != verified.sha256
        {
            return Err(Error::Conflict(
                "verified object does not match intent".into(),
            ));
        }
        if intent.finalized {
            let existing=tx.query_row(&format!("SELECT {ATTACHMENT_COLUMNS} FROM attachments a WHERE a.intent_id=?1 AND a.deleted_at IS NULL"),[&intent.id],attachment_row).optional()?.ok_or(Error::NotFound)?;
            tx.commit()?;
            return Ok(existing);
        }
        revision(intent.revision, verified.revision)?;
        if intent.expires_at <= now() {
            return Err(Error::Conflict("upload intent expired".into()));
        }
        let id = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO attachments(id,intent_id,task_id,comment_id,creator_id,filename,size,sha256,object_key) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,intent.id,intent.task_id,intent.comment_id,intent.creator_id,intent.filename,intent.size as i64,intent.sha256,intent.object_key])?;
        tx.execute(
            "UPDATE upload_intents SET finalized_at=unixepoch(),revision=revision+1 WHERE id=?1",
            [&intent.id],
        )?;
        let result = attachment_raw(&tx, &id)?;
        discussion_event(
            &tx,
            &task,
            actor,
            "attachment.create",
            serde_json::json!({"id":id,"comment_id":result.comment_id}),
            None,
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn attachments(&self, actor: &str, task: &str) -> Result<Vec<Attachment>, Error> {
        task_access(&self.conn, actor, task, 1)?;
        let mut stmt=self.conn.prepare(&format!("SELECT {ATTACHMENT_COLUMNS} FROM attachments a LEFT JOIN comments c ON c.id=a.comment_id WHERE a.task_id=?1 AND a.deleted_at IS NULL AND (a.comment_id IS NULL OR c.deleted_at IS NULL) ORDER BY a.created_at,a.id LIMIT 1000"))?;
        Ok(stmt
            .query_map([task], attachment_row)?
            .collect::<Result<_, _>>()?)
    }
    pub fn attachment(&self, actor: &str, id: &str) -> Result<Attachment, Error> {
        let result = attachment_raw(&self.conn, id)?;
        task_access(&self.conn, actor, &result.task_id, 1)?;
        comment_belongs(&self.conn, result.comment_id.as_deref(), &result.task_id)?;
        Ok(result)
    }
    pub fn delete_attachment(
        &mut self,
        actor: &str,
        id: &str,
        expected_revision: i64,
    ) -> Result<Attachment, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = attachment_raw(&tx, id)?;
        let task = task_access(&tx, actor, &old.task_id, 2)?;
        owner_or_author(&tx, actor, &task, &old.creator_id)?;
        revision(old.revision, expected_revision)?;
        comment_belongs(&tx, old.comment_id.as_deref(), &old.task_id)?;
        tx.execute(
            "UPDATE attachments SET deleted_at=unixepoch(),revision=revision+1 WHERE id=?1",
            [id],
        )?;
        let result = Attachment {
            deleted: true,
            revision: old.revision + 1,
            ..old
        };
        discussion_event(
            &tx,
            &task,
            actor,
            "attachment.delete",
            serde_json::json!({"id":id,"revision":result.revision}),
            None,
        )?;
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::{Capture, NewTask},
        workspace::{CreateList, ListRole, WorkspaceInput},
    };
    fn fixture() -> (Store, Task, String) {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let w = db
            .create_workspace(
                "dev",
                WorkspaceInput {
                    title: "Discussion".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let list = db
            .create_list(
                "dev",
                CreateList {
                    workspace_id: w.id,
                    title: "Discussion list".into(),
                    description: String::new(),
                    restricted: true,
                },
            )
            .unwrap();
        db.grant_list(
            "dev",
            &list.id,
            "other",
            ListRole::Editor,
            list.revision,
            true,
        )
        .unwrap();
        let task = db
            .capture(
                "dev",
                "discussion-fixture",
                Capture {
                    list_id: list.id.clone(),
                    tasks: vec![NewTask {
                        title: "Synthetic task".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        (db, task, list.id)
    }
    fn upload(comment_id: Option<String>) -> UploadInput {
        UploadInput {
            comment_id,
            filename: "../synthetic.txt".into(),
            size: 5,
            sha256: crate::object_storage::sha256_hex(b"hello"),
        }
    }
    #[test]
    fn chronological_comments_have_independent_cas_and_author_or_owner_checks() {
        let (mut db, task, list) = fixture();
        let first = db
            .create_comment(
                "dev",
                &task.id,
                "fixture-comment-1",
                NewComment {
                    body: "**Owner note**".into(),
                },
            )
            .unwrap();
        let other = db
            .create_comment(
                "other",
                &task.id,
                "fixture-comment-2",
                NewComment {
                    body: "Other note".into(),
                },
            )
            .unwrap();
        assert!(matches!(
            db.update_comment(
                "other",
                &first.id,
                EditComment {
                    revision: 1,
                    body: "Denied".into()
                }
            ),
            Err(Error::Forbidden)
        ));
        let edited = db
            .update_comment(
                "dev",
                &other.id,
                EditComment {
                    revision: 1,
                    body: "Owner moderation".into(),
                },
            )
            .unwrap();
        assert_eq!(edited.revision, 2);
        assert!(matches!(
            db.delete_comment("other", &other.id, 1),
            Err(Error::Conflict(_))
        ));
        let rows = db.comments("other", &task.id, 0, 100).unwrap();
        assert_eq!(
            rows.iter().map(|c| &c.id).collect::<Vec<_>>(),
            vec![&first.id, &other.id]
        );
        assert_eq!(
            db.comments("dev", &task.id, first.seq, 1).unwrap()[0].id,
            other.id
        );
        assert_eq!(
            db.subtree("dev", &task.id).unwrap()[0].revision,
            task.revision
        );
        let events = db.changes("other", 0).unwrap();
        assert!(events.iter().any(|e| e["kind"] == "comment.update"));
        let update = events
            .iter()
            .find(|e| e["kind"] == "comment.update")
            .unwrap();
        assert!(matches!(
            db.undo_update("dev", update["seq"].as_i64().unwrap(), task.revision),
            Err(Error::Invalid(_))
        ));
        let deleted = db.delete_comment("other", &other.id, 2).unwrap();
        assert!(deleted.deleted);
        assert!(deleted.body.is_empty());
        assert_eq!(db.comments("dev", &task.id, 0, 100).unwrap().len(), 1);
        let revision = db.list_info("dev", &list).unwrap().list.revision;
        db.grant_list("dev", &list, "other", ListRole::Viewer, revision, false)
            .unwrap();
        assert!(matches!(
            db.create_comment(
                "other",
                &task.id,
                "fixture-comment-3",
                NewComment {
                    body: "Denied viewer".into()
                }
            ),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            db.update_comment(
                "other",
                &first.id,
                EditComment {
                    revision: 1,
                    body: "Denied viewer".into()
                }
            ),
            Err(Error::Forbidden)
        ));
    }
    #[test]
    fn upload_intents_are_task_scoped_expiring_and_metadata_finalization_is_idempotent() {
        let (mut db, task, list) = fixture();
        let other_task = db
            .capture(
                "dev",
                "another",
                Capture {
                    list_id: list.clone(),
                    tasks: vec![NewTask {
                        title: "Other".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let comment = db
            .create_comment(
                "dev",
                &other_task.id,
                "fixture-comment-4",
                NewComment {
                    body: "Other task comment".into(),
                },
            )
            .unwrap();
        assert!(matches!(
            db.create_upload_intent("dev", &task.id, upload(Some(comment.id))),
            Err(Error::Invalid(_))
        ));
        let intent = db
            .create_upload_intent("other", &task.id, upload(None))
            .unwrap();
        assert_eq!(intent.filename, "synthetic.txt");
        assert!(!intent.object_key.contains("synthetic"));
        assert!(
            serde_json::to_string(&intent)
                .unwrap()
                .find("staging_key")
                .is_none()
        );
        let verified = VerifiedObject::synthetic(&intent);
        let attachment = db.finalize_attachment("other", verified.clone()).unwrap();
        assert_eq!(
            attachment.id,
            db.finalize_attachment("other", verified).unwrap().id
        );
        assert_eq!(db.attachments("dev", &task.id).unwrap().len(), 1);
        assert!(db.upload_intent("other", &intent.id).unwrap().finalized);
        assert!(matches!(
            db.delete_attachment("other", &attachment.id, 99),
            Err(Error::Conflict(_))
        ));
        let revision = db.list_info("dev", &list).unwrap().list.revision;
        db.revoke_list("dev", &list, "other", revision).unwrap();
        assert!(matches!(
            db.attachment("other", &attachment.id),
            Err(Error::Forbidden)
        ));
        db.delete_attachment("dev", &attachment.id, attachment.revision)
            .unwrap();
        assert!(matches!(
            db.attachment("dev", &attachment.id),
            Err(Error::NotFound)
        ));
        let intent = db
            .create_upload_intent("dev", &task.id, upload(None))
            .unwrap();
        db.conn
            .execute(
                "UPDATE upload_intents SET expires_at=0 WHERE id=?1",
                [&intent.id],
            )
            .unwrap();
        assert!(matches!(
            db.upload_intent("dev", &intent.id),
            Err(Error::Conflict(_))
        ));
        assert!(matches!(
            db.finalize_attachment("dev", VerifiedObject::synthetic(&intent)),
            Err(Error::Conflict(_))
        ));
    }
    #[test]
    fn comment_deletion_hides_its_attachments_and_permissions_are_rechecked() {
        let (mut db, task, _) = fixture();
        let comment = db
            .create_comment(
                "other",
                &task.id,
                "fixture-comment-5",
                NewComment {
                    body: "With attachment".into(),
                },
            )
            .unwrap();
        let intent = db
            .create_upload_intent("other", &task.id, upload(Some(comment.id.clone())))
            .unwrap();
        let attachment = db
            .finalize_attachment("other", VerifiedObject::synthetic(&intent))
            .unwrap();
        db.delete_comment("dev", &comment.id, comment.revision)
            .unwrap();
        assert!(db.attachments("dev", &task.id).unwrap().is_empty());
        assert!(matches!(
            db.attachment("dev", &attachment.id),
            Err(Error::NotFound)
        ));
        let pending = db
            .create_upload_intent("other", &task.id, upload(None))
            .unwrap();
        db.trash("dev", &task.id, task.revision).unwrap();
        assert!(matches!(
            db.finalize_attachment("other", VerifiedObject::synthetic(&pending)),
            Err(Error::NotFound)
        ));
    }
    #[test]
    fn comment_retries_are_idempotent_payload_bound_and_authorized_live() {
        let (mut db, task, list) = fixture();
        let first = db
            .create_comment(
                "other",
                &task.id,
                "retry-key",
                NewComment {
                    body: "Original comment".into(),
                },
            )
            .unwrap();
        let repeated = db
            .create_comment(
                "other",
                &task.id,
                "retry-key",
                NewComment {
                    body: "Original comment".into(),
                },
            )
            .unwrap();
        assert_eq!(first.id, repeated.id);
        assert_eq!(db.comments("dev", &task.id, 0, 100).unwrap().len(), 1);
        assert!(matches!(
            db.create_comment(
                "other",
                &task.id,
                "retry-key",
                NewComment {
                    body: "Changed comment".into()
                }
            ),
            Err(Error::Conflict(_))
        ));
        let revision = db.list_info("dev", &list).unwrap().list.revision;
        db.revoke_list("dev", &list, "other", revision).unwrap();
        assert!(matches!(
            db.create_comment(
                "other",
                &task.id,
                "retry-key",
                NewComment {
                    body: "Original comment".into()
                }
            ),
            Err(Error::Forbidden)
        ));
        let revision = db.list_info("dev", &list).unwrap().list.revision;
        db.grant_list("dev", &list, "other", ListRole::Editor, revision, true)
            .unwrap();
        db.delete_comment("other", &first.id, first.revision)
            .unwrap();
        assert!(matches!(
            db.create_comment(
                "other",
                &task.id,
                "retry-key",
                NewComment {
                    body: "Original comment".into()
                }
            ),
            Err(Error::NotFound)
        ));
        assert!(db.comments("dev", &task.id, 0, 100).unwrap().is_empty());
    }
    #[test]
    fn comment_references_keep_stable_targets_and_deleted_sources_leave_no_backlinks() {
        let (mut db, task, list) = fixture();
        let target = db
            .capture(
                "dev",
                "reference-target",
                Capture {
                    list_id: list,
                    tasks: vec![NewTask {
                        title: "Referenced task".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let text = format!("See %{}", target.reference);
        let comment = db
            .create_comment(
                "dev",
                &task.id,
                "reference-comment",
                NewComment { body: text.clone() },
            )
            .unwrap();
        let links = db.reference_backlinks("dev", "task", &target.id).unwrap();
        assert!(
            links
                .iter()
                .any(|link| link.source_kind == "comment" && link.source_id == comment.id)
        );
        db.move_subtree("dev", &target.id, "groceries", target.revision, false)
            .unwrap();
        assert!(
            db.reference_backlinks("dev", "task", &target.id)
                .unwrap()
                .iter()
                .any(|link| link.source_id == comment.id)
        );
        assert!(db.reference_backlinks("other", "task", &target.id).is_err());
        let comment = db
            .update_comment(
                "dev",
                &comment.id,
                EditComment {
                    revision: comment.revision,
                    body: text,
                },
            )
            .unwrap();
        db.delete_comment("dev", &comment.id, comment.revision)
            .unwrap();
        assert!(
            db.reference_backlinks("dev", "task", &target.id)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn filenames_body_and_upload_checks_are_bounded() {
        assert_eq!(
            safe_filename("C:\\folder\\safe\r\n.txt").unwrap(),
            "safe.txt"
        );
        assert_eq!(safe_filename("../../résumé.pdf").unwrap(), "résumé.pdf");
        assert!(safe_filename("..").is_err());
        assert!(safe_filename("folder/").is_err());
        let (mut db, task, _) = fixture();
        assert!(
            db.create_comment(
                "dev",
                &task.id,
                "fixture-comment-6",
                NewComment { body: " ".into() }
            )
            .is_err()
        );
        assert!(
            db.create_comment(
                "dev",
                &task.id,
                "fixture-comment-7",
                NewComment {
                    body: "a".repeat(100001)
                }
            )
            .is_err()
        );
        let mut input = upload(None);
        input.size = MAX_ATTACHMENT_BYTES + 1;
        assert!(db.create_upload_intent("dev", &task.id, input).is_err());
        let mut input = upload(None);
        input.sha256 = "zz".repeat(32);
        assert!(db.create_upload_intent("dev", &task.id, input).is_err());
        assert!(serde_json::from_str::<NewComment>(r#"{"body":"x","author_id":"other"}"#).is_err());
    }
}
