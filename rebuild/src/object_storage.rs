//! Private S3-compatible object transport. No presigned GET ever leaves this
//! module. Upload staging keys are separate from immutable finalized objects.
use crate::discussion::{Attachment, MAX_ATTACHMENT_BYTES, UploadIntent};
use base64::{Engine, engine::general_purpose::STANDARD};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("attachment storage configuration is invalid")]
    Configuration,
    #[error("attachment storage request failed")]
    Unavailable,
    #[error("attachment content does not match its upload intent")]
    Integrity,
    #[error("upload intent expired or already finalized")]
    IntentClosed,
    #[error("attachment exceeds 25 MiB")]
    TooLarge,
}
/// Deliberately has no Debug or Serialize implementation: credentials stay server-side.
pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub prefix: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}
impl S3Config {
    /// Unconfigured storage is disabled; a partially configured deployment fails closed.
    pub fn from_env() -> Result<Option<Self>, StorageError> {
        let names = [
            "TODOER_ATTACHMENTS_ENDPOINT",
            "TODOER_ATTACHMENTS_BUCKET",
            "TODOER_ATTACHMENTS_REGION",
            "TODOER_ATTACHMENTS_PREFIX",
            "TODOER_ATTACHMENTS_ACCESS_KEY_ID",
            "TODOER_ATTACHMENTS_SECRET_ACCESS_KEY",
        ];
        let values: Vec<_> = names.iter().map(|name| std::env::var(name).ok()).collect();
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        if values
            .iter()
            .any(|v| v.as_ref().is_none_or(|v| v.trim().is_empty()))
        {
            return Err(StorageError::Configuration);
        }
        let mut values = values.into_iter().map(Option::unwrap);
        Ok(Some(Self {
            endpoint: values.next().unwrap(),
            bucket: values.next().unwrap(),
            region: values.next().unwrap(),
            prefix: values.next().unwrap(),
            access_key_id: values.next().unwrap(),
            secret_access_key: values.next().unwrap(),
        }))
    }
}
#[derive(Clone)]
pub struct ObjectStorage {
    bucket: Bucket,
    credentials: Credentials,
    prefix: String,
    http: reqwest::Client,
}
#[derive(Serialize)]
pub struct SignedUpload {
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub expires_at: i64,
}
pub struct ObjectHead {
    pub size: u64,
    pub checksum_sha256: Option<String>,
}
/// Constructed only after server-side byte verification and immutable promotion.
#[derive(Clone)]
pub struct VerifiedObject {
    pub(crate) intent_id: String,
    pub(crate) revision: i64,
    pub(crate) size: u64,
    pub(crate) sha256: String,
    pub(crate) object_key: String,
}
impl VerifiedObject {
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    #[cfg(test)]
    pub(crate) fn synthetic(intent: &UploadIntent) -> Self {
        Self {
            intent_id: intent.id.clone(),
            revision: intent.revision,
            size: intent.size,
            sha256: intent.sha256.clone(),
            object_key: intent.object_key.clone(),
        }
    }
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs() as i64
}
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn checksum_base64(hex: &str) -> Result<String, StorageError> {
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StorageError::Integrity);
    }
    let bytes = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| StorageError::Integrity))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(STANDARD.encode(bytes))
}
pub fn content_disposition(filename: &str) -> String {
    let encoded: String = filename
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("attachment; filename=\"download\"; filename*=UTF-8''{encoded}")
}
impl ObjectStorage {
    pub fn new(config: S3Config) -> Result<Self, StorageError> {
        let endpoint: reqwest::Url = config
            .endpoint
            .parse()
            .map_err(|_| StorageError::Configuration)?;
        let local = matches!(
            endpoint.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        );
        if endpoint.scheme() != "https" && !(endpoint.scheme() == "http" && local)
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(StorageError::Configuration);
        }
        let prefix = config.prefix.trim_matches('/').to_owned();
        if prefix.is_empty()
            || prefix.len() > 200
            || prefix.chars().any(char::is_control)
            || prefix
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(StorageError::Configuration);
        }
        if config.access_key_id.is_empty()
            || config.secret_access_key.is_empty()
            || config.region.is_empty()
        {
            return Err(StorageError::Configuration);
        }
        let bucket = Bucket::new(endpoint, UrlStyle::Path, config.bucket, config.region)
            .map_err(|_| StorageError::Configuration)?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|_| StorageError::Configuration)?;
        Ok(Self {
            bucket,
            credentials: Credentials::new(config.access_key_id, config.secret_access_key),
            prefix,
            http,
        })
    }
    fn key(&self, key: &str) -> Result<String, StorageError> {
        // Object keys come from database-generated identifiers, never filenames.
        if !(key.starts_with("staging/") || key.starts_with("objects/"))
            || key
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
            || key.chars().any(char::is_control)
        {
            return Err(StorageError::Integrity);
        }
        Ok(format!("{}/{key}", self.prefix))
    }
    pub async fn health(&self) -> Result<(), StorageError> {
        let action = self.bucket.head_bucket(Some(&self.credentials));
        let response = self
            .http
            .head(action.sign(Duration::from_secs(60)))
            .send()
            .await
            .map_err(|_| StorageError::Unavailable)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(StorageError::Unavailable)
        }
    }
    pub fn signed_upload(&self, intent: &UploadIntent) -> Result<SignedUpload, StorageError> {
        if intent.finalized || intent.expires_at <= now() {
            return Err(StorageError::IntentClosed);
        }
        if intent.size > MAX_ATTACHMENT_BYTES {
            return Err(StorageError::TooLarge);
        }
        let key = self.key(&intent.staging_key)?;
        let mut action = self.bucket.put_object(Some(&self.credentials), &key);
        let headers: BTreeMap<String, String> = BTreeMap::from([
            ("content-type".into(), "application/octet-stream".into()),
            ("content-length".into(), intent.size.to_string()),
            ("if-none-match".into(), "*".into()),
            (
                "x-amz-checksum-sha256".into(),
                checksum_base64(&intent.sha256)?,
            ),
        ]);
        for (key, value) in &headers {
            action.headers_mut().insert(key.clone(), value.clone());
        }
        let seconds = (intent.expires_at - now()).clamp(1, 900) as u64;
        Ok(SignedUpload {
            url: action.sign(Duration::from_secs(seconds)).to_string(),
            headers,
            expires_at: intent.expires_at,
        })
    }
    pub async fn put_intent(
        &self,
        intent: &UploadIntent,
        bytes: Vec<u8>,
    ) -> Result<(), StorageError> {
        if bytes.len() as u64 != intent.size || sha256_hex(&bytes) != intent.sha256 {
            return Err(StorageError::Integrity);
        }
        let plan = self.signed_upload(intent)?;
        let mut request = self.http.put(&plan.url);
        for (key, value) in &plan.headers {
            request = request.header(key, value);
        }
        let response = request
            .body(bytes)
            .send()
            .await
            .map_err(|_| StorageError::Unavailable)?;
        if response.status() == reqwest::StatusCode::PRECONDITION_FAILED {
            self.verify_key(&intent.staging_key, intent.size, &intent.sha256)
                .await?;
            return Ok(());
        }
        if !response.status().is_success() {
            return Err(StorageError::Unavailable);
        }
        Ok(())
    }
    pub async fn head(&self, key: &str) -> Result<ObjectHead, StorageError> {
        let key = self.key(key)?;
        let action = self.bucket.head_object(Some(&self.credentials), &key);
        let response = self
            .http
            .head(action.sign(Duration::from_secs(60)))
            .send()
            .await
            .map_err(|_| StorageError::Unavailable)?;
        if !response.status().is_success() {
            return Err(StorageError::Unavailable);
        }
        let size = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .ok_or(StorageError::Integrity)?;
        let checksum = response
            .headers()
            .get("x-amz-checksum-sha256")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        Ok(ObjectHead {
            size,
            checksum_sha256: checksum,
        })
    }
    async fn get_key(&self, key: &str, limit: u64) -> Result<Vec<u8>, StorageError> {
        if limit > MAX_ATTACHMENT_BYTES {
            return Err(StorageError::TooLarge);
        }
        let key = self.key(key)?;
        let action = self.bucket.get_object(Some(&self.credentials), &key);
        let mut response = self
            .http
            .get(action.sign(Duration::from_secs(60)))
            .send()
            .await
            .map_err(|_| StorageError::Unavailable)?;
        if !response.status().is_success() {
            return Err(StorageError::Unavailable);
        }
        if response.content_length().is_some_and(|size| size > limit) {
            return Err(StorageError::TooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| StorageError::Unavailable)?
        {
            if bytes.len() as u64 + chunk.len() as u64 > limit {
                return Err(StorageError::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
    async fn verify_key(
        &self,
        key: &str,
        size: u64,
        sha256: &str,
    ) -> Result<Vec<u8>, StorageError> {
        let head = self.head(key).await?;
        if head.size != size
            || head.checksum_sha256.as_ref().is_some_and(|value| {
                checksum_base64(sha256).map_or(true, |expected| &expected != value)
            })
        {
            return Err(StorageError::Integrity);
        }
        // Always hash the bounded GET; S3-compatible providers differ in their
        // HEAD checksum support and promotion needs these verified bytes.
        let bytes = self.get_key(key, size).await?;
        if bytes.len() as u64 != size || sha256_hex(&bytes) != sha256 {
            return Err(StorageError::Integrity);
        }
        Ok(bytes)
    }
    pub async fn verify_and_promote(
        &self,
        intent: &UploadIntent,
    ) -> Result<VerifiedObject, StorageError> {
        if !intent.finalized && intent.expires_at <= now() {
            return Err(StorageError::IntentClosed);
        }
        if intent.size > MAX_ATTACHMENT_BYTES {
            return Err(StorageError::TooLarge);
        }
        if intent.finalized {
            self.verify_key(&intent.object_key, intent.size, &intent.sha256)
                .await?;
        } else {
            let bytes = self
                .verify_key(&intent.staging_key, intent.size, &intent.sha256)
                .await?;
            let key = self.key(&intent.object_key)?;
            let mut action = self.bucket.put_object(Some(&self.credentials), &key);
            let checksum = checksum_base64(&intent.sha256)?;
            action.headers_mut().insert("if-none-match", "*");
            action
                .headers_mut()
                .insert("content-type", "application/octet-stream");
            action
                .headers_mut()
                .insert("x-amz-checksum-sha256", checksum.clone());
            let response = self
                .http
                .put(action.sign(Duration::from_secs(60)))
                .header("if-none-match", "*")
                .header("content-type", "application/octet-stream")
                .header("x-amz-checksum-sha256", checksum)
                .body(bytes)
                .send()
                .await
                .map_err(|_| StorageError::Unavailable)?;
            if response.status() == reqwest::StatusCode::PRECONDITION_FAILED {
                self.verify_key(&intent.object_key, intent.size, &intent.sha256)
                    .await?;
            } else if !response.status().is_success() {
                return Err(StorageError::Unavailable);
            }
        }
        Ok(VerifiedObject {
            intent_id: intent.id.clone(),
            revision: intent.revision,
            size: intent.size,
            sha256: intent.sha256.clone(),
            object_key: intent.object_key.clone(),
        })
    }
    /// Caller must obtain metadata through Store::attachment for THIS request.
    /// The returned bytes must be served as octet-stream + attachment + nosniff.
    pub async fn get(&self, attachment: &Attachment) -> Result<Vec<u8>, StorageError> {
        self.verify_key(&attachment.object_key, attachment.size, &attachment.sha256)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::{Capture, NewTask, Store},
        discussion::UploadInput,
    };
    fn config(endpoint: String, bucket: String) -> S3Config {
        S3Config {
            endpoint,
            bucket,
            region: "us-east-1".into(),
            prefix: "synthetic-tests".into(),
            access_key_id: "synthetic-test".into(),
            secret_access_key: "synthetic-test-secret".into(),
        }
    }
    #[test]
    fn configuration_and_header_values_do_not_expose_untrusted_paths() {
        assert!(
            ObjectStorage::new(config("http://example.invalid".into(), "test".into())).is_err()
        );
        assert!(
            ObjectStorage::new(config(
                "http://user:password@127.0.0.1".into(),
                "test".into()
            ))
            .is_err()
        );
        let value = content_disposition("x\r\n<script>.txt");
        assert!(!value.contains('\r'));
        assert!(!value.contains('\n'));
        assert!(value.contains("%0D%0A"));
        assert_eq!(
            sha256_hex(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert!(checksum_base64(&"é".repeat(32)).is_err());
    }
    #[tokio::test]
    #[ignore = "requires an isolated local S3 protocol server; see attachments.md"]
    async fn local_s3_put_head_get_and_immutable_finalization() {
        let endpoint =
            std::env::var("TODOER_TEST_S3_ENDPOINT").expect("local fixture endpoint required");
        assert!(endpoint.starts_with("http://127.0.0.1:"));
        let bucket =
            std::env::var("TODOER_TEST_S3_BUCKET").expect("isolated fixture bucket required");
        assert!(bucket.starts_with("todoer-synthetic-"));
        let objects = ObjectStorage::new(config(endpoint.clone(), bucket.clone())).unwrap();
        objects.health().await.unwrap();
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let task = db
            .capture(
                "dev",
                "s3-protocol",
                Capture {
                    list_id: "inbox".into(),
                    tasks: vec![NewTask {
                        title: "Synthetic S3 task".into(),
                        description: String::new(),
                        parent_id: None,
                    }],
                },
            )
            .unwrap()
            .remove(0);
        let bytes = b"synthetic attachment bytes".to_vec();
        let intent = db
            .create_upload_intent(
                "dev",
                &task.id,
                UploadInput {
                    comment_id: None,
                    filename: "test.txt".into(),
                    size: bytes.len() as u64,
                    sha256: sha256_hex(&bytes),
                },
            )
            .unwrap();
        let plan = objects.signed_upload(&intent).unwrap();
        assert!(plan.url.contains("X-Amz-Signature="));
        // Use the public direct-upload contract for an actual signed HTTP PUT.
        let mut request = reqwest::Client::new().put(&plan.url);
        for (k, v) in &plan.headers {
            request = request.header(k, v);
        }
        let response = request.body(bytes.clone()).send().await.unwrap();
        assert!(response.status().is_success());
        let head = objects.head(&intent.staging_key).await.unwrap();
        assert_eq!(head.size, bytes.len() as u64);
        let verified = objects.verify_and_promote(&intent).await.unwrap();
        let attachment = db.finalize_attachment("dev", verified.clone()).unwrap();
        assert_eq!(objects.get(&attachment).await.unwrap(), bytes);
        assert_eq!(
            db.finalize_attachment("dev", verified).unwrap().id,
            attachment.id
        );
        let closed = db.upload_intent("dev", &intent.id).unwrap();
        assert!(matches!(
            objects.signed_upload(&closed),
            Err(StorageError::IntentClosed)
        ));
        assert!(matches!(
            objects.put_intent(&closed, bytes.clone()).await,
            Err(StorageError::IntentClosed)
        ));
        let mut retry = reqwest::Client::new().put(&plan.url);
        for (k, v) in &plan.headers {
            retry = retry.header(k, v);
        }
        assert_eq!(
            retry.body(bytes.clone()).send().await.unwrap().status(),
            reqwest::StatusCode::PRECONDITION_FAILED
        );
        let anonymous = reqwest::get(format!(
            "{}/{}/{}/{}",
            endpoint, bucket, "synthetic-tests", intent.object_key
        ))
        .await
        .unwrap();
        assert_eq!(anonymous.status(), reqwest::StatusCode::FORBIDDEN);
        let second = db
            .create_upload_intent(
                "dev",
                &task.id,
                UploadInput {
                    comment_id: None,
                    filename: "proxy.txt".into(),
                    size: bytes.len() as u64,
                    sha256: sha256_hex(&bytes),
                },
            )
            .unwrap();
        objects.put_intent(&second, bytes.clone()).await.unwrap();
        objects.put_intent(&second, bytes.clone()).await.unwrap();
        assert!(matches!(
            objects.put_intent(&second, b"bad".to_vec()).await,
            Err(StorageError::Integrity)
        ));
        let second_attachment = db
            .finalize_attachment("dev", objects.verify_and_promote(&second).await.unwrap())
            .unwrap();
        assert_eq!(objects.get(&second_attachment).await.unwrap(), bytes);
        db.delete_attachment("dev", &attachment.id, attachment.revision)
            .unwrap();
        assert!(db.attachment("dev", &attachment.id).is_err());
        // Soft deletion intentionally retains bytes; retention is a separate action.
        assert_eq!(objects.get(&attachment).await.unwrap(), bytes);
        println!(
            "local S3 protocol: direct signed PUT + proxy PUT, HEAD, bounded GET, checksum verification, private anonymous denial, immutable retries, metadata finalize and soft-delete passed"
        );
    }
}
