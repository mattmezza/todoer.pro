//! Synthetic router-to-provider tests: no live inference keys or payment traffic.
use super::*;
use crate::auth::{AuthStore, Permission};
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Notify;
use tower::ServiceExt;

#[derive(Default)]
struct ProviderGate {
    calls: AtomicUsize,
    block: AtomicBool,
    started: Notify,
    release: Notify,
}
struct FakeProvider {
    service: crate::assist::AssistService,
    gate: Arc<ProviderGate>,
    server: tokio::task::JoinHandle<()>,
}
impl FakeProvider {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let gate = Arc::new(ProviderGate::default());
        let handler_gate = gate.clone();
        let app = Router::new().route(
            "/chat",
            post(move |Json(body): Json<Value>| {
                let gate = handler_gate.clone();
                async move {
                    assert_eq!(body["model"], "openai/gpt-4.1-mini");
                    assert_eq!(body["provider"]["allow_fallbacks"], false);
                    let call = gate.calls.fetch_add(1, Ordering::SeqCst) + 1;
                    gate.started.notify_one();
                    if gate.block.load(Ordering::SeqCst) {
                        gate.release.notified().await;
                    }
                    Json(json!({
                        "id":format!("gen-synthetic-{call}"),
                        "model":"openai/gpt-4.1-mini",
                        "provider":"Synthetic provider",
                        "usage":{
                            "prompt_tokens":100,"completion_tokens":10,"cost":0.00005,
                            "prompt_tokens_details":{"cached_tokens":20},
                            "completion_tokens_details":{"reasoning_tokens":5}
                        },
                        "choices":[{"finish_reason":"stop","message":{"content":json!({
                            "list_title":null,"tasks":[{"title":"Synthetic first step","description":"","parent_index":null}]
                        }).to_string()}}]
                    }))
                }
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            service: crate::assist::AssistService::test_provider(format!("http://{address}/chat")),
            gate,
            server,
        }
    }
}
impl Drop for FakeProvider {
    fn drop(&mut self) {
        self.server.abort();
    }
}
struct Fixture {
    state: AppState,
    path: String,
    cookie: String,
    csrf: String,
    bearer: String,
    bearer_id: String,
}
impl Fixture {
    fn new(service: crate::assist::AssistService, production: bool) -> Self {
        let path = std::env::temp_dir()
            .join(format!(
                "todoer-ai-billing-http-{}.db",
                uuid::Uuid::new_v4()
            ))
            .to_string_lossy()
            .to_string();
        let mut db = Store::open(&path).unwrap();
        db.seed_dev().unwrap();
        db.conn.execute("INSERT INTO ai_account_policy(user_id,user_ai_enabled,managed_ai_allowed,funding,model_id,daily_limit_micro) VALUES('dev',1,1,'credits','openai/gpt-4.1-mini',1000000)",[]).unwrap();
        db.conn.execute("UPDATE ai_models SET enabled=1,input_micro_per_million=1000000,output_micro_per_million=4000000,cached_micro_per_million=100000,margin_bps=2000 WHERE id='openai/gpt-4.1-mini'",[]).unwrap();
        db.conn
            .execute("UPDATE ai_provider_policy SET usd_eur_ppm=900000", [])
            .unwrap();
        db.ai_credit("dev", "synthetic-integration-credit", 2_000_000, "topup")
            .unwrap();
        let auth = AuthStore::open(&path).unwrap();
        let session = auth.issue_session("dev", 3600).unwrap();
        auth.mark_recent(&session.token).unwrap();
        let origin = if production {
            "https://todoer.example"
        } else {
            "http://127.0.0.1:8092"
        };
        let agent = auth
            .issue_agent(
                "dev",
                &format!("{origin}/api/v1"),
                vec![Permission::Admin],
                Some(vec!["inbox".into()]),
                None,
                3600,
            )
            .unwrap();
        let state = if production {
            AppState::production_optional(Arc::new(Mutex::new(db)), auth, None, origin.into())
        } else {
            AppState::new(
                Arc::new(Mutex::new(db)),
                "synthetic-root-token-at-least-24-characters".into(),
                origin.into(),
            )
        }
        .with_assist(service);
        Self {
            state,
            path,
            cookie: format!(
                "todoer_session={}; todoer_csrf={}",
                session.token, session.csrf
            ),
            csrf: session.csrf,
            bearer: agent.token,
            bearer_id: agent.id,
        }
    }
    fn request(
        &self,
        method: &str,
        path: &str,
        body: Value,
        quote: Option<&str>,
        bearer: bool,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("origin", &self.state.origin)
            .header("content-type", "application/json");
        if bearer {
            builder = builder.header("authorization", format!("Bearer {}", self.bearer));
        } else {
            builder = builder
                .header("cookie", &self.cookie)
                .header("x-csrf-token", &self.csrf);
        }
        if let Some(quote) = quote {
            builder = builder.header("x-todoer-ai-quote", quote);
        }
        builder.body(Body::from(body.to_string())).unwrap()
    }
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Value,
        quote: Option<&str>,
        bearer: bool,
    ) -> (StatusCode, Value) {
        response(
            router(self.state.clone())
                .oneshot(self.request(method, path, body, quote, bearer))
                .await
                .unwrap(),
        )
        .await
    }
    fn wallet(&self) -> crate::ai_usage::Wallet {
        self.state.store.lock().unwrap().ai_wallet("dev").unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.path));
        }
    }
}
async fn response(response: Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
fn draft(key: &str) -> Value {
    json!({"idempotency_key":key,"brief":"A synthetic test checklist","target":{"kind":"existing_list","list_id":"inbox"}})
}

#[tokio::test]
async fn quote_does_not_dispatch_and_accepted_retry_is_cached_without_another_call() {
    let provider = FakeProvider::start().await;
    let f = Fixture::new(provider.service.clone(), true);
    let body = draft("router-exactly-once");
    let (status, quote) = f
        .send("POST", "/api/v1/assist/drafts", body.clone(), None, false)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(quote["status"], "quote");
    assert!(quote["quote"]["max_micro"].as_i64().unwrap() > 0);
    assert!(quote["quote"]["provider_estimated_max_micro_usd"].is_null());
    assert_eq!(provider.gate.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.wallet().held_micro, 0);
    let quote_id = quote["quote"]["id"].as_str().unwrap();
    let (status, result) = f
        .send(
            "POST",
            "/api/v1/assist/drafts",
            body.clone(),
            Some(quote_id),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["status"], "draft");
    assert_eq!(provider.gate.calls.load(Ordering::SeqCst), 1);
    let after = f.wallet();
    assert!(after.posted_micro < 2_000_000);
    assert_eq!(after.held_micro, 0);
    let (status, cached) = f
        .send("POST", "/api/v1/assist/drafts", body, Some(quote_id), false)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cached, result);
    assert_eq!(provider.gate.calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.wallet().posted_micro, after.posted_micro);
    let (status, usage) = f
        .send("GET", "/api/account/ai/usage", Value::Null, None, false)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        usage["operations"][0]["customer_micro"].as_i64().unwrap(),
        2_000_000 - after.posted_micro
    );
    assert!(usage["operations"][0]["byok_cost_micro_usd"].is_null());
    assert!(
        !serde_json::to_string(&usage)
            .unwrap()
            .contains("provider_micro_usd")
    );
}

