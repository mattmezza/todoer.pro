//! Browser-only billing settings and a bounded raw-body Stripe webhook.
use super::*;
use crate::billing::{
    AutoTopupInput, BillingAdminInput, ReconcileInput, StripeService, TopupInput,
};
use axum::body::Bytes;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/account/billing", get(overview))
        .route("/api/account/billing/topups", post(topup))
        .route("/api/admin/billing/reconcile", post(admin_reconcile))
        .route(
            "/api/admin/billing/users/{id}",
            axum::routing::put(admin_update),
        )
        .route(
            "/api/account/billing/auto-topup",
            axum::routing::put(automatic),
        )
        .route(
            "/api/billing/stripe/webhook",
            post(webhook).layer(DefaultBodyLimit::max(262_144)),
        )
        .layer(axum::middleware::map_response(
            |mut response: Response| async move {
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    axum::http::HeaderValue::from_static("private, no-store"),
                );
                response
            },
        ))
}
fn browser(s: &AppState, h: &HeaderMap, write: bool, recent: bool) -> Result<String, ApiError> {
    if write && !s.origin_ok(h) {
        return Err(Error::Forbidden.into());
    }
    if h.contains_key(header::AUTHORIZATION) {
        return Err(Error::Forbidden.into());
    }
    let session = if s.production.is_some() {
        h.get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|c| {
                c.split(';')
                    .find_map(|v| v.trim().strip_prefix("todoer_session="))
            })
    } else {
        session_cookie(h)
    }
    .ok_or(Error::Forbidden)?;
    let user = s.auth(h, write)?;
    if user.starts_with("credential:") {
        return Err(Error::Forbidden.into());
    }
    if recent && let Some(auth) = &s.production {
        let confirmed = auth
            .store
            .lock()
            .map_err(|_| Error::Forbidden)?
            .recent(session)?;
        if confirmed != user {
            return Err(Error::Forbidden.into());
        }
    }
    Ok(user)
}
async fn change<T: Send + 'static>(
    s: &AppState,
    h: &HeaderMap,
    user: String,
    recent: bool,
    f: impl FnOnce(&mut Store, &str) -> Result<T, Error> + Send + 'static,
) -> Result<T, ApiError> {
    let hash = if s.production.is_some() {
        let token = h
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|c| {
                c.split(';')
                    .find_map(|v| v.trim().strip_prefix("todoer_session="))
            })
            .ok_or(Error::Forbidden)?;
        Some(crate::auth::hash(token))
    } else {
        None
    };
    s.run(move|db|{
        if let Some(hash)=hash{
            let live:bool=db.conn.query_row("SELECT EXISTS(SELECT 1 FROM auth_sessions s WHERE s.token_hash=?1 AND s.user_id=?2 AND s.revoked=0 AND s.expires_at>unixepoch() AND (?3=0 OR EXISTS(SELECT 1 FROM auth_recent_sessions r WHERE r.token_hash=s.token_hash AND r.authenticated_at>unixepoch()-300)))",rusqlite::params![hash,user,recent],|r|r.get(0))?;
            if !live{return Err(Error::Forbidden);}
        }
        f(db,&user)
    }).await
}
fn service() -> Result<StripeService, ApiError> {
    StripeService::from_env()?.ok_or_else(|| {
        Error::Invalid("Credit purchases are not enabled on this instance".into()).into()
    })
}
async fn admin_update(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(target): Path<String>,
    Json(input): Json<BillingAdminInput>,
) -> Result<Json<Value>, ApiError> {
    let actor = browser(&s, &h, true, true)?;
    Ok(Json(
        change(&s, &h, actor, true, move |db, admin| {
            db.billing_admin_update(admin, &target, input)
        })
        .await?,
    ))
}
async fn admin_reconcile(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<ReconcileInput>,
) -> Result<Json<Value>, ApiError> {
    let actor = browser(&s, &h, true, true)?;
    let purchase = input.purchase_id.clone();
    change(&s, &h, actor.clone(), true, move |db, admin| {
        crate::ai_policy::require_admin(&db.conn, admin)?;
        db.billing_purchase(&purchase)?;
        Ok(())
    })
    .await?;
    let service = StripeService::for_reconciliation()?
        .ok_or_else(|| Error::Invalid("Stripe is not configured".into()))?;
    let value = service.payment(&input.payment_intent_id).await?;
    browser(&s, &h, true, true)?;
    Ok(Json(
        change(&s, &h, actor, true, move |db, admin| {
            db.billing_admin_reconcile(admin, input, &value, &service)
        })
        .await?,
    ))
}
async fn overview(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    let user = browser(&s, &h, false, false)?;
    let configured = StripeService::from_env()?;
    let currencies = configured
        .as_ref()
        .map(StripeService::currencies)
        .unwrap_or(json!([{"code":"EUR","eur_rate_micros":1000000}]));
    let enabled = configured.is_some();
    let mut value = s.run(move |db| db.billing_overview(&user)).await?;
    value["enabled"] = json!(enabled);
    value["currencies"] = currencies;
    Ok(Json(value))
}
async fn topup(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<TopupInput>,
) -> Result<Json<Value>, ApiError> {
    let user = browser(&s, &h, true, true)?;
    let service = service()?;
    let cloned = service.clone();
    let purchase = change(&s, &h, user.clone(), true, move |db, account| {
        db.billing_begin(account, input, &cloned)
    })
    .await?;
    if let Some(url) = purchase.checkout_url {
        return Ok(Json(
            json!({"purchase_id":purchase.id,"checkout_url":url,"status":purchase.status}),
        ));
    }
    if purchase.status != "created" || purchase.created_at < chrono::Utc::now().timestamp() - 600 {
        return Err(Error::Conflict(
            "This purchase requires reconciliation; review billing status".into(),
        )
        .into());
    }
    let customer = change(&s, &h, user.clone(), true, move |db, account| {
        db.billing_customer(account)
    })
    .await?;
    let customer = match customer {
        Some(id) => id,
        None => {
            let customer = service.customer(&user).await?;
            browser(&s, &h, true, true)?;
            let id = customer.clone();
            change(&s, &h, user.clone(), true, move |db, account| {
                db.billing_save_customer(account, &id)
            })
            .await?;
            customer
        }
    };
    browser(&s, &h, true, true)?;
    let response = service.checkout(&purchase, &customer, &s.origin).await?;
    let id = purchase.id.clone();
    let value = response.clone();
    s.run(move |db| db.billing_checkout_saved(&id, &value))
        .await?;
    Ok(Json(
        json!({"purchase_id":purchase.id,"checkout_url":response["url"],"status":"pending"}),
    ))
}
async fn automatic(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(input): Json<AutoTopupInput>,
) -> Result<Json<Value>, ApiError> {
    let user = browser(&s, &h, true, true)?;
    if input.enabled {
        let service = service()?;
        let account = user.clone();
        let stale = s
            .run(move |db| db.billing_authentication_payments(&account))
            .await?;
        for (purchase, payment) in stale {
            let value = service.cancel_payment(&payment).await?;
            s.run(move |db| db.billing_save_payment_response(&purchase, &value))
                .await?;
        }
        browser(&s, &h, true, true)?;
    }
    Ok(Json(
        change(&s, &h, user, true, move |db, account| {
            db.billing_set_auto(account, input)
        })
        .await?,
    ))
}
async fn webhook(
    State(s): State<AppState>,
    h: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let service = StripeService::for_reconciliation()?
        .ok_or_else(|| Error::Invalid("Stripe webhook is not configured".into()))?;
    let signature = h
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Error::Invalid("Payment signature required".into()))?;
    let mut event = service.verify(&body, signature, chrono::Utc::now().timestamp())?;
    if matches!(
        event["type"].as_str(),
        Some("charge.refunded" | "charge.dispute.created" | "charge.dispute.closed")
    ) && event["data"]["object"]["metadata"]["purchase_id"].is_null()
        && let Some(payment) = event["data"]["object"]["payment_intent"].as_str()
    {
        let value = service.payment(payment).await?;
        if let Some(purchase) = value["metadata"]["purchase_id"].as_str() {
            event["data"]["object"]["metadata"] = json!({"purchase_id":purchase});
        }
    }
    s.run(move |db| db.billing_event(&event)).await?;
    Ok(Json(json!({"received":true})))
}
pub(super) fn start_worker(s: AppState) -> Option<tokio::task::JoinHandle<()>> {
    let service = StripeService::for_reconciliation().ok().flatten()?;
    Some(tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            if s.stopping.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let _ = crate::billing::tick(s.store.clone(), &service).await;
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    #[tokio::test]
    async fn billing_is_cookie_only_and_mutations_require_csrf() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let s = AppState::new(
            Arc::new(Mutex::new(db)),
            "synthetic-billing-token-at-least-24".into(),
            "http://127.0.0.1:8092".into(),
        );
        let csrf = s.csrf.clone();
        let app = router(s);
        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/account/billing")
                    .header(
                        "authorization",
                        "Bearer synthetic-billing-token-at-least-24",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::FORBIDDEN);
        let signin = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/signin")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("origin", "http://127.0.0.1:8092")
                    .body(Body::from(format!("csrf={csrf}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(signin.status(), StatusCode::SEE_OTHER);
        let cookie = signin.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/account/billing")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let rejected=app.clone().oneshot(Request::builder().method("PUT").uri("/api/account/billing/auto-topup").header("cookie",&cookie).header("origin","http://127.0.0.1:8092").header("content-type","application/json").body(Body::from(json!({"enabled":false,"amount_minor":1000,"threshold_minor":200,"monthly_cap_minor":5000,"cooldown_seconds":3600,"consent":false}).to_string())).unwrap()).await.unwrap();
        assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
        let admin_rejected=app.clone().oneshot(Request::builder().method("PUT").uri("/api/admin/billing/users/other").header("cookie",&cookie).header("origin","http://127.0.0.1:8092").header("x-csrf-token",&csrf).header("content-type","application/json").body(Body::from(json!({"blocked":true,"forgive_debt":false,"reason":"Synthetic admin review"}).to_string())).unwrap()).await.unwrap();
        assert_eq!(admin_rejected.status(), StatusCode::FORBIDDEN);
        let reconcile_rejected=app.clone().oneshot(Request::builder().method("POST").uri("/api/admin/billing/reconcile").header("cookie",&cookie).header("origin","http://127.0.0.1:8092").header("x-csrf-token",&csrf).header("content-type","application/json").body(Body::from(json!({"purchase_id":"synthetic-purchase","payment_intent_id":"pi_synthetic","reason":"Synthetic admin reconciliation"}).to_string())).unwrap()).await.unwrap();
        assert_eq!(reconcile_rejected.status(), StatusCode::FORBIDDEN);
        let forbidden = app
            .oneshot(
                Request::builder()
                    .uri("/api/account/billing")
                    .header("cookie", &cookie)
                    .header(
                        "authorization",
                        "Bearer synthetic-billing-token-at-least-24",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    }
}
