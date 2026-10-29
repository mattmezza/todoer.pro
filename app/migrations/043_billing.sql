CREATE TABLE billing_accounts (
 user_id TEXT PRIMARY KEY REFERENCES users(id),
 stripe_customer_id TEXT UNIQUE,
 debt_micro INTEGER NOT NULL DEFAULT 0 CHECK(debt_micro>=0),
 blocked INTEGER NOT NULL DEFAULT 0 CHECK(blocked IN (0,1))
);
CREATE TABLE billing_purchases (
 id TEXT PRIMARY KEY,
 user_id TEXT NOT NULL REFERENCES users(id),
 idempotency_key TEXT NOT NULL,
 amount_minor INTEGER NOT NULL CHECK(amount_minor>0),
 currency TEXT NOT NULL CHECK(currency IN ('eur','usd','gbp','cad')),
 credit_micro INTEGER NOT NULL CHECK(credit_micro>0),
 fx_rate_micros INTEGER NOT NULL CHECK(fx_rate_micros>0),
 mode TEXT NOT NULL CHECK(mode IN ('manual','auto')),
 save_method INTEGER NOT NULL DEFAULT 0,
 status TEXT NOT NULL DEFAULT 'created',
 stripe_session_id TEXT UNIQUE,
 stripe_payment_id TEXT UNIQUE,
 stripe_charge_id TEXT UNIQUE,
 checkout_url TEXT,
 receipt_url TEXT,
 payment_fee_minor INTEGER CHECK(payment_fee_minor>=0),
 payment_fee_currency TEXT,
 fee_fx_rate_micros INTEGER,
 payment_fee_micro_eur INTEGER CHECK(payment_fee_micro_eur>=0),
 refunded_minor INTEGER NOT NULL DEFAULT 0,
 created_at INTEGER NOT NULL DEFAULT(unixepoch()),
 updated_at INTEGER NOT NULL DEFAULT(unixepoch()),
 paid_at INTEGER,
 refunded_at INTEGER,
 UNIQUE(user_id,idempotency_key)
);
CREATE INDEX billing_purchases_user ON billing_purchases(user_id,created_at DESC,id);
CREATE UNIQUE INDEX billing_auto_inflight ON billing_purchases(user_id) WHERE mode='auto' AND status IN ('created','pending','requires_action','unknown');
CREATE TABLE billing_auto_topup (
 user_id TEXT PRIMARY KEY REFERENCES users(id),
 enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0,1)),
 paused_reason TEXT NOT NULL DEFAULT '',
 amount_micro INTEGER NOT NULL DEFAULT 10000000 CHECK(amount_micro>=5000000),
 threshold_micro INTEGER NOT NULL DEFAULT 2000000 CHECK(threshold_micro>=0),
 monthly_cap_micro INTEGER NOT NULL DEFAULT 50000000 CHECK(monthly_cap_micro>=0),
 cooldown_seconds INTEGER NOT NULL DEFAULT 3600 CHECK(cooldown_seconds>=60),
 consent_version TEXT NOT NULL DEFAULT '',
 consent_at INTEGER,
 payment_method_id TEXT,
 last_attempt_at INTEGER
);
CREATE TABLE billing_stripe_events (
 id TEXT PRIMARY KEY,
 event_type TEXT NOT NULL,
 purchase_id TEXT,
 created_at INTEGER NOT NULL DEFAULT(unixepoch())
);
CREATE TABLE billing_adjustments (
 id TEXT PRIMARY KEY,
 purchase_id TEXT NOT NULL REFERENCES billing_purchases(id),
 amount_micro INTEGER NOT NULL CHECK(amount_micro>=0),
 debt_micro INTEGER NOT NULL CHECK(debt_micro>=0),
 created_at INTEGER NOT NULL DEFAULT(unixepoch())
);
CREATE TABLE billing_disputes (
 id TEXT PRIMARY KEY,
 purchase_id TEXT NOT NULL REFERENCES billing_purchases(id),
 state TEXT NOT NULL,
 amount_minor INTEGER NOT NULL CHECK(amount_minor>0),
 terminal INTEGER NOT NULL DEFAULT 0 CHECK(terminal IN (0,1)),
 created_at INTEGER NOT NULL DEFAULT(unixepoch())
);
