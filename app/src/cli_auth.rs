//! Browser-approved, PKCE-bound CLI credentials. Never transfer browser cookies.
use crate::core::{Error, Store};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
fn secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
pub fn rank(permission: &str) -> Result<i64, Error> {
    match permission {
        "read" => Ok(1),
        "write" => Ok(2),
        "admin" => Ok(3),
        _ => Err(invalid("Permission must be read, write or admin")),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginStart {
    pub redirect_uri: String,
    pub challenge: String,
    pub state: String,
    pub permission: String,
}
#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    pub id: String,
    pub redirect_uri: String,
    pub state: String,
    pub requested_rank: i64,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct LoginGrant {
    pub token: String,
    pub credential_id: String,
    pub expires_at: i64,
    pub permission: String,
    pub spaces: Vec<String>,
    pub lists: Vec<String>,
    pub all_resources: bool,
}
impl Store {
    pub fn begin_cli_login(&mut self, input: LoginStart) -> Result<LoginRequest, Error> {
        let redirect =
            reqwest::Url::parse(&input.redirect_uri).map_err(|_| invalid("Invalid callback"))?;
        if redirect.scheme() != "http"
            || !matches!(redirect.host_str(), Some("127.0.0.1" | "[::1]" | "::1"))
            || redirect.port().is_none()
            || redirect.path() != "/callback"
            || redirect.query().is_some()
            || redirect.fragment().is_some()
            || !redirect.username().is_empty()
            || redirect.password().is_some()
        {
            return Err(invalid(
                "Callback must be an explicit IP-loopback HTTP port at /callback",
            ));
        }
        if input.challenge.len() != 43
            || !input
                .challenge
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !(32..=128).contains(&input.state.len())
            || !input
                .state
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(invalid("S256 PKCE and a random bounded state are required"));
        }
        let requested_rank = rank(&input.permission)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM cli_login_requests WHERE expires_at<=unixepoch()",
            [],
        )?;
        let count: i64 =
            tx.query_row("SELECT count(*) FROM cli_login_requests", [], |r| r.get(0))?;
        if count >= 1000 {
            return Err(invalid("Login request capacity reached"));
        }
        let request = LoginRequest {
            id: secret(),
            redirect_uri: input.redirect_uri,
            state: input.state,
            requested_rank,
        };
        tx.execute("INSERT INTO cli_login_requests(id,redirect_uri,challenge,state,requested_rank,expires_at) VALUES(?1,?2,?3,?4,?5,unixepoch()+600)",params![request.id,request.redirect_uri,input.challenge,request.state,requested_rank])?;
        tx.commit()?;
        Ok(request)
    }
    pub fn cli_login_request(&self, id: &str) -> Result<LoginRequest, Error> {
        self.conn.query_row("SELECT id,redirect_uri,state,requested_rank FROM cli_login_requests WHERE id=?1 AND consumed=0 AND code_hash IS NULL AND expires_at>unixepoch()",[id],|r|Ok(LoginRequest{id:r.get(0)?,redirect_uri:r.get(1)?,state:r.get(2)?,requested_rank:r.get(3)?})).optional()?.ok_or(Error::NotFound)
    }
    pub fn approve_cli_login(
        &mut self,
        actor: &str,
        id: &str,
        permission: &str,
        spaces: Vec<String>,
        lists: Vec<String>,
        all: bool,
    ) -> Result<String, Error> {
        if actor.starts_with("credential:") {
            return Err(Error::Forbidden);
        }
        let request = self.cli_login_request(id)?;
        let required = rank(permission)?;
        if required > request.requested_rank
            || (all && (!spaces.is_empty() || !lists.is_empty()))
            || (!all && spaces.is_empty() && lists.is_empty())
            || spaces.len() + lists.len() > 500
        {
            return Err(invalid(
                "Choose permissions and either selected spaces/lists or all resources",
            ));
        }
        let mut resolved_spaces = Vec::new();
        let mut resolved_lists = Vec::new();
        for value in spaces {
            let id = crate::defaults::resolve(&self.conn, "workspace", &value)?;
            if self.workspace_permission(actor, &id)? < required {
                return Err(Error::Forbidden);
            }
            resolved_spaces.push(id);
        }
        for value in lists {
            let id = crate::defaults::resolve(&self.conn, "list", &value)?;
            if self.list_permission(actor, &id)? < required {
                return Err(Error::Forbidden);
            }
            resolved_lists.push(id);
        }
        if all {
            for list in self.lists(actor)? {
                if self.list_permission(actor, &list.id)? < required {
                    return Err(Error::Forbidden);
                }
            }
        }
        resolved_spaces.sort();
        resolved_spaces.dedup();
        resolved_lists.sort();
        resolved_lists.dedup();
        let code = secret();
        let changed=self.conn.execute("UPDATE cli_login_requests SET actor=?2,permission=?3,list_ids=?4,workspace_ids=?5,code_hash=?6,expires_at=unixepoch()+120 WHERE id=?1 AND consumed=0 AND code_hash IS NULL AND expires_at>unixepoch()",params![id,actor,permission,if all{None}else{Some(serde_json::to_string(&resolved_lists).unwrap())},if all{None}else{Some(serde_json::to_string(&resolved_spaces).unwrap())},hash(&code)])?;
        if changed != 1 {
            return Err(Error::Conflict("Login request changed".into()));
        }
        Ok(code)
    }
    pub fn deny_cli_login(&self, id: &str) -> Result<(), Error> {
        let changed=self.conn.execute("UPDATE cli_login_requests SET consumed=1 WHERE id=?1 AND consumed=0 AND expires_at>unixepoch()",[id])?;
        if changed != 1 {
            return Err(Error::NotFound);
        }
        Ok(())
    }
    pub fn exchange_cli_login(
        &mut self,
        id: &str,
        code: &str,
        verifier: &str,
        audience: &str,
    ) -> Result<LoginGrant, Error> {
        if !(43..=128).contains(&verifier.len())
            || !verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        {
            return Err(Error::Forbidden);
        }
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let row:(String,String,String,Option<String>,Option<String>)=tx.query_row("SELECT actor,permission,challenge,list_ids,workspace_ids FROM cli_login_requests WHERE id=?1 AND code_hash=?2 AND consumed=0 AND expires_at>unixepoch()",params![id,hash(code)],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?.ok_or(Error::Forbidden)?;
        if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != row.2 {
            return Err(Error::Forbidden);
        }
        let grant = LoginGrant {
            token: secret(),
            credential_id: uuid::Uuid::new_v4().to_string(),
            expires_at: chrono::Utc::now().timestamp() + 2_592_000,
            permission: row.1.clone(),
            spaces: row
                .4
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|_| invalid("Invalid saved spaces"))?
                .unwrap_or_default(),
            lists: row
                .3
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|_| invalid("Invalid saved lists"))?
                .unwrap_or_default(),
            all_resources: row.3.is_none() && row.4.is_none(),
        };
        tx.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,list_ids,workspace_ids,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![grant.credential_id,row.0,hash(&grant.token),audience,serde_json::to_string(&vec![row.1]).unwrap(),row.3,row.4,grant.expires_at])?;
        tx.execute("UPDATE cli_login_requests SET consumed=1 WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(grant)
    }
    pub fn revoke_cli_token(&self, token: &str, audience: &str) -> Result<(), Error> {
        self.conn.execute(
            "UPDATE auth_agents SET revoked=1 WHERE token_hash=?1 AND audience=?2",
            params![hash(token), audience],
        )?;
        Ok(())
    }
    pub(crate) fn cli_bearer(
        &self,
        token: &str,
        audience: &str,
        write: bool,
    ) -> Result<String, Error> {
        let id:Option<String>=self.conn.query_row("SELECT id FROM auth_agents WHERE token_hash=?1 AND audience=?2 AND revoked=0 AND (expires_at=-1 OR expires_at>unixepoch()) AND EXISTS(SELECT 1 FROM json_each(permissions) WHERE value='admin' OR value='write' OR (?3=0 AND value='read'))",params![hash(token),audience,write],|r|r.get(0)).optional()?;
        id.map(|id| format!("credential:{id}"))
            .ok_or(Error::Forbidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{CreateList, ListChange, WorkspaceInput};
    const VERIFIER: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cMabcdefghijklmnopqrstuvwxyz";
    fn request(db: &mut Store, permission: &str) -> LoginRequest {
        db.begin_cli_login(LoginStart {
            redirect_uri: "http://127.0.0.1:54321/callback".into(),
            challenge: URL_SAFE_NO_PAD.encode(Sha256::digest(VERIFIER.as_bytes())),
            state: "random-state-at-least-thirty-two-characters".into(),
            permission: permission.into(),
        })
        .unwrap()
    }
    fn space(db: &mut Store, title: &str) -> String {
        db.create_workspace(
            "dev",
            WorkspaceInput {
                title: title.into(),
                description: String::new(),
            },
        )
        .unwrap()
        .id
    }
    fn list(db: &mut Store, space: &str, title: &str) -> crate::core::List {
        db.create_list(
            "dev",
            CreateList {
                workspace_id: space.into(),
                title: title.into(),
                description: String::new(),
                restricted: false,
            },
        )
        .unwrap()
    }
    #[test]
    fn pkce_is_single_use_and_scopes_are_a_live_union() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let a = space(&mut db, "A");
        let b = space(&mut db, "B");
        let c = space(&mut db, "C");
        let first = list(&mut db, &a, "first");
        let second = list(&mut db, &b, "second");
        let hidden = list(&mut db, &b, "hidden");
        let outside = list(&mut db, &c, "outside");
        let pending = request(&mut db, "write");
        let code = db
            .approve_cli_login(
                "dev",
                &pending.id,
                "read",
                vec![a.clone()],
                vec![second.id.clone()],
                false,
            )
            .unwrap();
        assert!(
            db.exchange_cli_login(
                &pending.id,
                &code,
                &"wrong".repeat(12),
                "https://todoer.example/api/v1"
            )
            .is_err()
        );
        let grant = db
            .exchange_cli_login(
                &pending.id,
                &code,
                VERIFIER,
                "https://todoer.example/api/v1",
            )
            .unwrap();
        assert!(
            db.exchange_cli_login(
                &pending.id,
                &code,
                VERIFIER,
                "https://todoer.example/api/v1"
            )
            .is_err()
        );
        let actor = db
            .cli_bearer(&grant.token, "https://todoer.example/api/v1", false)
            .unwrap();
        assert!(
            db.cli_bearer(&grant.token, "https://todoer.example/mcp", false)
                .is_err()
        );
        assert!(
            db.cli_bearer(&grant.token, "https://todoer.example/api/v1", true)
                .is_err()
        );
        let visible = db
            .lists(&actor)
            .unwrap()
            .into_iter()
            .map(|l| l.id)
            .collect::<Vec<_>>();
        assert!(visible.contains(&first.id) && visible.contains(&second.id));
        assert!(!visible.contains(&hidden.id) && !visible.contains(&outside.id));
        assert_eq!(db.workspace_permission(&actor, &a).unwrap(), 1);
        assert_eq!(db.workspace_permission(&actor, &b).unwrap(), 0);
        let future = list(&mut db, &a, "future");
        assert!(db.lists(&actor).unwrap().iter().any(|l| l.id == future.id));
        assert!(
            db.set_notification_preferences(&actor, crate::notifications::Preferences::default())
                .is_err()
        );
        assert!(
            db.update_list(
                &actor,
                &first.id,
                ListChange {
                    revision: first.revision,
                    title: Some("not allowed".into()),
                    description: None,
                    archived: None,
                    color: None
                }
            )
            .is_err()
        );
        db.revoke_cli_token(&grant.token, "https://todoer.example/api/v1")
            .unwrap();
        assert!(
            db.cli_bearer(&grant.token, "https://todoer.example/api/v1", false)
                .is_err()
        );
        assert!(db.lists(&actor).unwrap().is_empty());
    }
    #[test]
    fn approval_is_explicit_bounded_and_admin_requires_ownership() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let a = space(&mut db, "A");
        let pending = request(&mut db, "read");
        assert!(
            db.approve_cli_login("dev", &pending.id, "admin", vec![a.clone()], vec![], false)
                .is_err()
        );
        assert!(
            db.approve_cli_login("dev", &pending.id, "read", vec![], vec![], false)
                .is_err()
        );
        assert!(
            db.approve_cli_login(
                "credential:fake",
                &pending.id,
                "read",
                vec![a.clone()],
                vec![],
                false
            )
            .is_err()
        );
        assert!(
            db.approve_cli_login("dev", &pending.id, "read", vec![a.clone()], vec![], true)
                .is_err()
        );
        let pending = request(&mut db, "admin");
        assert!(
            db.approve_cli_login(
                "dev",
                &pending.id,
                "admin",
                vec![],
                vec!["groceries".into()],
                false
            )
            .is_err()
        );
        let code = db
            .approve_cli_login("dev", &pending.id, "admin", vec![a.clone()], vec![], false)
            .unwrap();
        let grant = db
            .exchange_cli_login(
                &pending.id,
                &code,
                VERIFIER,
                "https://todoer.example/api/v1",
            )
            .unwrap();
        let actor = db
            .cli_bearer(&grant.token, "https://todoer.example/api/v1", true)
            .unwrap();
        assert_eq!(db.workspace_permission(&actor, &a).unwrap(), 3);
        db.conn
            .execute(
                "UPDATE auth_agents SET expires_at=0 WHERE id=?1",
                [grant.credential_id],
            )
            .unwrap();
        assert!(
            db.cli_bearer(&grant.token, "https://todoer.example/api/v1", true)
                .is_err()
        );
    }
    #[test]
    fn callback_and_denial_cannot_be_replayed() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        for uri in [
            "https://evil.example/callback",
            "http://localhost:1234/callback",
            "http://127.0.0.1:1234/callback?evil=1",
            "http://127.0.0.1:1234/other",
        ] {
            assert!(
                db.begin_cli_login(LoginStart {
                    redirect_uri: uri.into(),
                    challenge: "x".repeat(43),
                    state: "s".repeat(40),
                    permission: "read".into()
                })
                .is_err()
            );
        }
        let pending = request(&mut db, "read");
        db.deny_cli_login(&pending.id).unwrap();
        assert!(db.cli_login_request(&pending.id).is_err());
        assert!(
            db.approve_cli_login(
                "dev",
                &pending.id,
                "read",
                vec![],
                vec!["inbox".into()],
                false
            )
            .is_err()
        );
    }
}
