CREATE TABLE ai_provider_policy (
 id TEXT PRIMARY KEY CHECK(id='openrouter'), enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN(0,1)),
 margin_bps INTEGER NOT NULL DEFAULT 0 CHECK(margin_bps BETWEEN 0 AND 9500),
 usd_eur_ppm INTEGER NOT NULL DEFAULT 0 CHECK(usd_eur_ppm BETWEEN 0 AND 10000000),
 daily_limit_micro INTEGER NOT NULL DEFAULT 5000000 CHECK(daily_limit_micro>0), version INTEGER NOT NULL DEFAULT 1
);
INSERT INTO ai_provider_policy(id) VALUES('openrouter');
CREATE TABLE ai_models (
 id TEXT PRIMARY KEY,name TEXT NOT NULL,provider TEXT NOT NULL DEFAULT 'openrouter' REFERENCES ai_provider_policy(id),
 enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN(0,1)),classify INTEGER NOT NULL DEFAULT 1,draft INTEGER NOT NULL DEFAULT 1,
 input_micro_per_million INTEGER NOT NULL DEFAULT 0 CHECK(input_micro_per_million BETWEEN 0 AND 1000000000),
 output_micro_per_million INTEGER NOT NULL DEFAULT 0 CHECK(output_micro_per_million BETWEEN 0 AND 1000000000),
 cached_micro_per_million INTEGER NOT NULL DEFAULT 0 CHECK(cached_micro_per_million BETWEEN 0 AND input_micro_per_million),
 margin_bps INTEGER NOT NULL DEFAULT -1 CHECK(margin_bps BETWEEN -1 AND 9500),version INTEGER NOT NULL DEFAULT 1,
 daily_limit_micro INTEGER NOT NULL DEFAULT 1000000 CHECK(daily_limit_micro>0)
);
INSERT INTO ai_models(id,name) VALUES ('google/gemini-2.5-flash-lite','Gemini 2.5 Flash Lite'),('openai/gpt-4.1-mini','GPT-4.1 Mini'),('anthropic/claude-sonnet-4.6','Claude Sonnet 4.6');
CREATE TABLE ai_wallet_entries (
 id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),operation_id TEXT NOT NULL UNIQUE,
 kind TEXT NOT NULL,amount_micro INTEGER NOT NULL CHECK(amount_micro BETWEEN -1000000000000 AND 1000000000000),created_at INTEGER NOT NULL DEFAULT(unixepoch())
);
CREATE INDEX ai_wallet_user ON ai_wallet_entries(user_id,created_at);
CREATE TABLE ai_credit_holds (
 operation_id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),amount_micro INTEGER NOT NULL CHECK(amount_micro>=0),
 free_micro INTEGER NOT NULL DEFAULT 0 CHECK(free_micro>=0 AND free_micro<=amount_micro),state TEXT NOT NULL CHECK(state IN('held','released','settled')),created_at INTEGER NOT NULL DEFAULT(unixepoch())
);
CREATE TABLE ai_operations (
 id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),actor TEXT NOT NULL,request_hash TEXT NOT NULL,
 kind TEXT NOT NULL,funding TEXT NOT NULL CHECK(funding IN('credits','byok')),model_id TEXT NOT NULL,
 actual_model TEXT,route_json TEXT NOT NULL DEFAULT '{}',provider TEXT,status TEXT NOT NULL CHECK(status IN('quoted','dispatching','settled','released','unknown')),
 max_micro INTEGER NOT NULL,input_rate INTEGER NOT NULL,output_rate INTEGER NOT NULL,cached_rate INTEGER NOT NULL,
 margin_bps INTEGER NOT NULL,usd_eur_ppm INTEGER NOT NULL,rate_version INTEGER NOT NULL,provider_version INTEGER NOT NULL,
 input_bound INTEGER NOT NULL,output_bound INTEGER NOT NULL,input_tokens INTEGER,output_tokens INTEGER,cached_tokens INTEGER,reasoning_tokens INTEGER,
 customer_micro INTEGER NOT NULL DEFAULT 0,free_micro INTEGER NOT NULL DEFAULT 0,free_provider_cost_micro_eur INTEGER NOT NULL DEFAULT 0,dispatched_at INTEGER,provider_micro_usd INTEGER,provider_micro_eur INTEGER,generation_id TEXT,
 created_at INTEGER NOT NULL DEFAULT(unixepoch()),expires_at INTEGER NOT NULL,settled_at INTEGER
);
CREATE INDEX ai_operations_user ON ai_operations(user_id,created_at,id);
CREATE UNIQUE INDEX ai_operations_generation ON ai_operations(generation_id) WHERE generation_id IS NOT NULL;
CREATE TABLE ai_operation_keys(user_id TEXT NOT NULL,kind TEXT NOT NULL,request_key TEXT NOT NULL,operation_id TEXT NOT NULL REFERENCES ai_operations(id),PRIMARY KEY(user_id,kind,request_key));
CREATE TRIGGER ai_ledger_no_update BEFORE UPDATE ON ai_wallet_entries BEGIN SELECT RAISE(ABORT,'credit ledger is append only'); END;
CREATE TRIGGER ai_ledger_no_delete BEFORE DELETE ON ai_wallet_entries BEGIN SELECT RAISE(ABORT,'credit ledger is append only'); END;
CREATE TABLE ai_usage_audit(id INTEGER PRIMARY KEY,actor TEXT NOT NULL,action TEXT NOT NULL,target TEXT NOT NULL,created_at INTEGER NOT NULL DEFAULT(unixepoch()));
CREATE TABLE ai_free_grants(reference TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),admin_id TEXT NOT NULL,amount_micro INTEGER NOT NULL CHECK(amount_micro>0),reason TEXT NOT NULL,created_at INTEGER NOT NULL DEFAULT(unixepoch()));
