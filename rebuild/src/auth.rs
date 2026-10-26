//! Credential storage. OIDC signature, issuer, audience and nonce verification must
//! happen before passing trusted claims to `accept_identity`.
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("{0}")]
    Invalid(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub actor: String,
    pub credential_id: Option<String>,
    pub permissions: Vec<Permission>,
    pub list_ids: Option<Vec<String>>,
    pub workspace: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct SessionGrant {
    pub token: String,
    pub csrf: String,
    pub expires_at: i64,
}
#[derive(Debug, Serialize)]
pub struct AgentGrant {
    pub id: String,
    pub token: String,
    pub expires_at: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Read,
    Write,
    Admin,
}
pub struct AuthStore {
    conn: Connection,
}
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs() as i64
}
fn expiry(ttl: i64) -> Result<i64, AuthError> {
    if !(1..=31_536_000).contains(&ttl) {
        return Err(AuthError::Invalid(
            "credential lifetime must be 1 second to 1 year".into(),
        ));
    }
    Ok(now() + ttl)
}
fn email(value: &str) -> Result<String, AuthError> {
    let value = value.trim().to_lowercase();
    if value.len() > 320 || !value.contains('@') || value.chars().any(char::is_whitespace) {
        return Err(AuthError::Invalid("invalid email".into()));
    }
    Ok(value)
}
impl AuthStore {
    pub fn open(path: &str) -> Result<Self, AuthError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.execute_batch(include_str!("../migrations/004_auth.sql"))?;
        Ok(Self { conn })
    }
    pub fn memory() -> Result<Self, AuthError> {
        Self::open(":memory:")
    }
    /// Trusted administrative bootstrap only; do not expose as public registration.
    pub fn allow_email(&self, value: &str) -> Result<(), AuthError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO auth_email_allowlist(email) VALUES(?1)",
            [email(value)?],
        )?;
        Ok(())
    }
    pub fn accept_identity(
        &mut self,
        issuer: &str,
        subject: &str,
        address: &str,
        email_verified: bool,
    ) -> Result<Identity, AuthError> {
        if issuer.is_empty() || subject.is_empty() || issuer.len() > 2048 || subject.len() > 2048 {
            return Err(AuthError::Invalid("issuer and subject are required".into()));
        }
        if !email_verified {
            return Err(AuthError::Forbidden);
        }
        let address = email(address)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(identity)=tx.query_row("SELECT u.id,u.name FROM auth_identities i JOIN users u ON u.id=i.user_id WHERE i.issuer=?1 AND i.subject=?2",params![issuer,subject],|r|Ok(Identity{id:r.get(0)?,name:r.get(1)?})).optional()?{return Ok(identity)}
        let allowed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM auth_email_allowlist WHERE email=?1)",
            [&address],
            |r| r.get(0),
        )?;
        if !allowed {
            return Err(AuthError::Forbidden);
        }
        let identity = Identity {
            id: Uuid::new_v4().to_string(),
            name: "Todoer user".into(),
        };
        tx.execute(
            "INSERT INTO users(id,name) VALUES(?1,?2)",
            params![identity.id, identity.name],
        )?;
        tx.execute(
            "INSERT INTO auth_identities(issuer,subject,user_id,email) VALUES(?1,?2,?3,?4)",
            params![issuer, subject, identity.id, address],
        )?;
        tx.commit()?;
        Ok(identity)
    }
    pub fn issue_session(&self, actor: &str, ttl_secs: i64) -> Result<SessionGrant, AuthError> {
        let grant = SessionGrant {
            token: secret(),
            csrf: secret(),
            expires_at: expiry(ttl_secs)?,
        };
        self.conn.execute("INSERT INTO auth_sessions(id,user_id,token_hash,csrf_hash,expires_at) VALUES(?1,?2,?3,?4,?5)",params![Uuid::new_v4().to_string(),actor,hash(&grant.token),hash(&grant.csrf),grant.expires_at])?;
        Ok(grant)
    }
    pub fn authenticate_session(
        &self,
        token: &str,
        csrf: Option<&str>,
        require_csrf: bool,
    ) -> Result<Principal, AuthError> {
        let row=self.conn.query_row("SELECT user_id,id,csrf_hash FROM auth_sessions WHERE token_hash=?1 AND revoked=0 AND expires_at>?2",params![hash(token),now()],|r|Ok((Principal{actor:r.get(0)?,credential_id:Some(r.get(1)?),permissions:vec![Permission::Admin],list_ids:None,workspace:None},r.get::<_,String>(2)?))).optional()?.ok_or(AuthError::Unauthorized)?;
        if require_csrf && csrf.map(hash).as_deref() != Some(row.1.as_str()) {
            return Err(AuthError::Forbidden);
        }
        Ok(row.0)
    }
    pub fn revoke_session(&self, token: &str) -> Result<(), AuthError> {
        self.conn.execute(
            "UPDATE auth_sessions SET revoked=1 WHERE token_hash=?1",
            [hash(token)],
        )?;
        Ok(())
    }
    pub fn issue_agent(
        &self,
        actor: &str,
        audience: &str,
        permissions: Vec<Permission>,
        list_ids: Option<Vec<String>>,
        workspace: Option<String>,
        ttl_secs: i64,
    ) -> Result<AgentGrant, AuthError> {
        if audience.trim().is_empty() || permissions.is_empty() || audience.len() > 500 {
            return Err(AuthError::Invalid(
                "audience and permissions are required".into(),
            ));
        }
        if let Some(ref lists) = list_ids
            && lists.is_empty()
        {
            return Err(AuthError::Invalid("list scope must not be empty".into()));
        }
        let grant = AgentGrant {
            id: Uuid::new_v4().to_string(),
            token: secret(),
            expires_at: expiry(ttl_secs)?,
        };
        self.conn.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,list_ids,workspace,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![grant.id,actor,hash(&grant.token),audience,serde_json::to_string(&permissions).unwrap(),list_ids.map(|v|serde_json::to_string(&v).unwrap()),workspace,grant.expires_at])?;
        Ok(grant)
    }
    pub fn authenticate_agent(
        &self,
        token: &str,
        audience: &str,
        permission: Permission,
        list: Option<&str>,
        workspace: Option<&str>,
    ) -> Result<Principal, AuthError> {
        let (mut principal,expected,permissions,lists,scope):(Principal,String,String,Option<String>,Option<String>)=self.conn.query_row("SELECT user_id,id,audience,permissions,list_ids,workspace FROM auth_agents WHERE token_hash=?1 AND revoked=0 AND expires_at>?2",params![hash(token),now()],|r|Ok((Principal{actor:r.get(0)?,credential_id:Some(r.get(1)?),permissions:vec![Permission::Admin],list_ids:None,workspace:None},r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?.ok_or(AuthError::Unauthorized)?;
        if expected != audience {
            return Err(AuthError::Unauthorized);
        }
        let permissions: Vec<Permission> =
            serde_json::from_str(&permissions).map_err(|_| AuthError::Unauthorized)?;
        if !permissions.iter().any(|p| {
            *p == Permission::Admin
                || *p == permission
                || (*p == Permission::Write && permission == Permission::Read)
        }) {
            return Err(AuthError::Forbidden);
        }
        if let Some(ref scope) = scope
            && workspace.is_some_and(|w| w != scope)
        {
            return Err(AuthError::Forbidden);
        }
        let lists: Option<Vec<String>> = lists
            .map(|v| serde_json::from_str(&v).map_err(|_| AuthError::Unauthorized))
            .transpose()?;
        if let Some(ref lists) = lists
            && list.is_some_and(|l| !lists.iter().any(|s| s == l))
        {
            return Err(AuthError::Forbidden);
        }
        // Authentication is not authorization. The shared core must apply these
        // restrictions and current membership on every operation, even discovery.
        principal.permissions = permissions;
        principal.list_ids = lists;
        principal.workspace = scope;
        Ok(principal)
    }
    pub fn revoke_agent(&self, actor: &str, id: &str) -> Result<(), AuthError> {
        let changed = self.conn.execute(
            "UPDATE auth_agents SET revoked=1 WHERE id=?1 AND user_id=?2",
            params![id, actor],
        )?;
        if changed == 0 {
            return Err(AuthError::Forbidden);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> AuthStore {
        let s = AuthStore::memory().unwrap();
        s.conn.execute_batch("CREATE TABLE users(id TEXT PRIMARY KEY,name TEXT NOT NULL);INSERT INTO users VALUES('alice','Alice');").unwrap();
        s
    }
    #[test]
    fn identities_are_stable_and_registration_is_closed() {
        let mut s = store();
        assert!(matches!(
            s.accept_identity("issuer", "subject", "a@example.com", true),
            Err(AuthError::Forbidden)
        ));
        s.allow_email("A@example.com").unwrap();
        assert!(
            s.accept_identity("issuer", "subject", "a@example.com", false)
                .is_err()
        );
        let first = s
            .accept_identity("issuer", "subject", "a@example.com", true)
            .unwrap();
        let again = s
            .accept_identity("issuer", "subject", "changed@example.com", true)
            .unwrap();
        assert_eq!(first.id, again.id);
        let other = s
            .accept_identity("issuer", "different-subject", "a@example.com", true)
            .unwrap();
        assert_ne!(first.id, other.id);
        assert_eq!(first.name, "Todoer user");
    }
    #[test]
    fn sessions_are_hashed_expiring_revocable_and_csrf_protected() {
        let s = store();
        let grant = s.issue_session("alice", 60).unwrap();
        assert_eq!(
            s.authenticate_session(&grant.token, None, false)
                .unwrap()
                .actor,
            "alice"
        );
        assert!(matches!(
            s.authenticate_session(&grant.token, None, true),
            Err(AuthError::Forbidden)
        ));
        assert!(
            s.authenticate_session(&grant.token, Some(&grant.csrf), true)
                .is_ok()
        );
        let (token, csrf): (String, String) = s
            .conn
            .query_row("SELECT token_hash,csrf_hash FROM auth_sessions", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_ne!(token, grant.token);
        assert_ne!(csrf, grant.csrf);
        s.revoke_session(&grant.token).unwrap();
        assert!(matches!(
            s.authenticate_session(&grant.token, None, false),
            Err(AuthError::Unauthorized)
        ));
        let grant = s.issue_session("alice", 60).unwrap();
        s.conn
            .execute("UPDATE auth_sessions SET expires_at=0", [])
            .unwrap();
        assert!(s.authenticate_session(&grant.token, None, false).is_err());
    }
    #[test]
    fn agent_scopes_and_permissions_survive_authentication() {
        let s = store();
        let grant = s
            .issue_agent(
                "alice",
                "todoer",
                vec![Permission::Read],
                Some(vec!["inbox".into()]),
                Some("home".into()),
                60,
            )
            .unwrap();
        assert!(matches!(
            s.authenticate_agent(&grant.token, "wrong", Permission::Read, None, None),
            Err(AuthError::Unauthorized)
        ));
        assert!(matches!(
            s.authenticate_agent(&grant.token, "todoer", Permission::Write, None, None),
            Err(AuthError::Forbidden)
        ));
        assert!(
            s.authenticate_agent(
                &grant.token,
                "todoer",
                Permission::Read,
                Some("private"),
                None
            )
            .is_err()
        );
        assert!(
            s.authenticate_agent(
                &grant.token,
                "todoer",
                Permission::Read,
                None,
                Some("other")
            )
            .is_err()
        );
        let p = s
            .authenticate_agent(&grant.token, "todoer", Permission::Read, None, None)
            .unwrap();
        assert_eq!(p.list_ids, Some(vec!["inbox".into()]));
        assert_eq!(p.workspace.as_deref(), Some("home"));
        assert_eq!(p.permissions, vec![Permission::Read]);
        assert!(s.revoke_agent("someone-else", &grant.id).is_err());
        s.revoke_agent("alice", &grant.id).unwrap();
        assert!(
            s.authenticate_agent(&grant.token, "todoer", Permission::Read, None, None)
                .is_err()
        );
    }
}
