//! Google OIDC authorization-code helper. No tokens or secrets implement Debug.
//!
//! Callers must keep PendingLogin server-side, bind it to the initiating browser
//! using a fresh HttpOnly cookie, and remove it before calling complete_login.
//! A verified email is registration policy input, never a stable identity key.
//! Flow follows https://docs.rs/openidconnect/4.0.1/openidconnect/ and
//! https://developers.google.com/identity/openid-connect/openid-connect.
use openidconnect::{
    AccessTokenHash, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet,
    EndpointNotSet, EndpointSet, IssuerUrl, Nonce, OAuth2TokenResponse, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata, CoreTokenResponse},
};
use std::time::{Duration, Instant};

pub const GOOGLE_ISSUER: &str = "https://accounts.google.com";
pub const LOGIN_LIFETIME: Duration = Duration::from_secs(600);
const NETWORK_TIMEOUT: Duration = Duration::from_secs(15);
type DiscoveredClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

#[derive(Clone)]
pub struct GoogleConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

#[derive(Debug, thiserror::Error)]
pub enum OidcError {
    #[error("invalid Google OIDC configuration")]
    Configuration,
    #[error("Google OIDC discovery failed")]
    Discovery,
    #[error("Google OIDC request timed out")]
    Timeout,
    #[error("sign-in state does not match")]
    StateMismatch,
    #[error("sign-in request expired")]
    Expired,
    #[error("Google authorization code exchange failed")]
    Exchange,
    #[error("Google did not return an ID token")]
    MissingIdToken,
    #[error("Google ID token validation failed")]
    InvalidIdentity,
    #[error("a verified email is required to register")]
    VerifiedEmailRequired,
}

