//! Prepaid purchases: immutable local binding, signed confirmation, durable retries.
//! Card data and raw webhook payloads are never stored.
use crate::core::{Error, Store, actor_identity};
use hmac::{Hmac, Mac};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;

fn invalid(s: &str) -> Error {
    Error::Invalid(s.into())
}
const MAX_TOPUP_MICRO: i64 = 1_000_000_000;
const STRIPE_VERSION: &str = "2026-10-28.endive";
const CONSENT_VERSION: &str = "auto-topup-v1";

#[derive(Clone)]
pub struct StripeService {
    secret: String,
    webhook_secret: String,
    client: reqwest::Client,
    endpoint: String,
    rates: BTreeMap<String, i64>,
    accepts_purchases: bool,
    fee_bps: i64,
    fee_fixed_micro: i64,
}
fn configured_fee(name: &str, default: i64, maximum: i64) -> Result<i64, Error> {
    let value = std::env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .parse::<i64>()
        .map_err(|_| invalid("Invalid billing fee configuration"))?;
    if !(0..=maximum).contains(&value) {
        return Err(invalid("Invalid billing fee configuration"));
    }
    Ok(value)
}
#[derive(Debug, Serialize)]
pub struct CreditQuote {
    pub currency: String,
    pub amount_minor: i64,
    pub gross_micro: i64,
    pub payment_fee_micro: i64,
    pub forex_fee_micro: i64,
    pub credit_micro: i64,
    pub rate_micros: i64,
}
impl StripeService {
    pub fn from_env() -> Result<Option<Self>, Error> {
        Self::load(false)
    }
    /// Disabling new purchases must not discard webhooks or pending reconciliation.
    pub fn for_reconciliation() -> Result<Option<Self>, Error> {
        Self::load(true)
    }
    fn load(reconcile: bool) -> Result<Option<Self>, Error> {
        let enabled = std::env::var("TODOER_BILLING_ENABLED").as_deref() == Ok("1");
        if !enabled && (!reconcile || std::env::var("TODOER_STRIPE_SECRET_KEY").is_err()) {
            return Ok(None);
        }
        let secret = std::env::var("TODOER_STRIPE_SECRET_KEY")
            .map_err(|_| invalid("Stripe is not configured"))?;
        let webhook_secret = std::env::var("TODOER_STRIPE_WEBHOOK_SECRET")
            .map_err(|_| invalid("Stripe webhook is not configured"))?;
        if stripe_key_livemode(&secret).is_none() || !webhook_secret.starts_with("whsec_") {
            return Err(invalid("Invalid Stripe configuration"));
        }
        let mut rates = BTreeMap::from([("eur".to_string(), 1_000_000)]);
        if let Ok(value) = std::env::var("TODOER_BILLING_CURRENCY_RATES") {
            let configured: BTreeMap<String, i64> = serde_json::from_str(&value)
                .map_err(|_| invalid("Invalid billing currency rates"))?;
            for (currency, rate) in configured {
                if !matches!(currency.as_str(), "usd" | "gbp" | "cad")
                    || !(100_000..=10_000_000).contains(&rate)
                {
                    return Err(invalid("Invalid billing currency rates"));
                }
                rates.insert(currency, rate);
            }
        }
        Ok(Some(Self {
            secret,
            webhook_secret,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(15))
                .build()
                .map_err(|_| invalid("Stripe client unavailable"))?,
            endpoint: "https://api.stripe.com/v1".into(),
            rates,
            accepts_purchases: enabled,
            fee_bps: configured_fee("TODOER_BILLING_PAYMENT_FEE_BPS", 0, 5000)?,
            fee_fixed_micro: configured_fee(
                "TODOER_BILLING_PAYMENT_FEE_FIXED_MICRO_EUR",
                0,
                1_000_000,
            )?,
        }))
    }
    pub fn currencies(&self) -> Value {
        json!(
            self.rates
                .iter()
                .map(|(code, rate)| json!({"code":code.to_uppercase(),"eur_rate_micros":rate,"payment_fee_bps":self.fee_bps,"payment_fee_fixed_micro":self.fee_fixed_micro,"forex_fee_bps":if code=="eur"{0}else{100}}))
                .collect::<Vec<_>>()
        )
    }
    pub fn quote(&self, amount_minor: i64, currency: &str) -> Result<CreditQuote, Error> {
        let code = currency.to_lowercase();
        let rate = *self
            .rates
            .get(&code)
            .ok_or_else(|| invalid("Currency is not enabled"))?;
        let gross = i128::from(amount_minor) * 10_000_000_000 / i128::from(rate);
        if !(5_000_000..=i128::from(MAX_TOPUP_MICRO)).contains(&gross) {
            return Err(invalid(
                "Payment must be between EUR 5 and EUR 1000 before conversion deductions",
            ));
        }
        let gross = gross as i64;
        // This is the operator's published fee allowance, not a claim about Stripe's
        // eventual card-specific fee. The wallet credit is fixed before Checkout.
        let payment_fee = ((i128::from(gross) * i128::from(self.fee_bps) + 9999) / 10000) as i64
            + self.fee_fixed_micro;
        let forex_fee = if code == "eur" { 0 } else { (gross + 99) / 100 };
        let credit = gross - payment_fee - forex_fee;
        if credit <= 0 {
            return Err(invalid("Payment is too small after conversion deductions"));
        }
        Ok(CreditQuote {
            currency: code,
            amount_minor,
            gross_micro: gross,
            payment_fee_micro: payment_fee,
            forex_fee_micro: forex_fee,
            credit_micro: credit,
            rate_micros: rate,
        })
    }
    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        form: &[(String, String)],
        key: Option<&str>,
    ) -> Result<Value, Error> {
        let mut req = self
            .client
            .request(method, format!("{}{path}", self.endpoint))
            .bearer_auth(&self.secret)
            .header("Stripe-Version", STRIPE_VERSION);
        if let Some(k) = key {
            req = req.header("Idempotency-Key", k);
        }
        if !form.is_empty() {
            req = req.form(form);
        }
        let mut response = req.send().await.map_err(|_| {
            invalid("Payment service unavailable; check purchase status before retrying")
        })?;
        let success = response.status().is_success();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| invalid("Payment response interrupted"))?
        {
            if bytes.len() + chunk.len() > 262_144 {
                return Err(invalid("Invalid payment response"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let body: Value =
            serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid payment response"))?;
        if !success {
            let intent = &body["error"]["payment_intent"];
            if intent["id"].as_str().is_some_and(|s| s.starts_with("pi_"))
                && intent["status"].as_str().is_some()
            {
                return Ok(intent.clone());
            }
            return Err(invalid(
                "Payment was not completed; check billing before retrying",
            ));
        }
        Ok(body)
    }
    pub async fn customer(&self, user: &str) -> Result<String, Error> {
        use sha2::Digest;
        let key = format!("todoer-customer-{:x}", Sha256::digest(user.as_bytes()));
        let v = self
            .request(reqwest::Method::POST, "/customers", &[], Some(&key))
            .await?;
        id(&v, "id", "cus_")
    }
    pub async fn checkout(
        &self,
        p: &Purchase,
        customer: &str,
        origin: &str,
    ) -> Result<Value, Error> {
        let url = reqwest::Url::parse(origin).map_err(|_| invalid("Invalid app origin"))?;
        if !(url.scheme() == "https"
            || (url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))))
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(invalid("Invalid app origin"));
        }
        let mut f = vec![
            ("mode".into(), "payment".into()),
            ("customer".into(), customer.into()),
            (
                "success_url".into(),
                format!("{origin}/settings/billing?purchase={}", p.id),
            ),
            ("cancel_url".into(), format!("{origin}/settings/billing")),
            ("client_reference_id".into(), p.id.clone()),
            ("metadata[purchase_id]".into(), p.id.clone()),
            (
                "payment_intent_data[metadata][purchase_id]".into(),
                p.id.clone(),
            ),
            (
                "line_items[0][price_data][currency]".into(),
                p.currency.clone(),
            ),
            (
                "line_items[0][price_data][unit_amount]".into(),
                p.amount_minor.to_string(),
            ),
            (
                "line_items[0][price_data][product_data][name]".into(),
                format!(
                    "Todoer AI credits · EUR {}.{:06}",
                    p.credit_micro / 1_000_000,
                    p.credit_micro % 1_000_000
                ),
            ),
            ("line_items[0][quantity]".into(), "1".into()),
        ];
        if p.save_method {
            f.push((
                "payment_intent_data[setup_future_usage]".into(),
                "off_session".into(),
            ));
        }
        self.request(
            reqwest::Method::POST,
            "/checkout/sessions",
            &f,
            Some(&format!("todoer-checkout-{}", p.id)),
        )
        .await
    }
    pub async fn automatic(
        &self,
        p: &Purchase,
        customer: &str,
        method: &str,
    ) -> Result<Value, Error> {
        self.request(
            reqwest::Method::POST,
            "/payment_intents",
            &[
                ("amount".into(), p.amount_minor.to_string()),
                ("currency".into(), p.currency.clone()),
                ("customer".into(), customer.into()),
                ("payment_method".into(), method.into()),
                ("off_session".into(), "true".into()),
                ("confirm".into(), "true".into()),
                ("metadata[purchase_id]".into(), p.id.clone()),
            ],
            Some(&format!("todoer-auto-{}", p.id)),
        )
        .await
    }
    pub async fn cancel_payment(&self, payment: &str) -> Result<Value, Error> {
        if !payment.starts_with("pi_")
            || !payment
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(invalid("Invalid payment reference"));
        }
        self.request(
            reqwest::Method::POST,
            &format!("/payment_intents/{payment}/cancel"),
            &[],
            Some(&format!("todoer-cancel-{payment}")),
        )
        .await
    }
    pub async fn session(&self, session: &str) -> Result<Value, Error> {
        if !session.starts_with("cs_")
            || !session
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(invalid("Invalid checkout reference"));
        }
        self.request(
            reqwest::Method::GET,
            &format!("/checkout/sessions/{session}"),
            &[],
            None,
        )
        .await
    }
    pub async fn payment(&self, payment: &str) -> Result<Value, Error> {
        if !payment.starts_with("pi_")
            || !payment
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(invalid("Invalid payment reference"));
        }
        self.request(
            reqwest::Method::GET,
            &format!("/payment_intents/{payment}?expand%5B%5D=latest_charge.balance_transaction"),
            &[],
            None,
        )
        .await
    }
    pub fn verify(&self, body: &[u8], signature: &str, at: i64) -> Result<Value, Error> {
        if body.len() > 262_144 || signature.len() > 4096 {
            return Err(invalid("Invalid payment event"));
        }
        let mut timestamp = None;
        let mut signatures = Vec::new();
        for field in signature.split(',') {
            if let Some((k, v)) = field.split_once('=') {
                match k {
                    "t" => timestamp = v.parse::<i64>().ok(),
                    "v1" => signatures.push(v),
                    _ => {}
                }
            }
        }
        let t = timestamp.ok_or_else(|| invalid("Invalid payment signature"))?;
        if (i128::from(at) - i128::from(t)).abs() > 300 {
            return Err(invalid("Expired payment signature"));
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(self.webhook_secret.as_bytes())
            .map_err(|_| invalid("Invalid webhook configuration"))?;
        mac.update(t.to_string().as_bytes());
        mac.update(b".");
        mac.update(body);
        let valid = signatures.into_iter().any(|signature| {
            let mut decoded = [0u8; 32];
            if signature.len() != 64 || !signature.is_ascii() {
                return false;
            }
            for (i, byte) in decoded.iter_mut().enumerate() {
                let Ok(b) = u8::from_str_radix(&signature[i * 2..i * 2 + 2], 16) else {
                    return false;
                };
                *byte = b;
            }
            mac.clone().verify_slice(&decoded).is_ok()
        });
        if !valid {
            return Err(invalid("Invalid payment signature"));
        }
        let event: Value =
            serde_json::from_slice(body).map_err(|_| invalid("Invalid payment event"))?;
        let livemode = stripe_key_livemode(&self.secret)
            .ok_or_else(|| invalid("Invalid Stripe configuration"))?;
        if event["livemode"].as_bool() != Some(livemode) {
            return Err(invalid("Payment environment mismatch"));
        }
        Ok(event)
    }
}
fn stripe_key_livemode(secret: &str) -> Option<bool> {
    if secret.starts_with("sk_live_") || secret.starts_with("rk_live_") {
        Some(true)
    } else if secret.starts_with("sk_test_") || secret.starts_with("rk_test_") {
        Some(false)
    } else {
        None
    }
}
fn id(value: &Value, key: &str, prefix: &str) -> Result<String, Error> {
    value[key]
        .as_str()
        .filter(|s| {
            s.starts_with(prefix)
                && s.len() < 256
                && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        })
        .map(str::to_string)
        .ok_or_else(|| invalid("Invalid payment reference"))
}
#[derive(Clone, Serialize)]
pub struct Purchase {
    pub id: String,
    pub user_id: String,
    pub amount_minor: i64,
    pub currency: String,
    pub credit_micro: i64,
    pub gross_micro: i64,
    pub payment_fee_allowance_micro: i64,
    pub forex_fee_micro: i64,
    pub status: String,
    pub created_at: i64,
    #[serde(skip)]
    pub save_method: bool,
    #[serde(skip)]
    pub mode: String,
    #[serde(skip)]
    pub session: Option<String>,
    #[serde(skip)]
    pub payment: Option<String>,
    pub checkout_url: Option<String>,
    pub receipt_url: Option<String>,
    pub paid_at: Option<i64>,
}
fn purchase_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Purchase> {
    Ok(Purchase {
        id: r.get(0)?,
        user_id: r.get(1)?,
        amount_minor: r.get(2)?,
        currency: r.get(3)?,
        credit_micro: r.get(4)?,
        status: r.get(5)?,
        created_at: r.get(6)?,
        save_method: r.get(7)?,
        mode: r.get(8)?,
        session: r.get(9)?,
        payment: r.get(10)?,
        checkout_url: r.get(11)?,
        receipt_url: r.get(12)?,
        paid_at: r.get(13)?,
        gross_micro: r.get(14)?,
        payment_fee_allowance_micro: r.get(15)?,
        forex_fee_micro: r.get(16)?,
    })
}
const PURCHASE_COLUMNS: &str = "id,user_id,amount_minor,currency,credit_micro,status,created_at,save_method,mode,stripe_session_id,stripe_payment_id,checkout_url,receipt_url,paid_at,gross_micro,payment_fee_allowance_micro,forex_fee_micro";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopupInput {
    pub amount_minor: i64,
    pub currency: String,
    pub idempotency_key: String,
    #[serde(default)]
    pub save_for_auto_topup: bool,
    #[serde(default)]
    pub quoted_credit_micro: Option<i64>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutoTopupInput {
    pub enabled: bool,
    pub amount_minor: i64,
    pub threshold_minor: i64,
    pub monthly_cap_minor: i64,
    pub cooldown_seconds: i64,
    #[serde(default)]
    pub consent: bool,
    #[serde(default)]
    pub quoted_credit_micro: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BillingAdminInput {
    pub blocked: bool,
    pub forgive_debt: bool,
    pub reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconcileInput {
    pub purchase_id: String,
    pub payment_intent_id: String,
    pub reason: String,
}

impl Store {
    /// Explicit operational review: debt write-offs are not customer credits or cash.
    pub fn billing_admin_update(
        &mut self,
        actor: &str,
        target: &str,
        input: BillingAdminInput,
    ) -> Result<Value, Error> {
        let reason = input.reason.trim();
        if reason.is_empty() || reason.len() > 500 || reason.chars().any(char::is_control) {
            return Err(invalid("Provide an audit reason (up to 500 bytes)"));
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let admin = crate::ai_policy::require_admin(&tx, actor)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE id=?1)",
            [target],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::NotFound);
        }
        tx.execute(
            "INSERT OR IGNORE INTO billing_accounts(user_id) VALUES(?1)",
            [target],
        )?;
        let (before_blocked, debt): (bool, i64) = tx.query_row(
            "SELECT blocked,debt_micro FROM billing_accounts WHERE user_id=?1",
            [target],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if !input.blocked && debt > 0 && !input.forgive_debt {
            return Err(invalid(
                "Explicit debt forgiveness is required before unblocking this account",
            ));
        }
        let remaining = if input.forgive_debt { 0 } else { debt };
        let writeoff = debt - remaining;
        tx.execute(
            "UPDATE billing_accounts SET blocked=?2,debt_micro=?3 WHERE user_id=?1",
            params![target, input.blocked, remaining],
        )?;
        // An administrative unblock never silently authorizes future card charges.
        tx.execute(
            "UPDATE billing_auto_topup SET enabled=0,paused_reason=?2 WHERE user_id=?1",
            params![
                target,
                if input.blocked {
                    "admin_blocked"
                } else {
                    "admin_review_complete"
                }
            ],
        )?;
        let before = json!({"billing_blocked":before_blocked,"debt_micro":debt});
        let after = json!({"billing_blocked":input.blocked,"debt_micro":remaining,"writeoff_micro":writeoff});
        tx.execute("INSERT INTO ai_admin_audit(actor,target,action,reason,before_json,after_json) VALUES(?1,?2,?3,?4,?5,?6)",params![admin,target,if writeoff>0{"billing.writeoff"}else{"billing.access"},reason,before.to_string(),after.to_string()])?;
        tx.commit()?;
        Ok(
            json!({"user_id":target,"billing_blocked":input.blocked,"debt_micro":remaining,"writeoff_micro":writeoff}),
        )
    }
    /// Bind only a PaymentIntent fetched from Stripe by the trusted HTTP service.
    /// Never creates, confirms or retries a charge.
    pub fn billing_admin_reconcile(
        &mut self,
        actor: &str,
        input: ReconcileInput,
        value: &Value,
        service: &StripeService,
    ) -> Result<Value, Error> {
        let reason = input.reason.trim();
        if reason.is_empty() || reason.len() > 500 || reason.chars().any(char::is_control) {
            return Err(invalid("Provide an audit reason (up to 500 bytes)"));
        }
        let mut tx = crate::agent_ops::StoreWrite::new(self)?;
        let admin = crate::ai_policy::require_admin(&tx.conn, actor)?;
        let payment = id_value(value, "id", "pi_")?;
        let purchase = tx.billing_purchase(&input.purchase_id)?;
        if payment != input.payment_intent_id
            || value["metadata"]["purchase_id"].as_str() != Some(purchase.id.as_str())
            || value["amount"].as_i64() != Some(purchase.amount_minor)
            || value["currency"].as_str() != Some(purchase.currency.as_str())
        {
            return Err(invalid("Payment purchase, amount or currency mismatch"));
        }
        if purchase
            .payment
            .as_ref()
            .is_some_and(|saved| saved != &payment)
        {
            return Err(Error::Conflict(
                "Purchase is already bound to a different payment".into(),
            ));
        }
        let other: bool = tx.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM billing_purchases WHERE stripe_payment_id=?1 AND id!=?2)",
            params![payment, purchase.id],
            |r| r.get(0),
        )?;
        if other {
            return Err(Error::Conflict(
                "Payment is already bound to a different purchase".into(),
            ));
        }
        let customer: Option<String> = tx
            .conn
            .query_row(
                "SELECT stripe_customer_id FROM billing_accounts WHERE user_id=?1",
                [&purchase.user_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if customer.is_none() || customer.as_deref() != value["customer"].as_str() {
            return Err(invalid("Payment customer mismatch"));
        }
        let before = json!({"purchase_id":purchase.id,"status":purchase.status,"payment_intent_id":purchase.payment});
        tx.billing_save_payment_response(&purchase.id, value)?;
        tx.billing_payment_details(&purchase.id, value, &service.rates)?;
        let saved = tx.billing_purchase(&purchase.id)?;
        let after =
            json!({"purchase_id":saved.id,"status":saved.status,"payment_intent_id":saved.payment});
        tx.conn.execute("INSERT INTO ai_admin_audit(actor,target,action,reason,before_json,after_json) VALUES(?1,?2,'billing.reconcile',?3,?4,?5)",params![admin,purchase.user_id,reason,before.to_string(),after.to_string()])?;
        tx.commit()?;
        Ok(after)
    }
    pub fn billing_customer(&mut self, user: &str) -> Result<Option<String>, Error> {
        self.conn.execute(
            "INSERT OR IGNORE INTO billing_accounts(user_id) VALUES(?1)",
            [user],
        )?;
        Ok(self.conn.query_row(
            "SELECT stripe_customer_id FROM billing_accounts WHERE user_id=?1",
            [user],
            |r| r.get(0),
        )?)
    }
    pub fn billing_save_customer(&self, user: &str, customer: &str) -> Result<(), Error> {
        self.conn.execute("UPDATE billing_accounts SET stripe_customer_id=?2 WHERE user_id=?1 AND (stripe_customer_id IS NULL OR stripe_customer_id=?2)",params![user,customer])?;
        Ok(())
    }
    pub fn billing_begin(
        &mut self,
        actor: &str,
        input: TopupInput,
        service: &StripeService,
    ) -> Result<Purchase, Error> {
        let (user, credential) = actor_identity(&self.conn, actor)?;
        if credential.is_some() {
            return Err(Error::Forbidden);
        }
        if input.idempotency_key.is_empty()
            || input.idempotency_key.len() > 128
            || input.idempotency_key.chars().any(char::is_control)
        {
            return Err(invalid("Invalid purchase request key"));
        }
        let currency = input.currency.to_lowercase();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.query_row(
            "SELECT coalesce((SELECT blocked FROM billing_accounts WHERE user_id=?1),0)",
            [&user],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(invalid("Billing requires administrator review"));
        }
        let prior=tx.query_row(&format!("SELECT {PURCHASE_COLUMNS} FROM billing_purchases WHERE user_id=?1 AND idempotency_key=?2"),params![user,input.idempotency_key],purchase_row).optional()?;
        if let Some(p) = prior {
            if p.amount_minor != input.amount_minor
                || p.currency != currency
                || p.save_method != input.save_for_auto_topup
            {
                return Err(Error::Conflict(
                    "Purchase request key was used with different details".into(),
                ));
            }
            return Ok(p);
        }
        let quote = service.quote(input.amount_minor, &input.currency)?;
        if input
            .quoted_credit_micro
            .is_some_and(|accepted| accepted != quote.credit_micro)
        {
            return Err(Error::Conflict(
                "Conversion changed. Refresh billing and review the credit amount before paying."
                    .into(),
            ));
        }
        let pending:i64=tx.query_row("SELECT count(*) FROM billing_purchases WHERE user_id=?1 AND status IN ('created','pending') AND created_at>unixepoch()-3600",[&user],|r|r.get(0))?;
        if pending >= 5 {
            return Err(invalid("Too many pending top-ups; review billing status"));
        }
        let id = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO billing_purchases(id,user_id,idempotency_key,amount_minor,currency,credit_micro,fx_rate_micros,mode,save_method,gross_micro,payment_fee_allowance_micro,forex_fee_micro) VALUES(?1,?2,?3,?4,?5,?6,?7,'manual',?8,?9,?10,?11)",params![id,user,input.idempotency_key,input.amount_minor,currency,quote.credit_micro,quote.rate_micros,input.save_for_auto_topup,quote.gross_micro,quote.payment_fee_micro,quote.forex_fee_micro])?;
        tx.commit()?;
        self.billing_purchase(&id)
    }
    pub fn billing_purchase(&self, id: &str) -> Result<Purchase, Error> {
        self.conn
            .query_row(
                &format!("SELECT {PURCHASE_COLUMNS} FROM billing_purchases WHERE id=?1"),
                [id],
                purchase_row,
            )
            .optional()?
            .ok_or(Error::NotFound)
    }
    pub fn billing_checkout_saved(&self, id: &str, value: &Value) -> Result<(), Error> {
        let session = id_value(value, "id", "cs_")?;
        let url = value["url"]
            .as_str()
            .filter(|s| s.starts_with("https://checkout.stripe.com/") && s.len() < 4096)
            .ok_or_else(|| invalid("Invalid checkout redirect"))?;
        self.conn.execute("UPDATE billing_purchases SET stripe_session_id=?2,checkout_url=?3,stripe_payment_id=coalesce(stripe_payment_id,?4),status=CASE WHEN status='created' THEN 'pending' ELSE status END,updated_at=unixepoch() WHERE id=?1 AND (stripe_session_id IS NULL OR stripe_session_id=?2)",params![id,session,url,value["payment_intent"].as_str()])?;
        Ok(())
    }
    pub fn billing_auto(&self, user: &str) -> Result<Value, Error> {
        Ok(self.conn.query_row("SELECT enabled,amount_micro/10000,threshold_micro/10000,monthly_cap_micro/10000,cooldown_seconds,paused_reason,payment_method_id IS NOT NULL FROM billing_auto_topup WHERE user_id=?1",[user],|r|Ok(json!({"enabled":r.get::<_,bool>(0)?,"amount_minor":r.get::<_,i64>(1)?,"threshold_minor":r.get::<_,i64>(2)?,"monthly_cap_minor":r.get::<_,i64>(3)?,"cooldown_seconds":r.get::<_,i64>(4)?,"paused_reason":r.get::<_,String>(5)?,"payment_method_configured":r.get::<_,bool>(6)?}))).optional()?.unwrap_or(json!({"enabled":false,"amount_minor":1000,"threshold_minor":200,"monthly_cap_minor":5000,"cooldown_seconds":3600,"paused_reason":"","payment_method_configured":false})))
    }
    pub fn billing_set_auto(
        &mut self,
        actor: &str,
        input: AutoTopupInput,
        service: Option<&StripeService>,
    ) -> Result<Value, Error> {
        let (user, credential) = actor_identity(&self.conn, actor)?;
        if credential.is_some() {
            return Err(Error::Forbidden);
        }
        if !(500..=100_000).contains(&input.amount_minor)
            || !(0..=100_000).contains(&input.threshold_minor)
            || !(input.amount_minor..=1_000_000).contains(&input.monthly_cap_minor)
            || !(60..=604800).contains(&input.cooldown_seconds)
        {
            return Err(invalid("Invalid auto-top-up limits"));
        }
        let (fee_bps, fee_fixed) = if input.enabled {
            let service = service.ok_or_else(|| invalid("Billing is not enabled"))?;
            let quote = service.quote(input.amount_minor, "eur")?;
            if input
                .quoted_credit_micro
                .is_some_and(|accepted| accepted != quote.credit_micro)
            {
                return Err(Error::Conflict("Conversion changed. Refresh billing and review the credits before enabling auto-top-up.".into()));
            }
            (service.fee_bps, service.fee_fixed_micro)
        } else {
            (0, 0)
        };
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if input.enabled {
            if !input.consent {
                return Err(invalid(
                    "Explicit consent is required for automatic payments",
                ));
            }
            let eligible:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM ai_account_policy p JOIN billing_accounts b USING(user_id) JOIN billing_auto_topup a USING(user_id) WHERE p.user_id=?1 AND p.user_ai_enabled=1 AND p.managed_ai_allowed=1 AND p.funding='credits' AND b.blocked=0 AND b.stripe_customer_id IS NOT NULL AND a.payment_method_id IS NOT NULL AND EXISTS(SELECT 1 FROM ai_models m JOIN ai_provider_policy pp ON pp.id=m.provider WHERE m.id=p.model_id AND m.enabled=1 AND pp.enabled=1))",[&user],|r|r.get(0))?;
            if !eligible {
                return Err(invalid(
                    "Enable managed AI and complete a consented top-up with a saved payment method first",
                ));
            }
        }
        tx.execute("INSERT INTO billing_auto_topup(user_id,enabled,amount_micro,threshold_micro,monthly_cap_micro,cooldown_seconds,consent_version,consent_at) VALUES(?1,?2,?3,?4,?5,?6,?7,unixepoch()) ON CONFLICT(user_id) DO UPDATE SET enabled=excluded.enabled,amount_micro=excluded.amount_micro,threshold_micro=excluded.threshold_micro,monthly_cap_micro=excluded.monthly_cap_micro,cooldown_seconds=excluded.cooldown_seconds,consent_version=CASE WHEN excluded.enabled=1 THEN excluded.consent_version ELSE consent_version END,consent_at=CASE WHEN excluded.enabled=1 THEN excluded.consent_at ELSE consent_at END,paused_reason=CASE WHEN excluded.enabled=1 THEN '' ELSE 'user_paused' END",params![user,input.enabled,input.amount_minor*10000,input.threshold_minor*10000,input.monthly_cap_minor*10000,input.cooldown_seconds,CONSENT_VERSION])?;
        if input.enabled {
            tx.execute(
                "UPDATE billing_auto_topup SET fee_bps=?2,fee_fixed_micro=?3 WHERE user_id=?1",
                params![user, fee_bps, fee_fixed],
            )?;
        }
        tx.commit()?;
        self.billing_auto(&user)
    }
    pub fn billing_overview(&self, actor: &str) -> Result<Value, Error> {
        let (user, credential) = actor_identity(&self.conn, actor)?;
        if credential.is_some() {
            return Err(Error::Forbidden);
        }
        let wallet = self.ai_wallet(actor)?;
        let mut q=self.conn.prepare(&format!("SELECT {PURCHASE_COLUMNS} FROM billing_purchases WHERE user_id=?1 ORDER BY created_at DESC,id DESC LIMIT 100"))?;
        let purchases = q
            .query_map([&user], purchase_row)?
            .collect::<Result<Vec<_>, _>>()?;
        let blocked = self
            .conn
            .query_row(
                "SELECT blocked FROM billing_accounts WHERE user_id=?1",
                [&user],
                |r| r.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(false);
        Ok(
            json!({"wallet":wallet,"purchases":purchases,"auto_topup":self.billing_auto(&user)?,"blocked":blocked}),
        )
    }
    /// Processes an already signature-verified event. Only facts needed for money are retained.
    pub fn billing_event(&mut self, event: &Value) -> Result<(), Error> {
        let event_id = event["id"]
            .as_str()
            .filter(|s| s.starts_with("evt_") && s.len() < 256)
            .ok_or_else(|| invalid("Invalid payment event ID"))?;
        let event_type = event["type"]
            .as_str()
            .filter(|s| s.len() < 100)
            .ok_or_else(|| invalid("Invalid payment event type"))?;
        let object = &event["data"]["object"];
        let tx = crate::agent_ops::begin_atomic(&mut self.conn)?;
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM billing_stripe_events WHERE id=?1)",
            [event_id],
            |r| r.get::<_, bool>(0),
        )? {
            return Ok(());
        }
        let success = matches!(
            event_type,
            "checkout.session.completed"
                | "checkout.session.async_payment_succeeded"
                | "payment_intent.succeeded"
        );
        let refund = event_type == "charge.refunded";
        let dispute = matches!(
            event_type,
            "charge.dispute.created" | "charge.dispute.closed"
        );
        let failure = matches!(
            event_type,
            "payment_intent.payment_failed"
                | "checkout.session.async_payment_failed"
                | "checkout.session.expired"
        );
        if !(success || refund || dispute || failure) {
            // Unknown signed events have no financial effect and need no retained payload.
            tx.execute(
                "INSERT INTO billing_stripe_events(id,event_type) VALUES(?1,?2)",
                params![event_id, event_type],
            )?;
            tx.commit()?;
            return Ok(());
        }
        if event_type == "checkout.session.completed" && object["payment_status"] != "paid" {
            tx.execute(
                "INSERT INTO billing_stripe_events(id,event_type) VALUES(?1,?2)",
                params![event_id, event_type],
            )?;
            tx.commit()?;
            return Ok(());
        }
        let purchase_id = object["metadata"]["purchase_id"]
            .as_str()
            .or_else(|| object["client_reference_id"].as_str());
        let payment_id = if event_type.starts_with("payment_intent.") {
            object["id"].as_str()
        } else {
            object["payment_intent"].as_str()
        };
        let purchase = if let Some(p) = purchase_id {
            tx.query_row(
                &format!("SELECT {PURCHASE_COLUMNS} FROM billing_purchases WHERE id=?1"),
                [p],
                purchase_row,
            )
            .optional()?
        } else if let Some(p) = payment_id {
            tx.query_row(
                &format!(
                    "SELECT {PURCHASE_COLUMNS} FROM billing_purchases WHERE stripe_payment_id=?1"
                ),
                [p],
                purchase_row,
            )
            .optional()?
        } else {
            None
        };
        let Some(p) = purchase else {
            // Not a Todoer purchase. Never associate an event by email/customer alone.
            tx.execute(
                "INSERT INTO billing_stripe_events(id,event_type) VALUES(?1,?2)",
                params![event_id, event_type],
            )?;
            tx.commit()?;
            return Ok(());
        };
        let customer = tx
            .query_row(
                "SELECT stripe_customer_id FROM billing_accounts WHERE user_id=?1",
                [&p.user_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        if let Some(c) = object["customer"].as_str()
            && customer.as_deref() != Some(c)
        {
            return Err(invalid("Payment customer mismatch"));
        }
        if let (Some(saved), Some(incoming)) = (p.payment.as_deref(), payment_id)
            && saved != incoming
        {
            return Err(invalid("Payment reference mismatch"));
        }
        if success {
            if customer.as_deref() != object["customer"].as_str() || customer.is_none() {
                return Err(invalid("Payment customer mismatch"));
            }
            let payment = payment_id
                .filter(|s| s.starts_with("pi_") && s.len() < 256)
                .ok_or_else(|| invalid("Payment reference missing"))?;
            let amount = if event_type.starts_with("checkout.") {
                object["amount_total"].as_i64()
            } else {
                object["amount_received"].as_i64()
            };
            if amount != Some(p.amount_minor) || object["currency"].as_str() != Some(&p.currency) {
                return Err(invalid("Payment amount or currency mismatch"));
            }
            if event_type == "payment_intent.succeeded" && object["status"] != "succeeded" {
                return Err(invalid("Payment is not confirmed"));
            }
            let session = if event_type.starts_with("checkout.") {
                Some(id_value(object, "id", "cs_")?)
            } else {
                None
            };
            if let (Some(saved), Some(incoming)) = (p.session.as_deref(), session.as_deref())
                && saved != incoming
            {
                return Err(invalid("Checkout reference mismatch"));
            }
            let refunded: i64 = tx.query_row(
                "SELECT refunded_minor FROM billing_purchases WHERE id=?1",
                [&p.id],
                |r| r.get(0),
            )?;
            let grant = (i128::from(p.credit_micro) * i128::from(p.amount_minor - refunded)
                / i128::from(p.amount_minor)) as i64;
            tx.execute("INSERT OR IGNORE INTO ai_wallet_entries(id,user_id,operation_id,kind,amount_micro,created_at) VALUES(?1,?2,?3,'topup',?4,unixepoch())",params![Uuid::new_v4().to_string(),p.user_id,format!("stripe:{payment}"),grant])?;
            tx.execute("UPDATE billing_purchases SET stripe_payment_id=?2,stripe_session_id=coalesce(stripe_session_id,?3),stripe_charge_id=coalesce(stripe_charge_id,?4),status=CASE WHEN refunded_minor=amount_minor THEN 'refunded' ELSE 'paid' END,paid_at=coalesce(paid_at,unixepoch()),updated_at=unixepoch() WHERE id=?1",params![p.id,payment,session,object["latest_charge"].as_str().or_else(||object["latest_charge"]["id"].as_str())])?;
            // Only consented purchases may establish a reusable method. A signed PI success
            // includes the Customer and actual attached payment method; never accept user IDs.
            if p.save_method
                && customer.is_some()
                && object["setup_future_usage"] == "off_session"
                && let Some(method) = object["payment_method"]
                    .as_str()
                    .filter(|m| m.starts_with("pm_") && m.len() < 256)
            {
                tx.execute("INSERT INTO billing_auto_topup(user_id,payment_method_id,consent_version,consent_at) VALUES(?1,?2,?3,unixepoch()) ON CONFLICT(user_id) DO UPDATE SET payment_method_id=excluded.payment_method_id",params![p.user_id,method,CONSENT_VERSION])?;
            }
        } else if refund {
            if object["amount"].as_i64() != Some(p.amount_minor)
                || object["currency"].as_str() != Some(&p.currency)
            {
                return Err(invalid("Refund amount or currency mismatch"));
            }
            let cumulative = object["amount_refunded"]
                .as_i64()
                .filter(|v| (0..=p.amount_minor).contains(v))
                .ok_or_else(|| invalid("Invalid refund amount"))?;
            reverse_credit(&tx, &p, cumulative, event_id)?;
        } else if dispute {
            let dispute_id = id_value(object, "id", "dp_")?;
            let amount = object["amount"]
                .as_i64()
                .filter(|v| (1..=p.amount_minor).contains(v))
                .ok_or_else(|| invalid("Invalid dispute amount"))?;
            if object["currency"].as_str() != Some(&p.currency) {
                return Err(invalid("Dispute currency mismatch"));
            }
            let state = object["status"].as_str().unwrap_or("unknown");
            let terminal = event_type == "charge.dispute.closed";
            let prior: Option<(String, bool)> = tx
                .query_row(
                    "SELECT state,terminal FROM billing_disputes WHERE id=?1",
                    [&dispute_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if prior.as_ref().is_none_or(|(_, closed)| !*closed) {
                tx.execute("INSERT INTO billing_disputes(id,purchase_id,state,amount_minor,terminal) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET state=excluded.state,terminal=excluded.terminal",params![dispute_id,p.id,state,amount,terminal])?;
                if terminal && state == "won" {
                    tx.execute("UPDATE billing_purchases SET status='paid',updated_at=unixepoch() WHERE id=?1 AND status='disputed'",[&p.id])?;
                    tx.execute("UPDATE billing_accounts SET blocked=CASE WHEN debt_micro>0 OR EXISTS(SELECT 1 FROM billing_disputes d JOIN billing_purchases p ON p.id=d.purchase_id WHERE p.user_id=?1 AND d.state!='won') THEN 1 ELSE 0 END WHERE user_id=?1",[&p.user_id])?;
                } else {
                    tx.execute(
                        "UPDATE billing_accounts SET blocked=1 WHERE user_id=?1",
                        [&p.user_id],
                    )?;
                    tx.execute("UPDATE billing_purchases SET status='disputed',updated_at=unixepoch() WHERE id=?1",[&p.id])?;
                    tx.execute("UPDATE billing_auto_topup SET enabled=0,paused_reason='payment_dispute' WHERE user_id=?1",[&p.user_id])?;
                    if terminal && state == "lost" {
                        let refunded: i64 = tx.query_row(
                            "SELECT refunded_minor FROM billing_purchases WHERE id=?1",
                            [&p.id],
                            |r| r.get(0),
                        )?;
                        reverse_credit(&tx, &p, (refunded + amount).min(p.amount_minor), event_id)?;
                    }
                }
            }
        } else if failure {
            tx.execute("UPDATE billing_purchases SET status=CASE WHEN status IN ('paid','refunded','disputed') THEN status ELSE 'failed' END,updated_at=unixepoch() WHERE id=?1",[&p.id])?;
            if p.mode == "auto" {
                tx.execute("UPDATE billing_auto_topup SET enabled=0,paused_reason='payment_failed' WHERE user_id=?1",[&p.user_id])?;
            }
        }
        tx.execute(
            "INSERT INTO billing_stripe_events(id,event_type,purchase_id) VALUES(?1,?2,?3)",
            params![event_id, event_type, p.id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn billing_save_payment_response(
        &mut self,
        purchase: &str,
        value: &Value,
    ) -> Result<(), Error> {
        if value["metadata"]["purchase_id"].as_str() != Some(purchase) {
            return Err(invalid("Payment purchase mismatch"));
        }
        let payment = id_value(value, "id", "pi_")?;
        let state = value["status"].as_str().unwrap_or("unknown");
        if state == "succeeded" {
            // Same validator and idempotent ledger path as webhook delivery.
            let event = json!({"id":format!("evt_reconcile_{payment}"),"type":"payment_intent.succeeded","data":{"object":value}});
            self.billing_event(&event)?;
        } else {
            let status = match state {
                "requires_action" => "requires_action",
                "processing" => "pending",
                "requires_payment_method" | "canceled" => "failed",
                _ => "pending",
            };
            self.conn.execute("UPDATE billing_purchases SET stripe_payment_id=?2,status=?3,updated_at=unixepoch() WHERE id=?1 AND status IN ('created','pending','unknown','requires_action')",params![purchase,payment,status])?;
            if matches!(status, "requires_action" | "failed") {
                self.conn.execute("UPDATE billing_auto_topup SET enabled=0,paused_reason=?2 WHERE user_id=(SELECT user_id FROM billing_purchases WHERE id=?1)",params![purchase,if status=="requires_action"{"authentication_required"}else{"payment_failed"}])?;
            }
        }
        Ok(())
    }
    pub fn billing_payment_details(
        &self,
        purchase: &str,
        value: &Value,
        rates: &BTreeMap<String, i64>,
    ) -> Result<(), Error> {
        if value["metadata"]["purchase_id"].as_str() != Some(purchase) {
            return Err(invalid("Payment purchase mismatch"));
        }
        let charge = &value["latest_charge"];
        let fee = charge["balance_transaction"]["fee"]
            .as_i64()
            .filter(|v| (0..=100_000_000).contains(v));
        let currency = charge["balance_transaction"]["currency"].as_str();
        let rate = currency.and_then(|c| rates.get(c)).copied();
        let fee_micro = fee
            .zip(rate)
            .and_then(|(f, r)| i64::try_from(i128::from(f) * 10_000_000_000 / i128::from(r)).ok());
        let receipt = charge["receipt_url"]
            .as_str()
            .filter(|r| r.len() < 4096)
            .filter(|r| {
                reqwest::Url::parse(r).is_ok_and(|u| {
                    u.scheme() == "https"
                        && u.host_str() == Some("pay.stripe.com")
                        && u.username().is_empty()
                        && u.password().is_none()
                })
            });
        self.conn.execute("UPDATE billing_purchases SET receipt_url=coalesce(?2,receipt_url),payment_fee_minor=coalesce(payment_fee_minor,?3),payment_fee_currency=coalesce(payment_fee_currency,?4),fee_fx_rate_micros=coalesce(fee_fx_rate_micros,?5),payment_fee_micro_eur=coalesce(payment_fee_micro_eur,?6),updated_at=unixepoch() WHERE id=?1",params![purchase,receipt,fee,currency,rate,fee_micro])?;
        Ok(())
    }
    pub fn billing_claim_auto(
        &mut self,
        service: &StripeService,
    ) -> Result<Option<(Purchase, String, String)>, Error> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE billing_auto_topup SET enabled=0,paused_reason='ai_unavailable' WHERE enabled=1 AND (NOT EXISTS(SELECT 1 FROM ai_account_policy p WHERE p.user_id=billing_auto_topup.user_id AND p.user_ai_enabled=1 AND p.managed_ai_allowed=1 AND p.funding='credits' AND EXISTS(SELECT 1 FROM ai_models m JOIN ai_provider_policy pp ON pp.id=m.provider WHERE m.id=p.model_id AND m.enabled=1 AND pp.enabled=1)) OR EXISTS(SELECT 1 FROM billing_accounts b WHERE b.user_id=billing_auto_topup.user_id AND b.blocked=1))",[])?;
        // A new fee policy requires fresh, explicit standing payment consent.
        tx.execute("UPDATE billing_auto_topup SET enabled=0,paused_reason='fee_policy_changed' WHERE enabled=1 AND (fee_bps<>?1 OR fee_fixed_micro<>?2)",params![service.fee_bps,service.fee_fixed_micro])?;
        // Retry only recent, already-claimed cycles with the exact same Stripe key.
        let retry:Option<(String,String,String)>=tx.query_row("SELECT p.id,b.stripe_customer_id,a.payment_method_id FROM billing_purchases p JOIN billing_accounts b USING(user_id) JOIN billing_auto_topup a USING(user_id) WHERE p.mode='auto' AND p.status='created' AND p.created_at>unixepoch()-600 AND a.enabled=1 AND p.updated_at<unixepoch()-30 ORDER BY p.created_at LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((id, customer, method)) = retry {
            tx.execute(
                "UPDATE billing_purchases SET updated_at=unixepoch() WHERE id=?1",
                [&id],
            )?;
            tx.commit()?;
            return Ok(Some((self.billing_purchase(&id)?, customer, method)));
        }
        tx.execute("UPDATE billing_auto_topup SET enabled=0,paused_reason='payment_unknown' WHERE user_id IN (SELECT user_id FROM billing_purchases WHERE mode='auto' AND status='created' AND created_at<=unixepoch()-600)",[])?;
        tx.execute("UPDATE billing_purchases SET status='unknown' WHERE mode='auto' AND status='created' AND created_at<=unixepoch()-600",[])?;
        let candidate:Option<(String,i64,String,String)>=tx.query_row("SELECT a.user_id,a.amount_micro,b.stripe_customer_id,a.payment_method_id FROM billing_auto_topup a JOIN billing_accounts b USING(user_id) WHERE a.enabled=1 AND a.payment_method_id IS NOT NULL AND b.stripe_customer_id IS NOT NULL AND b.blocked=0 AND a.consent_at IS NOT NULL AND a.consent_version=?1 AND (a.last_attempt_at IS NULL OR a.last_attempt_at<=unixepoch()-a.cooldown_seconds) AND coalesce((SELECT sum(amount_micro) FROM ai_wallet_entries WHERE user_id=a.user_id),0)<a.threshold_micro AND NOT EXISTS(SELECT 1 FROM billing_purchases WHERE user_id=a.user_id AND mode='auto' AND status IN ('created','pending','unknown','requires_action')) AND coalesce((SELECT sum(amount_minor*10000) FROM billing_purchases WHERE user_id=a.user_id AND mode='auto' AND status NOT IN ('failed','refunded') AND created_at>=CAST(strftime('%s','now','start of month') AS INTEGER)),0)+a.amount_micro<=a.monthly_cap_micro ORDER BY a.user_id LIMIT 1",[CONSENT_VERSION],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let Some((user, amount, customer, method)) = candidate else {
            tx.commit()?;
            return Ok(None);
        };
        let id = Uuid::new_v4().to_string();
        let quote = service.quote(amount / 10000, "eur")?;
        tx.execute("INSERT INTO billing_purchases(id,user_id,idempotency_key,amount_minor,currency,credit_micro,fx_rate_micros,mode,gross_micro,payment_fee_allowance_micro,forex_fee_micro) VALUES(?1,?2,?3,?4,'eur',?5,1000000,'auto',?6,?7,0)",params![id,user,format!("auto:{id}"),amount/10000,quote.credit_micro,quote.gross_micro,quote.payment_fee_micro])?;
        tx.execute(
            "UPDATE billing_auto_topup SET last_attempt_at=unixepoch() WHERE user_id=?1",
            [&user],
        )?;
        tx.commit()?;
        Ok(Some((self.billing_purchase(&id)?, customer, method)))
    }
    pub fn billing_authentication_payments(
        &self,
        user: &str,
    ) -> Result<Vec<(String, String)>, Error> {
        let mut q=self.conn.prepare("SELECT id,stripe_payment_id FROM billing_purchases WHERE user_id=?1 AND mode='auto' AND status='requires_action' AND stripe_payment_id IS NOT NULL LIMIT 10")?;
        Ok(q.query_map([user], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn billing_pending_sessions(&self) -> Result<Vec<(String, String)>, Error> {
        let mut q=self.conn.prepare("SELECT id,stripe_session_id FROM billing_purchases WHERE mode='manual' AND stripe_session_id IS NOT NULL AND status IN ('created','pending','unknown') ORDER BY updated_at LIMIT 20")?;
        Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn billing_pending_payments(&self) -> Result<Vec<(String, String)>, Error> {
        let mut q=self.conn.prepare("SELECT id,stripe_payment_id FROM billing_purchases WHERE stripe_payment_id IS NOT NULL AND (status IN ('pending','unknown','requires_action') OR (status IN ('paid','refunded','disputed') AND payment_fee_micro_eur IS NULL) OR (status='paid' AND save_method=1 AND NOT EXISTS(SELECT 1 FROM billing_auto_topup a WHERE a.user_id=billing_purchases.user_id AND a.payment_method_id IS NOT NULL))) ORDER BY updated_at LIMIT 20")?;
        Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }
}
fn id_value(value: &Value, key: &str, prefix: &str) -> Result<String, Error> {
    id(value, key, prefix)
}
fn reverse_credit(
    tx: &rusqlite::Connection,
    p: &Purchase,
    cumulative: i64,
    event: &str,
) -> Result<(), Error> {
    let previous: i64 = tx.query_row(
        "SELECT refunded_minor FROM billing_purchases WHERE id=?1",
        [&p.id],
        |r| r.get(0),
    )?;
    if cumulative <= previous {
        return Ok(());
    }
    let original_granted:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM ai_wallet_entries WHERE user_id=?1 AND kind='topup' AND operation_id='stripe:'||?2)",params![p.user_id,p.payment.as_deref().unwrap_or("")],|r|r.get(0))?;
    let before = (i128::from(p.credit_micro) * i128::from(p.amount_minor - previous)
        / i128::from(p.amount_minor)) as i64;
    let after = (i128::from(p.credit_micro) * i128::from(p.amount_minor - cumulative)
        / i128::from(p.amount_minor)) as i64;
    let debit = before - after;
    let (taken, debt) = if original_granted {
        let available:i64=tx.query_row("SELECT max(0,coalesce((SELECT sum(amount_micro) FROM ai_wallet_entries WHERE user_id=?1 AND kind NOT IN ('free_grant','ai_usage_free','free_revoke')),0)-coalesce((SELECT sum(amount_micro-free_micro) FROM ai_credit_holds WHERE user_id=?1 AND state='held'),0))",[&p.user_id],|r|r.get(0))?;
        let taken = debit.min(available);
        let debt = debit - taken;
        tx.execute("INSERT INTO ai_wallet_entries(id,user_id,operation_id,kind,amount_micro,created_at) VALUES(?1,?2,?3,'refund',?4,unixepoch())",params![Uuid::new_v4().to_string(),p.user_id,format!("stripe-adjust:{event}"),-taken])?;
        tx.execute("UPDATE billing_accounts SET debt_micro=debt_micro+?2,blocked=CASE WHEN ?2>0 THEN 1 ELSE blocked END WHERE user_id=?1",params![p.user_id,debt])?;
        if debt > 0 {
            tx.execute("UPDATE billing_auto_topup SET enabled=0,paused_reason='billing_review' WHERE user_id=?1",[&p.user_id])?;
        }
        (taken, debt)
    } else {
        (0, 0)
    };
    let gross_before = (i128::from(p.gross_micro) * i128::from(p.amount_minor - previous)
        / i128::from(p.amount_minor)) as i64;
    let gross_after = (i128::from(p.gross_micro) * i128::from(p.amount_minor - cumulative)
        / i128::from(p.amount_minor)) as i64;
    let gross_refund = gross_before - gross_after;
    tx.execute("INSERT INTO billing_adjustments(id,purchase_id,amount_micro,debt_micro,gross_refund_micro,conversion_reversal_micro) VALUES(?1,?2,?3,?4,?5,?6)",params![event,p.id,taken,debt,gross_refund,gross_refund-debit])?;
    tx.execute("UPDATE billing_purchases SET refunded_minor=?2,refunded_at=unixepoch(),status=CASE WHEN ?2=amount_minor THEN 'refunded' ELSE status END,updated_at=unixepoch() WHERE id=?1",params![p.id,cumulative])?;
    Ok(())
}
/// A single bounded worker pass. The caller owns scheduling/shutdown. No DB lock crosses IO.
pub async fn tick(store: Arc<Mutex<Store>>, service: &StripeService) -> Result<(), Error> {
    let sessions = {
        store
            .lock()
            .map_err(|_| invalid("Billing database unavailable"))?
            .billing_pending_sessions()?
    };
    for (purchase, session) in sessions {
        if let Ok(value) = service.session(&session).await {
            if value["metadata"]["purchase_id"].as_str() != Some(purchase.as_str()) {
                return Err(invalid("Checkout purchase mismatch"));
            }
            let event = if value["payment_status"] == "paid" {
                Some(
                    json!({"id":format!("evt_reconcile_{session}"),"type":"checkout.session.completed","data":{"object":value}}),
                )
            } else if value["status"] == "expired" {
                Some(
                    json!({"id":format!("evt_expired_{session}"),"type":"checkout.session.expired","data":{"object":value}}),
                )
            } else {
                None
            };
            if let Some(event) = event {
                store
                    .lock()
                    .map_err(|_| invalid("Billing database unavailable"))?
                    .billing_event(&event)?;
            }
        }
    }
    let pending = {
        store
            .lock()
            .map_err(|_| invalid("Billing database unavailable"))?
            .billing_pending_payments()?
    };
    for (purchase, payment) in pending {
        if let Ok(value) = service.payment(&payment).await {
            store
                .lock()
                .map_err(|_| invalid("Billing database unavailable"))?
                .billing_save_payment_response(&purchase, &value)?;
            store
                .lock()
                .map_err(|_| invalid("Billing database unavailable"))?
                .billing_payment_details(&purchase, &value, &service.rates)?;
        }
    }
    if !service.accepts_purchases {
        return Ok(());
    }
    if std::env::var("TODOER_AI_ENABLED").is_ok_and(|s| s != "1") {
        store.lock().map_err(|_|invalid("Billing database unavailable"))?.conn.execute("UPDATE billing_auto_topup SET enabled=0,paused_reason='global_ai_disabled' WHERE enabled=1",[])?;
        return Ok(());
    }
    let cycle = {
        store
            .lock()
            .map_err(|_| invalid("Billing database unavailable"))?
            .billing_claim_auto(service)?
    };
    if let Some((purchase, customer, method)) = cycle {
        // Policy is checked once more immediately before dispatch. A disabled cycle is never resent.
        let permitted = {
            let db = store
                .lock()
                .map_err(|_| invalid("Billing database unavailable"))?;
            db.conn.query_row("SELECT EXISTS(SELECT 1 FROM billing_auto_topup a JOIN ai_account_policy p USING(user_id) JOIN billing_accounts b USING(user_id) WHERE a.user_id=?1 AND a.enabled=1 AND p.user_ai_enabled=1 AND p.managed_ai_allowed=1 AND p.funding='credits' AND b.blocked=0 AND EXISTS(SELECT 1 FROM ai_models m JOIN ai_provider_policy pp ON pp.id=m.provider WHERE m.id=p.model_id AND m.enabled=1 AND pp.enabled=1))",[&purchase.user_id],|r|r.get::<_,bool>(0))?
        };
        if permitted && let Ok(value) = service.automatic(&purchase, &customer, &method).await {
            store
                .lock()
                .map_err(|_| invalid("Billing database unavailable"))?
                .billing_save_payment_response(&purchase.id, &value)?;
            store
                .lock()
                .map_err(|_| invalid("Billing database unavailable"))?
                .billing_payment_details(&purchase.id, &value, &service.rates)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn service() -> StripeService {
        StripeService {
            secret: "sk_test_synthetic".into(),
            webhook_secret: "whsec_synthetic".into(),
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            endpoint: "http://127.0.0.1:1/v1".into(),
            rates: BTreeMap::from([("eur".into(), 1_000_000), ("usd".into(), 1_250_000)]),
            accepts_purchases: true,
            fee_bps: 0,
            fee_fixed_micro: 0,
        }
    }
    fn db() -> Store {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        s
    }
    fn purchase(s: &mut Store, key: &str) -> Purchase {
        s.billing_customer("dev").unwrap();
        s.billing_save_customer("dev", "cus_synthetic").unwrap();
        s.billing_begin(
            "dev",
            TopupInput {
                amount_minor: 1000,
                currency: "EUR".into(),
                idempotency_key: key.into(),
                save_for_auto_topup: true,
                quoted_credit_micro: None,
            },
            &service(),
        )
        .unwrap()
    }
    fn success(p: &Purchase, event: &str) -> Value {
        json!({"id":event,"type":"payment_intent.succeeded","data":{"object":{"id":"pi_synthetic","status":"succeeded","amount":p.amount_minor,"amount_received":p.amount_minor,"currency":p.currency,"customer":"cus_synthetic","metadata":{"purchase_id":p.id},"payment_method":"pm_synthetic","setup_future_usage":"off_session","latest_charge":"ch_synthetic"}}})
    }
    fn refunded(p: &Purchase, event: &str, amount: i64) -> Value {
        json!({"id":event,"type":"charge.refunded","data":{"object":{"id":"ch_synthetic","payment_intent":"pi_synthetic","amount":p.amount_minor,"amount_refunded":amount,"currency":p.currency,"customer":"cus_synthetic","metadata":{"purchase_id":p.id}}}})
    }
    #[test]
    fn conversion_deductions_freeze_net_credits_and_refund_proportionally() {
        let mut service = service();
        service.fee_bps = 200;
        service.fee_fixed_micro = 200_000;
        let quote = service.quote(1000, "USD").unwrap();
        assert_eq!(
            (
                quote.gross_micro,
                quote.payment_fee_micro,
                quote.forex_fee_micro,
                quote.credit_micro
            ),
            (8_000_000, 360_000, 80_000, 7_560_000)
        );
        assert_eq!(service.quote(1000, "EUR").unwrap().credit_micro, 9_600_000);
        let mut db = db();
        let input = |key: &str, accepted| TopupInput {
            amount_minor: 1000,
            currency: "USD".into(),
            idempotency_key: key.into(),
            save_for_auto_topup: false,
            quoted_credit_micro: Some(accepted),
        };
        assert!(
            db.billing_begin("dev", input("stale", 8_000_000), &service)
                .is_err()
        );
        let p = db
            .billing_begin("dev", input("net", 7_560_000), &service)
            .unwrap();
        assert_eq!((p.gross_micro, p.credit_micro), (8_000_000, 7_560_000));
        db.billing_customer("dev").unwrap();
        db.billing_save_customer("dev", "cus_synthetic").unwrap();
        db.billing_event(&success(&p, "evt_net_paid")).unwrap();
        assert_eq!(db.ai_wallet("dev").unwrap().posted_micro, 7_560_000);
        db.billing_event(&refunded(&p, "evt_net_refund", 500))
            .unwrap();
        assert_eq!(db.ai_wallet("dev").unwrap().posted_micro, 3_780_000);
        let amounts:(i64,i64)=db.conn.query_row("SELECT gross_refund_micro,conversion_reversal_micro FROM billing_adjustments WHERE id='evt_net_refund'",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(amounts, (4_000_000, 220_000));
        service.fee_bps = 300;
        let retry = db
            .billing_begin("dev", input("net", 7_560_000), &service)
            .unwrap();
        assert_eq!(retry.credit_micro, 7_560_000);
        let finance = db.ai_finance(None, 0, i64::MAX).unwrap();
        assert_eq!(finance["topups_micro"], 8_000_000);
        assert_eq!(finance["refunds_micro"], 4_000_000);
        assert_eq!(finance["conversion_revenue_micro"], 220_000);
    }
    #[test]
    fn exact_currency_quotes_and_minimum_are_frozen_on_purchase() {
        let service = service();
        let quote = service.quote(625, "USD").unwrap();
        assert_eq!(
            (
                quote.currency.as_str(),
                quote.gross_micro,
                quote.credit_micro,
                quote.forex_fee_micro
            ),
            ("usd", 5_000_000, 4_950_000, 50_000)
        );
        assert!(service.quote(624, "USD").is_err());
        assert!(service.quote(i64::MAX, "EUR").is_err());
        assert!(service.quote(500, "JPY").is_err());
        let mut s = db();
        let p = purchase(&mut s, "request");
        let retry = purchase(&mut s, "request");
        assert_eq!(p.id, retry.id);
        assert!(
            s.billing_begin(
                "dev",
                TopupInput {
                    amount_minor: 1100,
                    currency: "EUR".into(),
                    idempotency_key: "request".into(),
                    save_for_auto_topup: true,
                    quoted_credit_micro: None
                },
                &service
            )
            .is_err()
        );
    }
    #[test]
    fn signatures_use_raw_body_constant_time_digest_and_timestamp_tolerance() {
        let service = service();
        let body =
            br#"{"id":"evt_signature","livemode":false,"type":"other","data":{"object":{}}}"#;
        let timestamp = 12345;
        let mut mac = Hmac::<Sha256>::new_from_slice(service.webhook_secret.as_bytes()).unwrap();
        mac.update(b"12345.");
        mac.update(body);
        let hash = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let signature = format!(
            "t={timestamp},v1=0000000000000000000000000000000000000000000000000000000000000000,v1={hash}"
        );
        assert!(service.verify(body, &signature, timestamp).is_ok());
        assert!(service.verify(body, &signature, timestamp + 301).is_err());
        assert!(service.verify(b"{}", &signature, timestamp).is_err());
        assert!(
            service
                .verify(body, &format!("t=12345,v1={}", "é".repeat(32)), timestamp)
                .is_err()
        );
        let mut live = service.clone();
        live.secret = "sk_live_synthetic".into();
        assert!(live.verify(body, &signature, timestamp).is_err());
        live.secret = "rk_live_synthetic".into();
        assert!(live.verify(body, &signature, timestamp).is_err());
        live.secret = "rk_test_synthetic".into();
        assert!(live.verify(body, &signature, timestamp).is_ok());
        let live_body = br#"{"id":"evt_live","livemode":true,"type":"other","data":{"object":{}}}"#;
        let mut mac = Hmac::<Sha256>::new_from_slice(service.webhook_secret.as_bytes()).unwrap();
        mac.update(b"12345.");
        mac.update(live_body);
        let live_hash = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let live_signature = format!("t=12345,v1={live_hash}");
        live.secret = "rk_live_synthetic".into();
        assert!(live.verify(live_body, &live_signature, timestamp).is_ok());
        live.secret = "rk_test_synthetic".into();
        assert!(live.verify(live_body, &live_signature, timestamp).is_err());
        assert_eq!(stripe_key_livemode("sk_test_synthetic"), Some(false));
        assert_eq!(stripe_key_livemode("rk_live_synthetic"), Some(true));
        assert_eq!(stripe_key_livemode("sk_unknown_synthetic"), None);
    }
    #[test]
    fn payment_credit_is_once_and_mismatch_rolls_back_event_receipt() {
        let mut s = db();
        let p = purchase(&mut s, "payment");
        let mut bad = success(&p, "evt_retry");
        bad["data"]["object"]["amount_received"] = json!(1001);
        assert!(s.billing_event(&bad).is_err());
        let good = success(&p, "evt_retry");
        s.billing_event(&good).unwrap();
        s.billing_event(&good).unwrap();
        s.billing_event(&success(&p, "evt_duplicate_success"))
            .unwrap();
        assert_eq!(s.ai_wallet("dev").unwrap().posted_micro, 10_000_000);
        assert_eq!(
            s.billing_auto("dev").unwrap()["payment_method_configured"],
            true
        );
        let mut other = success(&p, "evt_wrong_customer");
        other["data"]["object"]["customer"] = json!("cus_other");
        assert!(s.billing_event(&other).is_err());
        assert_eq!(s.ai_wallet("other").unwrap().posted_micro, 0);
    }
    #[test]
    fn partial_refund_after_spend_never_overdraws_and_replay_is_harmless() {
        let mut s = db();
        let p = purchase(&mut s, "refund");
        s.billing_event(&success(&p, "evt_paid")).unwrap();
        s.ai_credit("dev", "synthetic-usage", -9_000_000, "ai_usage")
            .unwrap();
        s.billing_event(&refunded(&p, "evt_refund", 500)).unwrap();
        assert_eq!(s.ai_wallet("dev").unwrap().posted_micro, 0);
        let (debt, blocked): (i64, bool) = s
            .conn
            .query_row(
                "SELECT debt_micro,blocked FROM billing_accounts WHERE user_id='dev'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((debt, blocked), (4_000_000, true));
        s.billing_event(&refunded(&p, "evt_old_refund", 200))
            .unwrap();
        s.billing_event(&refunded(&p, "evt_refund", 500)).unwrap();
        assert_eq!(
            s.conn
                .query_row(
                    "SELECT debt_micro FROM billing_accounts WHERE user_id='dev'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            4_000_000
        );
    }
    #[test]
    fn admin_reconciliation_verifies_binding_and_cannot_duplicate_credit() {
        let mut s = db();
        let p = purchase(&mut s, "lost-auto-response");
        s.conn
            .execute("INSERT INTO auth_admins(user_id) VALUES('other')", [])
            .unwrap();
        s.conn
            .execute(
                "UPDATE billing_purchases SET mode='auto',status='unknown' WHERE id=?1",
                [&p.id],
            )
            .unwrap();
        let input = || ReconcileInput {
            purchase_id: p.id.clone(),
            payment_intent_id: "pi_synthetic".into(),
            reason: "Verified payment after interrupted response".into(),
        };
        let value = success(&p, "evt_reconciled")["data"]["object"].clone();
        let mut wrong = value.clone();
        wrong["amount"] = json!(1001);
        assert!(
            s.billing_admin_reconcile("other", input(), &wrong, &service())
                .is_err()
        );
        wrong = value.clone();
        wrong["customer"] = json!("cus_another_account");
        assert!(
            s.billing_admin_reconcile("other", input(), &wrong, &service())
                .is_err()
        );
        wrong = value.clone();
        wrong["metadata"]["purchase_id"] = json!("another-purchase");
        assert!(
            s.billing_admin_reconcile("other", input(), &wrong, &service())
                .is_err()
        );
        let another = purchase(&mut s, "other-local-purchase");
        s.conn
            .execute(
                "UPDATE billing_purchases SET stripe_payment_id='pi_synthetic' WHERE id=?1",
                [&another.id],
            )
            .unwrap();
        assert!(
            s.billing_admin_reconcile("other", input(), &value, &service())
                .is_err()
        );
        s.conn
            .execute(
                "UPDATE billing_purchases SET stripe_payment_id=NULL WHERE id=?1",
                [&another.id],
            )
            .unwrap();
        assert_eq!(s.ai_wallet("dev").unwrap().posted_micro, 0);
        assert!(
            s.billing_admin_reconcile("dev", input(), &value, &service())
                .is_err()
        );
        let saved = s
            .billing_admin_reconcile("other", input(), &value, &service())
            .unwrap();
        assert_eq!(saved["status"], "paid");
        s.billing_admin_reconcile("other", input(), &value, &service())
            .unwrap();
        assert_eq!(s.ai_wallet("dev").unwrap().posted_micro, 10_000_000);
        assert_eq!(
            s.conn
                .query_row(
                    "SELECT count(*) FROM ai_admin_audit WHERE action='billing.reconcile'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
    }
    #[test]
    fn administrator_debt_forgiveness_is_explicit_audited_and_not_revenue() {
        let mut s = db();
        let p = purchase(&mut s, "admin-writeoff");
        s.billing_event(&success(&p, "evt_paid_writeoff")).unwrap();
        s.ai_credit("dev", "spent-before-writeoff", -9_000_000, "ai_usage")
            .unwrap();
        s.billing_event(&refunded(&p, "evt_refund_writeoff", 500))
            .unwrap();
        let update = |forgive| BillingAdminInput {
            blocked: false,
            forgive_debt: forgive,
            reason: "Reviewed payment dispute outcome".into(),
        };
        assert!(
            s.billing_admin_update("other", "dev", update(true))
                .is_err()
        );
        s.conn
            .execute("INSERT INTO auth_admins(user_id) VALUES('other')", [])
            .unwrap();
        assert!(
            s.billing_admin_update("other", "dev", update(false))
                .is_err()
        );
        let before_ledger: i64 = s
            .conn
            .query_row("SELECT count(*) FROM ai_wallet_entries", [], |r| r.get(0))
            .unwrap();
        let result = s
            .billing_admin_update("other", "dev", update(true))
            .unwrap();
        assert_eq!(result["writeoff_micro"], 4_000_000);
        assert_eq!(result["debt_micro"], 0);
        assert_eq!(result["billing_blocked"], false);
        assert_eq!(
            s.conn
                .query_row("SELECT count(*) FROM ai_wallet_entries", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            before_ledger
        );
        let (action,after):(String,String)=s.conn.query_row("SELECT action,after_json FROM ai_admin_audit WHERE target='dev' ORDER BY id DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(action, "billing.writeoff");
        assert_eq!(
            serde_json::from_str::<Value>(&after).unwrap()["writeoff_micro"],
            4_000_000
        );
        assert_eq!(s.billing_auto("dev").unwrap()["enabled"], false);
        assert!(
            s.billing_admin_update("credential:fake-admin", "dev", update(true))
                .is_err()
        );
    }
    #[test]
    fn paid_refunds_preserve_free_grants_and_subtract_only_paid_holds() {
        let mut s = db();
        let p = purchase(&mut s, "refund-with-free-grant");
        s.billing_event(&success(&p, "evt_paid_with_free")).unwrap();
        s.ai_credit("dev", "synthetic-paid-usage", -7_000_000, "ai_usage")
            .unwrap();
        s.ai_credit("dev", "synthetic-free-grant", 7_000_000, "free_grant")
            .unwrap();
        s.conn.execute("INSERT INTO ai_credit_holds(operation_id,user_id,amount_micro,free_micro,state) VALUES('synthetic-mixed-hold','dev',4000000,3000000,'held')",[]).unwrap();
        s.billing_event(&refunded(&p, "evt_refund_preserves_free", 500))
            .unwrap();
        let paid:i64=s.conn.query_row("SELECT sum(amount_micro) FROM ai_wallet_entries WHERE user_id='dev' AND kind NOT IN ('free_grant','ai_usage_free','free_revoke')",[],|r|r.get(0)).unwrap();
        let free:i64=s.conn.query_row("SELECT sum(amount_micro) FROM ai_wallet_entries WHERE user_id='dev' AND kind IN ('free_grant','ai_usage_free','free_revoke')",[],|r|r.get(0)).unwrap();
        let debt: i64 = s
            .conn
            .query_row(
                "SELECT debt_micro FROM billing_accounts WHERE user_id='dev'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(paid, 1_000_000);
        assert_eq!(free, 7_000_000);
        assert_eq!(debt, 3_000_000);
    }
    #[test]
    fn refund_before_success_grants_only_remaining_credit() {
        let mut s = db();
        let p = purchase(&mut s, "refund-first");
        s.billing_event(&refunded(&p, "evt_refund_first", 300))
            .unwrap();
        s.billing_event(&success(&p, "evt_paid_late")).unwrap();
        assert_eq!(s.ai_wallet("dev").unwrap().posted_micro, 7_000_000);
    }
    #[test]
    fn automatic_payments_pause_when_consented_fee_policy_changes() {
        let mut db = db();
        db.conn.execute("INSERT INTO ai_account_policy(user_id,user_ai_enabled,managed_ai_allowed,funding,model_id) VALUES('dev',1,1,'credits','openai/gpt-4.1-mini')",[]).unwrap();
        db.conn
            .execute(
                "UPDATE ai_models SET enabled=1 WHERE id='openai/gpt-4.1-mini'",
                [],
            )
            .unwrap();
        db.conn.execute("INSERT INTO billing_auto_topup(user_id,enabled,consent_at,consent_version,fee_bps,fee_fixed_micro) VALUES('dev',1,unixepoch(),?1,0,0)",[CONSENT_VERSION]).unwrap();
        let mut changed = service();
        changed.fee_bps = 200;
        assert!(db.billing_claim_auto(&changed).unwrap().is_none());
        let state = db.billing_auto("dev").unwrap();
        assert_eq!(state["enabled"], false);
        assert_eq!(state["paused_reason"], "fee_policy_changed");
    }
    #[test]
    fn automatic_cycle_is_unique_capped_and_stops_when_ai_is_off() {
        let mut s = db();
        let p = purchase(&mut s, "auto");
        s.billing_event(&success(&p, "evt_paid_auto")).unwrap();
        s.conn.execute("INSERT INTO ai_account_policy(user_id,user_ai_enabled,managed_ai_allowed,funding,model_id) VALUES('dev',1,1,'credits','openai/gpt-4.1-mini') ON CONFLICT(user_id) DO UPDATE SET user_ai_enabled=1,managed_ai_allowed=1,funding='credits',model_id='openai/gpt-4.1-mini'",[]).unwrap();
        s.conn
            .execute(
                "UPDATE ai_models SET enabled=1 WHERE id='openai/gpt-4.1-mini'",
                [],
            )
            .unwrap();
        let settings = AutoTopupInput {
            enabled: true,
            amount_minor: 1000,
            threshold_minor: 200,
            monthly_cap_minor: 1000,
            cooldown_seconds: 60,
            consent: true,
            quoted_credit_micro: None,
        };
        s.billing_set_auto("dev", settings, Some(&service()))
            .unwrap();
        assert!(s.billing_claim_auto(&service()).unwrap().is_none());
        s.ai_credit("dev", "used-auto", -9_000_000, "ai_usage")
            .unwrap();
        let first = s.billing_claim_auto(&service()).unwrap().unwrap();
        assert_eq!(first.0.amount_minor, 1000);
        assert!(s.billing_claim_auto(&service()).unwrap().is_none());
        s.conn
            .execute(
                "UPDATE billing_purchases SET status='paid' WHERE id=?1",
                [&first.0.id],
            )
            .unwrap();
        s.conn
            .execute(
                "UPDATE billing_auto_topup SET last_attempt_at=unixepoch()-100",
                [],
            )
            .unwrap();
        assert!(s.billing_claim_auto(&service()).unwrap().is_none());
        s.conn
            .execute(
                "UPDATE ai_account_policy SET user_ai_enabled=0 WHERE user_id='dev'",
                [],
            )
            .unwrap();
        assert!(s.billing_claim_auto(&service()).unwrap().is_none());
        assert_eq!(s.billing_auto("dev").unwrap()["enabled"], false);
    }
    #[test]
    fn off_session_authentication_pauses_future_charges_without_credit() {
        let mut s = db();
        let p = purchase(&mut s, "needs-auth");
        s.billing_save_payment_response(&p.id,&json!({"id":"pi_synthetic","status":"requires_action","metadata":{"purchase_id":p.id}})).unwrap();
        assert_eq!(s.billing_purchase(&p.id).unwrap().status, "requires_action");
        assert_eq!(s.ai_wallet("dev").unwrap().posted_micro, 0);
    }
    #[test]
    fn dispute_close_before_create_is_terminal_and_fees_stay_internal() {
        let mut s = db();
        let p = purchase(&mut s, "dispute");
        s.billing_event(&success(&p, "evt_paid_dispute")).unwrap();
        let event = |name: &str, kind: &str, state: &str| json!({"id":name,"type":kind,"data":{"object":{"id":"dp_synthetic","status":state,"amount":300,"currency":"eur","payment_intent":"pi_synthetic","metadata":{"purchase_id":p.id}}}});
        s.billing_event(&event("evt_closed", "charge.dispute.closed", "won"))
            .unwrap();
        s.billing_event(&event(
            "evt_created_late",
            "charge.dispute.created",
            "needs_response",
        ))
        .unwrap();
        assert!(
            !s.conn
                .query_row(
                    "SELECT blocked FROM billing_accounts WHERE user_id='dev'",
                    [],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap()
        );
        let mut value = success(&p, "evt_metadata")["data"]["object"].clone();
        value["latest_charge"] = json!({"id":"ch_synthetic","receipt_url":"https://pay.stripe.com/receipts/synthetic","balance_transaction":{"fee":42,"currency":"eur"}});
        s.billing_payment_details(&p.id, &value, &service().rates)
            .unwrap();
        assert_eq!(
            s.conn
                .query_row(
                    "SELECT payment_fee_micro_eur FROM billing_purchases WHERE id=?1",
                    [&p.id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            420_000
        );
        let public = serde_json::to_value(s.billing_purchase(&p.id).unwrap()).unwrap();
        assert!(public.get("payment_fee_micro_eur").is_none());
        assert!(
            public["receipt_url"]
                .as_str()
                .unwrap()
                .starts_with("https://pay.stripe.com/")
        );
    }
    #[tokio::test]
    async fn real_http_checkout_uses_frozen_amount_and_server_redirect() {
        use axum::{
            Json, Router,
            extract::{Form, State},
            http::HeaderMap,
            routing::post,
        };
        let captured = Arc::new(Mutex::new(None));
        type Captured = Arc<Mutex<Option<(HeaderMap, BTreeMap<String, String>)>>>;
        async fn create(
            State(c): State<Captured>,
            h: HeaderMap,
            Form(f): Form<BTreeMap<String, String>>,
        ) -> Json<Value> {
            *c.lock().unwrap() = Some((h, f));
            Json(json!({"id":"cs_synthetic","url":"https://checkout.stripe.com/c/pay/synthetic"}))
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/v1/checkout/sessions", post(create))
            .with_state(captured.clone());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut service = service();
        service.endpoint = format!("http://{address}/v1");
        let mut s = db();
        let p = purchase(&mut s, "http");
        let value = service
            .checkout(&p, "cus_synthetic", "https://todoer.example")
            .await
            .unwrap();
        s.billing_checkout_saved(&p.id, &value).unwrap();
        let (h, f) = captured.lock().unwrap().take().unwrap();
        assert_eq!(h["idempotency-key"], format!("todoer-checkout-{}", p.id));
        assert_eq!(f["line_items[0][price_data][unit_amount]"], "1000");
        assert_eq!(f["line_items[0][price_data][currency]"], "eur");
        assert!(!f.contains_key("payment_method_types[0]"));
        assert_eq!(
            f["success_url"],
            format!("https://todoer.example/settings/billing?purchase={}", p.id)
        );
        assert!(!f.contains_key("customer_email"));
        assert_eq!(s.ai_wallet("dev").unwrap().posted_micro, 0);
        task.abort();
    }
}
