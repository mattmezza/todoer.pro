//! Browser account flows. Challenge state is bounded, expiring and server-only.
use super::*;
use crate::{
    account_security::{self, AuthPolicy},
    auth::{AuthError, AuthStore},
};
use std::collections::HashMap;
use webauthn_rs::prelude::*;

pub(super) struct AccountRuntime {
    key: String,
    webauthn: Option<Webauthn>,
    flows: Mutex<HashMap<String, Flow>>,
    ceremonies: Mutex<HashMap<String, Ceremony>>,
}
#[derive(Clone)]
struct Flow {
    user: String,
    epoch: i64,
    next: String,
    reset: Option<String>,
    verified: bool,
    expires: i64,
}
enum CeremonyState {
    Register(PasskeyRegistration, String),
    Authenticate(PasskeyAuthentication),
    Totp(String),
}
struct Ceremony {
    binding: String,
    user: String,
    epoch: i64,
    expires: i64,
    state: CeremonyState,
}
impl AccountRuntime {
    pub fn new(origin: &str, key: String) -> Self {
        let webauthn = Url::parse(origin).ok().and_then(|url| {
            url.host_str()
                .and_then(|host| WebauthnBuilder::new(host, &url).ok())
                .and_then(|b| b.rp_name("Todoer").build().ok())
        });
        Self {
            key,
            webauthn,
            flows: Mutex::new(HashMap::new()),
            ceremonies: Mutex::new(HashMap::new()),
        }
    }
    fn flow(&self, headers: &HeaderMap, take: bool) -> Result<Flow, ApiError> {
        let token = cookie(headers, "todoer_account_flow").ok_or(AuthError::Unauthorized)?;
        let mut flows = self.flows.lock().map_err(|_| AuthError::Unauthorized)?;
        flows.retain(|_, f| f.expires > crate::auth::now());
        if take {
            flows.remove(&crate::auth::hash(token))
        } else {
            flows.get(&crate::auth::hash(token)).cloned()
        }
        .ok_or_else(|| AuthError::Unauthorized.into())
    }
    fn put_flow(&self, flow: Flow) -> Result<String, ApiError> {
        let mut flows = self.flows.lock().map_err(|_| AuthError::Unauthorized)?;
        flows.retain(|_, f| f.expires > crate::auth::now());
        if flows.len() >= 1000 {
            return Err(problem("Too many sign-in attempts; try again shortly"));
        }
        let token = crate::auth::secret();
        flows.insert(crate::auth::hash(&token), flow);
        Ok(token)
    }
    fn put_ceremony(&self, ceremony: Ceremony) -> Result<String, ApiError> {
        let mut all = self
            .ceremonies
            .lock()
            .map_err(|_| AuthError::Unauthorized)?;
        all.retain(|_, c| c.expires > crate::auth::now());
        if all.len() >= 1000 {
            return Err(problem("Too many security challenges"));
        }
        let id = crate::auth::secret();
        all.insert(id.clone(), ceremony);
        Ok(id)
    }
    fn take_ceremony(&self, id: &str, binding: &str) -> Result<Ceremony, ApiError> {
        let mut all = self
            .ceremonies
            .lock()
            .map_err(|_| AuthError::Unauthorized)?;
        // Wrong browser must not consume another browser's challenge.
        if all
            .get(id)
            .is_none_or(|c| c.binding != binding || c.expires <= crate::auth::now())
        {
            return Err(AuthError::Unauthorized.into());
        }
        Ok(all.remove(id).unwrap())
    }
}
fn problem(message: &str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, "account", message.into())
}
fn cookie<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    h.get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|s| s.trim().strip_prefix(&format!("{name}=")))
}
fn same_origin(s: &AppState, h: &HeaderMap) -> Result<(), ApiError> {
    if h.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(&s.origin) {
        return Err(AuthError::Forbidden.into());
    }
    Ok(())
}
fn add_cookie(r: &mut Response, name: &str, value: &str, max_age: i64) {
    r.headers_mut().append(
        header::SET_COOKIE,
        format!("{name}={value}; Max-Age={max_age}; HttpOnly; Secure; SameSite=Lax; Path=/")
            .parse()
            .unwrap(),
    );
}
async fn auth_run<T: Send + 'static>(
    s: &AppState,
    f: impl FnOnce(&mut AuthStore) -> Result<T, AuthError> + Send + 'static,
) -> Result<T, ApiError> {
    let auth = s
        .production
        .clone()
        .ok_or_else(|| problem("Use the real authentication test instance for account security"))?;
    tokio::task::spawn_blocking(move || {
        let mut store = auth.store.lock().map_err(|_| AuthError::Unauthorized)?;
        f(&mut store)
    })
    .await
    .map_err(|_| problem("Account service unavailable"))?
    .map_err(ApiError::from)
}
fn hashers() -> &'static Arc<tokio::sync::Semaphore> {
    static HASHERS: std::sync::OnceLock<Arc<tokio::sync::Semaphore>> = std::sync::OnceLock::new();
    HASHERS.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(4)))
}
// Recheck after queued work as well as at request entry: a logout/reset that
// completed while hashing or waiting must invalidate the pending change.
async fn auth_change<T: Send + 'static>(
    s: &AppState,
    h: &HeaderMap,
    user: String,
    f: impl FnOnce(&mut AuthStore) -> Result<T, AuthError> + Send + 'static,
) -> Result<T, ApiError> {
    let token = cookie(h, "todoer_session")
        .ok_or(AuthError::Unauthorized)?
        .to_owned();
    auth_run(s, move |db| {
        if db.recent(&token)? != user {
            return Err(AuthError::Forbidden);
        }
        f(db)
    })
    .await
}
async fn password_hash(value: String) -> Result<String, ApiError> {
    let permit = hashers()
        .clone()
        .try_acquire_owned()
        .map_err(|_| problem("Sign-in is busy; try again shortly"))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        account_security::hash_password(&value)
    })
    .await
    .map_err(|_| problem("Password hashing unavailable"))?
    .map_err(ApiError::from)
}
async fn throttle(s: &AppState, address: &str) -> Result<(), ApiError> {
    let key = format!("account:{}", address.trim().to_lowercase());
    auth_run(s, move |db| {
        db.throttle("account-global", 100)?;
        db.throttle(&key, 10)
    })
    .await
}
async fn session_user(s: &AppState, h: &HeaderMap, recent: bool) -> Result<String, ApiError> {
    same_origin(s, h)?;
    let user = s.auth(h, true)?;
    if user.starts_with("credential:") {
        return Err(AuthError::Forbidden.into());
    }
    let token = cookie(h, "todoer_session")
        .ok_or(AuthError::Unauthorized)?
        .to_string();
    if recent {
        auth_run(s, move |db| db.recent(&token)).await
    } else {
        Ok(user)
    }
}
pub(super) async fn finish_login(
    s: &AppState,
    user: String,
    epoch: i64,
    next: String,
) -> Result<Response, ApiError> {
    let actor = user.clone();
    s.run(move |db| db.ensure_user_defaults(&actor)).await?;
    let grant = auth_run(s, move |db| db.issue_verified_session(&user, epoch)).await?;
    let mut response =
        Json(json!({"redirect":browser_auth::safe_next(Some(&next))})).into_response();
    add_cookie(&mut response, "todoer_session", &grant.token, 43200);
    // Existing csrf_for expects the token in the cookie; JS reads the matching meta tag.
    add_cookie(&mut response, "todoer_csrf", &grant.csrf, 43200);
    add_cookie(&mut response, "todoer_account_flow", "", 0);
    Ok(response)
}
pub(super) fn mfa_redirect(
    s: &AppState,
    user: String,
    epoch: i64,
    next: String,
) -> Result<Response, ApiError> {
    let token = s.accounts.put_flow(Flow {
        user,
        epoch,
        next: next.clone(),
        reset: None,
        verified: false,
        expires: crate::auth::now() + 300,
    })?;
    let mut response = Redirect::to("/signin?step=mfa").into_response();
    add_cookie(&mut response, "todoer_account_flow", &token, 300);
    Ok(response)
}
async fn send_mail(address: String, subject: &str, body: String) -> Result<(), ApiError> {
    let mail = crate::notifications::Notifications::from_env()
        .map_err(ApiError::from)?
        .ok_or_else(|| problem("Account email delivery is not configured"))?;
    mail.send_account(&address, subject, &body)
        .await
        .map_err(ApiError::from)
}
fn mail_available() -> Result<(), ApiError> {
    if crate::notifications::Notifications::from_env()
        .map_err(ApiError::from)?
        .is_none()
    {
        return Err(problem("Account email delivery is not configured"));
    }
    Ok(())
}
#[derive(Deserialize)]
struct RegisterInput {
    email: String,
    name: String,
    password: String,
}
async fn register(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<RegisterInput>,
) -> Result<Response, ApiError> {
    same_origin(&s, &h)?;
    mail_available()?;
    throttle(&s, &input.email).await?;
    let encoded = password_hash(input.password).await?;
    let email = input.email.clone();
    let token = auth_run(&s, move |db| {
        db.registration_token(&input.email, &input.name, &encoded)
    })
    .await?;
    if let Some(token) = token {
        send_mail(email,"Verify your Todoer account",format!("Confirm your email to create your Todoer account:\n\n{}/verify-email#token={}\n\nThis link expires in one hour. If you did not request an account, ignore this message.",s.origin,token)).await?;
    }
    Ok(Json(json!({"message":"If this email can register, a verification link is on its way. Check your inbox."})).into_response())
}
#[derive(Deserialize)]
struct TokenInput {
    token: String,
}
async fn verify(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<TokenInput>,
) -> Result<Response, ApiError> {
    same_origin(&s, &h)?;
    throttle(&s, "verify").await?;
    let (user, epoch) = auth_run(&s, move |db| {
        let u = db.verify_registration(&input.token)?;
        let epoch = db.epoch(&u.id)?;
        Ok((u.id, epoch))
    })
    .await?;
    finish_login(&s, user, epoch, "/inbox".into()).await
}
#[derive(Deserialize)]
struct LoginInput {
    email: String,
    password: String,
    next: Option<String>,
}
async fn login(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<LoginInput>,
) -> Result<Response, ApiError> {
    same_origin(&s, &h)?;
    throttle(&s, &input.email).await?;
    let address = input.email;
    let record = auth_run(&s, move |db| db.password_record(&address)).await?;
    let permit = hashers()
        .clone()
        .try_acquire_owned()
        .map_err(|_| problem("Sign-in is busy; try again shortly"))?;
    let password = input.password;
    let (user, expected, valid) = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let (user, encoded) = record.unwrap_or_else(|| {
            (
                String::new(),
                DUMMY
                    .get_or_init(|| {
                        account_security::hash_password("unusable random dummy password").unwrap()
                    })
                    .clone(),
            )
        });
        let valid = account_security::verify_password(&password, &encoded);
        (user, encoded, valid)
    })
    .await
    .map_err(|_| problem("Sign-in unavailable"))?;
    if !valid || user.is_empty() {
        return Err(problem("Email or password did not match"));
    }
    let checked = user.clone();
    let (epoch, mfa) = auth_run(&s, move |db| {
        let valid: bool = db.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM auth_passwords WHERE user_id=?1 AND password_hash=?2)",
            rusqlite::params![checked, expected],
            |r| r.get(0),
        )?;
        if !valid {
            return Err(AuthError::Unauthorized);
        }
        Ok((db.epoch(&checked)?, db.has_mfa(&checked)?))
    })
    .await?;
    let next = browser_auth::safe_next(input.next.as_deref());
    if mfa {
        let token = s.accounts.put_flow(Flow {
            user,
            epoch,
            next,
            reset: None,
            verified: false,
            expires: crate::auth::now() + 300,
        })?;
        let mut r = Json(json!({"mfa":true})).into_response();
        add_cookie(&mut r, "todoer_account_flow", &token, 300);
        Ok(r)
    } else {
        finish_login(&s, user, epoch, next).await
    }
}
#[derive(Deserialize)]
struct CodeInput {
    code: String,
}
async fn mfa_code(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<CodeInput>,
) -> Result<Response, ApiError> {
    same_origin(&s, &h)?;
    let flow = s.accounts.flow(&h, true)?;
    if flow.reset.is_some() {
        return Err(AuthError::Forbidden.into());
    }
    throttle(&s, &flow.user).await?;
    let user = flow.user.clone();
    let key = s.accounts.key.clone();
    let epoch = flow.epoch;
    auth_run(&s, move |db| {
        if db.epoch(&user)? != epoch || !db.check_code(&user, &input.code, &key)? {
            return Err(AuthError::Invalid(
                "Code did not match. Sign in again to retry.".into(),
            ));
        }
        Ok(())
    })
    .await?;
    finish_login(&s, flow.user, flow.epoch, flow.next).await
}
#[derive(Deserialize)]
struct EmailInput {
    email: String,
}
async fn forgot(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<EmailInput>,
) -> Result<Json<Value>, ApiError> {
    same_origin(&s, &h)?;
    mail_available()?;
    throttle(&s, &input.email).await?;
    let address = input.email.clone();
    let token = auth_run(&s, move |db| db.reset_token(&input.email)).await?;
    if let Some(token) = token {
        send_mail(address,"Reset your Todoer password",format!("Choose a new password:\n\n{}/reset-password#token={}\n\nThis link expires in 30 minutes and can be used once. Your existing MFA is still required. If you did not request a reset, ignore this message.",s.origin,token)).await?;
    }
    Ok(Json(
        json!({"message":"If this account has a password, a reset link is on its way."}),
    ))
}
#[derive(Deserialize)]
struct ResetInput {
    token: String,
    password: String,
    #[serde(default)]
    code: String,
}
async fn reset(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<ResetInput>,
) -> Result<Json<Value>, ApiError> {
    same_origin(&s, &h)?;
    throttle(&s, "reset").await?;
    let encoded = password_hash(input.password).await?;
    let proof = s.accounts.flow(&h, true).ok().filter(|f| {
        f.verified && f.reset.as_deref() == Some(crate::auth::hash(&input.token).as_str())
    });
    let key = s.accounts.key.clone();
    auth_run(&s, move |db| {
        db.reset_password(
            &input.token,
            &encoded,
            &input.code,
            &key,
            proof.as_ref().map(|f| (f.user.as_str(), f.epoch)),
        )
    })
    .await?;
    Ok(Json(
        json!({"message":"Password updated. Sign in with your new password.","redirect":"/signin"}),
    ))
}
async fn status(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    let user = s.auth(&h, false)?;
    if user.starts_with("credential:") {
        return Err(AuthError::Forbidden.into());
    }
    auth_run(&s, move |db| {
        Ok(serde_json::to_value(db.security_status(&user)?).unwrap())
    })
    .await
    .map(Json)
}
#[derive(Deserialize)]
struct PasswordInput {
    password: String,
}
async fn set_password(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<PasswordInput>,
) -> Result<Json<Value>, ApiError> {
    let user = session_user(&s, &h, true).await?;
    let encoded = password_hash(input.password).await?;
    let session = cookie(&h, "todoer_session")
        .ok_or(AuthError::Unauthorized)?
        .to_owned();
    auth_change(&s, &h, user.clone(), move |db| {
        if db.recent(&session)? != user {
            return Err(AuthError::Forbidden);
        }
        db.set_password(&user, &encoded)
    })
    .await?;
    Ok(Json(
        json!({"redirect":"/signin","message":"Password saved. Sign in again."}),
    ))
}
async fn totp_start(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    let user = session_user(&s, &h, true).await?;
    let checked_user = user.clone();
    let checked = user.clone();
    let key = s.accounts.key.clone();
    let (seed, secret, url) = auth_run(&s, move |db| db.totp_setup(&checked, &key)).await?;
    let binding = crate::auth::hash(cookie(&h, "todoer_session").ok_or(AuthError::Unauthorized)?);
    let epoch = auth_run(&s, move |db| db.epoch(&user)).await?;
    let id = s.accounts.put_ceremony(Ceremony {
        binding,
        user: checked_user,
        epoch,
        expires: crate::auth::now() + 300,
        state: CeremonyState::Totp(seed),
    })?;
    Ok(Json(json!({"setup_id":id,"secret":secret,"url":url})))
}
#[derive(Deserialize)]
struct SetupInput {
    setup_id: String,
    code: String,
}
async fn totp_finish(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<SetupInput>,
) -> Result<Json<Value>, ApiError> {
    let user = session_user(&s, &h, true).await?;
    throttle(&s, &user).await?;
    let binding = crate::auth::hash(cookie(&h, "todoer_session").ok_or(AuthError::Unauthorized)?);
    let ceremony = s.accounts.take_ceremony(&input.setup_id, &binding)?;
    if ceremony.user != user {
        return Err(AuthError::Forbidden.into());
    }
    let CeremonyState::Totp(seed) = ceremony.state else {
        return Err(AuthError::Forbidden.into());
    };
    let key = s.accounts.key.clone();
    let codes = auth_change(&s, &h, user.clone(), move |db| {
        if db.epoch(&user)? != ceremony.epoch {
            return Err(AuthError::Unauthorized);
        }
        db.enable_totp(&user, &seed, &input.code, &key)
    })
    .await?;
    Ok(Json(json!({"recovery_codes":codes,"reauthenticate":true})))
}
async fn totp_disable(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    let user = session_user(&s, &h, true).await?;
    auth_change(&s, &h, user.clone(), move |db| db.disable_totp(&user)).await?;
    Ok(Json(json!({"redirect":"/signin"})))
}
async fn recovery(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    let user = session_user(&s, &h, true).await?;
    let codes = auth_change(&s, &h, user.clone(), move |db| db.recovery_codes(&user)).await?;
    Ok(Json(json!({"recovery_codes":codes})))
}
#[derive(Deserialize)]
struct KeyStartInput {
    #[serde(default)]
    label: String,
    #[serde(default)]
    email: String,
    reset_token: Option<String>,
}
async fn register_key(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<KeyStartInput>,
) -> Result<Json<Value>, ApiError> {
    let user = session_user(&s, &h, true).await?;
    let copy = user.clone();
    let (handle, email, keys, epoch) = auth_run(&s, move |db| {
        Ok((
            db.passkey_handle(&copy)?,
            db.account_email(&copy)?,
            db.passkeys(&copy)?,
            db.epoch(&copy)?,
        ))
    })
    .await?;
    let webauthn = s
        .accounts
        .webauthn
        .as_ref()
        .ok_or_else(|| problem("WebAuthn is unavailable for this origin"))?;
    let excludes = keys.iter().map(|k| k.cred_id().clone()).collect();
    let (options, state) = webauthn
        .start_passkey_registration(handle, &email, &email, Some(excludes))
        .map_err(|_| problem("Could not start passkey registration"))?;
    let binding = crate::auth::hash(cookie(&h, "todoer_session").ok_or(AuthError::Unauthorized)?);
    let id = s.accounts.put_ceremony(Ceremony {
        binding,
        user,
        epoch,
        expires: crate::auth::now() + 300,
        state: CeremonyState::Register(state, input.label),
    })?;
    Ok(Json(json!({"challenge_id":id,"options":options})))
}
#[derive(Deserialize)]
struct RegisterKeyFinish {
    challenge_id: String,
    credential: RegisterPublicKeyCredential,
}
async fn register_key_finish(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<RegisterKeyFinish>,
) -> Result<Json<Value>, ApiError> {
    let user = session_user(&s, &h, true).await?;
    let binding = crate::auth::hash(cookie(&h, "todoer_session").ok_or(AuthError::Unauthorized)?);
    let ceremony = s.accounts.take_ceremony(&input.challenge_id, &binding)?;
    if ceremony.user != user {
        return Err(AuthError::Forbidden.into());
    }
    let CeremonyState::Register(state, label) = ceremony.state else {
        return Err(AuthError::Forbidden.into());
    };
    let key = s
        .accounts
        .webauthn
        .as_ref()
        .ok_or(AuthError::Unauthorized)?
        .finish_passkey_registration(&input.credential, &state)
        .map_err(|_| problem("Passkey registration could not be verified"))?;
    let codes = auth_change(&s, &h, user.clone(), move |db| {
        if db.epoch(&user)? != ceremony.epoch {
            return Err(AuthError::Unauthorized);
        }
        db.save_passkey(&user, &label, &key)
    })
    .await?;
    Ok(Json(json!({"recovery_codes":codes,"reauthenticate":true})))
}
async fn key_authenticate(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<KeyStartInput>,
) -> Result<Response, ApiError> {
    same_origin(&s, &h)?;
    auth_run(&s, |db| db.throttle("passkey-global", 100)).await?;
    let flow = if let Some(token) = input.reset_token {
        let owner = token.clone();
        let (user, epoch) = auth_run(&s, move |db| {
            let u = db.reset_owner(&owner)?;
            let e = db.epoch(&u)?;
            Ok((u, e))
        })
        .await?;
        Flow {
            user,
            epoch,
            next: "/signin".into(),
            reset: Some(crate::auth::hash(&token)),
            verified: false,
            expires: crate::auth::now() + 300,
        }
    } else if let Ok(flow) = s.accounts.flow(&h, false) {
        if flow.reset.is_some() {
            return Err(AuthError::Forbidden.into());
        }
        flow
    } else {
        let email = input.email;
        let (user,epoch)=auth_run(&s,move|db|{let normalized=crate::auth::email(&email)?;let user=match db.password_record(&normalized)? {Some((u,_))=>u,None=>db.conn.query_row("SELECT user_id FROM auth_identities WHERE email=?1 ORDER BY user_id LIMIT 1",[normalized],|r|r.get(0)).map_err(|_|AuthError::Unauthorized)?};let epoch=db.epoch(&user)?;Ok((user,epoch))}).await?;
        Flow {
            user,
            epoch,
            next: "/today".into(),
            reset: None,
            verified: false,
            expires: crate::auth::now() + 300,
        }
    };
    throttle(&s, &flow.user).await?;
    let copy = flow.user.clone();
    let epoch = flow.epoch;
    let keys = auth_run(&s, move |db| {
        if db.epoch(&copy)? != epoch {
            return Err(AuthError::Unauthorized);
        }
        db.passkeys(&copy)
    })
    .await?;
    let (options, state) = s
        .accounts
        .webauthn
        .as_ref()
        .ok_or(AuthError::Unauthorized)?
        .start_passkey_authentication(&keys)
        .map_err(|_| problem("No matching passkey. Use another sign-in method."))?;
    let user = flow.user.clone();
    let token = s.accounts.put_flow(flow)?;
    let binding = crate::auth::hash(&token);
    let id = s.accounts.put_ceremony(Ceremony {
        binding,
        user,
        epoch,
        expires: crate::auth::now() + 300,
        state: CeremonyState::Authenticate(state),
    })?;
    let mut response = Json(json!({"challenge_id":id,"options":options})).into_response();
    add_cookie(&mut response, "todoer_account_flow", &token, 300);
    Ok(response)
}
#[derive(Deserialize)]
struct KeyAuthFinish {
    challenge_id: String,
    credential: PublicKeyCredential,
}
async fn key_auth_finish(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<KeyAuthFinish>,
) -> Result<Response, ApiError> {
    same_origin(&s, &h)?;
    let binding =
        crate::auth::hash(cookie(&h, "todoer_account_flow").ok_or(AuthError::Unauthorized)?);
    let ceremony = s.accounts.take_ceremony(&input.challenge_id, &binding)?;
    let flow = s.accounts.flow(&h, false)?;
    if flow.user != ceremony.user || flow.epoch != ceremony.epoch {
        return Err(AuthError::Unauthorized.into());
    }
    let CeremonyState::Authenticate(state) = ceremony.state else {
        return Err(AuthError::Forbidden.into());
    };
    let result = s
        .accounts
        .webauthn
        .as_ref()
        .ok_or(AuthError::Unauthorized)?
        .finish_passkey_authentication(&input.credential, &state)
        .map_err(|_| problem("Passkey could not be verified"))?;
    let user = flow.user.clone();
    let epoch = flow.epoch;
    auth_run(&s, move |db| {
        if db.epoch(&user)? != epoch {
            return Err(AuthError::Unauthorized);
        }
        db.update_passkeys(&user, &result)
    })
    .await?;
    if flow.reset.is_some() {
        let mut flows = s
            .accounts
            .flows
            .lock()
            .map_err(|_| AuthError::Unauthorized)?;
        let saved = flows.get_mut(&binding).ok_or(AuthError::Unauthorized)?;
        saved.verified = true;
        return Ok(Json(json!({"verified":true})).into_response());
    }
    s.accounts.flow(&h, true)?;
    finish_login(&s, flow.user, flow.epoch, flow.next).await
}
#[derive(Deserialize)]
struct RemoveKey {
    id: String,
}
async fn remove_key(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<RemoveKey>,
) -> Result<Json<Value>, ApiError> {
    let user = session_user(&s, &h, true).await?;
    auth_change(&s, &h, user.clone(), move |db| {
        db.remove_passkey(&user, &input.id)
    })
    .await?;
    Ok(Json(json!({"redirect":"/signin"})))
}
async fn get_policy(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    let user = s.auth(&h, false)?;
    if user.starts_with("credential:") {
        return Err(AuthError::Forbidden.into());
    }
    auth_run(&s, move |db| {
        if !account_security::is_admin(&db.conn, &user)? {
            return Err(AuthError::Forbidden);
        }
        Ok(serde_json::to_value(db.auth_policy()?).unwrap())
    })
    .await
    .map(Json)
}
async fn put_policy(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<AuthPolicy>,
) -> Result<Json<AuthPolicy>, ApiError> {
    let user = session_user(&s, &h, true).await?;
    let configured = s.production.as_ref().is_some_and(|a| a.oidc.is_some());
    auth_change(&s, &h, user.clone(), move |db| {
        db.policy_update(&user, input, configured)
    })
    .await
    .map(Json)
}
async fn security_page(State(s): State<AppState>, h: HeaderMap) -> Result<Response, ApiError> {
    let user = match s.auth(&h, false) {
        Ok(u) if !u.starts_with("credential:") => u,
        _ => return Ok(Redirect::to("/signin?next=%2Fsettings%2Fsecurity").into_response()),
    };
    let production = s.production.is_some();
    let admin = if production {
        auth_run(&s, move |db| account_security::is_admin(&db.conn, &user)).await?
    } else {
        false
    };
    let mut body = include_str!("../templates/account-security.html").to_string();
    if !admin {
        body = body.replace("data-auth-policy-panel", "data-auth-policy-panel hidden");
    }
    if !production {
        body = "<p>Account security is available on the real authentication instance.</p>".into();
    }
    scheduling_http::render_settings(&s, &h, "Account security", body, "security").await
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/local/register", post(register))
        .route("/auth/local/verify", post(verify))
        .route("/auth/local/login", post(login))
        .route("/auth/local/mfa", post(mfa_code))
        .route("/auth/local/forgot", post(forgot))
        .route("/auth/local/reset", post(reset))
        .route("/api/v1/account/security", get(status))
        .route("/api/v1/account/password", post(set_password))
        .route("/api/v1/account/totp/start", post(totp_start))
        .route("/api/v1/account/totp/finish", post(totp_finish))
        .route("/api/v1/account/totp/disable", post(totp_disable))
        .route("/api/v1/account/recovery-codes", post(recovery))
        .route("/api/v1/account/passkeys/start", post(register_key))
        .route("/api/v1/account/passkeys/finish", post(register_key_finish))
        .route("/api/v1/account/passkeys/remove", post(remove_key))
        .route("/auth/passkey/start", post(key_authenticate))
        .route("/auth/passkey/finish", post(key_auth_finish))
        .route("/api/v1/admin/auth-policy", get(get_policy).put(put_policy))
        .route("/settings/security", get(security_page))
}
