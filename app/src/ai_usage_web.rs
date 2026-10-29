use super::*;
use crate::ai_usage::{Model, ProviderPolicy};
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/account/ai/models", get(models))
        .route("/api/account/ai/usage", get(usage))
        .route("/api/admin/ai/models", get(admin_models).put(update_model))
        .route("/api/admin/ai/provider", get(provider).put(update_provider))
        .route("/api/admin/ai/finance", get(finance))
        .route("/api/admin/ai/adjustments", post(adjust))
        .route("/api/admin/ai/grants", post(grant))
        .route("/api/admin/ai/reconcile", post(reconcile))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("private, no-store"),
        ))
}
async fn browser(s: &AppState, h: &HeaderMap, write: bool) -> Result<String, ApiError> {
    let a = s.auth(h, write)?;
    if a.starts_with("credential:") || h.contains_key("authorization") {
        return Err(Error::Forbidden.into());
    }
    Ok(a)
}
async fn admin(s: &AppState, h: &HeaderMap, write: bool) -> Result<String, ApiError> {
    let a = if write && s.production.is_some() {
        account_http::session_user(s, h, true).await?
    } else {
        browser(s, h, write).await?
    };
    let u = a.clone();
    s.run(move |db| crate::ai_policy::require_admin(&db.conn, &u))
        .await?;
    Ok(a)
}
async fn admin_run<T: Send + 'static>(
    s: &AppState,
    h: &HeaderMap,
    actor: String,
    recent: bool,
    f: impl FnOnce(&mut Store, &str) -> Result<T, Error> + Send + 'static,
) -> Result<T, ApiError> {
    let hash = if s.production.is_some() {
        let token = h
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                v.split(';')
                    .find_map(|p| p.trim().strip_prefix("todoer_session="))
            })
            .ok_or(Error::Forbidden)?;
        Some(crate::auth::hash(token))
    } else {
        None
    };
    s.run(move|db|{
        if let Some(hash)=hash {let live:bool=db.conn.query_row("SELECT EXISTS(SELECT 1 FROM auth_sessions s WHERE s.token_hash=?1 AND s.user_id=?2 AND s.revoked=0 AND s.expires_at>unixepoch() AND (?3=0 OR EXISTS(SELECT 1 FROM auth_recent_sessions r WHERE r.token_hash=s.token_hash AND r.authenticated_at>unixepoch()-300)))",rusqlite::params![hash,actor,recent],|r|r.get(0))?;if !live{return Err(Error::Forbidden);}}
        crate::ai_policy::require_admin(&db.conn,&actor)?;f(db,&actor)
    }).await
}
async fn models(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    browser(&s, &h, false).await?;
    Ok(Json(
        json!({"models":s.run(|db|db.ai_models(false)).await?}),
    ))
}
#[derive(Deserialize)]
struct Page {
    limit: Option<i64>,
    before: Option<String>,
    funding: Option<String>,
    status: Option<String>,
    format: Option<String>,
}
async fn usage(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(p): Query<Page>,
) -> Result<Response, ApiError> {
    let a = browser(&s, &h, false).await?;
    let rows = s
        .run(move |db| {
            db.ai_usage(
                &a,
                p.limit.unwrap_or(50),
                p.before.as_deref(),
                p.funding.as_deref(),
                p.status.as_deref(),
            )
        })
        .await?;
    if p.format.as_deref() == Some("csv") {
        let mut w = csv::Writer::from_writer(Vec::new());
        let fields = [
            "id",
            "kind",
            "funding",
            "model_id",
            "provider",
            "status",
            "input_tokens",
            "output_tokens",
            "cached_tokens",
            "reasoning_tokens",
            "customer_micro",
            "currency",
            "byok_cost_micro_usd",
            "created_at",
        ];
        w.write_record(fields)
            .map_err(|_| Error::Invalid("cannot export usage".into()))?;
        for r in rows {
            w.write_record(fields.iter().map(|k| {
                let cell = r[k].as_str().map(str::to_owned).unwrap_or_else(|| {
                    if r[k].is_null() {
                        String::new()
                    } else {
                        r[k].to_string()
                    }
                });
                if cell.starts_with(['=', '+', '-', '@', '\t', '\r']) {
                    format!("'{cell}")
                } else {
                    cell
                }
            }))
            .map_err(|_| Error::Invalid("cannot export usage".into()))?;
        }
        let bytes = w
            .into_inner()
            .map_err(|_| Error::Invalid("cannot export usage".into()))?;
        return Ok((
            [
                (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=ai-usage.csv",
                ),
            ],
            bytes,
        )
            .into_response());
    }
    let next = rows
        .last()
        .map(|r| format!("{}:{}", r["created_at"], r["id"].as_str().unwrap_or("")));
    Ok(Json(json!({"operations":rows,"next_before":next})).into_response())
}
async fn admin_models(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    let a = admin(&s, &h, false).await?;
    Ok(Json(
        json!({"models":admin_run(&s,&h,a,false,|db,_|db.ai_models(true)).await?}),
    ))
}
async fn update_model(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(m): Json<Model>,
) -> Result<Json<Value>, ApiError> {
    let a = admin(&s, &h, true).await?;
    admin_run(&s, &h, a, true, move |db, a| db.ai_model_update(a, m)).await?;
    Ok(Json(json!({"saved":true})))
}
async fn provider(
    State(s): State<AppState>,
    h: HeaderMap,
) -> Result<Json<ProviderPolicy>, ApiError> {
    let a = admin(&s, &h, false).await?;
    Ok(Json(
        admin_run(&s, &h, a, false, |db, _| db.ai_provider_policy()).await?,
    ))
}
async fn update_provider(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(p): Json<ProviderPolicy>,
) -> Result<Json<Value>, ApiError> {
    let a = admin(&s, &h, true).await?;
    admin_run(&s, &h, a, true, move |db, a| db.ai_provider_update(a, p)).await?;
    Ok(Json(json!({"saved":true})))
}
#[derive(Deserialize)]
struct FinanceQuery {
    user_id: Option<String>,
    from: Option<i64>,
    to: Option<i64>,
}
async fn finance(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<FinanceQuery>,
) -> Result<Json<Value>, ApiError> {
    let a = admin(&s, &h, false).await?;
    Ok(Json(
        admin_run(&s, &h, a, false, move |db, _| {
            db.ai_finance(
                q.user_id.as_deref(),
                q.from.unwrap_or(0),
                q.to.unwrap_or_else(|| chrono::Utc::now().timestamp()),
            )
        })
        .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Adjustment {
    user_id: String,
    amount_micro: i64,
    reference: String,
    reason: String,
}
async fn adjust(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(i): Json<Adjustment>,
) -> Result<Json<Value>, ApiError> {
    let a = admin(&s, &h, true).await?;
    admin_run(&s,&h,a,true,move|db,a|{if i.reason.trim().is_empty()||i.reason.len()>500||i.amount_micro==0||i.amount_micro.unsigned_abs()>1_000_000_000{return Err(Error::Invalid("adjustment requires a reason and bounded amount".into()));}
        if i.amount_micro>0 {return db.ai_grant(a,&i.user_id,i.amount_micro,&format!("adjustment:{}",i.reference),&i.reason);}
        let mut tx=crate::agent_ops::StoreWrite::new(db)?;
        if i.amount_micro<0&&tx.ai_wallet_user(&i.user_id)?.paid_available_micro < -i.amount_micro{return Err(Error::Conflict("adjustment exceeds available balance".into()));}
        tx.ai_credit(&i.user_id,&format!("admin:{}",i.reference),i.amount_micro,"adjustment")?;
        tx.conn.execute("INSERT INTO ai_admin_audit(actor,target,action,reason,before_json,after_json) VALUES(?1,?2,'credit.adjust',?3,'{}',?4)",rusqlite::params![a,i.user_id,i.reason,json!({"amount_micro":i.amount_micro,"reference":i.reference}).to_string()])?;tx.commit()
    }).await?;
    Ok(Json(json!({"saved":true})))
}
async fn grant(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(i): Json<Adjustment>,
) -> Result<Json<Value>, ApiError> {
    let a = admin(&s, &h, true).await?;
    admin_run(&s, &h, a, true, move |db, a| {
        db.ai_grant(a, &i.user_id, i.amount_micro, &i.reference, &i.reason)
    })
    .await?;
    Ok(Json(json!({"saved":true})))
}
async fn reconcile(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    let a = admin(&s, &h, true).await?;
    let released=admin_run(&s,&h,a,true,move|db,a|{let n=db.ai_reconcile_expired()?;db.conn.execute("INSERT INTO ai_usage_audit(actor,action,target) VALUES(?1,'holds.reconcile','expired')",[a])?;Ok(n)}).await?;
    Ok(Json(json!({"released":released})))
}