#[tokio::test]
async fn logout_while_provider_is_running_blocks_delivery_and_preserves_expense_without_charge() {
    let provider = FakeProvider::start().await;
    let f = Fixture::new(provider.service.clone(), true);
    let body = draft("logout-in-flight");
    let (_, quote) = f
        .send("POST", "/api/v1/assist/drafts", body.clone(), None, false)
        .await;
    let quote_id = quote["quote"]["id"].as_str().unwrap().to_owned();
    provider.gate.block.store(true, Ordering::SeqCst);
    let app = router(f.state.clone());
    let request = f.request(
        "POST",
        "/api/v1/assist/drafts",
        body,
        Some(&quote_id),
        false,
    );
    let pending = tokio::spawn(async move { app.oneshot(request).await.unwrap() });
    tokio::time::timeout(Duration::from_secs(3), provider.gate.started.notified())
        .await
        .unwrap();
    assert!(f.wallet().held_micro > 0);
    assert_eq!(
        f.send("POST", "/logout", Value::Null, None, false).await.0,
        StatusCode::SEE_OTHER
    );
    provider.gate.release.notify_one();
    let result = tokio::time::timeout(Duration::from_secs(3), pending)
        .await
        .unwrap()
        .unwrap();
    let (status, body) = response(result).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_ne!(body["status"], "draft");
    assert_eq!(f.wallet().posted_micro, 2_000_000);
    assert_eq!(f.wallet().held_micro, 0);
    let db = f.state.store.lock().unwrap();
    let (state, cost, charged): (String, Option<i64>, i64) = db
        .conn
        .query_row(
            "SELECT status,provider_micro_usd,customer_micro FROM ai_operations WHERE id=?1",
            [quote_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(state, "released");
    assert_eq!(cost, Some(50));
    assert_eq!(charged, 0);
    assert_eq!(
        db.conn
            .query_row("SELECT count(*) FROM assist_drafts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn revoked_agent_in_flight_cannot_deliver_but_still_releases_hold_and_records_expense() {
    let provider = FakeProvider::start().await;
    let f = Fixture::new(provider.service.clone(), true);
    let body = draft("revoke-in-flight");
    let (_, quote) = f
        .send("POST", "/api/v1/assist/drafts", body.clone(), None, true)
        .await;
    let quote_id = quote["quote"]["id"].as_str().unwrap().to_owned();
    provider.gate.block.store(true, Ordering::SeqCst);
    let app = router(f.state.clone());
    let request = f.request("POST", "/api/v1/assist/drafts", body, Some(&quote_id), true);
    let pending = tokio::spawn(async move { app.oneshot(request).await.unwrap() });
    tokio::time::timeout(Duration::from_secs(3), provider.gate.started.notified())
        .await
        .unwrap();
    f.state
        .production
        .as_ref()
        .unwrap()
        .store
        .lock()
        .unwrap()
        .revoke_agent("dev", &f.bearer_id)
        .unwrap();
    provider.gate.release.notify_one();
    let result = tokio::time::timeout(Duration::from_secs(3), pending)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.status(), StatusCode::FORBIDDEN);
    assert_eq!(f.wallet().posted_micro, 2_000_000);
    assert_eq!(f.wallet().held_micro, 0);
    let db = f.state.store.lock().unwrap();
    let cost: Option<i64> = db
        .conn
        .query_row(
            "SELECT provider_micro_usd FROM ai_operations WHERE id=?1",
            [quote_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cost, Some(50));
}

#[test]
fn scoped_development_bearer_delivery_does_not_lock_the_store_recursively() {
    // Bound runtime shutdown too, so a mutex regression fails rather than hanging CI.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(8), async {
            let provider = FakeProvider::start().await;
            let f = Fixture::new(provider.service.clone(), false);
            let body = draft("development-bearer");
            let (status, quote) = f
                .send("POST", "/api/v1/assist/drafts", body.clone(), None, true)
                .await;
            assert_eq!(status, StatusCode::OK);
            let quote_id = quote["quote"]["id"].as_str().unwrap();
            let (status, result) = f
                .send("POST", "/api/v1/assist/drafts", body, Some(quote_id), true)
                .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(result["status"], "draft");
            assert_eq!(provider.gate.calls.load(Ordering::SeqCst), 1);
        })
        .await
    });
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert!(
        result.is_ok(),
        "scoped development bearer deadlocked during delivery"
    );
}

