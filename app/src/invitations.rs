//! Email-bound, revocable invitations. The link MAC is derived at delivery time;
//! neither raw invitation credentials nor provider tokens are persisted.
use crate::core::{Error, Store, actor_identity};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
#[derive(Clone)]
pub struct InvitationSigner {
    key: [u8; 32],
    origin: String,
}
impl InvitationSigner {
    pub fn new(key: &str, origin: String) -> Result<Self, Error> {
        if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Invalid(
                "TODOER_SERVER_KEY must contain 64 hexadecimal characters".into(),
            ));
        }
        let mut bytes = [0; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&key[i * 2..i * 2 + 2], 16)
                .map_err(|_| Error::Invalid("Invalid TODOER_SERVER_KEY".into()))?;
        }
        Ok(Self { key: bytes, origin })
    }
    fn signature(&self, invite: &Invitation) -> Hmac<Sha256> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key).expect("fixed key");
        mac.update(b"todoer-invitation-v1\0");
        mac.update(
            serde_json::to_string(&(
                &invite.id,
                &invite.inviter,
                &invite.email,
                &invite.target_kind,
                &invite.target_id,
                &invite.role,
                invite.expires_at,
            ))
            .unwrap()
            .as_bytes(),
        );
        mac
    }
    pub fn token(&self, invite: &Invitation) -> String {
        format!(
            "{}.{}",
            invite.id,
            URL_SAFE_NO_PAD.encode(self.signature(invite).finalize().into_bytes())
        )
    }
    pub fn link(&self, invite: &Invitation) -> String {
        format!("{}/invitations/{}", self.origin, self.token(invite))
    }
    fn verify(&self, invite: &Invitation, token: &str) -> Result<(), Error> {
        let (id, sig) = token.split_once('.').ok_or(Error::Forbidden)?;
        if id != invite.id {
            return Err(Error::Forbidden);
        }
        self.signature(invite)
            .verify_slice(&URL_SAFE_NO_PAD.decode(sig).map_err(|_| Error::Forbidden)?)
            .map_err(|_| Error::Forbidden)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invitation {
    pub id: String,
    pub inviter: String,
    pub target_kind: String,
    pub target_id: String,
    pub email: String,
    pub role: String,
    pub expires_at: i64,
    pub revoked: bool,
    pub claimed_by: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InviteInput {
    pub target_kind: String,
    pub target_id: String,
    pub email: String,
    pub role: String,
    pub acknowledge_access_change: bool,
}
#[derive(Debug, Serialize)]
pub struct InvitationGrant {
    pub invitation: Invitation,
    pub link: String,
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Invitation> {
    Ok(Invitation {
        id: r.get(0)?,
        inviter: r.get(1)?,
        target_kind: r.get(2)?,
        target_id: r.get(3)?,
        email: r.get(4)?,
        role: r.get(5)?,
        expires_at: r.get(6)?,
        revoked: r.get(7)?,
        claimed_by: r.get(8)?,
    })
}
const COLS: &str = "id,inviter,target_kind,target_id,email,role,expires_at,revoked,claimed_by";
fn raw(c: &Connection, id: &str) -> Result<Invitation, Error> {
    c.query_row(
        &format!("SELECT {COLS} FROM invitations WHERE id=?1"),
        [id],
        row,
    )
    .optional()?
    .ok_or(Error::NotFound)
}
fn require_owner(c: &Connection, actor: &str, kind: &str, id: &str) -> Result<(), Error> {
    let rank: Option<i64> = match kind {
        "workspace" => c
            .query_row(
                "SELECT rank FROM actor_workspace_access WHERE actor_id=?1 AND workspace_id=?2",
                params![actor, id],
                |r| r.get(0),
            )
            .optional()?,
        "list" => c
            .query_row(
                "SELECT rank FROM actor_list_access WHERE actor_id=?1 AND list_id=?2",
                params![actor, id],
                |r| r.get(0),
            )
            .optional()?,
        _ => {
            return Err(Error::Invalid(
                "Invitation target must be workspace or list".into(),
            ));
        }
    };
    if rank.unwrap_or(0) < 3 {
        Err(Error::Forbidden)
    } else {
        Ok(())
    }
}
impl Store {
    pub fn create_invitation(
        &mut self,
        actor: &str,
        key: &str,
        input: InviteInput,
        signer: &InvitationSigner,
    ) -> Result<InvitationGrant, Error> {
        if key.is_empty() || key.len() > 200 {
            return Err(Error::Invalid(
                "Idempotency-Key must contain 1–200 bytes".into(),
            ));
        }
        if !input.acknowledge_access_change {
            return Err(Error::Conflict(
                "Acknowledge that this invitation grants access to the selected resource".into(),
            ));
        }
        let email = input.email.trim().to_lowercase();
        if email.len() > 320
            || !email
                .parse::<lettre::message::Mailbox>()
                .is_ok_and(|value| value.name.is_none())
        {
            return Err(Error::Invalid("Invalid invitation email".into()));
        }
        if !["owner", "editor", "viewer"].contains(&input.role.as_str())
            || input.target_kind == "list" && input.role == "owner"
        {
            return Err(Error::Invalid("Invalid invitation role".into()));
        }
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        require_owner(&tx, actor, &input.target_kind, &input.target_id)?;
        let (user, _) = actor_identity(&tx, actor)?;
        let payload = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&(&input.target_kind, &input.target_id, &email, &input.role))
                    .unwrap()
            )
        );
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT id,payload_hash FROM invitations WHERE inviter=?1 AND idempotency_key=?2",
                params![user, key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((id, old)) = existing {
            if old != payload {
                return Err(Error::Conflict(
                    "Idempotency key reused for a different invitation".into(),
                ));
            }
            let invitation = raw(&tx, &id)?;
            let hash = format!("{:x}", Sha256::digest(signer.token(&invitation).as_bytes()));
            let active: bool = tx.query_row("SELECT token_hash=?1 AND revoked=0 AND claimed_by IS NULL AND expires_at>unixepoch() FROM invitations WHERE id=?2", params![hash,id], |r|r.get(0))?;
            if !active {
                return Err(Error::Conflict("Invitation closed or signing key changed; use a new idempotency key to issue another invitation".into()));
            }
            return Ok(InvitationGrant {
                link: signer.link(&invitation),
                invitation,
            });
        }
        let now = chrono::Utc::now().timestamp();
        let recent: i64 = tx.query_row(
            "SELECT COUNT(*) FROM invitations WHERE inviter=?1 AND created_at>?2",
            params![user, now - 3600],
            |r| r.get(0),
        )?;
        if recent >= 30 {
            return Err(Error::Invalid(
                "Invitation limit reached; try again later".into(),
            ));
        }
        let invitation = Invitation {
            id: Uuid::new_v4().to_string(),
            inviter: user,
            email,
            role: input.role,
            target_kind: input.target_kind,
            target_id: input.target_id,
            expires_at: now + 7 * 86400,
            revoked: false,
            claimed_by: None,
        };
        let token = signer.token(&invitation);
        let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
        tx.execute("INSERT INTO invitations(id,token_hash,inviter,target_kind,target_id,email,role,expires_at,idempotency_key,payload_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![invitation.id,hash,invitation.inviter,invitation.target_kind,invitation.target_id,invitation.email,invitation.role,invitation.expires_at,key,payload])?;
        tx.execute(
            "INSERT INTO invitation_outbox(id,invitation_id,available_at) VALUES(?1,?2,?3)",
            params![Uuid::new_v4().to_string(), invitation.id, now],
        )?;
        tx.commit()?;
        Ok(InvitationGrant {
            link: signer.link(&invitation),
            invitation,
        })
    }
    pub fn invitations(&self, actor: &str, kind: &str, id: &str) -> Result<Vec<Invitation>, Error> {
        require_owner(&self.conn, actor, kind, id)?;
        let mut q=self.conn.prepare(&format!("SELECT {COLS} FROM invitations WHERE target_kind=?1 AND target_id=?2 ORDER BY created_at DESC LIMIT 100"))?;
        Ok(q.query_map(params![kind, id], row)?
            .collect::<Result<_, _>>()?)
    }
    pub fn revoke_invitation(&mut self, actor: &str, id: &str) -> Result<(), Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let invitation = raw(&tx, id)?;
        if invitation.claimed_by.is_some() {
            return Err(Error::Conflict(
                "Invitation was already accepted; remove access through sharing settings".into(),
            ));
        }
        require_owner(&tx, actor, &invitation.target_kind, &invitation.target_id)?;
        tx.execute("UPDATE invitations SET revoked=1 WHERE id=?1", [id])?;
        tx.execute("UPDATE invitation_outbox SET status='cancelled',lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE invitation_id=?1 AND status IN ('pending','leased')",[id])?;
        tx.commit()?;
        Ok(())
    }
    /// Returns no title before a fresh, verified Google identity has been bound.
    pub fn check_invitation(
        &self,
        token: &str,
        email: Option<&str>,
        signer: &InvitationSigner,
    ) -> Result<Invitation, Error> {
        if token.len() > 150 {
            return Err(Error::Forbidden);
        }
        let (id, _) = token.split_once('.').ok_or(Error::Forbidden)?;
        let invitation = raw(&self.conn, id)?;
        signer.verify(&invitation, token)?;
        let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
        let matches: bool = self.conn.query_row(
            "SELECT token_hash=?1 FROM invitations WHERE id=?2",
            params![hash, id],
            |r| r.get(0),
        )?;
        if !matches {
            return Err(Error::Forbidden);
        }
        if invitation.revoked
            || invitation.claimed_by.is_some()
            || invitation.expires_at <= chrono::Utc::now().timestamp()
        {
            return Err(Error::Forbidden);
        }
        require_owner(
            &self.conn,
            &invitation.inviter,
            &invitation.target_kind,
            &invitation.target_id,
        )?;
        if email.is_some_and(|email| email.to_lowercase() != invitation.email) {
            return Err(Error::Forbidden);
        }
        Ok(invitation)
    }
    /// Only call with the current signed Google callback's verified email.
    pub fn claim_invitation(
        &mut self,
        actor: &str,
        token: &str,
        verified_email: &str,
        signer: &InvitationSigner,
    ) -> Result<Invitation, Error> {
        let mut tx = crate::agent_ops::StoreWrite::new(self)?;
        let invitation = tx.check_invitation(token, Some(verified_email), signer)?;
        match invitation.target_kind.as_str() {
            "workspace" => {
                let workspace = tx.workspace(&invitation.inviter, &invitation.target_id)?;
                let role = match invitation.role.as_str() {
                    "owner" => crate::workspace::WorkspaceRole::Owner,
                    "editor" => crate::workspace::WorkspaceRole::Editor,
                    _ => crate::workspace::WorkspaceRole::Viewer,
                };
                tx.set_workspace_member(
                    &invitation.inviter,
                    &invitation.target_id,
                    actor,
                    role,
                    workspace.revision,
                    true,
                )?;
            }
            "list" => {
                let info = tx.list_info(&invitation.inviter, &invitation.target_id)?;
                let role = if invitation.role == "editor" {
                    crate::workspace::ListRole::Editor
                } else {
                    crate::workspace::ListRole::Viewer
                };
                tx.grant_list(
                    &invitation.inviter,
                    &invitation.target_id,
                    actor,
                    role,
                    info.list.revision,
                    true,
                )?;
            }
            _ => return Err(Error::Forbidden),
        }
        tx.conn.execute(
            "UPDATE invitations SET claimed_by=?1 WHERE id=?2 AND claimed_by IS NULL",
            params![actor, invitation.id],
        )?;
        tx.conn.execute("UPDATE invitation_outbox SET status='cancelled' WHERE invitation_id=?1 AND status IN ('pending','leased')",[&invitation.id])?;
        let result = raw(&tx.conn, &invitation.id)?;
        tx.commit()?;
        Ok(result)
    }
}

#[derive(Clone)]
struct InvitationJob {
    id: String,
    invitation_id: String,
    owner: String,
    token: String,
    attempts: i64,
}
impl Store {
    fn claim_invitation_mail(
        &mut self,
        owner: &str,
        now: i64,
    ) -> Result<Option<InvitationJob>, Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        tx.execute("UPDATE invitation_outbox SET status='dead',lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE attempts>=5 AND (status='pending' OR status='leased' AND lease_until<=?1)",[now])?;
        let row:Option<(String,String,i64)>=tx.query_row("SELECT id,invitation_id,attempts FROM invitation_outbox WHERE attempts<5 AND available_at<=?1 AND (status='pending' OR status='leased' AND lease_until<=?1) ORDER BY available_at,id LIMIT 1",[now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let job = if let Some((id, invitation_id, attempts)) = row {
            let token = Uuid::new_v4().to_string();
            tx.execute("UPDATE invitation_outbox SET status='leased',attempts=attempts+1,lease_owner=?1,lease_token=?2,lease_until=?3 WHERE id=?4",params![owner,token,now+60,id])?;
            Some(InvitationJob {
                id,
                invitation_id,
                owner: owner.into(),
                token,
                attempts: attempts + 1,
            })
        } else {
            None
        };
        tx.commit()?;
        Ok(job)
    }
    fn invitation_mail(
        &self,
        job: &InvitationJob,
        signer: &InvitationSigner,
    ) -> Result<Option<Invitation>, Error> {
        let owned:bool=self.conn.query_row("SELECT EXISTS(SELECT 1 FROM invitation_outbox WHERE id=?1 AND status='leased' AND lease_owner=?2 AND lease_token=?3 AND lease_until>unixepoch())",params![job.id,job.owner,job.token],|r|r.get(0))?;
        if !owned {
            return Ok(None);
        }
        let invitation = raw(&self.conn, &job.invitation_id)?;
        match self.check_invitation(&signer.token(&invitation), None, signer) {
            Ok(value) => Ok(Some(value)),
            Err(Error::Forbidden | Error::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }
    fn finish_invitation_mail(
        &self,
        job: &InvitationJob,
        status: &str,
        now: i64,
    ) -> Result<(), Error> {
        let next = now + (30i64 * 2i64.pow((job.attempts - 1).clamp(0, 6) as u32)).min(3600);
        self.conn.execute("UPDATE invitation_outbox SET status=?1,available_at=?2,last_error=?3,lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE id=?4 AND status='leased' AND lease_owner=?5 AND lease_token=?6",params![status,next,if status=="pending"{Some("SMTP delivery failed or ambiguous")}else{None},job.id,job.owner,job.token])?;
        Ok(())
    }
}
pub(crate) async fn deliver_invitation_mail(
    store: &std::sync::Arc<std::sync::Mutex<Store>>,
    smtp: &crate::notifications::Notifications,
    signer: &InvitationSigner,
    stopping: &std::sync::atomic::AtomicBool,
) -> Result<(), Error> {
    let owner = Uuid::new_v4().to_string();
    for _ in 0..5 {
        if stopping.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let worker = owner.clone();
        let job = crate::notifications::database(store, move |db| {
            db.claim_invitation_mail(&worker, chrono::Utc::now().timestamp())
        })
        .await?;
        let Some(job) = job else { break };
        let copy = job.clone();
        let signing = signer.clone();
        let invitation =
            crate::notifications::database(store, move |db| db.invitation_mail(&copy, &signing))
                .await?;
        let status = if let Some(invitation) = invitation {
            if stopping.load(std::sync::atomic::Ordering::SeqCst) {
                "pending"
            } else if smtp
                .send_invitation(&job.id, &invitation.email, &signer.link(&invitation))
                .await
                .is_ok()
            {
                "sent"
            } else if job.attempts >= 5 {
                "dead"
            } else {
                "pending"
            }
        } else {
            "cancelled"
        };
        crate::notifications::database(store, move |db| {
            db.finish_invitation_mail(&job, status, chrono::Utc::now().timestamp())
        })
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Store, InvitationSigner, String) {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn
            .execute(
                "INSERT INTO users(id,name) VALUES('guest','Synthetic guest')",
                [],
            )
            .unwrap();
        let w = db
            .create_workspace(
                "dev",
                crate::workspace::WorkspaceInput {
                    title: "Private".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        (
            db,
            InvitationSigner::new(&"42".repeat(32), "https://todoer.example.invalid".into())
                .unwrap(),
            w.id,
        )
    }
    fn input(id: &str) -> InviteInput {
        InviteInput {
            target_kind: "workspace".into(),
            target_id: id.into(),
            email: "guest@example.invalid".into(),
            role: "viewer".into(),
            acknowledge_access_change: true,
        }
    }
    #[test]
    fn invitation_claim_is_email_bound_atomic_and_single_use() {
        let (mut db, signer, w) = fixture();
        let grant = db
            .create_invitation("dev", "request", input(&w), &signer)
            .unwrap();
        assert_eq!(
            db.create_invitation("dev", "request", input(&w), &signer)
                .unwrap()
                .link,
            grant.link
        );
        assert_eq!(
            db.conn
                .query_row("SELECT count(*) FROM invitation_outbox", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        let token = signer.token(&grant.invitation);
        assert!(matches!(
            db.claim_invitation("guest", &token, "wrong@example.invalid", &signer),
            Err(Error::Forbidden)
        ));
        assert!(db.workspace("guest", &w).is_err());
        db.claim_invitation("guest", &token, "guest@example.invalid", &signer)
            .unwrap();
        assert_eq!(db.workspace_permission("guest", &w).unwrap(), 1);
        assert!(
            db.claim_invitation("guest", &token, "guest@example.invalid", &signer)
                .is_err()
        );
        assert_eq!(
            db.conn
                .query_row("SELECT status FROM invitation_outbox", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "cancelled"
        );
    }
    #[test]
    fn invitation_rechecks_owner_expiry_and_key_rotation() {
        let (mut db, signer, w) = fixture();
        assert!(
            db.create_invitation("guest", "x", input(&w), &signer)
                .is_err()
        );
        let grant = db
            .create_invitation("dev", "request", input(&w), &signer)
            .unwrap();
        let token = signer.token(&grant.invitation);
        let rotated =
            InvitationSigner::new(&"24".repeat(32), "https://todoer.example.invalid".into())
                .unwrap();
        assert!(db.check_invitation(&token, None, &rotated).is_err());
        assert!(
            db.create_invitation("dev", "request", input(&w), &rotated)
                .is_err()
        );
        let mut changed = input(&w);
        changed.role = "editor".into();
        assert!(matches!(
            db.create_invitation("dev", "request", changed, &signer),
            Err(Error::Conflict(_))
        ));
        db.conn
            .execute("UPDATE invitations SET expires_at=1", [])
            .unwrap();
        assert!(db.check_invitation(&token, None, &signer).is_err());
        let fresh = db
            .create_invitation("dev", "fresh", input(&w), &signer)
            .unwrap();
        db.conn.execute("UPDATE workspace_members SET role='viewer' WHERE user_id='dev' AND workspace_id=?1",[&w]).unwrap();
        assert!(
            db.check_invitation(&signer.token(&fresh.invitation), None, &signer)
                .is_err()
        );
    }
    #[tokio::test]
    #[ignore = "requires scripts/rebuild-smtp-check.py isolated capture service"]
    async fn smtp_invitation_delivery_retries_without_duplicate() {
        let (mut db, signer, w) = fixture();
        db.create_invitation("dev", "smtp", input(&w), &signer)
            .unwrap();
        let db = std::sync::Arc::new(std::sync::Mutex::new(db));
        let smtp = crate::notifications::Notifications::from_env()
            .unwrap()
            .unwrap();
        let stopping = std::sync::atomic::AtomicBool::new(false);
        deliver_invitation_mail(&db, &smtp, &signer, &stopping)
            .await
            .unwrap();
        deliver_invitation_mail(&db, &smtp, &signer, &stopping)
            .await
            .unwrap();
        assert_eq!(
            db.lock()
                .unwrap()
                .conn
                .query_row("SELECT status FROM invitation_outbox", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "sent"
        );
    }
    #[test]
    fn invalid_signing_keys_are_rejected_without_panics() {
        for key in ["短".repeat(32), "a".repeat(63), "z".repeat(64)] {
            assert!(InvitationSigner::new(&key, String::new()).is_err());
        }
    }
}
