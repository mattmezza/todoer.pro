//! Account-level inference controls. Provider keys are write-only and owner-bound.
use crate::core::{Error, Store, actor_identity, actor_user};
use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct AiPolicy {
    pub user_id: String,
    pub user_ai_enabled: bool,
    pub managed_ai_allowed: bool,
    pub byok_allowed: bool,
    pub funding: String,
    pub model_id: Option<String>,
    pub key_configured: bool,
    pub key_status: String,
    pub access_request: Option<String>,
    pub daily_limit_micro: i64,
    pub revision: i64,
    pub billing_blocked: bool,
    pub debt_micro: i64,
}
// Never derive Debug or Serialize: api_key contains the decrypted user secret.
pub struct AiFunding {
    pub policy_revision: i64,
    pub user_id: String,
    pub funding: String,
    pub model_id: String,
    pub api_key: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyUpdate {
    pub user_ai_enabled: bool,
    pub funding: String,
    pub model_id: Option<String>,
    pub revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminUpdate {
    pub daily_limit_micro: Option<i64>,
    pub managed_ai_allowed: bool,
    pub byok_allowed: bool,
    pub decision: Option<String>,
    pub reason: String,
    pub revision: i64,
}
#[derive(Serialize)]
pub struct AccountNotice {
    pub id: i64,
    pub kind: String,
    pub state: String,
    pub created_at: i64,
    pub unread: bool,
}
#[derive(Serialize)]
pub struct AccessRequest {
    pub user_id: String,
    pub state: String,
    pub requested_at: i64,
    pub decided_at: Option<i64>,
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn account(c: &Connection, actor: &str) -> Result<String, Error> {
    let (user, _) = actor_identity(c, actor)?;
    actor_user(c, &user, 1)?;
    Ok(user)
}
pub(crate) fn browser_account(c: &Connection, actor: &str) -> Result<String, Error> {
    if actor.starts_with("credential:") {
        return Err(Error::Forbidden);
    }
    account(c, actor)
}
pub fn require_admin(c: &Connection, actor: &str) -> Result<String, Error> {
    let user = browser_account(c, actor)?;
    if !crate::account_security::is_admin(c, &user).map_err(|_| Error::Forbidden)? {
        return Err(Error::Forbidden);
    }
    Ok(user)
}
fn ensure(c: &Connection, user: &str) -> Result<(), Error> {
    c.execute(
        "INSERT OR IGNORE INTO ai_account_policy(user_id) VALUES(?1)",
        [user],
    )?;
    Ok(())
}
fn read(c: &Connection, user: &str) -> Result<AiPolicy, Error> {
    ensure(c, user)?;
    Ok(c.query_row("SELECT user_ai_enabled,managed_ai_allowed,byok_allowed,funding,model_id,key_status,revision,daily_limit_micro,EXISTS(SELECT 1 FROM ai_provider_keys k WHERE k.user_id=p.user_id),(SELECT state FROM ai_access_requests a WHERE a.user_id=p.user_id),COALESCE((SELECT blocked FROM billing_accounts b WHERE b.user_id=p.user_id),0),COALESCE((SELECT debt_micro FROM billing_accounts b WHERE b.user_id=p.user_id),0) FROM ai_account_policy p WHERE user_id=?1",[user],|r|Ok(AiPolicy {
        user_id:user.into(), user_ai_enabled:r.get(0)?,managed_ai_allowed:r.get(1)?,byok_allowed:r.get(2)?,funding:r.get(3)?,model_id:r.get(4)?,key_status:r.get(5)?,revision:r.get(6)?,daily_limit_micro:r.get(7)?,key_configured:r.get(8)?,access_request:r.get(9)?,billing_blocked:r.get(10)?,debt_micro:r.get(11)?
    }))?)
}
fn current_revision(p: &AiPolicy, revision: i64) -> Result<(), Error> {
    if p.revision != revision {
        return Err(Error::Conflict(
            "AI settings changed; reload before saving".into(),
        ));
    }
    Ok(())
}
const ADMIN_ACCOUNT_PREFIX_SQL: &str = r"WITH
              names AS (SELECT id FROM users WHERE name COLLATE NOCASE LIKE ?1 ESCAPE '\' ORDER BY name COLLATE NOCASE,id LIMIT ?2),
              ids AS (SELECT id FROM users WHERE id COLLATE NOCASE LIKE ?1 ESCAPE '\' ORDER BY id COLLATE NOCASE LIMIT ?2),
              emails AS (SELECT user_id AS id FROM auth_identities WHERE email COLLATE NOCASE LIKE ?1 ESCAPE '\' ORDER BY email COLLATE NOCASE,user_id LIMIT ?2),
              matches AS (SELECT id FROM names UNION SELECT id FROM ids UNION SELECT id FROM emails)
              SELECT u.id FROM matches JOIN users u ON u.id=matches.id ORDER BY u.name COLLATE NOCASE,u.id LIMIT ?2";

fn active_model(c: &Connection, model: &str) -> Result<(), Error> {
    let enabled:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM ai_models m JOIN ai_provider_policy p ON p.id=m.provider WHERE m.id=?1 AND m.enabled=1 AND m.archived=0 AND p.enabled=1)",[model],|r|r.get(0))?;
    if !enabled {
        return Err(invalid("Choose an available supported model"));
    }
    Ok(())
}
fn pause(c: &Connection, user: &str, reason: &str) -> Result<(), Error> {
    c.execute(
        "UPDATE billing_auto_topup SET enabled=0,paused_reason=?2 WHERE user_id=?1",
        params![user, reason],
    )?;
    Ok(())
}
fn key_id(material: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("todoer:byok:key-id:v1:{material}"))
    )[..16]
        .into()
}
fn cipher(material: &str) -> Result<ChaCha20Poly1305, Error> {
    if material.len() < 32 {
        return Err(invalid("BYOK encryption is not configured"));
    }
    let digest = Sha256::digest(format!("todoer:byok:openrouter:v1:{material}"));
    Ok(ChaCha20Poly1305::new_from_slice(&digest).expect("32 byte digest"))
}
fn encrypt(material: &str, user: &str, key: &str) -> Result<String, Error> {
    let random = Uuid::new_v4();
    let nonce = &random.as_bytes()[..12];
    let mut bytes = nonce.to_vec();
    bytes.extend(
        cipher(material)?
            .encrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: key.as_bytes(),
                    aad: format!("openrouter:{user}:v1").as_bytes(),
                },
            )
            .map_err(|_| invalid("BYOK encryption unavailable"))?,
    );
    Ok(STANDARD.encode(bytes))
}
fn decrypt(material: &str, user: &str, encrypted: &str) -> Result<String, Error> {
    let bytes = STANDARD
        .decode(encrypted)
        .map_err(|_| invalid("BYOK key unavailable"))?;
    if bytes.len() < 28 {
        return Err(invalid("BYOK key unavailable"));
    }
    let decrypted = cipher(material)?
        .decrypt(
            Nonce::from_slice(&bytes[..12]),
            Payload {
                msg: &bytes[12..],
                aad: format!("openrouter:{user}:v1").as_bytes(),
            },
        )
        .map_err(|_| invalid("BYOK key unavailable"))?;
    String::from_utf8(decrypted).map_err(|_| invalid("BYOK key unavailable"))
}
pub fn encryption_material() -> Result<String, Error> {
    let value = std::env::var("TODOER_BYOK_ENCRYPTION_KEY")
        .or_else(|_| std::env::var("TODOER_SERVER_KEY"))
        .map_err(|_| invalid("BYOK encryption is not configured"))?;
    if value.len() < 32 {
        return Err(invalid("BYOK encryption is not configured"));
    }
    Ok(value)
}
pub fn validate_key_format(key: &str) -> Result<(), Error> {
    if key.len() < 16 || key.len() > 512 || key.bytes().any(|b| !b.is_ascii_graphic()) {
        return Err(invalid("Enter a valid OpenRouter API key"));
    }
    Ok(())
}
impl Store {
    pub fn ai_policy(&self, actor: &str) -> Result<AiPolicy, Error> {
        read(&self.conn, &account(&self.conn, actor)?)
    }
    pub fn ai_update_policy(
        &mut self,
        actor: &str,
        input: PolicyUpdate,
    ) -> Result<AiPolicy, Error> {
        let user = browser_account(&self.conn, actor)?;
        let tx = self.conn.transaction()?;
        let p = read(&tx, &user)?;
        current_revision(&p, input.revision)?;
        if !matches!(input.funding.as_str(), "none" | "credits" | "byok") {
            return Err(invalid("Choose a supported funding source"));
        }
        if let Some(model) = &input.model_id
            && (input.user_ai_enabled || input.model_id != p.model_id)
        {
            active_model(&tx, model)?;
        }
        let validate_funding = input.user_ai_enabled || input.funding != p.funding;
        if validate_funding && input.funding == "credits" && !p.managed_ai_allowed {
            return Err(Error::Forbidden);
        }
        if validate_funding
            && input.funding == "byok"
            && (!p.byok_allowed || !p.key_configured || p.key_status != "valid")
        {
            return Err(invalid("Configure a valid OpenRouter key first"));
        }
        if input.user_ai_enabled && (input.funding == "none" || input.model_id.is_none()) {
            return Err(invalid(
                "Choose a funding source and model before enabling AI",
            ));
        }
        tx.execute("UPDATE ai_account_policy SET user_ai_enabled=?2,funding=?3,model_id=?4,revision=revision+1 WHERE user_id=?1",params![user,input.user_ai_enabled,input.funding,input.model_id])?;
        if !input.user_ai_enabled || input.funding != "credits" {
            pause(
                &tx,
                &user,
                if input.funding == "byok" {
                    "byok_active"
                } else {
                    "ai_disabled"
                },
            )?;
        }
        tx.commit()?;
        read(&self.conn, &user)
    }
    pub fn ai_request_access(&mut self, actor: &str) -> Result<AiPolicy, Error> {
        let user = browser_account(&self.conn, actor)?;
        let tx = self.conn.transaction()?;
        let p = read(&tx, &user)?;
        if p.managed_ai_allowed || p.access_request.as_deref() == Some("pending") {
            tx.commit()?;
            return read(&self.conn, &user);
        }
        let last: Option<i64> = tx
            .query_row(
                "SELECT requested_at FROM ai_access_requests WHERE user_id=?1",
                [&user],
                |r| r.get(0),
            )
            .optional()?;
        if last.is_some_and(|at| at > chrono::Utc::now().timestamp() - 86400) {
            return Err(invalid("Please wait before requesting access again"));
        }
        tx.execute("INSERT INTO ai_access_requests(user_id,state) VALUES(?1,'pending') ON CONFLICT(user_id) DO UPDATE SET state='pending',requested_at=unixepoch(),decided_at=NULL,decided_by=NULL",[&user])?;
        tx.execute(
            "UPDATE ai_account_policy SET revision=revision+1 WHERE user_id=?1",
            [&user],
        )?;
        tx.commit()?;
        read(&self.conn, &user)
    }
    pub fn ai_key_check(&mut self, actor: &str, model: &str, revision: i64) -> Result<(), Error> {
        let user = browser_account(&self.conn, actor)?;
        let tx = self.conn.transaction()?;
        let p = read(&tx, &user)?;
        current_revision(&p, revision)?;
        if !p.byok_allowed {
            return Err(Error::Forbidden);
        }
        active_model(&tx, model)?;
        tx.execute("INSERT INTO ai_key_checks VALUES(?1,unixepoch(),1) ON CONFLICT(user_id) DO UPDATE SET attempts=CASE WHEN window_at<unixepoch()-600 THEN 1 ELSE attempts+1 END,window_at=CASE WHEN window_at<unixepoch()-600 THEN unixepoch() ELSE window_at END",[&user])?;
        let attempts: i64 = tx.query_row(
            "SELECT attempts FROM ai_key_checks WHERE user_id=?1",
            [&user],
            |r| r.get(0),
        )?;
        tx.commit()?;
        if attempts > 5 {
            return Err(invalid("Too many key checks; try again later"));
        }
        Ok(())
    }
    /// Caller must first validate this exact key with OpenRouter. Nothing is stored on validation failure.
    pub fn ai_save_verified_key(
        &mut self,
        actor: &str,
        key: &str,
        model: &str,
        revision: i64,
        material: &str,
    ) -> Result<AiPolicy, Error> {
        validate_key_format(key)?;
        let user = browser_account(&self.conn, actor)?;
        let encrypted = encrypt(material, &user, key)?;
        let tx = self.conn.transaction()?;
        let p = read(&tx, &user)?;
        current_revision(&p, revision)?;
        if !p.byok_allowed {
            return Err(Error::Forbidden);
        }
        active_model(&tx, model)?;
        tx.execute("INSERT INTO ai_provider_keys(user_id,ciphertext,key_id) VALUES(?1,?2,?3) ON CONFLICT(user_id) DO UPDATE SET ciphertext=excluded.ciphertext,key_id=excluded.key_id,validated_at=unixepoch()",params![user,encrypted,key_id(material)])?;
        tx.execute("UPDATE ai_account_policy SET funding='byok',model_id=?2,key_status='valid',revision=revision+1 WHERE user_id=?1",params![user,model])?;
        pause(&tx, &user, "byok_active")?;
        tx.commit()?;
        read(&self.conn, &user)
    }
    pub fn ai_remove_key(&mut self, actor: &str) -> Result<AiPolicy, Error> {
        let user = browser_account(&self.conn, actor)?;
        let tx = self.conn.transaction()?;
        ensure(&tx, &user)?;
        tx.execute("DELETE FROM ai_provider_keys WHERE user_id=?1", [&user])?;
        tx.execute("UPDATE ai_account_policy SET key_status='revoked',funding=CASE WHEN funding='byok' THEN 'none' ELSE funding END,revision=revision+1 WHERE user_id=?1",[&user])?;
        pause(&tx, &user, "key_removed")?;
        tx.commit()?;
        read(&self.conn, &user)
    }
    pub fn ai_invalidate_key(&mut self, actor: &str) -> Result<(), Error> {
        let revision = self.ai_policy(actor)?.revision;
        self.ai_invalidate_key_revision(actor, revision)
    }
    pub fn ai_invalidate_key_revision(
        &mut self,
        actor: &str,
        expected_revision: i64,
    ) -> Result<(), Error> {
        let user = account(&self.conn, actor)?;
        let tx = self.conn.transaction()?;
        let changed=tx.execute("UPDATE ai_account_policy SET key_status='invalid',revision=revision+1 WHERE user_id=?1 AND key_status='valid' AND revision=?2",params![user,expected_revision])?;
        if changed > 0 {
            pause(&tx, &user, "key_invalid")?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn ai_resolve(&self, actor: &str) -> Result<AiFunding, Error> {
        self.ai_resolve_with_material(actor, None)
    }
    fn ai_resolve_with_material(
        &self,
        actor: &str,
        material: Option<&str>,
    ) -> Result<AiFunding, Error> {
        let p = self.ai_policy(actor)?;
        if std::env::var("TODOER_AI_ENABLED").is_ok_and(|s| s != "1") || !p.user_ai_enabled {
            return Err(invalid("AI is switched off"));
        }
        let model = p
            .model_id
            .ok_or_else(|| invalid("Choose a supported model"))?;
        active_model(&self.conn, &model)?;
        let api_key = match p.funding.as_str() {
            "credits" if p.managed_ai_allowed => {
                let blocked: bool = self.conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM billing_accounts WHERE user_id=?1 AND blocked=1)",
                    [&p.user_id],
                    |r| r.get(0),
                )?;
                if blocked {
                    return Err(invalid("Billing requires attention"));
                }
                None
            }
            "byok" if p.byok_allowed && p.key_status == "valid" => {
                let (encrypted, id): (String, String) = self
                    .conn
                    .query_row(
                        "SELECT ciphertext,key_id FROM ai_provider_keys WHERE user_id=?1",
                        [&p.user_id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?
                    .ok_or_else(|| invalid("Configure an OpenRouter key"))?;
                let owned;
                let current = if let Some(s) = material {
                    s
                } else {
                    owned = encryption_material()?;
                    &owned
                };
                let previous = std::env::var("TODOER_BYOK_PREVIOUS_ENCRYPTION_KEY").ok();
                let chosen = if key_id(current) == id {
                    current
                } else {
                    previous
                        .as_deref()
                        .filter(|s| key_id(s) == id)
                        .ok_or_else(|| invalid("BYOK key unavailable; reconfigure your key"))?
                };
                Some(decrypt(chosen, &p.user_id, &encrypted)?)
            }
            _ => {
                return Err(invalid(
                    "AI funding is unavailable; choose a funding source",
                ));
            }
        };
        Ok(AiFunding {
            policy_revision: p.revision,
            user_id: p.user_id,
            funding: p.funding,
            model_id: model,
            api_key,
        })
    }
    pub fn ai_account_notices(
        &self,
        actor: &str,
        after: Option<i64>,
        limit: usize,
    ) -> Result<Vec<AccountNotice>, Error> {
        let user = browser_account(&self.conn, actor)?;
        Ok(self.conn.prepare("SELECT id,kind,state,created_at,read_at IS NULL FROM ai_account_notices WHERE user_id=?1 AND (?2 IS NULL OR id>?2) ORDER BY id LIMIT ?3")?.query_map(params![user,after,limit.clamp(1,100) as i64],|r|Ok(AccountNotice{id:r.get(0)?,kind:r.get(1)?,state:r.get(2)?,created_at:r.get(3)?,unread:r.get(4)?}))?.collect::<Result<_,_>>()?)
    }
    pub fn ai_read_notices(&mut self, actor: &str, through: i64) -> Result<(), Error> {
        let user = browser_account(&self.conn, actor)?;
        if through < 0 {
            return Err(invalid("Invalid notification cursor"));
        }
        self.conn.execute("UPDATE ai_account_notices SET read_at=COALESCE(read_at,unixepoch()) WHERE user_id=?1 AND id<=?2",params![user,through])?;
        Ok(())
    }
    pub fn ai_admin_audit(
        &self,
        actor: &str,
        after: Option<i64>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, Error> {
        require_admin(&self.conn, actor)?;
        Ok(self.conn.prepare("SELECT id,actor,target,action,reason,before_json,after_json,created_at FROM ai_admin_audit WHERE (?1 IS NULL OR id>?1) ORDER BY id LIMIT ?2")?.query_map(params![after,limit.clamp(1,100) as i64],|r|Ok(serde_json::json!({"id":r.get::<_,i64>(0)?,"actor":r.get::<_,String>(1)?,"target":r.get::<_,Option<String>>(2)?,"action":r.get::<_,String>(3)?,"reason":r.get::<_,String>(4)?,"before":serde_json::from_str::<serde_json::Value>(&r.get::<_,String>(5)?).unwrap_or(serde_json::Value::Null),"after":serde_json::from_str::<serde_json::Value>(&r.get::<_,String>(6)?).unwrap_or(serde_json::Value::Null),"created_at":r.get::<_,i64>(7)?})))?.collect::<Result<_,_>>()?)
    }
    pub fn ai_admin_users(
        &self,
        actor: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AiPolicy>, Error> {
        require_admin(&self.conn, actor)?;
        let ids = self
            .conn
            .prepare("SELECT id FROM users WHERE (?1 IS NULL OR id>?1) ORDER BY id LIMIT ?2")?
            .query_map(params![after, limit.clamp(1, 100) as i64], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter().map(|id| read(&self.conn, &id)).collect()
    }
    /// Indexed prefix search; each branch contributes at most twenty candidates.
    pub fn ai_admin_search_users(
        &self,
        actor: &str,
        query: &str,
        selected: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AiPolicy>, Error> {
        require_admin(&self.conn, actor)?;
        let query = query.trim();
        if query.len() > 200 || selected.is_some_and(|id| id.len() > 100) {
            return Err(invalid("Account search is too long"));
        }
        if selected.is_none() && query.is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 20) as i64;
        let ids = if let Some(selected) = selected {
            self.conn
                .prepare("SELECT id FROM users WHERE id=?1")?
                .query_map([selected], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let prefix = format!(
                "{}%",
                query
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            self.conn
                .prepare(ADMIN_ACCOUNT_PREFIX_SQL)?
                .query_map(params![prefix, limit], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        ids.into_iter().map(|id| read(&self.conn, &id)).collect()
    }
    pub fn ai_admin_requests(
        &self,
        actor: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AccessRequest>, Error> {
        require_admin(&self.conn, actor)?;
        Ok(self.conn.prepare("SELECT user_id,state,requested_at,decided_at FROM ai_access_requests WHERE state='pending' AND (?1 IS NULL OR user_id>?1) ORDER BY user_id LIMIT ?2")?.query_map(params![after,limit.clamp(1,100) as i64],|r|Ok(AccessRequest{user_id:r.get(0)?,state:r.get(1)?,requested_at:r.get(2)?,decided_at:r.get(3)?}))?.collect::<Result<_,_>>()?)
    }
    pub fn ai_admin_update(
        &mut self,
        actor: &str,
        target: &str,
        input: AdminUpdate,
    ) -> Result<AiPolicy, Error> {
        let admin = require_admin(&self.conn, actor)?;
        browser_account(&self.conn, target)?;
        if input.reason.trim().is_empty() || input.reason.len() > 500 {
            return Err(invalid("Provide an audit reason (up to 500 bytes)"));
        }
        if input
            .decision
            .as_deref()
            .is_some_and(|s| !matches!(s, "approved" | "denied" | "revoked"))
        {
            return Err(invalid("Invalid access decision"));
        }
        if input.decision.as_deref() == Some("approved") && !input.managed_ai_allowed
            || matches!(input.decision.as_deref(), Some("denied" | "revoked"))
                && input.managed_ai_allowed
        {
            return Err(invalid("Decision and managed-AI permission must agree"));
        }
        if input
            .daily_limit_micro
            .is_some_and(|limit| !(1..=1_000_000_000).contains(&limit))
        {
            return Err(invalid(
                "Daily spend limit must be between 1 and 1000000000 micro-EUR",
            ));
        }
        let tx = self.conn.transaction()?;
        let before = read(&tx, target)?;
        current_revision(&before, input.revision)?;
        tx.execute("UPDATE ai_account_policy SET managed_ai_allowed=?2,byok_allowed=?3,daily_limit_micro=COALESCE(?4,daily_limit_micro),funding=CASE WHEN funding='credits' AND ?2=0 OR funding='byok' AND ?3=0 THEN 'none' ELSE funding END,revision=revision+1 WHERE user_id=?1",params![target,input.managed_ai_allowed,input.byok_allowed,input.daily_limit_micro])?;
        let decision_notice = input.decision.clone();
        if let Some(decision) = input.decision {
            tx.execute("INSERT INTO ai_access_requests(user_id,state,decided_at,decided_by) VALUES(?1,?2,unixepoch(),?3) ON CONFLICT(user_id) DO UPDATE SET state=excluded.state,decided_at=excluded.decided_at,decided_by=excluded.decided_by",params![target,decision,admin])?;
        }
        if !input.managed_ai_allowed || !input.byok_allowed {
            pause(&tx, target, "access_changed")?;
        }
        let after = read(&tx, target)?;
        tx.execute("INSERT INTO ai_admin_audit(actor,target,action,reason,before_json,after_json) VALUES(?1,?2,'access',?3,?4,?5)",params![admin,target,input.reason,serde_json::to_string(&before).map_err(|_|invalid("Cannot record audit"))?,serde_json::to_string(&after).map_err(|_|invalid("Cannot record audit"))?])?;
        if let Some(state) = decision_notice {
            let event_id = tx.last_insert_rowid();
            tx.execute("INSERT INTO ai_account_notices(user_id,kind,state,event_id) VALUES(?1,'access',?2,?3)", params![target,state,event_id])?;
        }
        tx.commit()?;
        Ok(after)
    }
    pub fn ai_rotate_keys(
        &mut self,
        actor: &str,
        current: &str,
        previous: &str,
    ) -> Result<usize, Error> {
        let admin = require_admin(&self.conn, actor)?;
        cipher(current)?;
        cipher(previous)?;
        if key_id(current) == key_id(previous) {
            return Err(invalid(
                "Configure distinct current and previous encryption keys",
            ));
        }
        let tx = self.conn.transaction()?;
        let rows = tx
            .prepare("SELECT user_id,ciphertext FROM ai_provider_keys WHERE key_id=?1 LIMIT 100")?
            .query_map([key_id(previous)], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (user, value) in &rows {
            let plaintext = decrypt(previous, user, value)?;
            tx.execute(
                "UPDATE ai_provider_keys SET ciphertext=?2,key_id=?3 WHERE user_id=?1",
                params![user, encrypt(current, user, &plaintext)?, key_id(current)],
            )?;
        }
        tx.execute("INSERT INTO ai_admin_audit(actor,action,reason,before_json,after_json) VALUES(?1,'key_rotation','Deployment encryption-key rotation','{}',?2)",params![admin,serde_json::json!({"rotated":rows.len()}).to_string()])?;
        tx.commit()?;
        Ok(rows.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MATERIAL: &str = "synthetic-byok-encryption-key-with-at-least-32-bytes";
    const KEY: &str = "sk-or-v1-synthetic-key-never-valid-at-a-provider";
    fn fixture() -> Store {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn
            .execute(
                "UPDATE ai_models SET enabled=1 WHERE id='openai/gpt-4.1-mini'",
                [],
            )
            .unwrap();
        db.conn
            .execute("INSERT INTO auth_admins(user_id) VALUES('dev')", [])
            .unwrap();
        db
    }
    fn model() -> String {
        "openai/gpt-4.1-mini".into()
    }
    fn enable(db: &mut Store, user: &str) {
        let p = db.ai_policy(user).unwrap();
        db.ai_update_policy(
            user,
            PolicyUpdate {
                user_ai_enabled: true,
                funding: p.funding,
                model_id: p.model_id,
                revision: p.revision,
            },
        )
        .unwrap();
    }
    #[test]
    fn administrator_account_search_is_bounded_literal_and_exact_selection_is_private() {
        let db = fixture();
        for i in 0..35 {
            let id = format!("search-{i:02}");
            db.conn
                .execute(
                    "INSERT INTO users(id,name) VALUES(?1,?2)",
                    params![id, format!("Search person {i:02}")],
                )
                .unwrap();
        }
        db.conn.execute("INSERT INTO auth_identities(issuer,subject,user_id,email) VALUES('synthetic','search-34','search-34','search-person@example.invalid')",[]).unwrap();
        assert_eq!(
            db.ai_admin_search_users("dev", "Search", None, 1000)
                .unwrap()
                .len(),
            20
        );
        assert!(
            db.ai_admin_search_users("dev", "", None, 20)
                .unwrap()
                .is_empty()
        );
        assert!(
            db.ai_admin_search_users("dev", "%", None, 20)
                .unwrap()
                .is_empty()
        );
        assert!(
            db.ai_admin_search_users("other", "Search", None, 20)
                .is_err()
        );
        let email = db
            .ai_admin_search_users("dev", "search-person@", None, 20)
            .unwrap();
        assert_eq!(email.len(), 1);
        assert_eq!(email[0].user_id, "search-34");
        let exact = db
            .ai_admin_search_users("dev", "", Some("search-09"), 20)
            .unwrap();
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].user_id, "search-09");
        assert!(
            db.ai_admin_search_users("dev", &"a".repeat(201), None, 20)
                .is_err()
        );
        let plan = db
            .conn
            .prepare(&format!("EXPLAIN QUERY PLAN {ADMIN_ACCOUNT_PREFIX_SQL}"))
            .unwrap()
            .query_map(params!["search%", 20], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for index in [
            "users_admin_name_prefix",
            "users_admin_id_prefix",
            "identities_admin_email_prefix",
        ] {
            assert!(
                plan.iter()
                    .any(|step| step.contains("SEARCH") && step.contains(index)),
                "Search must use {index}: {plan:?}"
            );
        }
    }
    #[test]
    fn default_off_request_workflow_and_live_admin_roles() {
        let mut db = fixture();
        let p = db.ai_policy("other").unwrap();
        assert!(!p.user_ai_enabled);
        assert!(!p.managed_ai_allowed);
        assert!(p.byok_allowed);
        assert_eq!(p.funding, "none");
        assert!(
            db.ai_resolve_with_material("other", Some(MATERIAL))
                .is_err()
        );
        let p = db.ai_request_access("other").unwrap();
        assert_eq!(p.access_request.as_deref(), Some("pending"));
        let again = db.ai_request_access("other").unwrap();
        assert_eq!(again.revision, p.revision);
        assert!(db.ai_admin_users("other", None, 50).is_err());
        let update = || AdminUpdate {
            daily_limit_micro: None,
            managed_ai_allowed: true,
            byok_allowed: true,
            decision: Some("approved".into()),
            reason: "Synthetic approval".into(),
            revision: p.revision,
        };
        assert!(db.ai_admin_update("other", "other", update()).is_err());
        let approved = db.ai_admin_update("dev", "other", update()).unwrap();
        assert!(approved.managed_ai_allowed);
        assert!(!approved.user_ai_enabled);
        assert_eq!(approved.access_request.as_deref(), Some("approved"));
        let notices = db.ai_account_notices("other", None, 10).unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].state, "approved");
        assert!(notices[0].unread);
        assert!(db.ai_account_notices("dev", None, 10).unwrap().is_empty());
        db.ai_read_notices("other", notices[0].id).unwrap();
        assert!(!db.ai_account_notices("other", None, 10).unwrap()[0].unread);
        assert_eq!(
            db.conn
                .query_row("SELECT count(*) FROM ai_admin_audit", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        db.conn.execute("DELETE FROM auth_admins", []).unwrap();
        assert!(db.ai_admin_users("dev", None, 50).is_err());
    }
    #[test]
    fn encrypted_key_is_owner_bound_and_setup_does_not_override_kill_switch() {
        let mut db = fixture();
        let p = db.ai_policy("other").unwrap();
        let p = db
            .ai_save_verified_key("other", KEY, &model(), p.revision, MATERIAL)
            .unwrap();
        assert_eq!(p.funding, "byok");
        assert!(!p.user_ai_enabled);
        assert!(p.key_configured);
        let cipher: String = db
            .conn
            .query_row(
                "SELECT ciphertext FROM ai_provider_keys WHERE user_id='other'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!cipher.contains(KEY));
        assert!(!serde_json::to_string(&p).unwrap().contains(KEY));
        assert!(decrypt(MATERIAL, "dev", &cipher).is_err());
        assert!(
            db.ai_resolve_with_material("other", Some(MATERIAL))
                .is_err()
        );
        enable(&mut db, "other");
        let resolved = db
            .ai_resolve_with_material("other", Some(MATERIAL))
            .unwrap();
        assert_eq!(resolved.api_key.as_deref(), Some(KEY));
        assert_eq!(resolved.funding, "byok");
        assert!(
            db.ai_resolve_with_material(
                "other",
                Some("a-different-encryption-key-at-least-32-bytes")
            )
            .is_err()
        );
        db.ai_remove_key("other").unwrap();
        let p = db.ai_policy("other").unwrap();
        assert_eq!(p.funding, "none");
        assert!(!p.key_configured);
        assert!(
            db.ai_resolve_with_material("other", Some(MATERIAL))
                .is_err()
        );
    }
    #[test]
    fn byok_disable_and_model_disable_never_fall_back_to_credits() {
        let mut db = fixture();
        let p = db.ai_policy("other").unwrap();
        db.ai_save_verified_key("other", KEY, &model(), p.revision, MATERIAL)
            .unwrap();
        enable(&mut db, "other");
        db.conn
            .execute("UPDATE ai_models SET enabled=0 WHERE id=?1", [model()])
            .unwrap();
        assert!(
            db.ai_resolve_with_material("other", Some(MATERIAL))
                .is_err()
        );
        let disabled = db.ai_policy("other").unwrap();
        db.ai_update_policy(
            "other",
            PolicyUpdate {
                user_ai_enabled: false,
                funding: disabled.funding,
                model_id: disabled.model_id,
                revision: disabled.revision,
            },
        )
        .unwrap();
        db.conn
            .execute("UPDATE ai_models SET enabled=1 WHERE id=?1", [model()])
            .unwrap();
        enable(&mut db, "other");
        let revision = db.ai_policy("other").unwrap().revision;
        let p = db
            .ai_admin_update(
                "dev",
                "other",
                AdminUpdate {
                    daily_limit_micro: None,
                    managed_ai_allowed: true,
                    byok_allowed: false,
                    decision: Some("approved".into()),
                    reason: "Synthetic access test".into(),
                    revision,
                },
            )
            .unwrap();
        assert_eq!(p.funding, "none");
        assert!(
            db.ai_resolve_with_material("other", Some(MATERIAL))
                .is_err()
        );
        assert!(p.revision > 1);
        assert!(p.key_configured);
    }
    #[test]
    fn settings_revision_throttle_rotation_and_credential_admin_rejection() {
        let mut db = fixture();
        let p = db.ai_policy("other").unwrap();
        for _ in 0..5 {
            db.ai_key_check("other", &model(), p.revision).unwrap();
        }
        assert!(db.ai_key_check("other", &model(), p.revision).is_err());
        assert!(
            db.ai_save_verified_key("other", KEY, "unsupported/model", p.revision, MATERIAL)
                .is_err()
        );
        db.ai_save_verified_key("other", KEY, &model(), p.revision, MATERIAL)
            .unwrap();
        assert!(
            db.ai_save_verified_key("other", KEY, &model(), p.revision, MATERIAL)
                .is_err()
        );
        let rotated = "synthetic-rotated-key-material-with-32-bytes";
        assert_eq!(db.ai_rotate_keys("dev", rotated, MATERIAL).unwrap(), 1);
        enable(&mut db, "other");
        assert_eq!(
            db.ai_resolve_with_material("other", Some(rotated))
                .unwrap()
                .api_key
                .as_deref(),
            Some(KEY)
        );
        assert!(require_admin(&db.conn, "credential:fake-admin").is_err());
        assert!(db.ai_remove_key("credential:fake-admin").is_err());
    }
    #[test]
    fn late_provider_key_error_does_not_revoke_a_replacement_key() {
        let mut db = fixture();
        let p = db.ai_policy("other").unwrap();
        db.ai_save_verified_key("other", KEY, &model(), p.revision, MATERIAL)
            .unwrap();
        enable(&mut db, "other");
        let old = db
            .ai_resolve_with_material("other", Some(MATERIAL))
            .unwrap();
        let replacement = "sk-or-v1-synthetic-replacement-never-used";
        db.ai_save_verified_key(
            "other",
            replacement,
            &model(),
            old.policy_revision,
            MATERIAL,
        )
        .unwrap();
        db.ai_invalidate_key_revision("other", old.policy_revision)
            .unwrap();
        let fresh = db
            .ai_resolve_with_material("other", Some(MATERIAL))
            .unwrap();
        assert_eq!(fresh.api_key.as_deref(), Some(replacement));
        db.ai_invalidate_key_revision("other", fresh.policy_revision)
            .unwrap();
        assert_eq!(db.ai_policy("other").unwrap().key_status, "invalid");
        assert!(
            db.ai_resolve_with_material("other", Some(MATERIAL))
                .is_err()
        );
        let p = db.ai_policy("other").unwrap();
        db.ai_update_policy(
            "other",
            PolicyUpdate {
                user_ai_enabled: false,
                funding: p.funding,
                model_id: p.model_id,
                revision: p.revision,
            },
        )
        .unwrap();
    }
}
