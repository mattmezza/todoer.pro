//! Session-bound Web Push. Payloads contain only generic text and an opaque link.
use crate::core::{Error, Store, actor_user, task};
use crate::scheduling::{Channel, ReminderJob};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;
use web_push::{ContentEncoding, SubscriptionInfo, VapidSignatureBuilder, WebPushMessageBuilder};

#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("invalid Web Push configuration or subscription")]
    Invalid,
    #[error("Web Push provider unavailable")]
    Unavailable,
}
#[derive(Clone)]
pub struct PushTransport {
    private_key: String,
    public_key: String,
    subject: String,
    extra_hosts: Vec<String>,
    #[cfg(test)]
    local_test: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceKeys {
    pub p256dh: String,
    pub auth: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterDevice {
    pub endpoint: String,
    pub keys: DeviceKeys,
    pub label: String,
}
#[derive(Debug, Serialize)]
pub struct Device {
    pub id: String,
    pub label: String,
    pub active: bool,
    pub session_live: bool,
    pub created_at: i64,
    pub endpoint_hash: String,
}
#[derive(Debug, Serialize)]
pub struct DeviceSettings {
    pub opt_in: bool,
    pub devices: Vec<Device>,
}
struct Delivery {
    device_id: String,
    device_revision: i64,
    token: String,
    endpoint: String,
    p256dh: String,
    auth: String,
}
#[derive(Debug, Default, Serialize)]
pub struct PushReport {
    pub sent: usize,
    pub deferred: usize,
    pub cancelled: usize,
    pub failed: usize,
}
enum SendResult {
    Sent,
    Gone,
    Retry(i64),
    Dead,
}
fn invalid(s: &str) -> Error {
    Error::Invalid(s.into())
}
fn human(c: &rusqlite::Connection, actor: &str) -> Result<String, Error> {
    if actor.starts_with("credential:") {
        return Err(Error::Forbidden);
    }
    actor_user(c, actor, 3)
}
fn live(c: &rusqlite::Connection, user: &str, session: &str, now: i64) -> Result<bool, Error> {
    Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM auth_sessions WHERE id=?1 AND user_id=?2 AND revoked=0 AND expires_at>?3)",params![session,user,now],|r|r.get(0))?)
}
fn valid_keys(keys: &DeviceKeys) -> bool {
    URL_SAFE_NO_PAD
        .decode(&keys.p256dh)
        .is_ok_and(|v| v.len() == 65 && v[0] == 4)
        && URL_SAFE_NO_PAD
            .decode(&keys.auth)
            .is_ok_and(|v| v.len() == 16)
}
fn host_name(host: &str) -> bool {
    host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|s| {
            !s.is_empty()
                && s.len() <= 63
                && !s.starts_with('-')
                && !s.ends_with('-')
                && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}
fn endpoint(value: &str, extra: &[String]) -> Result<reqwest::Url, PushError> {
    if value.len() > 4096 {
        return Err(PushError::Invalid);
    }
    let u = reqwest::Url::parse(value).map_err(|_| PushError::Invalid)?;
    let h = u.host_str().ok_or(PushError::Invalid)?;
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_some()
        || u.port_or_known_default() != Some(443)
        || !host_name(h)
        || h.parse::<IpAddr>().is_ok()
    {
        return Err(PushError::Invalid);
    }
    if h != "fcm.googleapis.com"
        && h != "updates.push.services.mozilla.com"
        && !h.ends_with(".push.apple.com")
        && !extra.iter().any(|x| x == h)
    {
        return Err(PushError::Invalid);
    }
    Ok(u)
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            !v.is_private()
                && !v.is_loopback()
                && !v.is_link_local()
                && !v.is_multicast()
                && !v.is_broadcast()
                && !v.is_unspecified()
                && !v.is_documentation()
                && v.octets()[0] != 0
                && v.octets()[0] < 224
                && !(v.octets()[0] == 100 && (64..=127).contains(&v.octets()[1]))
                && !(v.octets()[0] == 198 && (18..=19).contains(&v.octets()[1]))
                && !(v.octets()[0] == 192 && v.octets()[1] == 0 && v.octets()[2] == 0)
        }
        IpAddr::V6(v) => {
            (v.segments()[0] & 0xe000) == 0x2000
                && !v.is_unicast_link_local()
                && !v.is_unspecified()
                && !(v.segments()[0] == 0x2001
                    && (v.segments()[1] < 0x200 || v.segments()[1] == 0xdb8))
                && v.segments()[0] != 0x2002
        }
    }
}
impl PushTransport {
    pub fn from_env() -> Result<Option<Self>, PushError> {
        let key = std::env::var("TODOER_PUSH_VAPID_PRIVATE_KEY").ok();
        let subject = std::env::var("TODOER_PUSH_VAPID_SUBJECT").ok();
        if key.is_none() && subject.is_none() {
            return Ok(None);
        }
        let private_key = key.ok_or(PushError::Invalid)?;
        let subject = subject.ok_or(PushError::Invalid)?;
        if !(subject.starts_with("mailto:") && subject.len() > 7
            || reqwest::Url::parse(&subject)
                .is_ok_and(|u| u.scheme() == "https" && u.host_str().is_some()))
            || subject.len() > 500
            || subject.chars().any(char::is_control)
        {
            return Err(PushError::Invalid);
        }
        let builder = VapidSignatureBuilder::from_base64_no_sub(&private_key)
            .map_err(|_| PushError::Invalid)?;
        let public_key = URL_SAFE_NO_PAD.encode(builder.get_public_key());
        let extra_hosts = std::env::var("TODOER_PUSH_EXTRA_HOSTS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();
        if extra_hosts
            .iter()
            .any(|s| !host_name(s) || s.parse::<IpAddr>().is_ok())
        {
            return Err(PushError::Invalid);
        }
        Ok(Some(Self {
            private_key,
            public_key,
            subject,
            extra_hosts,
            #[cfg(test)]
            local_test: false,
        }))
    }
    #[cfg(test)]
    pub(crate) fn test_fixture() -> Self {
        let private_key = URL_SAFE_NO_PAD.encode([7u8; 32]);
        let public_key = URL_SAFE_NO_PAD.encode(
            VapidSignatureBuilder::from_base64_no_sub(&private_key)
                .unwrap()
                .get_public_key(),
        );
        Self {
            private_key,
            public_key,
            subject: "mailto:synthetic@example.invalid".into(),
            extra_hosts: vec![],
            local_test: false,
        }
    }
    pub fn public_key(&self) -> &str {
        &self.public_key
    }
    pub fn validate_subscription(&self, input: &RegisterDevice) -> Result<(), Error> {
        endpoint(&input.endpoint, &self.extra_hosts)
            .map_err(|_| invalid("unsupported push provider endpoint"))?;
        if !valid_keys(&input.keys) {
            return Err(invalid("invalid push encryption keys"));
        }
        Ok(())
    }
    fn encrypted(
        &self,
        d: &Delivery,
        job: &ReminderJob,
    ) -> Result<web_push::WebPushMessage, PushError> {
        let sub = SubscriptionInfo::new(d.endpoint.clone(), d.p256dh.clone(), d.auth.clone());
        let mut signature = VapidSignatureBuilder::from_base64(&self.private_key, &sub)
            .map_err(|_| PushError::Invalid)?;
        signature.add_claim("sub", self.subject.clone());
        signature.add_claim(
            "aud",
            reqwest::Url::parse(&d.endpoint)
                .map_err(|_| PushError::Invalid)?
                .origin()
                .ascii_serialization(),
        );
        let sig = signature.build().map_err(|_| PushError::Invalid)?;
        let payload=serde_json::to_vec(&serde_json::json!({"title":if inbox_job(job) {"Todoer notification"} else if digest_job(job) {"Todoer summary"} else {"Todoer reminder"},"url":if inbox_job(job) {"/notifications".into()} else if digest_job(job) {"/review".into()} else {format!("/tasks/{}",job.task_id)},"intent_id":format!("{}:{}",job.id,d.device_id)})).map_err(|_|PushError::Invalid)?;
        let mut builder = WebPushMessageBuilder::new(&sub);
        builder.set_payload(ContentEncoding::Aes128Gcm, &payload);
        builder.set_vapid_signature(sig);
        // Zero TTL prevents a provider from retaining a stale reminder after logout.
        builder.set_ttl(0);
        builder.set_topic(
            URL_SAFE_NO_PAD
                .encode(&Sha256::digest(format!("{}:{}", job.id, d.device_id).as_bytes())[..24]),
        );
        builder.build().map_err(|_| PushError::Invalid)
    }
    async fn send(
        &self,
        d: &Delivery,
        job: &ReminderJob,
        store: Option<&Arc<Mutex<Store>>>,
    ) -> Result<SendResult, PushError> {
        #[cfg(test)]
        let local = self.local_test;
        #[cfg(not(test))]
        let local = false;
        let u = if local {
            reqwest::Url::parse(&d.endpoint).map_err(|_| PushError::Invalid)?
        } else {
            endpoint(&d.endpoint, &self.extra_hosts)?
        };
        let host = u.host_str().ok_or(PushError::Invalid)?;
        let addresses = tokio::time::timeout(
            Duration::from_secs(3),
            tokio::net::lookup_host((host, u.port_or_known_default().ok_or(PushError::Invalid)?)),
        )
        .await
        .map_err(|_| PushError::Unavailable)?
        .map_err(|_| PushError::Unavailable)?
        .collect::<Vec<SocketAddr>>();
        if addresses.is_empty() || !local && addresses.iter().any(|a| !public_ip(a.ip())) {
            return Err(PushError::Invalid);
        }
        // Pin the validated DNS answer; disable redirects and environment proxies.
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(5))
            .resolve_to_addrs(host, &addresses)
            .build()
            .map_err(|_| PushError::Unavailable)?;
        let message = self.encrypted(d, job)?;
        let payload = message.payload.ok_or(PushError::Invalid)?;
        let mut request = client
            .post(u)
            .header("TTL", message.ttl)
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream");
        if let Some(topic) = message.topic {
            request = request.header("Topic", topic);
        }
        for (name, value) in payload.crypto_headers {
            request = request.header(name, value);
        }
        if let Some(store) = store
            && !locked(store, |s| {
                s.push_delivery_live(job, d, Utc::now().timestamp())
            })
            .map_err(|_| PushError::Unavailable)?
        {
            return Err(PushError::Unavailable);
        }
        let response = request
            .body(payload.content)
            .send()
            .await
            .map_err(|_| PushError::Unavailable)?;
        let status = response.status().as_u16();
        Ok(match status {
            200..=299 => SendResult::Sent,
            404 | 410 => SendResult::Gone,
            429 | 500..=599 => {
                let retry = response
                    .headers()
                    .get("retry-after")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|v| {
                        v.parse::<i64>().ok().or_else(|| {
                            chrono::DateTime::parse_from_rfc2822(v)
                                .ok()
                                .map(|t| t.timestamp() - Utc::now().timestamp())
                        })
                    })
                    .unwrap_or(60)
                    .clamp(1, 3600);
                SendResult::Retry(retry)
            }
            _ => SendResult::Dead,
        })
    }
}
impl Store {
    pub fn push_devices(&self, actor: &str) -> Result<DeviceSettings, Error> {
        let user = human(&self.conn, actor)?;
        let now = Utc::now().timestamp();
        let opt_in = self
            .conn
            .query_row(
                "SELECT opt_in FROM push_preferences WHERE user_id=?1",
                [&user],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(false);
        let mut q=self.conn.prepare("SELECT d.id,d.label,d.active,COALESCE(s.revoked=0 AND s.expires_at>?2,0),d.created_at,d.endpoint FROM push_devices d LEFT JOIN auth_sessions s ON s.id=d.session_id WHERE d.user_id=?1 ORDER BY d.created_at,d.id")?;
        let devices = q
            .query_map(params![user, now], |r| {
                Ok(Device {
                    id: r.get(0)?,
                    label: r.get(1)?,
                    active: r.get(2)?,
                    session_live: r.get(3)?,
                    created_at: r.get(4)?,
                    endpoint_hash: format!(
                        "{:x}",
                        Sha256::digest(r.get::<_, String>(5)?.as_bytes())
                    ),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DeviceSettings { opt_in, devices })
    }
    pub fn register_push_device(
        &mut self,
        actor: &str,
        session_id: &str,
        input: RegisterDevice,
    ) -> Result<Device, Error> {
        let now = Utc::now().timestamp();
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let user = human(&tx, actor)?;
        if !live(&tx, &user, session_id, now)? {
            return Err(Error::Forbidden);
        }
        if !valid_keys(&input.keys)
            || input.endpoint.len() > 4096
            || reqwest::Url::parse(&input.endpoint).is_err()
            || input.label.trim().is_empty()
            || input.label.chars().count() > 100
            || input.label.chars().any(char::is_control)
        {
            return Err(invalid("invalid device subscription"));
        }
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT id,user_id FROM push_devices WHERE endpoint=?1",
                [&input.endpoint],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if existing.as_ref().is_some_and(|(_, owner)| owner != &user) {
            return Err(Error::Conflict(
                "subscription belongs to another account; unsubscribe in this browser first".into(),
            ));
        }
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM push_devices WHERE user_id=?1 AND active=1 AND endpoint<>?2",
            params![user, input.endpoint],
            |r| r.get(0),
        )?;
        if count >= 10 {
            return Err(invalid("at most ten active push devices"));
        }
        let id = existing
            .map(|v| v.0)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        tx.execute("INSERT INTO push_devices(id,user_id,session_id,endpoint,p256dh,auth_secret,label,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8) ON CONFLICT(endpoint) DO UPDATE SET session_id=excluded.session_id,p256dh=excluded.p256dh,auth_secret=excluded.auth_secret,label=excluded.label,revision=push_devices.revision+1,active=1,updated_at=excluded.updated_at",params![id,user,session_id,input.endpoint,input.keys.p256dh,input.keys.auth,input.label.trim(),now])?;
        tx.execute("INSERT INTO push_preferences(user_id,opt_in) VALUES(?1,1) ON CONFLICT(user_id) DO UPDATE SET opt_in=1",[&user])?;
        let device = tx.query_row(
            "SELECT id,label,active,created_at,endpoint FROM push_devices WHERE id=?1",
            [id],
            |r| {
                Ok(Device {
                    id: r.get(0)?,
                    label: r.get(1)?,
                    active: r.get(2)?,
                    session_live: true,
                    created_at: r.get(3)?,
                    endpoint_hash: format!(
                        "{:x}",
                        Sha256::digest(r.get::<_, String>(4)?.as_bytes())
                    ),
                })
            },
        )?;
        tx.commit()?;
        Ok(device)
    }
    pub fn revoke_push_device(&mut self, actor: &str, id: &str) -> Result<(), Error> {
        let user = human(&self.conn, actor)?;
        if self.conn.execute(
            "UPDATE push_devices SET active=0,revision=revision+1 WHERE id=?1 AND user_id=?2",
            params![id, user],
        )? != 1
        {
            return Err(Error::NotFound);
        }
        Ok(())
    }
    pub fn set_push_opt_in(&mut self, actor: &str, opt_in: bool) -> Result<(), Error> {
        let user = human(&self.conn, actor)?;
        self.conn.execute("INSERT INTO push_preferences(user_id,opt_in) VALUES(?1,?2) ON CONFLICT(user_id) DO UPDATE SET opt_in=excluded.opt_in",params![user,opt_in])?;
        Ok(())
    }
    fn push_allowed(&self, job: &ReminderJob, now: i64) -> Result<bool, Error> {
        if inbox_job(job) {
            return self.inbox_push_live(&job.id, &job.recipient, &job.lease_owner, &job.lease_token, now);
        }
        if digest_job(job) {
            return self.push_digest_allowed(job, now);
        }
        if job.channel != Channel::Push
            || !self.reminder_eligible(&job.id, &job.lease_owner, &job.lease_token, now)?
        {
            return Ok(false);
        }
        let opted: bool = self.conn.query_row(
            "SELECT COALESCE((SELECT opt_in FROM push_preferences WHERE user_id=?1),0)",
            [&job.recipient],
            |r| r.get(0),
        )?;
        let prefs = self.notification_preferences(&job.recipient)?;
        Ok(opted
            && !prefs
                .list_opt_out
                .contains(&task(&self.conn, &job.task_id)?.list_id))
    }
    fn claim_push_delivery(
        &mut self,
        job: &ReminderJob,
        now: i64,
    ) -> Result<Option<Delivery>, Error> {
        if !self.push_allowed(job, now)? {
            return Ok(None);
        }
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        tx.execute(&format!("INSERT OR IGNORE INTO {}(reminder_id,device_id,available_at) SELECT ?1,d.id,?2 FROM push_devices d JOIN auth_sessions s ON s.id=d.session_id WHERE d.user_id=?3 AND d.active=1 AND s.revoked=0 AND s.expires_at>?2", delivery_table(job)),params![job.id,now,job.recipient])?;
        tx.execute(&format!("UPDATE {} SET status='dead',lease_token=NULL,lease_until=NULL WHERE reminder_id=?1 AND attempts>=5 AND status IN ('pending','leased') AND COALESCE(lease_until,0)<=?2", delivery_table(job)),params![job.id,now])?;
        let next:Option<(String,String,String,String,i64)>=tx.query_row(&format!("SELECT d.id,d.endpoint,d.p256dh,d.auth_secret,d.revision FROM {} p JOIN push_devices d ON d.id=p.device_id JOIN auth_sessions s ON s.id=d.session_id WHERE p.reminder_id=?1 AND p.attempts<5 AND p.available_at<=?2 AND (p.status='pending' OR (p.status='leased' AND p.lease_until<=?2)) AND d.active=1 AND s.revoked=0 AND s.expires_at>?2 ORDER BY d.id LIMIT 1", delivery_table(job)),params![job.id,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        let result = if let Some((device_id, endpoint, p256dh, auth, device_revision)) = next {
            let token = Uuid::new_v4().to_string();
            tx.execute(&format!("UPDATE {} SET status='leased',attempts=attempts+1,lease_token=?3,lease_until=?4 WHERE reminder_id=?1 AND device_id=?2", delivery_table(job)),params![job.id,device_id,token,now+10])?;
            Some(Delivery {
                device_id,
                device_revision,
                token,
                endpoint,
                p256dh,
                auth,
            })
        } else {
            None
        };
        tx.commit()?;
        Ok(result)
    }
    fn push_delivery_live(&self, job: &ReminderJob, d: &Delivery, now: i64) -> Result<bool, Error> {
        Ok(crate::notifications::quiet_until(&self.notification_preferences(&job.recipient)?,now)?.is_none() && self.push_allowed(job,now)? && self.conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {} p JOIN push_devices d ON d.id=p.device_id JOIN auth_sessions s ON s.id=d.session_id WHERE p.reminder_id=?1 AND p.device_id=?2 AND p.lease_token=?3 AND p.status='leased' AND p.lease_until>?4 AND d.active=1 AND d.revision=?5 AND s.revoked=0 AND s.expires_at>?4)", delivery_table(job)),params![job.id,d.device_id,d.token,now,d.device_revision],|r|r.get(0))?)
    }
    fn finish_push(
        &mut self,
        job: &ReminderJob,
        d: &Delivery,
        result: SendResult,
        now: i64,
    ) -> Result<(), Error> {
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        let owned:bool=tx.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {} WHERE reminder_id=?1 AND device_id=?2 AND lease_token=?3 AND status='leased' AND lease_until>?4)", delivery_table(job)),params![job.id,d.device_id,d.token,now],|r|r.get(0))?;
        if !owned {
            return Err(Error::Conflict("push delivery lease expired".into()));
        }
        let current: i64 = tx.query_row(
            "SELECT revision FROM push_devices WHERE id=?1",
            [&d.device_id],
            |r| r.get(0),
        )?;
        if current != d.device_revision {
            tx.execute(&format!("UPDATE {} SET status='pending',attempts=MAX(0,attempts-1),available_at=?4,lease_token=NULL,lease_until=NULL WHERE reminder_id=?1 AND device_id=?2 AND lease_token=?3", delivery_table(job)),params![job.id,d.device_id,d.token,now])?;
            tx.commit()?;
            return Ok(());
        }
        let (status, delay, error) = match result {
            SendResult::Sent => ("sent", 0, None),
            SendResult::Gone => {
                tx.execute(
                    "UPDATE push_devices SET active=0,revision=revision+1 WHERE id=?1 AND revision=?2",
                    params![d.device_id,d.device_revision],
                )?;
                ("dead", 0, Some("subscription expired"))
            }
            SendResult::Retry(n) => ("pending", n, Some("provider temporarily unavailable")),
            SendResult::Dead => ("dead", 0, Some("provider rejected delivery")),
        };
        tx.execute(&format!("UPDATE {} SET status=?4,available_at=?5,lease_token=NULL,lease_until=NULL,last_error=?6 WHERE reminder_id=?1 AND device_id=?2 AND lease_token=?3 AND status='leased' AND lease_until>?7", delivery_table(job)),params![job.id,d.device_id,d.token,status,now+delay,error,now])?;
        tx.commit()?;
        Ok(())
    }
    fn push_pending(&self, job: &ReminderJob, now: i64) -> Result<Option<i64>, Error> {
        Ok(self.conn.query_row(&format!("SELECT MIN(MAX(p.available_at,COALESCE(p.lease_until,0))) FROM {} p JOIN push_devices d ON d.id=p.device_id JOIN auth_sessions s ON s.id=d.session_id WHERE p.reminder_id=?1 AND p.status IN ('pending','leased') AND d.active=1 AND s.revoked=0 AND s.expires_at>?2", delivery_table(job)),params![job.id,now],|r|r.get(0))?)
    }
    fn defer_push(&mut self, job: &ReminderJob, at: Option<i64>, now: i64) -> Result<(), Error> {
        self.conn.execute(&format!("UPDATE {} SET status=?4,available_at=?5,attempts=MAX(0,attempts-1),lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE id=?1 AND status='leased' AND lease_owner=?2 AND lease_token=?3 AND lease_until>?6", parent_table(job)),params![job.id,job.lease_owner,job.lease_token,if at.is_some(){"pending"}else{"cancelled"},at.unwrap_or(now),now])?;
        Ok(())
    }
}
// Digest jobs use the existing private transport envelope, with a namespaced
// outbox ID and no task ID. No reminder row or synthetic task is persisted.
fn inbox_job(job: &ReminderJob) -> bool { job.id.starts_with("inbox:") }
fn digest_job(job: &ReminderJob) -> bool {
    job.id.starts_with("push-digest:")
}
fn delivery_table(job: &ReminderJob) -> &'static str {
    if inbox_job(job) { "notification_push_deliveries" } else if digest_job(job) {
        "push_digest_deliveries"
    } else {
        "push_deliveries"
    }
}
fn parent_table(job: &ReminderJob) -> &'static str {
    if inbox_job(job) { "notification_push_outbox" } else if digest_job(job) {
        "push_digest_outbox"
    } else {
        "reminder_outbox"
    }
}
impl Store {
    fn claim_inbox_push(&mut self, owner:&str, now:i64)->Result<Option<ReminderJob>,Error>{
        let tx=crate::agent_ops::begin_atomic(&mut self.conn)?;
        tx.execute("UPDATE notification_push_outbox SET status='dead',lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE attempts>=5 AND (status='pending' OR status='leased' AND lease_until<=?1)",[now])?;
        let row:Option<(String,String,String)>=tx.query_row("SELECT o.id,o.user_id,n.task_id FROM notification_push_outbox o JOIN notification_inbox n ON n.id=o.notification_id WHERE o.attempts<5 AND o.available_at<=?1 AND (o.status='pending' OR o.status='leased' AND o.lease_until<=?1) ORDER BY o.available_at,o.id LIMIT 1",[now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((id,recipient,task_id))=row else {tx.commit()?;return Ok(None)};
        let token=Uuid::new_v4().to_string();
        tx.execute("UPDATE notification_push_outbox SET status='leased',attempts=attempts+1,lease_owner=?2,lease_token=?3,lease_until=?4 WHERE id=?1",params![id,owner,token,now+60])?;
        tx.commit()?;
        Ok(Some(ReminderJob{id,recipient,task_id,schedule_revision:0,channel:Channel::Push,at:now,attempts:0,lease_owner:owner.into(),lease_token:token,lease_until:now+60}))
    }
    fn digest_clock(
        &self,
        job: &ReminderJob,
        now: i64,
    ) -> Result<crate::notifications::DigestTiming, Error> {
        let (kind, period): (String, String) = self.conn.query_row(
            "SELECT kind,period FROM push_digest_outbox WHERE id=?1",
            [&job.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        crate::notifications::digest_timing(
            &self.notification_preferences(&job.recipient)?,
            &kind,
            &period,
            now,
        )
    }
    fn push_digest_allowed(&self, job: &ReminderJob, now: i64) -> Result<bool, Error> {
        let row: Option<(String,String)> = self.conn.query_row("SELECT kind,period FROM push_digest_outbox WHERE id=?1 AND user_id=?2 AND status='leased' AND lease_owner=?3 AND lease_token=?4 AND lease_until>?5",params![job.id,job.recipient,job.lease_owner,job.lease_token,now],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let Some((kind, period)) = row else {
            return Ok(false);
        };
        let prefs = self.notification_preferences(&job.recipient)?;
        if !(if kind == "daily" {
            prefs.push_daily_opt_in
        } else {
            prefs.push_weekly_opt_in
        }) {
            return Ok(false);
        }
        if crate::notifications::digest_timing(&prefs, &kind, &period, now)?
            != crate::notifications::DigestTiming::Ready
        {
            return Ok(false);
        }
        Ok(self.conn.query_row("SELECT EXISTS(SELECT 1 FROM push_preferences WHERE user_id=?1 AND opt_in=1) AND EXISTS(SELECT 1 FROM push_devices d JOIN auth_sessions s ON s.id=d.session_id WHERE d.user_id=?1 AND d.active=1 AND s.revoked=0 AND s.expires_at>?3) AND EXISTS(SELECT 1 FROM actor_list_access a JOIN lists l ON l.id=a.list_id WHERE a.actor_id=?1 AND a.rank>=1 AND l.archived=0 AND l.id NOT IN(SELECT value FROM json_each(?2)))",params![job.recipient,serde_json::to_string(&prefs.list_opt_out).unwrap(),now],|r|r.get(0))?)
    }
    fn claim_push_digest(&mut self, owner: &str, now: i64) -> Result<Option<ReminderJob>, Error> {
        Ok(self
            .claim_digest_channel(owner, now, true)?
            .map(|d| ReminderJob {
                id: d.id,
                recipient: d.user,
                task_id: String::new(),
                schedule_revision: 0,
                channel: Channel::Push,
                at: now,
                attempts: 0,
                lease_owner: d.owner,
                lease_token: d.token,
                lease_until: now + 60,
            }))
    }
}
fn locked<T>(
    store: &Arc<Mutex<Store>>,
    f: impl FnOnce(&mut Store) -> Result<T, Error>,
) -> Result<T, Error> {
    let mut guard = store.lock().map_err(|_| invalid("store unavailable"))?;
    f(&mut guard)
}
pub async fn worker_tick(
    store: &Arc<Mutex<Store>>,
    transport: &PushTransport,
) -> Result<PushReport, Error> {
    let store = Arc::clone(store);
    let transport = transport.clone();
    // SQLite and its mutex run off the async executor, including authorization
    // checks immediately before sending. No mutex is held across network I/O.
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(worker_tick_inner(&store, &transport))
    })
    .await
    .map_err(|_| invalid("push worker unavailable"))?
}
async fn worker_tick_inner(
    store: &Arc<Mutex<Store>>,
    transport: &PushTransport,
) -> Result<PushReport, Error> {
    let owner = format!("push:{}", Uuid::new_v4());
    let now = Utc::now().timestamp();
    let jobs = locked(store, |s| {
        s.queue_push_digests(now)?;
        let mut jobs = s.claim_reminders_channel(&owner, now, 1, Channel::Push)?;
        if let Some(notice) = s.claim_inbox_push(&owner, now)? { jobs.push(notice); }
        if let Some(digest) = s.claim_push_digest(&owner, now)? {
            jobs.push(digest);
        }
        Ok(jobs)
    })?;
    let mut report = PushReport::default();
    for job in jobs {
        let now = Utc::now().timestamp();
        let prefs = locked(store, |s| s.notification_preferences(&job.recipient))?;
        if let Some(at) = crate::notifications::quiet_until(&prefs, now)? {
            locked(store, |s| s.defer_push(&job, Some(at), now))?;
            report.deferred += 1;
            continue;
        }
        if digest_job(&job)
            && let crate::notifications::DigestTiming::Defer(at) =
                locked(store, |s| s.digest_clock(&job, now))?
        {
            locked(store, |s| s.defer_push(&job, Some(at), now))?;
            report.deferred += 1;
            continue;
        }
        if !locked(store, |s| s.push_allowed(&job, now))? {
            locked(store, |s| s.defer_push(&job, None, now))?;
            report.cancelled += 1;
            continue;
        }
        // Two channel jobs share a bounded tick. Ten seconds per job leaves room
        // for DNS/HTTP and shutdown within the sixty-second parent leases.
        let started = std::time::Instant::now();
        for _ in 0..10 {
            if started.elapsed() >= Duration::from_secs(10) {
                break;
            }
            let Some(d) = locked(store, |s| {
                s.claim_push_delivery(&job, Utc::now().timestamp())
            })?
            else {
                break;
            };
            if !locked(store, |s| {
                s.push_delivery_live(&job, &d, Utc::now().timestamp())
            })? {
                break;
            }
            let result = transport
                .send(&d, &job, Some(store))
                .await
                .unwrap_or(SendResult::Retry(60));
            match &result {
                SendResult::Sent => report.sent += 1,
                SendResult::Retry(_) => report.failed += 1,
                _ => {}
            }
            locked(store, |s| {
                s.finish_push(&job, &d, result, Utc::now().timestamp())
            })?;
        }
        let now = Utc::now().timestamp();
        let wait = locked(store, |s| {
            if digest_job(&job) {
                Ok(match s.digest_clock(&job, now)? {
                    crate::notifications::DigestTiming::Defer(at) => Some(at),
                    _ => None,
                })
            } else {
                crate::notifications::quiet_until(&s.notification_preferences(&job.recipient)?, now)
            }
        })?;
        if let Some(at) = wait {
            locked(store, |s| s.defer_push(&job, Some(at), now))?;
            report.deferred += 1;
        } else if !locked(store, |s| s.push_allowed(&job, now))? {
            locked(store, |s| s.defer_push(&job, None, now))?;
            report.cancelled += 1;
        } else if let Some(at) = locked(store, |s| s.push_pending(&job, now))? {
            locked(store, |s| s.defer_push(&job, Some(at.max(now + 1)), now))?;
            report.deferred += 1;
        } else {
            locked(store, |s| {
                if inbox_job(&job) || digest_job(&job) {
                    s.conn.execute(&format!("UPDATE {} SET status='sent',lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE id=?1 AND status='leased' AND lease_owner=?2 AND lease_token=?3 AND lease_until>?4",parent_table(&job)),params![job.id,job.lease_owner,job.lease_token,now])?;
                    Ok(())
                } else {
                    s.acknowledge_reminder(&job.id, &job.lease_owner, &job.lease_token, now)
                }
            })?;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::{Capture, NewTask},
        scheduling::{Moment, Reminder, Schedule},
    };
    fn config() -> PushTransport {
        let private_key = URL_SAFE_NO_PAD.encode([7u8; 32]);
        let public_key = URL_SAFE_NO_PAD.encode(
            VapidSignatureBuilder::from_base64_no_sub(&private_key)
                .unwrap()
                .get_public_key(),
        );
        PushTransport {
            private_key,
            public_key,
            subject: "mailto:synthetic@example.invalid".into(),
            extra_hosts: vec![],
            local_test: true,
        }
    }
    fn input(endpoint: &str) -> RegisterDevice {
        let (key, auth) = ece::generate_keypair_and_auth_secret().unwrap();
        RegisterDevice {
            endpoint: endpoint.into(),
            keys: DeviceKeys {
                p256dh: URL_SAFE_NO_PAD.encode(key.pub_as_raw().unwrap()),
                auth: URL_SAFE_NO_PAD.encode(auth),
            },
            label: "Synthetic browser".into(),
        }
    }
    fn fixture() -> (Store, ReminderJob, String) {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        let now = Utc::now().timestamp();
        s.conn.execute("INSERT INTO auth_sessions(id,user_id,token_hash,csrf_hash,expires_at) VALUES('synthetic-session','dev','synthetic-hash','synthetic-csrf',?1)",[now+3600]).unwrap();
        let t = s
            .capture(
                "dev",
                "synthetic-push-task",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![NewTask {
                        title: "Private title must not enter payload".into(),
                        description: "Private body".into(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let local = chrono::DateTime::from_timestamp(now - 1, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        s.set_schedule(
            "dev",
            &t.id,
            t.revision,
            Schedule {
                reminders: vec![Reminder {
                    channel: Channel::Push,
                    at: Moment::Timed {
                        local,
                        timezone: "UTC".into(),
                    },
                }],
                ..Default::default()
            },
        )
        .unwrap();
        let d = s
            .register_push_device(
                "dev",
                "synthetic-session",
                input("https://fcm.googleapis.com/synthetic"),
            )
            .unwrap();
        let job = s
            .claim_reminders_channel("synthetic-worker", now, 1, Channel::Push)
            .unwrap()
            .remove(0);
        (s, job, d.id)
    }
    #[test]
    fn inbox_push_reuses_leases_and_rechecks_read_state_and_access() {
        let (mut s, reminder, _) = fixture();
        let now=Utc::now().timestamp();
        let t=task(&s.conn,&reminder.task_id).unwrap();
        s.set_schedule("dev",&t.id,t.revision,Schedule{due:Some(Moment::Timed{local:chrono::DateTime::from_timestamp(now-1,0).unwrap().format("%Y-%m-%dT%H:%M:%S").to_string(),timezone:"UTC".into()}),..Default::default()}).unwrap();
        s.queue_inbox_notifications(now).unwrap();s.queue_inbox_notifications(now).unwrap();
        let job=s.claim_inbox_push("worker",now).unwrap().unwrap();
        assert!(s.claim_inbox_push("other-worker",now).unwrap().is_none());
        assert!(s.push_allowed(&job,now).unwrap());
        s.conn.execute("UPDATE notification_push_outbox SET lease_token='replacement' WHERE id=?1",[&job.id]).unwrap();
        assert!(!s.push_allowed(&job,now).unwrap());
        s.conn.execute("UPDATE notification_push_outbox SET lease_token=?2 WHERE id=?1",params![job.id,job.lease_token]).unwrap();
        let delivery=s.claim_push_delivery(&job,now).unwrap().unwrap();
        assert!(s.push_delivery_live(&job,&delivery,now).unwrap());
        let inbox=s.notification_inbox("dev",None,false,25).unwrap();
        s.read_notifications("dev",inbox.latest_id,None).unwrap();
        assert!(!s.push_delivery_live(&job,&delivery,now).unwrap());
        s.conn.execute("UPDATE notification_inbox SET read_at=NULL",[]).unwrap();
        assert!(s.push_allowed(&job,now).unwrap());
        s.conn.execute("DELETE FROM memberships WHERE list_id='inbox' AND user_id='dev'",[]).unwrap();
        assert!(!s.push_allowed(&job,now).unwrap());
    }
    #[test]
    fn push_endpoint_and_keys_reject_ssrf() {
        for u in [
            "http://fcm.googleapis.com/a",
            "https://127.0.0.1/a",
            "https://fcm.googleapis.com.evil.invalid/a",
            "https://fcm.googleapis.com:8443/a",
            "https://user@fcm.googleapis.com/a",
            "https://updates.push.services.mozilla.com/a#secret",
            "https://localhost/a",
        ] {
            assert!(endpoint(u, &[]).is_err(), "{u}");
        }
        for u in [
            "https://fcm.googleapis.com/a",
            "https://updates.push.services.mozilla.com/wpush/v2/a",
            "https://web.push.apple.com/a",
        ] {
            assert!(endpoint(u, &[]).is_ok());
        }
        assert!(
            endpoint(
                "https://custom.example.invalid/a",
                &["custom.example.invalid".into()]
            )
            .is_ok()
        );
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "2001:db8::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()));
        }
        assert!(!valid_keys(&DeviceKeys {
            p256dh: "bad".into(),
            auth: "bad".into()
        }));
    }
    #[test]
    fn session_scope_live_revocation_and_opt_out() {
        let (mut s, job, id) = fixture();
        let now = Utc::now().timestamp();
        assert!(matches!(
            s.register_push_device("dev", "absent", input("https://fcm.googleapis.com/other")),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            s.register_push_device(
                "credential:absent",
                "synthetic-session",
                input("https://fcm.googleapis.com/other")
            ),
            Err(Error::Forbidden)
        ));
        assert!(s.push_allowed(&job, now).unwrap());
        let d = s.claim_push_delivery(&job, now).unwrap().unwrap();
        assert_eq!(d.device_id, id);
        assert!(s.push_delivery_live(&job, &d, now).unwrap());
        s.conn
            .execute("UPDATE auth_sessions SET expires_at=0", [])
            .unwrap();
        assert!(!s.push_delivery_live(&job, &d, now).unwrap());
        s.conn
            .execute("UPDATE auth_sessions SET expires_at=?1", [now + 3600])
            .unwrap();
        s.conn
            .execute("UPDATE auth_sessions SET revoked=1", [])
            .unwrap();
        assert!(!s.push_delivery_live(&job, &d, now).unwrap());
        assert!(!s.push_devices("dev").unwrap().devices[0].session_live);
        s.conn
            .execute("UPDATE auth_sessions SET revoked=0", [])
            .unwrap();
        s.set_push_opt_in("dev", false).unwrap();
        assert!(!s.push_delivery_live(&job, &d, now).unwrap());
        s.set_push_opt_in("dev", true).unwrap();
        s.revoke_push_device("dev", &id).unwrap();
        assert!(!s.push_delivery_live(&job, &d, now).unwrap());
        assert!(matches!(
            s.revoke_push_device("other", &id),
            Err(Error::Forbidden) | Err(Error::NotFound)
        ));
    }
    #[test]
    fn device_delivery_lease_retries_do_not_repeat_success() {
        let (mut s, job, _) = fixture();
        let now = Utc::now().timestamp();
        let first = s.claim_push_delivery(&job, now).unwrap().unwrap();
        assert!(s.claim_push_delivery(&job, now).unwrap().is_none());
        let expired = s.claim_push_delivery(&job, now + 11).unwrap().unwrap();
        assert_ne!(first.token, expired.token);
        assert!(
            s.finish_push(&job, &first, SendResult::Gone, now + 11)
                .is_err()
        );
        assert!(s.push_devices("dev").unwrap().devices[0].active);
        s.finish_push(&job, &expired, SendResult::Sent, now + 11)
            .unwrap();
        assert!(s.claim_push_delivery(&job, now + 12).unwrap().is_none());
        assert!(s.push_pending(&job, now + 12).unwrap().is_none());
        assert_eq!(
            s.conn
                .query_row("SELECT count(*) FROM push_deliveries", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn reenabling_device_fences_old_delivery_completion() {
        let (mut s, job, id) = fixture();
        let now = Utc::now().timestamp();
        let d = s.claim_push_delivery(&job, now).unwrap().unwrap();
        let renewed = s
            .register_push_device(
                "dev",
                "synthetic-session",
                input("https://fcm.googleapis.com/synthetic"),
            )
            .unwrap();
        assert_eq!(renewed.id, id);
        assert!(!s.push_delivery_live(&job, &d, now).unwrap());
        s.finish_push(&job, &d, SendResult::Gone, now).unwrap();
        assert!(s.push_devices("dev").unwrap().devices[0].active);
        assert!(s.claim_push_delivery(&job, now).unwrap().is_some());
    }

    #[test]
    fn push_digests_are_period_unique_channel_independent_and_live_authorized() {
        use crate::notifications::{DigestTiming, Preferences, digest_timing};
        use chrono::Datelike;
        let (mut s, _, device) = fixture();
        let now = Utc::now().timestamp();
        let prefs = Preferences {
            push_daily_opt_in: true,
            push_weekly_opt_in: true,
            daily_time: "00:00".into(),
            weekly_time: "00:00".into(),
            weekly_day: Utc::now().weekday().number_from_monday(),
            ..Default::default()
        };
        s.set_notification_preferences("dev", prefs.clone())
            .unwrap();
        assert_eq!(s.queue_digests(now).unwrap(), 0); // Email remains opted out.
        assert_eq!(s.queue_push_digests(now).unwrap(), 2);
        assert_eq!(s.queue_push_digests(now).unwrap(), 0);
        let job = s.claim_push_digest("digest-worker", now).unwrap().unwrap();
        assert!(s.push_allowed(&job, now).unwrap());
        let delivery = s.claim_push_delivery(&job, now).unwrap().unwrap();
        assert!(s.push_delivery_live(&job, &delivery, now).unwrap());
        s.revoke_push_device("dev", &device).unwrap();
        assert!(!s.push_delivery_live(&job, &delivery, now).unwrap());
        s.register_push_device(
            "dev",
            "synthetic-session",
            input("https://fcm.googleapis.com/synthetic"),
        )
        .unwrap();
        assert!(s.push_allowed(&job, now).unwrap());
        let mut muted = prefs.clone();
        muted.list_opt_out = s.lists("dev").unwrap().into_iter().map(|l| l.id).collect();
        s.set_notification_preferences("dev", muted).unwrap();
        assert!(!s.push_allowed(&job, now).unwrap());
        s.set_notification_preferences("dev", prefs.clone())
            .unwrap();
        s.conn
            .execute("DELETE FROM memberships WHERE user_id='dev'", [])
            .unwrap();
        s.conn
            .execute("DELETE FROM workspace_members WHERE user_id='dev'", [])
            .unwrap();
        assert!(!s.push_allowed(&job, now).unwrap());
        let monday = chrono::DateTime::parse_from_rfc3339("2026-10-26T12:00:00Z")
            .unwrap()
            .timestamp();
        let prefs = Preferences {
            daily_time: "13:00".into(),
            ..Default::default()
        };
        assert!(matches!(
            digest_timing(&prefs, "daily", "2026-10-26", monday).unwrap(),
            DigestTiming::Defer(_)
        ));
        assert_eq!(
            digest_timing(&Preferences::default(), "daily", "2026-10-25", monday).unwrap(),
            DigestTiming::Cancel
        );
    }
    #[tokio::test]
    async fn partial_http_failure_retries_only_failed_device_and_gone_disables() {
        partial_http_case(false).await;
    }
    #[tokio::test]
    async fn digest_partial_http_failure_does_not_replay_successful_device() {
        partial_http_case(true).await;
    }
    async fn partial_http_case(digest: bool) {
        use axum::{Router, routing::post};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let good = Arc::new(AtomicUsize::new(0));
        let retry = Arc::new(AtomicUsize::new(0));
        let g = good.clone();
        let r = retry.clone();
        let app = Router::new()
            .route(
                "/good",
                post(move || {
                    let g = g.clone();
                    async move {
                        g.fetch_add(1, Ordering::SeqCst);
                        axum::http::StatusCode::CREATED
                    }
                }),
            )
            .route(
                "/retry",
                post(move || {
                    let r = r.clone();
                    async move {
                        if r.fetch_add(1, Ordering::SeqCst) == 0 {
                            (
                                axum::http::StatusCode::TOO_MANY_REQUESTS,
                                [("retry-after", "120")],
                            )
                        } else {
                            (axum::http::StatusCode::CREATED, [("retry-after", "0")])
                        }
                    }
                }),
            )
            .route("/gone", post(|| async { axum::http::StatusCode::GONE }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (mut s, reminder, id) = fixture();
        let job = if digest {
            s.conn
                .execute("UPDATE reminder_outbox SET status='cancelled'", [])
                .unwrap();
            let now = Utc::now().timestamp();
            s.set_notification_preferences(
                "dev",
                crate::notifications::Preferences {
                    push_daily_opt_in: true,
                    daily_time: "00:00".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            s.queue_push_digests(now).unwrap();
            s.claim_push_digest("fixture", now).unwrap().unwrap()
        } else {
            reminder
        };
        let deliveries = delivery_table(&job);
        let outbox = parent_table(&job);
        // Release fixture lease for the actual worker and seed synthetic local
        // endpoints directly; production registration never permits HTTP.
        s.conn.execute(&format!("UPDATE {outbox} SET status='pending',lease_owner=NULL,lease_token=NULL,lease_until=NULL WHERE id=?1"),[&job.id]).unwrap();
        s.conn
            .execute(
                "UPDATE push_devices SET endpoint=?1 WHERE id=?2",
                params![format!("{base}/good"), id],
            )
            .unwrap();
        let retry_id = s
            .register_push_device(
                "dev",
                "synthetic-session",
                input("https://fcm.googleapis.com/retry"),
            )
            .unwrap()
            .id;
        let gone_id = s
            .register_push_device(
                "dev",
                "synthetic-session",
                input("https://fcm.googleapis.com/gone"),
            )
            .unwrap()
            .id;
        s.conn
            .execute(
                "UPDATE push_devices SET endpoint=?1 WHERE id=?2",
                params![format!("{base}/retry"), retry_id],
            )
            .unwrap();
        s.conn
            .execute(
                "UPDATE push_devices SET endpoint=?1 WHERE id=?2",
                params![format!("{base}/gone"), gone_id],
            )
            .unwrap();
        let store = Arc::new(Mutex::new(s));
        let before = Utc::now().timestamp();
        let first = worker_tick(&store, &config()).await.unwrap();
        assert_eq!(first.sent, 1);
        assert_eq!(first.failed, 1);
        assert_eq!(first.deferred, 1);
        {
            let s = store.lock().unwrap();
            let retry_at: i64 = s
                .conn
                .query_row(
                    &format!("SELECT available_at FROM {deliveries} WHERE device_id=?1"),
                    [retry_id],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(retry_at >= before + 120);
            let active: bool = s
                .conn
                .query_row(
                    "SELECT active FROM push_devices WHERE id=?1",
                    [gone_id],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(!active);
            s.conn
                .execute(
                    &format!("UPDATE {deliveries} SET available_at=0 WHERE status='pending'"),
                    [],
                )
                .unwrap();
            s.conn
                .execute(
                    &format!("UPDATE {outbox} SET available_at=0 WHERE id=?1"),
                    [&job.id],
                )
                .unwrap();
        }
        let second = worker_tick(&store, &config()).await.unwrap();
        assert_eq!(second.sent, 1);
        assert_eq!(second.failed, 0);
        assert_eq!(good.load(Ordering::SeqCst), 1);
        assert_eq!(retry.load(Ordering::SeqCst), 2);
        assert_eq!(
            store
                .lock()
                .unwrap()
                .conn
                .query_row(
                    &format!("SELECT status FROM {outbox} WHERE id=?1"),
                    [&job.id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            if digest { "sent" } else { "delivered" }
        );
        server.abort();
    }

    #[tokio::test]
    async fn actual_http_encrypted_payload_and_vapid_headers() {
        use axum::{Router, body::Bytes, http::HeaderMap, routing::post};
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let app = Router::new().route(
            "/push",
            post(move |h: HeaderMap, b: Bytes| {
                let tx = tx.clone();
                async move {
                    tx.send((h, b)).await.unwrap();
                    axum::http::StatusCode::CREATED
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/push", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (key, auth) = ece::generate_keypair_and_auth_secret().unwrap();
        let (_, job, _) = fixture();
        let d = Delivery {
            device_id: Uuid::new_v4().to_string(),
            device_revision: 1,
            token: "synthetic".into(),
            endpoint: url,
            p256dh: URL_SAFE_NO_PAD.encode(key.pub_as_raw().unwrap()),
            auth: URL_SAFE_NO_PAD.encode(auth),
        };
        assert!(matches!(
            config().send(&d, &job, None).await.unwrap(),
            SendResult::Sent
        ));
        let (headers, bytes) = rx.recv().await.unwrap();
        assert_eq!(headers["content-encoding"], "aes128gcm");
        assert_eq!(headers["ttl"], "0");
        let authorization = headers["authorization"].to_str().unwrap();
        assert!(authorization.starts_with("vapid t="));
        assert!(authorization.contains(", k="));
        let jwt = authorization
            .strip_prefix("vapid t=")
            .unwrap()
            .split(',')
            .next()
            .unwrap();
        let claims: serde_json::Value = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(jwt.split('.').nth(1).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(claims["sub"], "mailto:synthetic@example.invalid");
        assert!(
            claims["aud"]
                .as_str()
                .unwrap()
                .starts_with("http://127.0.0.1:")
        );
        let plain = ece::decrypt(&key.raw_components().unwrap(), &auth, &bytes).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&plain).unwrap();
        assert_eq!(value["title"], "Todoer reminder");
        assert_eq!(value["url"], format!("/tasks/{}", job.task_id));
        assert_eq!(value.as_object().unwrap().len(), 3);
        assert!(!String::from_utf8(plain).unwrap().contains("Private"));
        let mut digest = job.clone();
        digest.id = format!("push-digest:{}", Uuid::new_v4());
        digest.task_id.clear();
        assert!(matches!(
            config().send(&d, &digest, None).await.unwrap(),
            SendResult::Sent
        ));
        let (_, bytes) = rx.recv().await.unwrap();
        let plain = ece::decrypt(&key.raw_components().unwrap(), &auth, &bytes).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&plain).unwrap();
        assert_eq!(value["title"], "Todoer summary");
        assert_eq!(value["url"], "/review");
        assert_eq!(value.as_object().unwrap().len(), 3);
        server.abort();
    }
}