pub struct PendingLogin {
    state: CsrfToken,
    nonce: Nonce,
    pkce_verifier: PkceCodeVerifier,
    created: Instant,
}
impl PendingLogin {
    pub fn is_expired(&self) -> bool {
        self.created.elapsed() >= LOGIN_LIFETIME
    }
}
pub struct LoginStart {
    pub authorization_url: String,
    pub pending: PendingLogin,
}
#[derive(Debug, Clone)]
pub struct VerifiedIdentity {
    pub issuer: String,
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub display_name: Option<String>,
    pub picture_url: Option<String>,
}
impl VerifiedIdentity {
    /// Registration gate only; accounts must be keyed by (issuer, subject).
    pub fn registration_email(&self) -> Result<&str, OidcError> {
        self.email
            .as_deref()
            .filter(|email| self.email_verified && !email.is_empty())
            .ok_or(OidcError::VerifiedEmailRequired)
    }
}
pub struct GoogleOidc {
    client: DiscoveredClient,
    http: reqwest::Client,
    issuer: String,
    refresh: Option<(GoogleConfig, IssuerUrl)>,
    refreshed_client: std::sync::Mutex<Option<DiscoveredClient>>,
}
impl GoogleOidc {
    /// Offline construction for isolated route tests only. Empty keys cannot
    /// authenticate a real token; production callers must use discovery.
    #[cfg(test)]
    pub(crate) fn test_fixture() -> Self {
        tests::isolated_client()
    }
    pub async fn discover(config: GoogleConfig) -> Result<Self, OidcError> {
        Self::discover_for(
            config,
            IssuerUrl::new(GOOGLE_ISSUER.into()).map_err(|_| OidcError::Configuration)?,
        )
        .await
    }
    // Not exposed: public production discovery always uses the fixed Google issuer.
    async fn discover_for(config: GoogleConfig, issuer: IssuerUrl) -> Result<Self, OidcError> {
        let redirect = validate_config(&config)?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(NETWORK_TIMEOUT)
            .build()
            .map_err(|_| OidcError::Configuration)?;
        let metadata = tokio::time::timeout(
            NETWORK_TIMEOUT,
            CoreProviderMetadata::discover_async(issuer.clone(), &http),
        )
        .await
        .map_err(|_| OidcError::Timeout)?
        .map_err(|_| OidcError::Discovery)?;
        let refresh = Some((config.clone(), issuer));
        let mut provider = Self::from_metadata(config, redirect, metadata, http);
        provider.refresh = refresh;
        Ok(provider)
    }
    fn from_metadata(
        config: GoogleConfig,
        redirect: RedirectUrl,
        metadata: CoreProviderMetadata,
        http: reqwest::Client,
    ) -> Self {
        let issuer = metadata.issuer().as_str().to_owned();
        Self {
            client: CoreClient::from_provider_metadata(
                metadata,
                ClientId::new(config.client_id),
                Some(ClientSecret::new(config.client_secret)),
            )
            .set_redirect_uri(redirect),
            http,
            issuer,
            refresh: None,
            refreshed_client: std::sync::Mutex::new(None),
        }
    }
    fn client_snapshot(&self) -> DiscoveredClient {
        self.refreshed_client
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .as_ref()
            .unwrap_or(&self.client)
            .clone()
    }
    pub fn begin_login(&self) -> LoginStart {
        let (challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let client = self.client_snapshot();
        let (url, state, nonce) = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(Scope::new("email".into()))
            .add_scope(Scope::new("profile".into()))
            .set_pkce_challenge(challenge)
            .url();
        LoginStart {
            authorization_url: url.to_string(),
            pending: PendingLogin {
                state,
                nonce,
                pkce_verifier,
                created: Instant::now(),
            },
        }
    }
    /// Consumes pending state even if verification fails. The caller must remove
    /// its browser-bound pending record before invoking this method.
    pub async fn complete_login(
        &self,
        pending: PendingLogin,
        returned_state: &str,
        code: &str,
    ) -> Result<VerifiedIdentity, OidcError> {
        if pending.is_expired() {
            return Err(OidcError::Expired);
        }
        // The dependency's timing-resistant-secret-traits feature makes this
        // comparison constant-time rather than comparing exposed secret strings.
        if pending.state != CsrfToken::new(returned_state.into()) {
            return Err(OidcError::StateMismatch);
        }
        if code.is_empty() {
            return Err(OidcError::Exchange);
        }
        let client = self.client_snapshot();
        let request = client
            .exchange_code(AuthorizationCode::new(code.into()))
            .map_err(|_| OidcError::Configuration)?
            .set_pkce_verifier(pending.pkce_verifier);
        let response = tokio::time::timeout(NETWORK_TIMEOUT, request.request_async(&self.http))
            .await
            .map_err(|_| OidcError::Timeout)?
            .map_err(|_| OidcError::Exchange)?;
        let verified = self.verify_response(&client, &response, &pending.nonce);
        if matches!(verified, Err(OidcError::InvalidIdentity))
            && let Some((config, issuer)) = &self.refresh
        {
            // Refresh once per failed callback and verify the SAME token.
            // Never replay the one-use authorization code or skip claims.
            let fresh = Self::discover_for(config.clone(), issuer.clone()).await?;
            let verified = self.verify_response(&fresh.client, &response, &pending.nonce);
            *self
                .refreshed_client
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()) = Some(fresh.client);
            return verified;
        }
        verified
    }
    fn verify_response(
        &self,
        client: &DiscoveredClient,
        response: &CoreTokenResponse,
        nonce: &Nonce,
    ) -> Result<VerifiedIdentity, OidcError> {
        let token = response.id_token().ok_or(OidcError::MissingIdToken)?;
        let verifier = client.id_token_verifier();
        // Library verifies signature, issuer, audience, expiry and nonce.
        let claims = token
            .claims(&verifier, nonce)
            .map_err(|_| OidcError::InvalidIdentity)?;
        if let Some(expected) = claims.access_token_hash() {
            let actual = AccessTokenHash::from_token(
                response.access_token(),
                token
                    .signing_alg()
                    .map_err(|_| OidcError::InvalidIdentity)?,
                token
                    .signing_key(&verifier)
                    .map_err(|_| OidcError::InvalidIdentity)?,
            )
            .map_err(|_| OidcError::InvalidIdentity)?;
            if actual != *expected {
                return Err(OidcError::InvalidIdentity);
            }
        }
        if claims.issuer().as_str() != self.issuer || claims.subject().as_str().is_empty() {
            return Err(OidcError::InvalidIdentity);
        }
        Ok(VerifiedIdentity {
            issuer: claims.issuer().as_str().into(),
            subject: claims.subject().as_str().into(),
            email: claims.email().map(|email| email.as_str().into()),
            email_verified: claims.email_verified() == Some(true),
            picture_url: claims
                .picture()
                .and_then(|picture| picture.get(None))
                .map(|picture| picture.as_str().to_owned())
                .filter(|picture| picture.len() <= 2048 && picture.starts_with("https://")),
            display_name: claims
                .name()
                .and_then(|name| name.get(None))
                .map(|name| name.as_str().to_owned()),
        })
    }
}
fn validate_config(config: &GoogleConfig) -> Result<RedirectUrl, OidcError> {
    if config.client_id.trim().is_empty() || config.client_secret.trim().is_empty() {
        return Err(OidcError::Configuration);
    }
    let url = reqwest::Url::parse(&config.redirect_uri).map_err(|_| OidcError::Configuration)?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"));
    if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err(OidcError::Configuration);
    }
    RedirectUrl::new(config.redirect_uri.clone()).map_err(|_| OidcError::Configuration)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(redirect: &str) -> GoogleConfig {
        GoogleConfig {
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
            redirect_uri: redirect.into(),
        }
    }
    // Offline metadata creates no trust bypass in production discovery. Empty
    // signing keys deliberately cannot validate any ID token.
    pub(super) fn isolated_client() -> GoogleOidc {
        client_for_token("http://127.0.0.1:1/token")
    }
    fn client_for_token(endpoint: &str) -> GoogleOidc {
        let metadata: CoreProviderMetadata = serde_json::from_value(serde_json::json!({
            "issuer": GOOGLE_ISSUER,
            "authorization_endpoint": "https://accounts.google.com/o/oauth2/v2/auth",
            "token_endpoint": endpoint,
            "jwks_uri": "http://127.0.0.1:1/jwks",
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"]
        }))
        .unwrap();
        let config = config("http://127.0.0.1:8092/auth/google/callback");
        let redirect = validate_config(&config).unwrap();
        GoogleOidc::from_metadata(config, redirect, metadata, reqwest::Client::new())
    }
    #[test]
    fn redirects_reject_remote_http_credentials_and_fragments() {
        for redirect in [
            "http://example.com/callback",
            "https://user:pass@example.com/callback",
            "https://example.com/callback#fragment",
            "file:///tmp/callback",
        ] {
            assert!(validate_config(&config(redirect)).is_err());
        }
        assert!(validate_config(&config("https://todoer.example/auth/google/callback")).is_ok());
        assert!(validate_config(&config("http://127.0.0.1:8092/auth/google/callback")).is_ok());
    }
    #[test]
    fn login_requests_use_fresh_state_nonce_and_s256_pkce() {
        let client = isolated_client();
        let first = client.begin_login();
        let second = client.begin_login();
        let url = reqwest::Url::parse(&first.authorization_url).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["code_challenge_method"], "S256");
        assert!(
            query["scope"]
                .split_whitespace()
                .any(|scope| scope == "openid")
        );
        assert!(!query["code_challenge"].is_empty());
        assert_eq!(query["state"], *first.pending.state.secret());
        assert_eq!(query["nonce"], *first.pending.nonce.secret());
        assert_ne!(first.pending.state.secret(), second.pending.state.secret());
        assert_ne!(first.pending.nonce.secret(), second.pending.nonce.secret());
    }
    #[tokio::test]
    async fn callback_rejects_state_and_expiry_before_exchange() {
        let client = isolated_client();
        let pending = client.begin_login().pending;
        assert!(matches!(
            client.complete_login(pending, "wrong", "code").await,
            Err(OidcError::StateMismatch)
        ));
        let mut pending = client.begin_login().pending;
        let state = pending.state.secret().clone();
        pending.created = Instant::now() - LOGIN_LIFETIME;
        assert!(matches!(
            client.complete_login(pending, &state, "code").await,
            Err(OidcError::Expired)
        ));
    }
    #[tokio::test]
    async fn mock_exchange_transmits_pkce_and_rejects_untrusted_id_token() {
        use axum::{Json, Router, body::Bytes, extract::State, routing::post};
        use std::sync::{Arc, Mutex};
        let received = Arc::new(Mutex::new(String::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
        let router = Router::new().route("/token", post(|State(body): State<Arc<Mutex<String>>>, bytes: Bytes| async move {
            *body.lock().unwrap() = String::from_utf8(bytes.to_vec()).unwrap();
            Json(serde_json::json!({"access_token":"synthetic-access-token","token_type":"Bearer","id_token":"not-a-signed-jwt"}))
        })).with_state(received.clone());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = client_for_token(&endpoint);
        let pending = client.begin_login().pending;
        let state = pending.state.secret().clone();
        let verifier = pending.pkce_verifier.secret().clone();
        let outcome = client
            .complete_login(pending, &state, "synthetic-code")
            .await;
        // Malformed tokens may be rejected while parsing the token response;
        // validly parsed but untrusted signatures are rejected by claims().
        assert!(matches!(
            outcome,
            Err(OidcError::Exchange | OidcError::InvalidIdentity)
        ));
        let form = reqwest::Url::parse(&format!("http://localhost/?{}", received.lock().unwrap()))
            .unwrap();
        let fields: std::collections::HashMap<_, _> = form.query_pairs().into_owned().collect();
        assert_eq!(fields["code_verifier"], verifier);
        assert_eq!(fields["code"], "synthetic-code");
        assert_eq!(fields["grant_type"], "authorization_code");
        server.abort();
    }
    #[tokio::test]
    async fn signed_mock_tokens_verify_identity_and_reject_claim_attacks() {
        use axum::{Json, Router, extract::State, routing::post};
        use openidconnect::{
            AccessToken, PrivateSigningKey,
            core::{
                CoreIdToken, CoreIdTokenClaims, CoreJsonWebKeySet, CoreJwsSigningAlgorithm,
                CoreRsaPrivateSigningKey,
            },
        };
        use rsa::pkcs1::EncodeRsaPrivateKey;
        use std::sync::{Arc, Mutex};
        // Generate a fresh key in memory. No private test key is checked in or
        // written to disk, and these keys are never accepted by production.
        let rsa = rsa::RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).unwrap();
        let pem = rsa.to_pkcs1_pem(Default::default()).unwrap();
        let key = CoreRsaPrivateSigningKey::from_pem(&pem, None).unwrap();
        let response_token = Arc::new(Mutex::new(String::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
        let router = Router::new().route("/token", post(|State(token): State<Arc<Mutex<String>>>| async move {
            Json(serde_json::json!({"access_token":"synthetic-access","token_type":"Bearer","id_token":token.lock().unwrap().clone()}))
        })).with_state(response_token.clone());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let mut client = client_for_token(&endpoint);
        let metadata: CoreProviderMetadata = serde_json::from_value(serde_json::json!({
            "issuer": GOOGLE_ISSUER,
            "authorization_endpoint": "https://accounts.google.com/o/oauth2/v2/auth",
            "token_endpoint": endpoint,
            "jwks_uri": "http://127.0.0.1:1/jwks",
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"]
        }))
        .unwrap();
        let metadata = metadata.set_jwks(CoreJsonWebKeySet::new(vec![key.as_verification_key()]));
        let configuration = config("http://127.0.0.1:8092/auth/google/callback");
        let redirect = validate_config(&configuration).unwrap();
        client.client = CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(configuration.client_id),
            Some(ClientSecret::new(configuration.client_secret)),
        )
        .set_redirect_uri(redirect);
        for case in [
            "valid",
            "nonce",
            "audience",
            "issuer",
            "expired",
            "access_hash",
        ] {
            let pending = client.begin_login().pending;
            let state = pending.state.secret().clone();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let claims: CoreIdTokenClaims = serde_json::from_value(serde_json::json!({
                "iss": if case == "issuer" { "https://attacker.example" } else { GOOGLE_ISSUER },
                "sub":"stable-google-subject", "aud": if case == "audience" { "wrong-client" } else { "test-client" },
                "iat": now-10, "exp": if case == "expired" { now-1 } else { now+300 },
                "nonce": if case == "nonce" { "wrong-nonce" } else { pending.nonce.secret() },
                "email":"verified@example.test", "email_verified":true, "name":"Synthetic User"
            })).unwrap();
            let signed = CoreIdToken::new(
                claims,
                &key,
                CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
                Some(&AccessToken::new(if case == "access_hash" {
                    "substituted-access".into()
                } else {
                    "synthetic-access".into()
                })),
                None,
            )
            .unwrap();
            *response_token.lock().unwrap() = signed.to_string();
            let outcome = client
                .complete_login(pending, &state, "synthetic-code")
                .await;
            if case == "valid" {
                let identity = outcome.unwrap();
                assert_eq!(identity.issuer, GOOGLE_ISSUER);
                assert_eq!(identity.subject, "stable-google-subject");
                assert_eq!(
                    identity.registration_email().unwrap(),
                    "verified@example.test"
                );
                assert_eq!(identity.display_name.as_deref(), Some("Synthetic User"));
            } else {
                assert!(
                    matches!(outcome, Err(OidcError::InvalidIdentity)),
                    "case {case}"
                );
            }
        }
        server.abort();
    }
    #[tokio::test]
    async fn unknown_signing_key_refreshes_discovery_once_without_redeeming_code_again() {
        use axum::{
            Json, Router,
            routing::{get, post},
        };
        use openidconnect::{
            AccessToken, PrivateSigningKey,
            core::{
                CoreIdToken, CoreIdTokenClaims, CoreJwsSigningAlgorithm, CoreRsaPrivateSigningKey,
            },
        };
        use rsa::pkcs1::EncodeRsaPrivateKey;
        use std::sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        };
        let rsa = rsa::RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).unwrap();
        let key = CoreRsaPrivateSigningKey::from_pem(
            &rsa.to_pkcs1_pem(Default::default()).unwrap(),
            Some(openidconnect::JsonWebKeyId::new("rotated".into())),
        )
        .unwrap();
        let keys = Arc::new(Mutex::new(serde_json::json!({"keys":[]})));
        let token = Arc::new(Mutex::new(String::new()));
        let discovery_calls = Arc::new(AtomicUsize::new(0));
        let exchange_calls = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let metadata = serde_json::json!({"issuer":issuer,"authorization_endpoint":format!("{issuer}/authorize"),"token_endpoint":format!("{issuer}/token"),"jwks_uri":format!("{issuer}/jwks"),"response_types_supported":["code"],"subject_types_supported":["public"],"id_token_signing_alg_values_supported":["RS256"]});
        let router = Router::new()
            .route("/.well-known/openid-configuration", get({let count=discovery_calls.clone(); move || {let metadata=metadata.clone(); let count=count.clone(); async move { count.fetch_add(1, Ordering::SeqCst); Json(metadata) }}}))
            .route("/jwks", get({let keys=keys.clone(); move || {let keys=keys.clone(); async move {Json(keys.lock().unwrap().clone())}}}))
            .route("/token", post({let token=token.clone(); let count=exchange_calls.clone(); move || {let token=token.clone(); let count=count.clone(); async move {count.fetch_add(1, Ordering::SeqCst); Json(serde_json::json!({"access_token":"rotation-access","token_type":"Bearer","id_token":token.lock().unwrap().clone()}))}}}));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        // Only this private test helper accepts a local issuer. The public
        // discover() entry point always uses Google's HTTPS issuer.
        let client = GoogleOidc::discover_for(
            config("http://127.0.0.1:8092/auth/google/callback"),
            IssuerUrl::new(issuer.clone()).unwrap(),
        )
        .await
        .unwrap();
        *keys.lock().unwrap() = serde_json::json!({"keys":[key.as_verification_key()]});
        for iteration in 0..2 {
            let pending = client.begin_login().pending;
            let state = pending.state.secret().clone();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let claims: CoreIdTokenClaims=serde_json::from_value(serde_json::json!({"iss":issuer,"sub":"rotated-subject","aud":"test-client","iat":now,"exp":now+300,"nonce":pending.nonce.secret(),"email":"rotation@example.test","email_verified":true})).unwrap();
            *token.lock().unwrap() = CoreIdToken::new(
                claims,
                &key,
                CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
                Some(&AccessToken::new("rotation-access".into())),
                None,
            )
            .unwrap()
            .to_string();
            let identity = client
                .complete_login(pending, &state, "single-use-code")
                .await
                .unwrap();
            assert_eq!(identity.subject, "rotated-subject");
            assert_eq!(
                discovery_calls.load(Ordering::SeqCst),
                2,
                "refreshed keys should be cached"
            );
            assert_eq!(
                exchange_calls.load(Ordering::SeqCst),
                iteration + 1,
                "each authorization code is exchanged only once"
            );
        }
        server.abort();
    }
    #[test]
    fn registration_requires_verified_email_and_preserves_subject_identity() {
        let mut identity = VerifiedIdentity {
            issuer: GOOGLE_ISSUER.into(),
            subject: "stable-subject".into(),
            email: Some("user@example.test".into()),
            email_verified: false,
            display_name: None,
            picture_url: None,
        };
        assert!(matches!(
            identity.registration_email(),
            Err(OidcError::VerifiedEmailRequired)
        ));
        identity.email_verified = true;
        assert_eq!(identity.registration_email().unwrap(), "user@example.test");
        assert_eq!(identity.subject, "stable-subject");
        identity.email = None;
        assert!(identity.registration_email().is_err());
    }
}
