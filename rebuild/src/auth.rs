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
    #[error("Sign in again to change account security")]
    ReauthenticationRequired,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    pub workspace_ids: Option<Vec<String>>,
    pub id: String,
    pub audience: String,
    pub permissions: Vec<Permission>,
    pub list_ids: Option<Vec<String>>,
    pub workspace: Option<String>,
    pub expires_at: i64,
    pub revoked: bool,
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
    pub(crate) conn: Connection,
}
pub(crate) fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub(crate) fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
pub(crate) fn now() -> i64 {
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
pub(crate) fn email(value: &str) -> Result<String, AuthError> {
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
        conn.execute_batch(include_str!("../migrations/007_auth_oauth.sql"))?;
        conn.execute_batch(include_str!("../migrations/035_account_security.sql"))?;
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
    /// Replace deployment registration policy atomically; removed addresses must
    /// not remain implicitly allowed after a configuration update.
    pub fn replace_allowlist(&mut self, values: &[String]) -> Result<(), AuthError> {
        let normalized = values
            .iter()
            .map(|v| email(v))
            .collect::<Result<Vec<_>, _>>()?;
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM auth_email_allowlist", [])?;
        for address in normalized {
            tx.execute(
                "INSERT OR IGNORE INTO auth_email_allowlist(email) VALUES(?1)",
                [address],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn accept_identity(
        &mut self,
        issuer: &str,
        subject: &str,
        address: &str,
        email_verified: bool,
    ) -> Result<Identity, AuthError> {
        self.accept_identity_policy(issuer, subject, address, email_verified, false)
    }
    /// Only the verified Google callback may use this after validating a live,
    /// email-bound invitation through the shared application operations.
    pub(crate) fn accept_invited_identity(
        &mut self,
        issuer: &str,
        subject: &str,
        address: &str,
    ) -> Result<Identity, AuthError> {
        self.accept_identity_policy(issuer, subject, address, true, true)
    }
    fn accept_identity_policy(
        &mut self,
        issuer: &str,
        subject: &str,
        address: &str,
        email_verified: bool,
        invited: bool,
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
        if let Some(identity)=tx.query_row("SELECT u.id,u.name FROM auth_identities i JOIN users u ON u.id=i.user_id WHERE i.issuer=?1 AND i.subject=?2",params![issuer,subject],|r|Ok(Identity{id:r.get(0)?,name:r.get(1)?})).optional()?{
            tx.execute("UPDATE auth_identities SET email=?1 WHERE issuer=?2 AND subject=?3",params![address,issuer,subject])?;
            crate::account_security::bootstrap_admin(&tx, &identity.id, &address)?;
            tx.commit()?;
            return Ok(identity)
        }
        let allowed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM auth_email_allowlist WHERE email=?1)",
            [&address],
            |r| r.get(0),
        )?;
        if !crate::account_security::policy(&tx)?.signup_enabled { return Err(AuthError::Forbidden); }
        if !allowed && !invited {
            return Err(AuthError::Forbidden);
        }
        let identity = Identity {
            id: crate::defaults::short_id("u"),
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
        crate::account_security::bootstrap_admin(&tx, &identity.id, &address)?;
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
    pub fn list_agents(&self, actor: &str) -> Result<Vec<AgentMetadata>, AuthError> {
        let has_spaces: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('auth_agents') WHERE name='workspace_ids')",[],|r|r.get(0))?;
        let mut q=self.conn.prepare(&format!("SELECT id,audience,permissions,list_ids,workspace,expires_at,revoked,{} FROM auth_agents WHERE user_id=?1 ORDER BY rowid DESC LIMIT 200",if has_spaces { "workspace_ids" } else { "NULL" }))?;
        let rows = q.query_map([actor], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, bool>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })?;
        rows.map(|row| {
            let (id, audience, permissions, lists, workspace, expires_at, revoked, workspace_ids) =
                row?;
            Ok(AgentMetadata {
                workspace_ids: workspace_ids.map(|v| parse_json(&v)).transpose()?,
                id,
                audience,
                permissions: parse_json(&permissions)?,
                list_ids: lists.map(|v| parse_json(&v)).transpose()?,
                workspace,
                expires_at,
                revoked,
            })
        })
        .collect()
    }
    pub fn revoke_agent(&self, actor: &str, id: &str) -> Result<(), AuthError> {
        let tx = self.conn.unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE auth_agents SET revoked=1 WHERE id=?1 AND user_id=?2",
            params![id, actor],
        )?;
        if changed == 0 {
            return Err(AuthError::Forbidden);
        }
        let family: Option<String> = tx
            .query_row(
                "SELECT family_id FROM auth_oauth_refresh WHERE access_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(family) = family {
            revoke_family(&tx, &family)?
        }
        tx.commit()?;
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
    fn deployment_allowlist_replacement_removes_old_registration_access() {
        let mut db = store();
        db.replace_allowlist(&["old@example.invalid".into()])
            .unwrap();
        db.replace_allowlist(&["new@example.invalid".into()])
            .unwrap();
        assert!(
            db.accept_identity(
                "https://accounts.google.com",
                "old-subject",
                "old@example.invalid",
                true
            )
            .is_err()
        );
        assert!(
            db.accept_identity(
                "https://accounts.google.com",
                "new-subject",
                "new@example.invalid",
                true
            )
            .is_ok()
        );
        assert!(db.replace_allowlist(&["invalid".into()]).is_err());
        assert!(
            db.accept_identity(
                "https://accounts.google.com",
                "another-new-subject",
                "new@example.invalid",
                true
            )
            .is_ok()
        );
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationRequest {
    pub client_id: String,
    pub redirect_uri: String,
    pub resource: String,
    pub scope: String,
    pub state: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAuthorization {
    pub id: String,
    pub client_name: String,
    pub request: AuthorizationRequest,
    pub expires_at: i64,
}
#[derive(Debug, Serialize)]
pub struct AuthorizationCode {
    pub code: String,
    pub redirect_uri: String,
    pub state: String,
}
#[derive(Debug, Serialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub scope: String,
}
fn parse_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, AuthError> {
    serde_json::from_str(value).map_err(|_| AuthError::Invalid("invalid stored OAuth data".into()))
}
fn oauth_permissions(scope: &str) -> Result<Vec<Permission>, AuthError> {
    let mut out = Vec::new();
    for word in scope.split_whitespace() {
        let permission = match word {
            "tasks:read" => Permission::Read,
            "tasks:write" => Permission::Write,
            _ => return Err(AuthError::Invalid("unsupported OAuth scope".into())),
        };
        if !out.contains(&permission) {
            out.push(permission)
        }
    }
    if out.is_empty() {
        return Err(AuthError::Invalid("OAuth scope is required".into()));
    }
    Ok(out)
}
fn oauth_scope(permissions: &[Permission]) -> String {
    permissions
        .iter()
        .map(|p| match p {
            Permission::Read => "tasks:read",
            Permission::Write => "tasks:write",
            Permission::Admin => "unsupported",
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn redirect_valid(uri: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(uri) else {
        return false;
    };
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return false;
    }
    url.host_str().is_some()
        && (url.scheme() == "https"
            || (url.scheme() == "http"
                && url
                    .host_str()
                    .is_some_and(|h| matches!(h, "127.0.0.1" | "[::1]" | "::1"))))
}
fn canonical_resource(resource: &str, canonical: &str) -> Result<(), AuthError> {
    if resource != canonical || !redirect_valid(canonical) {
        return Err(AuthError::Invalid(
            "resource must exactly match the configured MCP URL".into(),
        ));
    }
    let url = reqwest::Url::parse(canonical)
        .map_err(|_| AuthError::Invalid("invalid resource".into()))?;
    if url.query().is_some() {
        return Err(AuthError::Invalid("resource cannot include a query".into()));
    }
    Ok(())
}
fn revoke_family(c: &Connection, family: &str) -> Result<(), AuthError> {
    c.execute(
        "UPDATE auth_oauth_families SET revoked=1 WHERE id=?1",
        [family],
    )?;
    c.execute("UPDATE auth_agents SET revoked=1 WHERE id IN(SELECT access_id FROM auth_oauth_refresh WHERE family_id=?1)",[family])?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn mint_oauth(
    c: &Connection,
    actor: &str,
    resource: &str,
    permissions: &str,
    lists: Option<&str>,
    workspace: Option<&str>,
    family: &str,
) -> Result<OAuthTokens, AuthError> {
    let access = secret();
    let refresh = secret();
    let id = Uuid::new_v4().to_string();
    c.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,list_ids,workspace,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,actor,hash(&access),resource,permissions,lists,workspace,now()+900])?;
    c.execute("INSERT INTO auth_oauth_refresh(token_hash,family_id,access_id,expires_at) VALUES(?1,?2,?3,?4)",params![hash(&refresh),family,id,now()+2_592_000])?;
    let permissions: Vec<Permission> = parse_json(permissions)?;
    Ok(OAuthTokens {
        access_token: access,
        token_type: "Bearer".into(),
        expires_in: 900,
        refresh_token: refresh,
        scope: oauth_scope(&permissions),
    })
}
impl AuthStore {
    pub fn register_client(
        &self,
        name: &str,
        redirect_uris: Vec<String>,
    ) -> Result<OAuthClient, AuthError> {
        if name.trim().is_empty()
            || name.len() > 200
            || redirect_uris.is_empty()
            || redirect_uris.len() > 10
            || redirect_uris
                .iter()
                .any(|uri| uri.len() > 2048 || !redirect_valid(uri))
        {
            return Err(AuthError::Invalid("public clients require a name and exact HTTPS or IP-loopback redirect URIs without fragments".into()));
        }
        let client = OAuthClient {
            client_id: Uuid::new_v4().to_string(),
            client_name: name.trim().into(),
            redirect_uris,
        };
        let tx = self.conn.unchecked_transaction()?;
        let count: i64 =
            tx.query_row("SELECT COUNT(*) FROM auth_oauth_clients", [], |r| r.get(0))?;
        if count >= 1000 {
            return Err(AuthError::Invalid(
                "client registration capacity reached".into(),
            ));
        }
        tx.execute(
            "INSERT INTO auth_oauth_clients VALUES(?1,?2,?3)",
            params![
                client.client_id,
                client.client_name,
                serde_json::to_string(&client.redirect_uris).unwrap()
            ],
        )?;
        tx.commit()?;
        Ok(client)
    }
    pub fn begin_authorization(
        &self,
        request: AuthorizationRequest,
        canonical: &str,
    ) -> Result<PendingAuthorization, AuthError> {
        canonical_resource(&request.resource, canonical)?;
        oauth_permissions(&request.scope)?;
        if request.code_challenge_method != "S256"
            || request.code_challenge.len() != 43
            || !request
                .code_challenge
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || request.state.is_empty()
            || request.state.len() > 2048
        {
            return Err(AuthError::Invalid(
                "S256 PKCE and a bounded nonempty state are required".into(),
            ));
        }
        let (name, redirects): (String, String) = self
            .conn
            .query_row(
                "SELECT name,redirect_uris FROM auth_oauth_clients WHERE id=?1",
                [&request.client_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or(AuthError::Unauthorized)?;
        let redirects: Vec<String> = parse_json(&redirects)?;
        if !redirects.contains(&request.redirect_uri) {
            return Err(AuthError::Invalid(
                "redirect URI does not match client registration".into(),
            ));
        }
        let pending = PendingAuthorization {
            id: secret(),
            client_name: name,
            request,
            expires_at: now() + 300,
        };
        self.conn.execute(
            "INSERT INTO auth_oauth_requests(id,request,expires_at) VALUES(?1,?2,?3)",
            params![
                pending.id,
                serde_json::to_string(&pending.request).unwrap(),
                pending.expires_at
            ],
        )?;
        Ok(pending)
    }
    pub fn authorization_request(&self, id: &str) -> Result<PendingAuthorization, AuthError> {
        let (encoded,expires):(String,i64)=self.conn.query_row("SELECT request,expires_at FROM auth_oauth_requests WHERE id=?1 AND expires_at>?2 AND approved=0",params![id,now()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or(AuthError::Unauthorized)?;
        let request: AuthorizationRequest = parse_json(&encoded)?;
        let name = self.conn.query_row(
            "SELECT name FROM auth_oauth_clients WHERE id=?1",
            [&request.client_id],
            |r| r.get(0),
        )?;
        Ok(PendingAuthorization {
            id: id.into(),
            client_name: name,
            request,
            expires_at: expires,
        })
    }
    pub fn deny_authorization(&self, id: &str) -> Result<(), AuthError> {
        let changed=self.conn.execute("UPDATE auth_oauth_requests SET approved=-1,consumed=1 WHERE id=?1 AND approved=0 AND consumed=0 AND expires_at>?2",params![id,now()])?;
        if changed != 1 {
            return Err(AuthError::Unauthorized);
        }
        Ok(())
    }
    /// Call only after explicit, CSRF-protected authenticated user approval.
    pub fn approve_authorization(
        &mut self,
        id: &str,
        actor: &str,
        permissions: Vec<Permission>,
        list_ids: Option<Vec<String>>,
        workspace: Option<String>,
    ) -> Result<AuthorizationCode, AuthError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let encoded:String=tx.query_row("SELECT request FROM auth_oauth_requests WHERE id=?1 AND expires_at>?2 AND approved=0",params![id,now()],|r|r.get(0)).optional()?.ok_or(AuthError::Unauthorized)?;
        let request: AuthorizationRequest = parse_json(&encoded)?;
        let requested = oauth_permissions(&request.scope)?;
        if permissions.is_empty()
            || permissions.iter().any(|p| !requested.contains(p))
            || list_ids.as_ref().is_some_and(|v| v.is_empty())
        {
            return Err(AuthError::Invalid(
                "approval must select requested permissions and nonempty resources".into(),
            ));
        }
        let code = secret();
        tx.execute("UPDATE auth_oauth_requests SET actor=?1,permissions=?2,list_ids=?3,workspace=?4,code_hash=?5,approved=1,expires_at=?6 WHERE id=?7",params![actor,serde_json::to_string(&permissions).unwrap(),list_ids.map(|v|serde_json::to_string(&v).unwrap()),workspace,hash(&code),now()+120,id])?;
        tx.commit()?;
        Ok(AuthorizationCode {
            code,
            redirect_uri: request.redirect_uri,
            state: request.state,
        })
    }
    pub fn exchange_code(
        &mut self,
        client_id: &str,
        redirect_uri: &str,
        code: &str,
        verifier: &str,
        resource: &str,
        canonical: &str,
    ) -> Result<OAuthTokens, AuthError> {
        canonical_resource(resource, canonical)?;
        if !(43..=128).contains(&verifier.len())
            || !verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
        {
            return Err(AuthError::Unauthorized);
        }
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (id,encoded,actor,permissions,lists,workspace,consumed):(String,String,String,String,Option<String>,Option<String>,bool)=tx.query_row("SELECT id,request,actor,permissions,list_ids,workspace,consumed FROM auth_oauth_requests WHERE code_hash=?1 AND approved=1 AND expires_at>?2",params![hash(code),now()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?.ok_or(AuthError::Unauthorized)?;
        let request: AuthorizationRequest = parse_json(&encoded)?;
        let challenge = oauth2::PkceCodeChallenge::from_code_verifier_sha256(
            &oauth2::PkceCodeVerifier::new(verifier.into()),
        );
        if request.client_id != client_id
            || request.redirect_uri != redirect_uri
            || request.resource != resource
            || request.code_challenge != challenge.as_str()
        {
            return Err(AuthError::Unauthorized);
        }
        if consumed {
            let family: String = tx.query_row(
                "SELECT family_id FROM auth_oauth_code_families WHERE request_id=?1",
                [&id],
                |r| r.get(0),
            )?;
            revoke_family(&tx, &family)?;
            tx.commit()?;
            return Err(AuthError::Unauthorized);
        }
        tx.execute(
            "UPDATE auth_oauth_requests SET consumed=1 WHERE id=?1",
            [&id],
        )?;
        let family = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO auth_oauth_families(id,client_id) VALUES(?1,?2)",
            params![family, client_id],
        )?;
        tx.execute(
            "INSERT INTO auth_oauth_code_families(request_id,family_id) VALUES(?1,?2)",
            params![id, family],
        )?;
        let tokens = mint_oauth(
            &tx,
            &actor,
            resource,
            &permissions,
            lists.as_deref(),
            workspace.as_deref(),
            &family,
        )?;
        tx.commit()?;
        Ok(tokens)
    }
    pub fn refresh_oauth(
        &mut self,
        client_id: &str,
        token: &str,
        resource: &str,
        canonical: &str,
    ) -> Result<OAuthTokens, AuthError> {
        canonical_resource(resource, canonical)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        type RefreshRow = (
            String,
            String,
            bool,
            bool,
            i64,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        );
        let (family,client,consumed,revoked,expires,actor,audience,permissions,lists,workspace):RefreshRow=tx.query_row("SELECT r.family_id,f.client_id,r.consumed,f.revoked,r.expires_at,a.user_id,a.audience,a.permissions,a.list_ids,a.workspace FROM auth_oauth_refresh r JOIN auth_oauth_families f ON f.id=r.family_id JOIN auth_agents a ON a.id=r.access_id WHERE r.token_hash=?1",[hash(token)],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?))).optional()?.ok_or(AuthError::Unauthorized)?;
        if client != client_id || audience != resource {
            return Err(AuthError::Unauthorized);
        }
        if consumed {
            revoke_family(&tx, &family)?;
            tx.commit()?;
            return Err(AuthError::Unauthorized);
        }
        if revoked || expires <= now() {
            return Err(AuthError::Unauthorized);
        }
        tx.execute(
            "UPDATE auth_oauth_refresh SET consumed=1 WHERE token_hash=?1",
            [hash(token)],
        )?;
        let result = mint_oauth(
            &tx,
            &actor,
            resource,
            &permissions,
            lists.as_deref(),
            workspace.as_deref(),
            &family,
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn revoke_oauth(&mut self, client_id: &str, token: &str) -> Result<(), AuthError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let family:Option<String>=tx.query_row("SELECT f.id FROM auth_oauth_families f JOIN auth_oauth_refresh r ON r.family_id=f.id JOIN auth_agents a ON a.id=r.access_id WHERE f.client_id=?1 AND (r.token_hash=?2 OR a.token_hash=?2)",params![client_id,hash(token)],|r|r.get(0)).optional()?;
        if let Some(family) = family {
            revoke_family(&tx, &family)?
        }
        tx.commit()?;
        Ok(())
    }
}
#[cfg(test)]
mod oauth_tests {
    use super::*;
    const RESOURCE: &str = "https://todoer.example/mcp";
    const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    fn setup() -> (AuthStore, OAuthClient) {
        let s = AuthStore::memory().unwrap();
        s.conn.execute_batch("CREATE TABLE users(id TEXT PRIMARY KEY,name TEXT NOT NULL);INSERT INTO users VALUES('alice','Alice');").unwrap();
        let client = s
            .register_client("Test client", vec!["http://127.0.0.1:1234/callback".into()])
            .unwrap();
        (s, client)
    }
    fn request(c: &OAuthClient) -> AuthorizationRequest {
        AuthorizationRequest {
            client_id: c.client_id.clone(),
            redirect_uri: c.redirect_uris[0].clone(),
            resource: RESOURCE.into(),
            scope: "tasks:read tasks:write".into(),
            state: "client-state".into(),
            code_challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".into(),
            code_challenge_method: "S256".into(),
        }
    }
    fn code(s: &mut AuthStore, c: &OAuthClient) -> AuthorizationCode {
        let pending = s.begin_authorization(request(c), RESOURCE).unwrap();
        s.approve_authorization(
            &pending.id,
            "alice",
            vec![Permission::Read],
            Some(vec!["inbox".into()]),
            None,
        )
        .unwrap()
    }
    #[test]
    fn public_client_validation_and_explicit_approval() {
        let (mut s, c) = setup();
        for redirect in [
            "http://evil.example/cb",
            "https://ok.example/cb#bad",
            "https://user@ok.example/cb",
            "javascript:alert(1)",
        ] {
            assert!(s.register_client("bad", vec![redirect.into()]).is_err());
        }
        let mut r = request(&c);
        r.code_challenge_method = "plain".into();
        assert!(s.begin_authorization(r, RESOURCE).is_err());
        let mut r = request(&c);
        r.resource = "https://attacker.example/mcp".into();
        assert!(s.begin_authorization(r, RESOURCE).is_err());
        let pending = s.begin_authorization(request(&c), RESOURCE).unwrap();
        assert_eq!(
            s.authorization_request(&pending.id).unwrap().request.state,
            "client-state"
        );
        assert!(
            s.exchange_code(
                &c.client_id,
                &c.redirect_uris[0],
                &pending.id,
                VERIFIER,
                RESOURCE,
                RESOURCE
            )
            .is_err()
        );
        assert!(
            s.approve_authorization(&pending.id, "alice", vec![Permission::Admin], None, None)
                .is_err()
        );
    }
    #[test]
    fn pkce_codes_bind_client_redirect_resource_and_are_single_use() {
        let (mut s, c) = setup();
        let grant = code(&mut s, &c);
        assert_eq!(grant.state, "client-state");
        assert!(
            s.exchange_code(
                "wrong",
                &c.redirect_uris[0],
                &grant.code,
                VERIFIER,
                RESOURCE,
                RESOURCE
            )
            .is_err()
        );
        assert!(
            s.exchange_code(
                &c.client_id,
                "https://evil.example/cb",
                &grant.code,
                VERIFIER,
                RESOURCE,
                RESOURCE
            )
            .is_err()
        );
        assert!(
            s.exchange_code(
                &c.client_id,
                &c.redirect_uris[0],
                &grant.code,
                &"a".repeat(43),
                RESOURCE,
                RESOURCE
            )
            .is_err()
        );
        let tokens = s
            .exchange_code(
                &c.client_id,
                &c.redirect_uris[0],
                &grant.code,
                VERIFIER,
                RESOURCE,
                RESOURCE,
            )
            .unwrap();

        let principal = s
            .authenticate_agent(&tokens.access_token, RESOURCE, Permission::Read, None, None)
            .unwrap();
        assert_eq!(principal.list_ids, Some(vec!["inbox".into()]));
        assert!(
            s.authenticate_agent(
                &tokens.access_token,
                RESOURCE,
                Permission::Write,
                None,
                None
            )
            .is_err()
        );
        assert!(
            s.authenticate_agent(
                &tokens.access_token,
                "https://other.example/mcp",
                Permission::Read,
                None,
                None
            )
            .is_err()
        );
        assert!(
            s.exchange_code(
                &c.client_id,
                &c.redirect_uris[0],
                &grant.code,
                VERIFIER,
                RESOURCE,
                RESOURCE
            )
            .is_err()
        );
        assert!(
            s.authenticate_agent(&tokens.access_token, RESOURCE, Permission::Read, None, None)
                .is_err()
        );
    }
    #[test]
    fn refresh_rotation_detects_reuse_and_revokes_the_family() {
        let (mut s, c) = setup();
        let grant = code(&mut s, &c);
        let first = s
            .exchange_code(
                &c.client_id,
                &c.redirect_uris[0],
                &grant.code,
                VERIFIER,
                RESOURCE,
                RESOURCE,
            )
            .unwrap();
        let second = s
            .refresh_oauth(&c.client_id, &first.refresh_token, RESOURCE, RESOURCE)
            .unwrap();
        assert_ne!(first.refresh_token, second.refresh_token);
        assert!(
            s.refresh_oauth(&c.client_id, &first.refresh_token, RESOURCE, RESOURCE)
                .is_err()
        );
        assert!(
            s.authenticate_agent(&second.access_token, RESOURCE, Permission::Read, None, None)
                .is_err()
        );
        assert!(
            s.refresh_oauth(&c.client_id, &second.refresh_token, RESOURCE, RESOURCE)
                .is_err()
        );
        let grant = code(&mut s, &c);
        let tokens = s
            .exchange_code(
                &c.client_id,
                &c.redirect_uris[0],
                &grant.code,
                VERIFIER,
                RESOURCE,
                RESOURCE,
            )
            .unwrap();
        s.revoke_oauth(&c.client_id, &tokens.access_token).unwrap();
        assert!(
            s.refresh_oauth(&c.client_id, &tokens.refresh_token, RESOURCE, RESOURCE)
                .is_err()
        );
    }
    #[test]
    fn denial_and_approval_are_terminal() {
        let (mut s, c) = setup();
        let p = s.begin_authorization(request(&c), RESOURCE).unwrap();
        s.deny_authorization(&p.id).unwrap();
        assert!(s.authorization_request(&p.id).is_err());
        assert!(
            s.approve_authorization(&p.id, "alice", vec![Permission::Read], None, None)
                .is_err()
        );
        assert!(s.deny_authorization(&p.id).is_err());
        let p = s.begin_authorization(request(&c), RESOURCE).unwrap();
        s.approve_authorization(&p.id, "alice", vec![Permission::Read], None, None)
            .unwrap();
        assert!(
            s.approve_authorization(&p.id, "alice", vec![Permission::Read], None, None)
                .is_err()
        );
        assert!(s.deny_authorization(&p.id).is_err());
    }
}
