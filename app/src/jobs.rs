//! Account-private, read-only operational visibility. Never returns transport data.
use crate::core::{Error, Store, actor_user};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct JobQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub status: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct JobStatus {
    pub source: String,
    pub id: String,
    pub kind: String,
    pub channel: String,
    pub status: String,
    pub attempts: i64,
    pub available_at: i64,
    pub lease_until: Option<i64>,
    pub failure_code: Option<String>,
    pub devices_pending: i64,
    pub devices_sent: i64,
    pub devices_dead: i64,
    pub devices_failed: i64,
}
#[derive(Debug, Serialize)]
pub struct JobPage {
    pub items: Vec<JobStatus>,
    pub next_cursor: Option<String>,
}
#[derive(Deserialize, Serialize)]
struct Cursor {
    user: String,
    source: String,
    id: String,
    status: String,
}
const SOURCES: &[&str] = &["email_digest", "invitation", "push_digest", "reminder"];
fn invalid() -> Error {
    Error::Invalid("invalid jobs pagination".into())
}
fn failure(status: &str, error: Option<&str>, device_failures: i64) -> Option<String> {
    let code = match error {
        Some("subscription expired") => "subscription_expired",
        Some("provider temporarily unavailable") => "provider_temporarily_unavailable",
        Some("provider rejected delivery") => "provider_rejected_delivery",
        Some(_) => "delivery_failed_or_ambiguous",
        None if status == "dead" => "retry_exhausted",
        None if device_failures > 0 => "device_delivery_failed",
        None => return None,
    };
    Some(code.into())
}
impl Store {
    pub fn job_status(&self, actor: &str, query: JobQuery) -> Result<JobPage, Error> {
        // An account-wide view must never turn a resource-bound grant into counts
        // about the account's other resources, even if that grant says admin.
        let user = actor_user(&self.conn, actor, 3)?;
        let limit = query.limit.unwrap_or(25);
        let status = query.status.unwrap_or_default();
        if ![
            "",
            "pending",
            "leased",
            "delivered",
            "sent",
            "cancelled",
            "dead",
            "failed",
        ]
        .contains(&status.as_str())
        {
            return Err(invalid());
        }
        if !(1..=100).contains(&limit) {
            return Err(invalid());
        }
        let (source, id) = if let Some(value) = query.cursor {
            if value.len() > 2048 {
                return Err(invalid());
            }
            let cursor: Cursor =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(value).map_err(|_| invalid())?)
                    .map_err(|_| invalid())?;
            if cursor.status != status
                || cursor.user != user
                || !SOURCES.contains(&cursor.source.as_str())
                || cursor.id.len() > 256
            {
                return Err(invalid());
            }
            (cursor.source, cursor.id)
        } else {
            (String::new(), String::new())
        };
        let tx = self.conn.unchecked_transaction()?;
        let mut q=tx.prepare("WITH jobs AS (
          SELECT 'reminder' source,id,'reminder' kind,channel,status,attempts,available_at,lease_until,last_error FROM reminder_outbox WHERE recipient=?1
          UNION ALL SELECT 'email_digest',id,kind,'email',status,attempts,available_at,lease_until,last_error FROM digest_outbox WHERE user_id=?1
          UNION ALL SELECT 'push_digest',id,kind,'push',status,attempts,available_at,lease_until,last_error FROM push_digest_outbox WHERE user_id=?1
          UNION ALL SELECT 'invitation',o.id,'invitation','email',o.status,o.attempts,o.available_at,o.lease_until,o.last_error FROM invitation_outbox o JOIN invitations i ON i.id=o.invitation_id WHERE i.inviter=?1
        ) SELECT source,id,kind,channel,status,attempts,available_at,lease_until,last_error FROM jobs WHERE (source,id)>(?2,?3) AND (?5='' OR status=?5 OR ?5='failed' AND (last_error IS NOT NULL OR status='dead' OR source='reminder' AND channel='push' AND EXISTS(SELECT 1 FROM push_deliveries d WHERE d.reminder_id=jobs.id AND d.last_error IS NOT NULL) OR source='push_digest' AND EXISTS(SELECT 1 FROM push_digest_deliveries d WHERE d.reminder_id=jobs.id AND d.last_error IS NOT NULL))) ORDER BY source,id LIMIT ?4")?;
        let mut items = q
            .query_map(params![user, source, id, (limit + 1) as i64, status], |r| {
                let status: String = r.get(4)?;
                let error: Option<String> = r.get(8)?;
                Ok(JobStatus {
                    source: r.get(0)?,
                    id: r.get(1)?,
                    kind: r.get(2)?,
                    channel: r.get(3)?,
                    failure_code: failure(&status, error.as_deref(), 0),
                    status,
                    attempts: r.get(5)?,
                    available_at: r.get(6)?,
                    lease_until: r.get(7)?,
                    devices_pending: 0,
                    devices_sent: 0,
                    devices_dead: 0,
                    devices_failed: 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > limit;
        items.truncate(limit);
        for row in &mut items {
            let table = match (row.source.as_str(), row.channel.as_str()) {
                ("reminder", "push") => Some("push_deliveries"),
                ("push_digest", _) => Some("push_digest_deliveries"),
                _ => None,
            };
            if let Some(table) = table {
                (row.devices_pending,row.devices_sent,row.devices_dead,row.devices_failed)=tx.query_row(&format!("SELECT COALESCE(SUM(status IN ('pending','leased')),0),COALESCE(SUM(status='sent'),0),COALESCE(SUM(status='dead'),0),COALESCE(SUM(last_error IS NOT NULL),0) FROM {table} WHERE reminder_id=?1"),[&row.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
                if row.failure_code.is_none() {
                    row.failure_code = failure(&row.status, None, row.devices_failed);
                }
            }
            // Status values are application enums; don't reflect unexpected DB text.
            if ![
                "pending",
                "leased",
                "delivered",
                "sent",
                "cancelled",
                "dead",
            ]
            .contains(&row.status.as_str())
            {
                row.status = "unknown".into();
            }
        }
        let next_cursor = if more {
            items.last().map(|r| {
                URL_SAFE_NO_PAD.encode(
                    serde_json::to_vec(&Cursor {
                        user,
                        source: r.source.clone(),
                        id: r.id.clone(),
                        status: status.clone(),
                    })
                    .expect("cursor strings serialize"),
                )
            })
        } else {
            None
        };
        drop(q);
        tx.commit()?;
        Ok(JobPage { items, next_cursor })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_private_keyset_pages_and_redacted_failures() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn.execute("INSERT INTO digest_outbox(id,user_id,kind,period,status,attempts,available_at,last_error) VALUES('a','dev','daily','2026-10-26','dead',5,1,'secret transport URL must never escape'),('b','dev','weekly','2026-10-26','pending',1,2,NULL),('private-other','other','daily','2026-10-26','dead',5,1,'other private failure')",[]).unwrap();
        let page = db
            .job_status(
                "dev",
                JobQuery {
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(page.items[0].id, "a");
        assert_eq!(
            page.items[0].failure_code.as_deref(),
            Some("delivery_failed_or_ambiguous")
        );
        let serialized = serde_json::to_string(&page).unwrap();
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("private-other"));
        let cursor = page.next_cursor.unwrap();
        let next = db
            .job_status(
                "dev",
                JobQuery {
                    cursor: Some(cursor.clone()),
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(next.items[0].id, "b");
        assert!(next.next_cursor.is_none());
        assert!(
            db.job_status(
                "other",
                JobQuery {
                    cursor: Some(cursor),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            db.job_status(
                "dev",
                JobQuery {
                    limit: Some(101),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(
            db.job_status("other", JobQuery::default())
                .unwrap()
                .items
                .len(),
            1
        );
    }
    #[test]
    fn all_outboxes_and_partial_push_failures_are_account_scoped() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn.execute_batch("INSERT INTO tasks(id,reference,list_id,title,position) VALUES('task','T-SYN','inbox','secret task title',0);
          INSERT INTO reminder_outbox(id,task_id,schedule_revision,recipient,channel,at,status,available_at,lease_until) VALUES('reminder-own','task',1,'dev','push',1,'leased',1,42),('reminder-other','task',2,'other','email',1,'pending',1,NULL);
          INSERT INTO push_digest_outbox(id,user_id,kind,period,available_at) VALUES('push-own','dev','daily','2026-10-26',1),('push-other','other','daily','2026-10-26',1);
          INSERT INTO invitations(id,token_hash,inviter,target_kind,target_id,email,role,expires_at,idempotency_key,payload_hash) VALUES('invite-own','secret-token','dev','list','inbox','synthetic@example.invalid','viewer',100,'own','hash'),('invite-other','secret-other-token','other','list','inbox','other@example.invalid','viewer',100,'other','hash');
          INSERT INTO invitation_outbox(id,invitation_id,available_at) VALUES('invite-job-own','invite-own',1),('invite-job-other','invite-other',1);
          INSERT INTO digest_outbox(id,user_id,kind,period,available_at) VALUES('email-own','dev','weekly','2026-10-26',1);
          INSERT INTO auth_sessions(id,user_id,token_hash,csrf_hash,expires_at) VALUES('session','dev','secret-session','secret-csrf',100);
          INSERT INTO push_devices(id,user_id,session_id,endpoint,p256dh,auth_secret,label,created_at,updated_at) VALUES('device','dev','session','https://secret.invalid/push','secret-key','secret-auth','secret-label',1,1);
          INSERT INTO push_deliveries(reminder_id,device_id,status,available_at,last_error) VALUES('reminder-own','device','dead',1,'secret provider error');
          INSERT INTO push_digest_deliveries(reminder_id,device_id,status,available_at,last_error) VALUES('push-own','device','pending',1,'secret provider error');").unwrap();
        let page = db.job_status("dev", JobQuery::default()).unwrap();
        assert_eq!(page.items.len(), 4);
        assert_eq!(
            db.job_status(
                "dev",
                JobQuery {
                    status: Some("failed".into()),
                    ..Default::default()
                }
            )
            .unwrap()
            .items
            .len(),
            2
        );
        let reminder = page.items.iter().find(|r| r.source == "reminder").unwrap();
        assert_eq!(reminder.lease_until, Some(42));
        assert_eq!(reminder.devices_dead, 1);
        assert_eq!(reminder.devices_failed, 1);
        assert_eq!(
            reminder.failure_code.as_deref(),
            Some("device_delivery_failed")
        );
        let digest = page
            .items
            .iter()
            .find(|r| r.source == "push_digest")
            .unwrap();
        assert_eq!(digest.devices_pending, 1);
        assert_eq!(digest.devices_failed, 1);
        let text = serde_json::to_string(&page).unwrap();
        for forbidden in ["secret", "other", "synthetic@example.invalid"] {
            assert!(!text.contains(forbidden));
        }
        assert_eq!(
            db.job_status("other", JobQuery::default())
                .unwrap()
                .items
                .len(),
            3
        );
    }
}