#[tokio::test]
async fn free_credit_grants_require_a_platform_admin_browser_session_and_are_idempotent() {
    let provider = FakeProvider::start().await;
    let f = Fixture::new(provider.service.clone(), true);
    let grant = json!({"user_id":"other","amount_micro":500000,"reference":"synthetic-welcome","reason":"Synthetic grant test"});
    assert_eq!(
        f.send("POST", "/api/admin/ai/grants", grant.clone(), None, false)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    f.state
        .store
        .lock()
        .unwrap()
        .conn
        .execute("INSERT INTO auth_admins(user_id) VALUES('dev')", [])
        .unwrap();
    // A token with admin task permission is still not a platform-admin browser session.
    assert_eq!(
        f.send("POST", "/api/admin/ai/grants", grant.clone(), None, true)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    for _ in 0..2 {
        let (status, result) = f
            .send("POST", "/api/admin/ai/grants", grant.clone(), None, false)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(result["saved"], true);
    }
    let db = f.state.store.lock().unwrap();
    let wallet = db.ai_wallet("other").unwrap();
    assert_eq!(wallet.posted_micro, 500000);
    assert_eq!(wallet.free_micro, 500000);
    assert_eq!(wallet.paid_micro, 0);
    let (entries, kind): (i64, String) = db
        .conn
        .query_row(
            "SELECT count(*),min(kind) FROM ai_wallet_entries WHERE user_id='other'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(entries, 1);
    assert_eq!(kind, "free_grant");
    assert_eq!(provider.gate.calls.load(Ordering::SeqCst), 0);
}
