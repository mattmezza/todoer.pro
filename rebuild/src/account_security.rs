//! Account authentication persistence. Passwords and bearer secrets never enter logs.
use crate::auth::{AuthError, AuthStore, Identity, email, hash, now, secret};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use totp_rs::{Algorithm, TOTP};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub struct AuthPolicy {
    pub google_enabled: bool,
    pub signup_enabled: bool,
    pub revision: i64,
}
fn invalid(message: &str) -> AuthError {
    AuthError::Invalid(message.into())
}
pub fn initialize_policy(c: &Connection, google: bool, signup: bool) -> Result<(), AuthError> {
    c.execute(
        "INSERT OR IGNORE INTO auth_policy(id,google_enabled,signup_enabled) VALUES(1,?1,?2)",
        params![google, signup],
    )?;
    Ok(())
}
pub fn policy(c: &Connection) -> Result<AuthPolicy, AuthError> {
    initialize_policy(c, true, true)?;
    Ok(c.query_row(
        "SELECT google_enabled,signup_enabled,revision FROM auth_policy WHERE id=1",
        [],
        |r| {
            Ok(AuthPolicy {
                google_enabled: r.get(0)?,
                signup_enabled: r.get(1)?,
                revision: r.get(2)?,
            })
        },
    )?)
}
pub fn is_admin(c: &Connection, user: &str) -> Result<bool, AuthError> {
    Ok(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM auth_admins WHERE user_id=?1)",
        [user],
        |r| r.get(0),
    )?)
}
pub fn bootstrap_admin(c: &Connection, user: &str, address: &str) -> Result<(), AuthError> {
    let allowed = std::env::var("TODOER_AUTH_ADMIN_EMAILS")
        .unwrap_or_default()
        .split(',')
        .any(|x| !x.trim().is_empty() && x.trim().eq_ignore_ascii_case(address));
    if allowed {
        c.execute("INSERT OR IGNORE INTO auth_admins VALUES(?1)", [user])?;
    }
    Ok(())
}
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    if !(15..=128).contains(&password.chars().count()) || password.len() > 512 {
        return Err(invalid("Use a password of 15–128 characters"));
    }
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes())
        .map_err(|_| invalid("Password hashing unavailable"))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|p| p.to_string())
        .map_err(|_| invalid("Password hashing unavailable"))
}
pub fn verify_password(password: &str, encoded: &str) -> bool {
    password.len() <= 512
        && PasswordHash::new(encoded).is_ok_and(|p| {
            Argon2::default()
                .verify_password(password.as_bytes(), &p)
                .is_ok()
        })
}
// Authenticator seeds are encrypted with a deployment key, bound to their owner.
fn cipher(key: &str) -> ChaCha20Poly1305 {
    let digest = Sha256::digest(format!("todoer:totp:v1:{key}"));
    ChaCha20Poly1305::new_from_slice(&digest).expect("32 byte digest")
}
fn encrypt(key: &str, user: &str, bytes: &[u8]) -> Result<String, AuthError> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let random = Uuid::new_v4();
    let nonce = &random.as_bytes()[..12];
    let mut out = nonce.to_vec();
    out.extend(
        cipher(key)
            .encrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: bytes,
                    aad: user.as_bytes(),
                },
            )
            .map_err(|_| invalid("Security configuration unavailable"))?,
    );
    Ok(STANDARD.encode(out))
}
fn decrypt(key: &str, user: &str, value: &str) -> Result<Vec<u8>, AuthError> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let bytes = STANDARD
        .decode(value)
        .map_err(|_| invalid("Security configuration unavailable"))?;
    if bytes.len() < 28 {
        return Err(invalid("Security configuration unavailable"));
    }
    cipher(key)
        .decrypt(
            Nonce::from_slice(&bytes[..12]),
            Payload {
                msg: &bytes[12..],
                aad: user.as_bytes(),
            },
        )
        .map_err(|_| invalid("Security configuration unavailable"))
}
fn totp(bytes: Vec<u8>, address: &str) -> Result<TOTP, AuthError> {
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        bytes,
        Some("Todoer".into()),
        address.into(),
    )
    .map_err(|_| invalid("Authenticator configuration unavailable"))
}
fn ensure_security(c: &Connection, user: &str) -> Result<(), AuthError> {
    c.execute(
        "INSERT OR IGNORE INTO auth_security(user_id,handle) VALUES(?1,?2)",
        params![user, Uuid::new_v4().to_string()],
    )?;
    Ok(())
}
fn rotate_codes(c: &Connection, user: &str) -> Result<Vec<String>, AuthError> {
    c.execute("DELETE FROM auth_recovery_codes WHERE user_id=?1", [user])?;
    let mut codes = Vec::new();
    for _ in 0..10 {
        let code = Uuid::new_v4().simple().to_string();
        c.execute(
            "INSERT INTO auth_recovery_codes VALUES(?1,?2)",
            params![user, hash(&code)],
        )?;
        codes.push(code);
    }
    Ok(codes)
}
fn code_matches(c: &Connection, user: &str, code: &str, key: &str) -> Result<bool, AuthError> {
    ensure_security(c, user)?;
    let (seed, last): (Option<String>, i64) = c.query_row(
        "SELECT totp_secret,last_totp_step FROM auth_security WHERE user_id=?1",
        [user],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if let Some(seed) = seed {
        let generator = totp(decrypt(key, user, &seed)?, "account")?;
        let step = now() / 30;
        for candidate in [step, step - 1, step + 1] {
            if candidate > last
                && code.len() == 6
                && generator.generate((candidate * 30) as u64) == code
            {
                c.execute(
                    "UPDATE auth_security SET last_totp_step=?2 WHERE user_id=?1",
                    params![user, candidate],
                )?;
                return Ok(true);
            }
        }
    }
    Ok(c.execute(
        "DELETE FROM auth_recovery_codes WHERE user_id=?1 AND hash=?2",
        params![user, hash(code.trim())],
    )? == 1)
}
#[derive(Serialize)]
pub struct SecurityStatus {
    pub email: String,
    pub has_password: bool,
    pub totp_enabled: bool,
    pub passkeys: Vec<PasskeySummary>,
    pub recovery_codes_remaining: i64,
}
#[derive(Serialize)]
pub struct PasskeySummary {
    pub id: String,
    pub label: String,
}
impl AuthStore {
    /// Epoch verification and session creation share a write transaction, so a
    /// concurrent password reset cannot mint a fresh session from old proof.
    pub fn issue_verified_session(
        &mut self,
        user: &str,
        epoch: i64,
    ) -> Result<crate::auth::SessionGrant, AuthError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let actual: i64 = tx.query_row(
            "SELECT epoch FROM auth_security WHERE user_id=?1",
            [user],
            |r| r.get(0),
        )?;
        if actual != epoch {
            return Err(AuthError::Unauthorized);
        }
        let grant = crate::auth::SessionGrant {
            token: secret(),
            csrf: secret(),
            expires_at: now() + 43200,
        };
        tx.execute("INSERT INTO auth_sessions(id,user_id,token_hash,csrf_hash,expires_at) VALUES(?1,?2,?3,?4,?5)",params![Uuid::new_v4().to_string(),user,hash(&grant.token),hash(&grant.csrf),grant.expires_at])?;
        tx.execute(
            "INSERT INTO auth_recent_sessions VALUES(?1,?2)",
            params![hash(&grant.token), now()],
        )?;
        tx.commit()?;
        Ok(grant)
    }
    pub fn auth_policy(&self) -> Result<AuthPolicy, AuthError> {
        policy(&self.conn)
    }
    pub fn policy_update(
        &mut self,
        user: &str,
        updated: AuthPolicy,
        google_configured: bool,
    ) -> Result<AuthPolicy, AuthError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if !is_admin(&tx, user)? {
            return Err(AuthError::Forbidden);
        }
        let previous = policy(&tx)?;
        if previous.revision != updated.revision {
            return Err(invalid(
                "Authentication settings changed; reload before saving",
            ));
        }
        if updated.google_enabled && !google_configured {
            return Err(invalid(
                "Configure Google client credentials before enabling Google login",
            ));
        }
        let alternative:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM auth_passwords WHERE user_id=?1 UNION ALL SELECT 1 FROM auth_passkeys WHERE user_id=?1)",[user],|r|r.get(0))?;
        if !updated.google_enabled && !alternative {
            return Err(invalid(
                "Add a password or passkey to your account before disabling Google login",
            ));
        }
        tx.execute("UPDATE auth_policy SET google_enabled=?1,signup_enabled=?2,revision=revision+1 WHERE id=1",params![updated.google_enabled,updated.signup_enabled])?;
        let mut after = updated.clone();
        after.revision = previous.revision + 1;
        tx.execute("INSERT INTO auth_policy_events(actor,before_policy,after_policy,created_at) VALUES(?1,?2,?3,?4)",params![user,serde_json::to_string(&previous).unwrap(),serde_json::to_string(&after).unwrap(),now()])?;
        tx.commit()?;
        self.auth_policy()
    }
    pub fn throttle(&self, key: &str, limit: i64) -> Result<(), AuthError> {
        self.conn.execute(
            "DELETE FROM auth_throttle WHERE window_start<?1",
            [now() - 900],
        )?;
        self.conn.execute("INSERT INTO auth_throttle VALUES(?1,?2,1) ON CONFLICT(key) DO UPDATE SET attempts=attempts+1",params![hash(key),now()])?;
        let attempts: i64 = self.conn.query_row(
            "SELECT attempts FROM auth_throttle WHERE key=?1",
            [hash(key)],
            |r| r.get(0),
        )?;
        if attempts > limit {
            return Err(invalid("Too many attempts. Try again in 15 minutes"));
        }
        Ok(())
    }
    pub fn recent(&self, token: &str) -> Result<String, AuthError> {
        let principal = self.authenticate_session(token, None, false)?;
        let recent:bool=self.conn.query_row("SELECT EXISTS(SELECT 1 FROM auth_recent_sessions WHERE token_hash=?1 AND authenticated_at>?2)",params![hash(token),now()-300],|r|r.get(0))?;
        if !recent {
            return Err(invalid("Sign in again to change account security"));
        }
        Ok(principal.actor)
    }
    pub fn mark_recent(&self, token: &str) -> Result<(), AuthError> {
        self.conn.execute(
            "DELETE FROM auth_recent_sessions WHERE authenticated_at<?1",
            [now() - 43200],
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO auth_recent_sessions VALUES(?1,?2)",
            params![hash(token), now()],
        )?;
        Ok(())
    }
    pub fn account_email(&self, user: &str) -> Result<String, AuthError> {
        self.conn.query_row("SELECT email FROM auth_identities WHERE user_id=?1 ORDER BY CASE WHEN issuer='local' THEN 0 ELSE 1 END LIMIT 1",[user],|r|r.get(0)).map_err(AuthError::from)
    }
    pub fn registration_token(
        &mut self,
        address: &str,
        name: &str,
        password_hash: &str,
    ) -> Result<Option<String>, AuthError> {
        let address = email(address)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if !policy(&tx)?.signup_enabled {
            return Err(invalid("Account creation is currently closed"));
        }
        let allowed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM auth_email_allowlist WHERE email=?1)",
            [&address],
            |r| r.get(0),
        )?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM auth_identities WHERE email=?1)",
            [&address],
            |r| r.get(0),
        )?;
        if !allowed || exists {
            return Ok(None);
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 200 {
            return Err(invalid("Enter a name of 1–200 characters"));
        }
        let token = secret();
        tx.execute("DELETE FROM auth_account_tokens WHERE expires_at<?1 OR (purpose='verify' AND email=?2)",params![now(),address])?;
        tx.execute("INSERT INTO auth_account_tokens(hash,purpose,email,payload,expires_at) VALUES(?1,'verify',?2,?3,?4)",params![hash(&token),address,serde_json::json!({"name":name,"hash":password_hash}).to_string(),now()+3600])?;
        tx.commit()?;
        Ok(Some(token))
    }
    pub fn verify_registration(&mut self, token: &str) -> Result<Identity, AuthError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if !policy(&tx)?.signup_enabled {
            return Err(invalid("Account creation is currently closed"));
        }
        let (address,payload):(String,String)=tx.query_row("SELECT email,payload FROM auth_account_tokens WHERE hash=?1 AND purpose='verify' AND expires_at>?2",params![hash(token),now()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(||invalid("Verification link expired or already used"))?;
        let allowed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM auth_email_allowlist WHERE email=?1)",
            [&address],
            |r| r.get(0),
        )?;
        if !allowed {
            return Err(AuthError::Forbidden);
        }
        let data: serde_json::Value =
            serde_json::from_str(&payload).map_err(|_| invalid("Invalid verification"))?;
        let user = Identity {
            id: crate::defaults::short_id("u"),
            name: data["name"].as_str().unwrap_or("Todoer user").into(),
        };
        tx.execute(
            "INSERT INTO users VALUES(?1,?2)",
            params![user.id, user.name],
        )?;
        tx.execute(
            "INSERT INTO auth_identities(issuer,subject,user_id,email) VALUES('local',?1,?1,?2)",
            params![user.id, address],
        )?;
        tx.execute(
            "INSERT INTO auth_passwords VALUES(?1,?2,?3)",
            params![
                user.id,
                address,
                data["hash"]
                    .as_str()
                    .ok_or_else(|| invalid("Invalid verification"))?
            ],
        )?;
        ensure_security(&tx, &user.id)?;
        crate::defaults::user(&tx, &user.id).map_err(|e| match e {
            crate::core::Error::Database(e) => AuthError::Database(e),
            other => invalid(&other.to_string()),
        })?;
        bootstrap_admin(&tx, &user.id, &address)?;
        tx.execute(
            "DELETE FROM auth_account_tokens WHERE hash=?1",
            [hash(token)],
        )?;
        tx.commit()?;
        Ok(user)
    }
    pub fn password_record(&self, address: &str) -> Result<Option<(String, String)>, AuthError> {
        Ok(self
            .conn
            .query_row(
                "SELECT user_id,password_hash FROM auth_passwords WHERE email=?1",
                [email(address)?],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }
    pub fn has_mfa(&self, user: &str) -> Result<bool, AuthError> {
        Ok(self.conn.query_row("SELECT EXISTS(SELECT 1 FROM auth_security WHERE user_id=?1 AND totp_secret IS NOT NULL UNION ALL SELECT 1 FROM auth_passkeys WHERE user_id=?1)",[user],|r|r.get(0))?)
    }
    pub fn epoch(&self, user: &str) -> Result<i64, AuthError> {
        ensure_security(&self.conn, user)?;
        Ok(self.conn.query_row(
            "SELECT epoch FROM auth_security WHERE user_id=?1",
            [user],
            |r| r.get(0),
        )?)
    }
    pub fn check_code(&mut self, user: &str, code: &str, key: &str) -> Result<bool, AuthError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let ok = code_matches(&tx, user, code, key)?;
        tx.commit()?;
        Ok(ok)
    }
    pub fn reset_token(&self, address: &str) -> Result<Option<String>, AuthError> {
        let Some((user, _)) = self.password_record(address)? else {
            return Ok(None);
        };
        let token = secret();
        let epoch = self.epoch(&user)?;
        self.conn.execute("DELETE FROM auth_account_tokens WHERE expires_at<?1 OR (purpose='reset' AND user_id=?2)",params![now(),user])?;
        self.conn.execute(
            "INSERT INTO auth_account_tokens VALUES(?1,'reset',?2,?3,'',?4,?5)",
            params![hash(&token), email(address)?, user, now() + 1800, epoch],
        )?;
        Ok(Some(token))
    }
    pub fn reset_owner(&self, token: &str) -> Result<String, AuthError> {
        self.conn.query_row("SELECT t.user_id FROM auth_account_tokens t JOIN auth_security s ON s.user_id=t.user_id AND s.epoch=t.epoch WHERE t.hash=?1 AND t.purpose='reset' AND t.expires_at>?2 AND EXISTS(SELECT 1 FROM auth_identities i WHERE i.user_id=t.user_id AND i.email=t.email)",params![hash(token),now()],|r|r.get(0)).optional()?.ok_or_else(||invalid("Reset link expired or already used"))
    }
    pub fn reset_password(
        &mut self,
        token: &str,
        encoded: &str,
        code: &str,
        key: &str,
        passkey_proof: Option<(&str, i64)>,
    ) -> Result<(), AuthError> {
        let user = self.reset_owner(token)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM auth_account_tokens t JOIN auth_security s ON s.user_id=t.user_id AND s.epoch=t.epoch WHERE t.hash=?1 AND t.expires_at>?2 AND t.purpose='reset' AND EXISTS(SELECT 1 FROM auth_identities i WHERE i.user_id=t.user_id AND i.email=t.email))",params![hash(token),now()],|r|r.get(0))?;
        if !valid {
            return Err(invalid("Reset link expired or already used"));
        }
        let epoch: i64 = tx.query_row(
            "SELECT epoch FROM auth_security WHERE user_id=?1",
            [&user],
            |r| r.get(0),
        )?;
        let mfa:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM auth_security WHERE user_id=?1 AND totp_secret IS NOT NULL UNION ALL SELECT 1 FROM auth_passkeys WHERE user_id=?1)",[&user],|r|r.get(0))?;
        if mfa
            && passkey_proof != Some((user.as_str(), epoch))
            && !code_matches(&tx, &user, code, key)?
        {
            return Err(invalid(
                "Enter an authenticator or recovery code, or verify your passkey",
            ));
        }
        tx.execute(
            "UPDATE auth_passwords SET password_hash=?2 WHERE user_id=?1",
            params![user, encoded],
        )?;
        tx.execute(
            "UPDATE auth_sessions SET revoked=1 WHERE user_id=?1",
            [&user],
        )?;
        tx.execute(
            "UPDATE auth_security SET epoch=epoch+1 WHERE user_id=?1",
            [&user],
        )?;
        tx.execute("DELETE FROM auth_account_tokens WHERE user_id=?1", [&user])?;
        tx.commit()?;
        Ok(())
    }
    pub fn set_password(&mut self, user: &str, encoded: &str) -> Result<(), AuthError> {
        let address = self.account_email(user)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO auth_passwords(user_id,email,password_hash) VALUES(?1,?2,?3) ON CONFLICT(user_id) DO UPDATE SET password_hash=excluded.password_hash",params![user,address,encoded])?;
        ensure_security(&tx, user)?;
        tx.execute(
            "UPDATE auth_security SET epoch=epoch+1 WHERE user_id=?1",
            [user],
        )?;
        tx.execute(
            "UPDATE auth_sessions SET revoked=1 WHERE user_id=?1",
            [user],
        )?;
        tx.execute("DELETE FROM auth_account_tokens WHERE user_id=?1", [user])?;
        tx.commit()?;
        Ok(())
    }
    pub fn totp_setup(&self, user: &str, key: &str) -> Result<(String, String, String), AuthError> {
        let bytes = [
            Uuid::new_v4().as_bytes().as_slice(),
            Uuid::new_v4().as_bytes().as_slice(),
        ]
        .concat();
        let generator = totp(bytes.clone(), &self.account_email(user)?)?;
        Ok((
            encrypt(key, user, &bytes)?,
            generator.get_secret_base32(),
            generator.get_url(),
        ))
    }
    pub fn enable_totp(
        &mut self,
        user: &str,
        encrypted: &str,
        code: &str,
        key: &str,
    ) -> Result<Vec<String>, AuthError> {
        let generator = totp(decrypt(key, user, encrypted)?, &self.account_email(user)?)?;
        let step = now() / 30;
        let matched = [step, step - 1, step + 1]
            .into_iter()
            .find(|s| code.len() == 6 && generator.generate((*s * 30) as u64) == code)
            .ok_or_else(|| invalid("Authenticator code did not match"))?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        ensure_security(&tx, user)?;
        let already: bool = tx.query_row(
            "SELECT totp_secret IS NOT NULL FROM auth_security WHERE user_id=?1",
            [user],
            |r| r.get(0),
        )?;
        if already {
            return Err(invalid("Authenticator is already enabled"));
        }
        tx.execute("UPDATE auth_security SET totp_secret=?2,last_totp_step=?3,epoch=epoch+1 WHERE user_id=?1",params![user,encrypted,matched])?;
        let codes = rotate_codes(&tx, user)?;
        tx.execute(
            "UPDATE auth_sessions SET revoked=1 WHERE user_id=?1",
            [user],
        )?;
        tx.commit()?;
        Ok(codes)
    }
    pub fn disable_totp(&mut self, user: &str) -> Result<(), AuthError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute("UPDATE auth_security SET totp_secret=NULL,last_totp_step=-1,epoch=epoch+1 WHERE user_id=?1",[user])?;
        let keys: i64 = tx.query_row(
            "SELECT count(*) FROM auth_passkeys WHERE user_id=?1",
            [user],
            |r| r.get(0),
        )?;
        if keys == 0 {
            tx.execute("DELETE FROM auth_recovery_codes WHERE user_id=?1", [user])?;
        }
        tx.execute(
            "UPDATE auth_sessions SET revoked=1 WHERE user_id=?1",
            [user],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn recovery_codes(&mut self, user: &str) -> Result<Vec<String>, AuthError> {
        if !self.has_mfa(user)? {
            return Err(invalid("Enable MFA first"));
        }
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let codes = rotate_codes(&tx, user)?;
        tx.commit()?;
        Ok(codes)
    }
    pub fn security_status(&self, user: &str) -> Result<SecurityStatus, AuthError> {
        ensure_security(&self.conn, user)?;
        let mut q = self.conn.prepare(
            "SELECT id,label FROM auth_passkeys WHERE user_id=?1 ORDER BY created_at,id",
        )?;
        let passkeys = q
            .query_map([user], |r| {
                Ok(PasskeySummary {
                    id: r.get(0)?,
                    label: r.get(1)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(SecurityStatus {
            email: self.account_email(user)?,
            has_password: self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM auth_passwords WHERE user_id=?1)",
                [user],
                |r| r.get(0),
            )?,
            totp_enabled: self.conn.query_row(
                "SELECT totp_secret IS NOT NULL FROM auth_security WHERE user_id=?1",
                [user],
                |r| r.get(0),
            )?,
            passkeys,
            recovery_codes_remaining: self.conn.query_row(
                "SELECT count(*) FROM auth_recovery_codes WHERE user_id=?1",
                [user],
                |r| r.get(0),
            )?,
        })
    }
    pub fn passkey_handle(&self, user: &str) -> Result<Uuid, AuthError> {
        ensure_security(&self.conn, user)?;
        let id: String = self.conn.query_row(
            "SELECT handle FROM auth_security WHERE user_id=?1",
            [user],
            |r| r.get(0),
        )?;
        Uuid::parse_str(&id).map_err(|_| invalid("Invalid account handle"))
    }
    pub fn passkeys(&self, user: &str) -> Result<Vec<webauthn_rs::prelude::Passkey>, AuthError> {
        let mut q = self
            .conn
            .prepare("SELECT credential FROM auth_passkeys WHERE user_id=?1")?;
        let rows = q
            .query_map([user], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|s| serde_json::from_str(&s).map_err(|_| invalid("Invalid stored credential")))
            .collect()
    }
    pub fn save_passkey(
        &mut self,
        user: &str,
        label: &str,
        key: &webauthn_rs::prelude::Passkey,
    ) -> Result<Option<Vec<String>>, AuthError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        ensure_security(&tx, user)?;
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM auth_passkeys WHERE user_id=?1",
            [user],
            |r| r.get(0),
        )?;
        if count >= 10 {
            return Err(invalid("At most 10 passkeys per account"));
        }
        let credential = serde_json::to_string(key).map_err(|_| invalid("Invalid credential"))?;
        let id = serde_json::to_string(key.cred_id()).unwrap();
        tx.execute(
            "INSERT INTO auth_passkeys VALUES(?1,?2,?3,?4,?5)",
            params![
                id,
                user,
                label.chars().take(100).collect::<String>(),
                credential,
                now()
            ],
        )?;
        let codes = if count == 0 {
            Some(rotate_codes(&tx, user)?)
        } else {
            None
        };
        tx.execute(
            "UPDATE auth_security SET epoch=epoch+1 WHERE user_id=?1",
            [user],
        )?;
        tx.execute(
            "UPDATE auth_sessions SET revoked=1 WHERE user_id=?1",
            [user],
        )?;
        tx.commit()?;
        Ok(codes)
    }
    pub fn update_passkeys(
        &mut self,
        user: &str,
        result: &webauthn_rs::prelude::AuthenticationResult,
    ) -> Result<(), AuthError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut keys = {
            let mut q = tx.prepare("SELECT credential FROM auth_passkeys WHERE user_id=?1")?;
            q.query_map([user], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for encoded in keys.drain(..) {
            let mut key: webauthn_rs::prelude::Passkey =
                serde_json::from_str(&encoded).map_err(|_| invalid("Invalid credential"))?;
            if result.cred_id() == key.cred_id() {
                let stored: serde_json::Value =
                    serde_json::from_str(&encoded).map_err(|_| invalid("Invalid credential"))?;
                let counter = stored["cred"]["counter"].as_u64().unwrap_or(0);
                if (counter > 0 || result.counter() > 0) && u64::from(result.counter()) <= counter {
                    return Err(AuthError::Unauthorized);
                }
                key.update_credential(result);
                tx.execute(
                    "UPDATE auth_passkeys SET credential=?2 WHERE id=?1 AND user_id=?3",
                    params![
                        serde_json::to_string(key.cred_id()).unwrap(),
                        serde_json::to_string(&key).unwrap(),
                        user
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn remove_passkey(&mut self, user: &str, id: &str) -> Result<(), AuthError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let alternatives:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM auth_passwords WHERE user_id=?1 UNION ALL SELECT 1 FROM auth_passkeys WHERE user_id=?1 AND id<>?2)",params![user,id],|r|r.get(0))?;
        if !alternatives && !policy(&tx)?.google_enabled {
            return Err(invalid("Keep at least one sign-in method"));
        }
        if tx.execute(
            "DELETE FROM auth_passkeys WHERE user_id=?1 AND id=?2",
            params![user, id],
        )? != 1
        {
            return Err(invalid("Passkey not found"));
        }
        ensure_security(&tx, user)?;
        tx.execute(
            "UPDATE auth_security SET epoch=epoch+1 WHERE user_id=?1",
            [user],
        )?;
        let mfa:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM auth_security WHERE user_id=?1 AND totp_secret IS NOT NULL UNION ALL SELECT 1 FROM auth_passkeys WHERE user_id=?1)",[user],|r|r.get(0))?;
        if !mfa {
            tx.execute("DELETE FROM auth_recovery_codes WHERE user_id=?1", [user])?;
        }
        tx.execute(
            "UPDATE auth_sessions SET revoked=1 WHERE user_id=?1",
            [user],
        )?;
        tx.commit()?;
        Ok(())
    }
}

impl crate::core::Store {
    pub fn initialize_auth_policy(&self, google: bool, signup: bool) -> Result<(), AuthError> {
        initialize_policy(&self.conn, google, signup)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> AuthStore {
        let mut core = crate::core::Store::memory().unwrap();
        core.seed_dev().unwrap();
        let db = AuthStore { conn: core.conn };
        db.allow_email("user@example.test").unwrap();
        db
    }
    fn account(db: &mut AuthStore) -> Identity {
        let encoded = hash_password("a sufficiently long password").unwrap();
        let token = db
            .registration_token("USER@example.test", "Example User", &encoded)
            .unwrap()
            .unwrap();
        db.verify_registration(&token).unwrap()
    }
    #[test]
    fn registration_is_verified_single_use_and_closed_on_policy_change() {
        let mut db = db();
        let hash = hash_password("a sufficiently long password").unwrap();
        assert!(
            db.registration_token("outside@example.test", "User", &hash)
                .unwrap()
                .is_none()
        );
        let token = db
            .registration_token("user@example.test", "User", &hash)
            .unwrap()
            .unwrap();
        assert!(db.password_record("user@example.test").unwrap().is_none());
        db.conn
            .execute("UPDATE auth_policy SET signup_enabled=0", [])
            .unwrap();
        assert!(db.verify_registration(&token).is_err());
        db.conn
            .execute("UPDATE auth_policy SET signup_enabled=1", [])
            .unwrap();
        let identity = db.verify_registration(&token).unwrap();
        assert_eq!(identity.name, "User");
        assert!(db.verify_registration(&token).is_err());
        let (_, encoded) = db.password_record("USER@example.test").unwrap().unwrap();
        assert!(verify_password("a sufficiently long password", &encoded));
        assert!(!verify_password("wrong", &encoded));
        assert_ne!(encoded, "a sufficiently long password");
        assert!(
            db.registration_token("user@example.test", "User", &hash)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn policy_is_persistent_admin_only_and_prevents_lockout() {
        let mut db = db();
        db.auth_policy().unwrap();
        initialize_policy(&db.conn, false, false).unwrap();
        assert!(db.auth_policy().unwrap().google_enabled);
        let mut policy = db.auth_policy().unwrap();
        policy.signup_enabled = false;
        assert!(db.policy_update("dev", policy.clone(), true).is_err());
        db.conn
            .execute("INSERT INTO auth_admins VALUES('dev')", [])
            .unwrap();
        db.policy_update("dev", policy.clone(), true).unwrap();
        assert!(!db.auth_policy().unwrap().signup_enabled);
        assert!(db.policy_update("dev", policy, true).is_err());
        let mut updated = db.auth_policy().unwrap();
        updated.google_enabled = false;
        assert!(db.policy_update("dev", updated.clone(), true).is_err());
        db.conn.execute("INSERT INTO auth_identities(issuer,subject,user_id,email) VALUES('local','dev','dev','dev@example.test')",[]).unwrap();
        db.set_password(
            "dev",
            &hash_password("a sufficiently long password").unwrap(),
        )
        .unwrap();
        db.policy_update("dev", updated, false).unwrap();
        let mut updated = db.auth_policy().unwrap();
        updated.google_enabled = true;
        assert!(db.policy_update("dev", updated, false).is_err());
    }
    #[test]
    fn reset_preserves_mfa_revokes_sessions_and_cannot_be_replayed() {
        let mut db = db();
        let user = account(&mut db);
        let grant = db.issue_session(&user.id, 3600).unwrap();
        db.mark_recent(&grant.token).unwrap();
        assert_eq!(db.recent(&grant.token).unwrap(), user.id);
        let (encrypted, _, _) = db.totp_setup(&user.id, "deployment-test-key").unwrap();
        let generator = totp(
            decrypt("deployment-test-key", &user.id, &encrypted).unwrap(),
            "user@example.test",
        )
        .unwrap();
        let code = generator.generate((now() / 30 * 30) as u64);
        let codes = db
            .enable_totp(&user.id, &encrypted, &code, "deployment-test-key")
            .unwrap();
        assert_eq!(codes.len(), 10);
        assert!(db.authenticate_session(&grant.token, None, false).is_err());
        assert!(
            !db.check_code(&user.id, &code, "deployment-test-key")
                .unwrap()
        );
        let future = generator.generate(((now() / 30 + 1) * 30) as u64);
        assert!(
            db.check_code(&user.id, &future, "deployment-test-key")
                .unwrap()
        );
        assert!(
            !db.check_code(&user.id, &future, "deployment-test-key")
                .unwrap()
        );
        assert!(decrypt("wrong-key", &user.id, &encrypted).is_err());
        assert!(decrypt("deployment-test-key", "other-user", &encrypted).is_err());
        let token = db.reset_token("user@example.test").unwrap().unwrap();
        let password = hash_password("the replacement long password").unwrap();
        assert!(
            db.reset_password(&token, &password, "invalid", "deployment-test-key", None)
                .is_err()
        );
        assert!(db.has_mfa(&user.id).unwrap());
        db.reset_password(&token, &password, &codes[0], "deployment-test-key", None)
            .unwrap();
        assert!(
            db.reset_password(&token, &password, &codes[1], "deployment-test-key", None)
                .is_err()
        );
        assert!(db.has_mfa(&user.id).unwrap());
        assert!(
            !db.check_code(&user.id, &codes[0], "deployment-test-key")
                .unwrap()
        );
        let (_, saved) = db.password_record("user@example.test").unwrap().unwrap();
        assert!(verify_password("the replacement long password", &saved));
        let token = db.reset_token("user@example.test").unwrap().unwrap();
        db.conn
            .execute(
                "UPDATE auth_account_tokens SET expires_at=0 WHERE purpose='reset'",
                [],
            )
            .unwrap();
        assert!(db.reset_owner(&token).is_err());
    }
    #[test]
    fn rate_limits_and_expired_verification_are_enforced() {
        let mut db = db();
        for _ in 0..3 {
            db.throttle("same-email", 3).unwrap();
        }
        assert!(db.throttle("same-email", 3).is_err());
        db.conn
            .execute("UPDATE auth_throttle SET window_start=0", [])
            .unwrap();
        db.throttle("same-email", 3).unwrap();
        let hash = hash_password("a sufficiently long password").unwrap();
        let token = db
            .registration_token("user@example.test", "User", &hash)
            .unwrap()
            .unwrap();
        db.conn
            .execute("UPDATE auth_account_tokens SET expires_at=0", [])
            .unwrap();
        assert!(db.verify_registration(&token).is_err());
        assert!(hash_password("short").is_err());
        assert!(hash_password(&"x".repeat(129)).is_err());
    }
}
