//! Content-free inference accounting. Customer charges and upstream expense are separate.
use crate::agent_ops::StoreWrite;
use crate::core::{Error, Store, actor_identity};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn invalid(s: &str) -> Error {
    Error::Invalid(s.into())
}
pub(crate) fn digest(v: &Value) -> Result<String, Error> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(v).map_err(|_| invalid("invalid inference request"))?)
    ))
}
fn ceil(n: i128, d: i128) -> Result<i64, Error> {
    if n < 0 || d <= 0 {
        return Err(invalid("invalid amount"));
    }
    i64::try_from((n + d - 1) / d).map_err(|_| invalid("amount overflow"))
}
pub fn decimal_micro(value: &Value) -> Option<i64> {
    let raw = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    let (mantissa, exponent) = raw
        .split_once(['e', 'E'])
        .map_or((raw.as_str(), 0), |(m, e)| {
            (m, e.parse::<i32>().unwrap_or(100))
        });
    if mantissa.starts_with('-') || exponent.unsigned_abs() > 12 {
        return None;
    }
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.len() + fraction.len() > 24
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{whole}{fraction}").parse::<i128>().ok()?;
    let scale = 6 + exponent - i32::try_from(fraction.len()).ok()?;
    let result = if scale >= 0 {
        digits.checked_mul(10i128.checked_pow(scale as u32)?)?
    } else {
        let d = 10i128.checked_pow((-scale) as u32)?;
        (digits + d - 1) / d
    };
    i64::try_from(result)
        .ok()
        .filter(|v| *v <= 1_000_000_000_000)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub classify: bool,
    pub draft: bool,
    pub input_micro_per_million: i64,
    pub output_micro_per_million: i64,
    pub cached_micro_per_million: i64,
    pub margin_bps: i64,
    pub version: i64,
    pub daily_limit_micro: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderPolicy {
    pub enabled: bool,
    pub margin_bps: i64,
    pub usd_eur_ppm: i64,
    pub daily_limit_micro: i64,
    pub version: i64,
}
#[derive(Serialize)]
pub struct Wallet {
    pub posted_micro: i64,
    pub held_micro: i64,
    pub available_micro: i64,
    pub currency: &'static str,
    pub free_micro: i64,
    pub paid_micro: i64,
    pub free_available_micro: i64,
    pub paid_available_micro: i64,
}
#[derive(Clone, Serialize)]
pub struct Quote {
    pub id: String,
    pub model_id: String,
    pub funding: String,
    pub max_micro: i64,
    pub currency: &'static str,
    pub expires_at: i64,
    pub provider_estimated_max_micro_usd: Option<i64>,
    pub input_rate: i64,
    pub output_rate: i64,
}
#[derive(Clone, Default)]
pub struct Usage {
    pub generation_id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub cost_micro_usd: Option<i64>,
}
impl Usage {
    pub fn parse(v: &Value) -> Self {
        fn count(v: &Value) -> Option<i64> {
            v.as_i64().filter(|n| (0..=100_000_000).contains(n))
        }
        fn label(v: &Value) -> Option<String> {
            v.as_str()
                .filter(|s| !s.is_empty() && s.len() <= 200 && !s.chars().any(char::is_control))
                .map(str::to_owned)
        }
        Self {
            generation_id: label(&v["id"]),
            model: label(&v["model"]),
            provider: label(&v["provider"]),
            input_tokens: count(&v["usage"]["prompt_tokens"]),
            output_tokens: count(&v["usage"]["completion_tokens"]),
            cached_tokens: count(&v["usage"]["prompt_tokens_details"]["cached_tokens"]),
            reasoning_tokens: count(&v["usage"]["completion_tokens_details"]["reasoning_tokens"]),
            cost_micro_usd: decimal_micro(&v["usage"]["cost"]),
        }
    }
}
fn valid_model_id(id: &str) -> bool {
    id.len() <= 150
        && id.split_once('/').is_some_and(|(provider, model)| {
            !provider.is_empty()
                && !model.is_empty()
                && !model.contains('/')
                && provider
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && model
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        })
}
impl Store {
    pub fn ai_provider_policy(&self) -> Result<ProviderPolicy, Error> {
        Ok(self.conn.query_row("SELECT enabled,margin_bps,usd_eur_ppm,daily_limit_micro,version FROM ai_provider_policy WHERE id='openrouter'",[],|r|Ok(ProviderPolicy{enabled:r.get(0)?,margin_bps:r.get(1)?,usd_eur_ppm:r.get(2)?,daily_limit_micro:r.get(3)?,version:r.get(4)?}))?)
    }
    pub fn ai_models(&self, admin: bool) -> Result<Vec<Value>, Error> {
        let provider = self.ai_provider_policy()?;
        let rows=self.conn.prepare("SELECT id,name,enabled,classify,draft,input_micro_per_million,output_micro_per_million,cached_micro_per_million,margin_bps,version,daily_limit_micro FROM ai_models WHERE archived=0 ORDER BY input_micro_per_million,id")?.query_map([],|r|Ok(Model{id:r.get(0)?,name:r.get(1)?,enabled:r.get(2)?,classify:r.get(3)?,draft:r.get(4)?,input_micro_per_million:r.get(5)?,output_micro_per_million:r.get(6)?,cached_micro_per_million:r.get(7)?,margin_bps:r.get(8)?,version:r.get(9)?,daily_limit_micro:r.get(10)?}))?.collect::<Result<Vec<_>,_>>()?;
        rows.into_iter()
            .filter(|m| admin || (m.enabled && provider.enabled))
            .map(|m| {
                let mut v = serde_json::to_value(&m).map_err(|_| invalid("invalid model"))?;
                if !admin {
                    let margin = if m.margin_bps < 0 {
                        provider.margin_bps
                    } else {
                        m.margin_bps
                    };
                    for key in [
                        "input_micro_per_million",
                        "output_micro_per_million",
                        "cached_micro_per_million",
                    ] {
                        v[key] = json!(ceil(
                            i128::from(v[key].as_i64().unwrap_or(0)) * 10000,
                            i128::from(10000 - margin)
                        )?);
                    }
                    v.as_object_mut().unwrap().remove("margin_bps");
                    v.as_object_mut().unwrap().remove("daily_limit_micro");
                }
                Ok(v)
            })
            .collect()
    }
    pub fn ai_model_update(&mut self, admin: &str, m: Model) -> Result<(), Error> {
        crate::ai_policy::require_admin(&self.conn, admin)?;
        if !valid_model_id(&m.id)
            || m.name.trim().is_empty()
            || m.name.len() > 120
            || m.margin_bps < -1
            || m.margin_bps > 9500
            || m.daily_limit_micro <= 0
            || [
                m.input_micro_per_million,
                m.output_micro_per_million,
                m.cached_micro_per_million,
            ]
            .iter()
            .any(|n| !(0..=1_000_000_000).contains(n))
            || m.cached_micro_per_million > m.input_micro_per_million
        {
            return Err(invalid("invalid model rates or limits"));
        }
        if m.enabled
            && (m.input_micro_per_million == 0
                || m.output_micro_per_million == 0
                || (!m.classify && !m.draft))
        {
            return Err(invalid("configure nonzero model rates before enabling"));
        }
        let tx = StoreWrite::new(self)?;
        if tx.conn.execute("UPDATE ai_models SET name=?2,enabled=?3,classify=?4,draft=?5,input_micro_per_million=?6,output_micro_per_million=?7,cached_micro_per_million=?8,margin_bps=?9,version=version+1,daily_limit_micro=?10 WHERE id=?1 AND version=?11 AND archived=0",params![m.id,m.name,m.enabled,m.classify,m.draft,m.input_micro_per_million,m.output_micro_per_million,m.cached_micro_per_million,m.margin_bps,m.daily_limit_micro,m.version])? !=1 {return Err(Error::Conflict("model changed or unsupported model".into()));}
        tx.conn.execute(
            "INSERT INTO ai_usage_audit(actor,action,target) VALUES(?1,'model.update',?2)",
            params![admin, m.id],
        )?;
        tx.conn.execute("INSERT INTO ai_admin_audit(actor,target,action,reason,before_json,after_json) VALUES(?1,?2,'model.update','Update supported model','{}','{}')", params![admin,m.id])?;
        tx.commit()
    }
    pub fn ai_model_create(&mut self, admin: &str, id: &str, name: &str) -> Result<(), Error> {
        crate::ai_policy::require_admin(&self.conn, admin)?;
        if !valid_model_id(id)
            || name.trim().is_empty()
            || name.len() > 120
            || name.chars().any(char::is_control)
        {
            return Err(invalid("Use a valid OpenRouter model ID and display name"));
        }
        let tx = StoreWrite::new(self)?;
        let existing: Option<bool> = tx
            .conn
            .query_row("SELECT archived FROM ai_models WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        match existing {
            Some(false) => {
                return Err(Error::Conflict(
                    "This model is already in the catalog".into(),
                ));
            }
            Some(true) => {
                tx.conn.execute("UPDATE ai_models SET archived=0,enabled=0,name=?2,version=version+1 WHERE id=?1", params![id,name.trim()])?;
            }
            None => {
                tx.conn.execute(
                    "INSERT INTO ai_models(id,name,classify,draft) VALUES(?1,?2,0,0)",
                    params![id, name.trim()],
                )?;
            }
        }
        tx.conn.execute(
            "INSERT INTO ai_usage_audit(actor,action,target) VALUES(?1,'model.add',?2)",
            params![admin, id],
        )?;
        tx.conn.execute("INSERT INTO ai_admin_audit(actor,target,action,reason,before_json,after_json) VALUES(?1,?2,'model.add','Add supported model','{}','{}')", params![admin,id])?;
        tx.commit()
    }
    pub fn ai_model_remove(&mut self, admin: &str, id: &str, version: i64) -> Result<(), Error> {
        crate::ai_policy::require_admin(&self.conn, admin)?;
        let tx = StoreWrite::new(self)?;
        if tx.conn.execute("UPDATE ai_models SET archived=1,enabled=0,version=version+1 WHERE id=?1 AND version=?2 AND archived=0", params![id,version])? != 1 {
            return Err(Error::Conflict("Model changed or was already removed. Refresh and try again.".into()));
        }
        tx.conn.execute(
            "INSERT INTO ai_usage_audit(actor,action,target) VALUES(?1,'model.remove',?2)",
            params![admin, id],
        )?;
        tx.conn.execute("INSERT INTO ai_admin_audit(actor,target,action,reason,before_json,after_json) VALUES(?1,?2,'model.remove','Remove supported model','{}','{}')", params![admin,id])?;
        tx.commit()
    }
    pub fn ai_provider_update(&mut self, admin: &str, p: ProviderPolicy) -> Result<(), Error> {
        crate::ai_policy::require_admin(&self.conn, admin)?;
        if !(0..=9500).contains(&p.margin_bps)
            || !(0..=10_000_000).contains(&p.usd_eur_ppm)
            || p.daily_limit_micro <= 0
        {
            return Err(invalid("invalid provider policy"));
        }
        let tx = StoreWrite::new(self)?;
        if tx.conn.execute("UPDATE ai_provider_policy SET enabled=?1,margin_bps=?2,usd_eur_ppm=?3,daily_limit_micro=?4,version=version+1 WHERE id='openrouter' AND version=?5",params![p.enabled,p.margin_bps,p.usd_eur_ppm,p.daily_limit_micro,p.version])?!=1{return Err(Error::Conflict("provider policy changed".into()));}
        tx.conn.execute("INSERT INTO ai_usage_audit(actor,action,target) VALUES(?1,'provider.update','openrouter')",[admin])?;
        tx.conn.execute("INSERT INTO ai_admin_audit(actor,target,action,reason,before_json,after_json) VALUES(?1,'openrouter','provider.update','Update provider controls','{}','{}')",[admin])?;
        tx.commit()
    }
    pub fn ai_wallet(&self, actor: &str) -> Result<Wallet, Error> {
        let user = actor_identity(&self.conn, actor)?.0;
        self.ai_wallet_user(&user)
    }
    pub(crate) fn ai_wallet_user(&self, user: &str) -> Result<Wallet, Error> {
        let posted = self.conn.query_row(
            "SELECT COALESCE(sum(amount_micro),0) FROM ai_wallet_entries WHERE user_id=?1",
            [user],
            |r| r.get::<_, i64>(0),
        )?;
        let held=self.conn.query_row("SELECT COALESCE(sum(amount_micro),0) FROM ai_credit_holds WHERE user_id=?1 AND state='held'",[user],|r|r.get::<_,i64>(0))?;
        let free:i64=self.conn.query_row("SELECT COALESCE(sum(amount_micro),0) FROM ai_wallet_entries WHERE user_id=?1 AND kind IN('free_grant','ai_usage_free','free_revoke')",[user],|r|r.get(0))?;
        let free_held:i64=self.conn.query_row("SELECT COALESCE(sum(free_micro),0) FROM ai_credit_holds WHERE user_id=?1 AND state='held'",[user],|r|r.get(0))?;
        Ok(Wallet {
            free_micro: free,
            paid_micro: posted - free,
            free_available_micro: (free - free_held).max(0),
            paid_available_micro: (posted - free - held + free_held).max(0),
            posted_micro: posted,
            held_micro: held,
            available_micro: (posted - held).max(0),
            currency: "EUR",
        })
    }
    pub fn ai_credit(
        &mut self,
        user: &str,
        reference: &str,
        amount: i64,
        kind: &str,
    ) -> Result<(), Error> {
        if reference.is_empty()
            || reference.len() > 250
            || amount.unsigned_abs() > 1_000_000_000_000
            || kind.len() > 40
        {
            return Err(invalid("invalid ledger entry"));
        }
        if let Some((u, a, k)) = self
            .conn
            .query_row(
                "SELECT user_id,amount_micro,kind FROM ai_wallet_entries WHERE operation_id=?1",
                [reference],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
        {
            if u != user || a != amount || k != kind {
                return Err(Error::Conflict("ledger reference mismatch".into()));
            }
            return Ok(());
        }
        self.conn.execute("INSERT INTO ai_wallet_entries(id,user_id,operation_id,kind,amount_micro) VALUES(?1,?2,?3,?4,?5)",params![Uuid::new_v4().to_string(),user,reference,kind,amount])?;
        Ok(())
    }
    pub fn ai_quote(
        &mut self,
        actor: &str,
        id: &str,
        kind: &str,
        key: &str,
        body: &Value,
    ) -> Result<Quote, Error> {
        let resolved = self.ai_resolve(actor)?;
        let input_bound = i64::try_from(
            serde_json::to_vec(body)
                .map_err(|_| invalid("invalid request"))?
                .len(),
        )
        .map_err(|_| invalid("request too large"))?
            + 8192;
        let output_bound = body["max_tokens"]
            .as_i64()
            .filter(|n| *n > 0 && *n <= 4096)
            .ok_or_else(|| invalid("invalid output bound"))?;
        let fingerprint = digest(body)?;
        let tx = StoreWrite::new(self)?;
        if let Some((existing,old))=tx.conn.query_row("SELECT k.operation_id,o.request_hash FROM ai_operation_keys k JOIN ai_operations o ON o.id=k.operation_id WHERE k.user_id=?1 AND k.kind=?2 AND k.request_key=?3",params![resolved.user_id,kind,key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?{
            if existing!=id||old!=fingerprint{return Err(Error::Conflict("inference key already used; retain the original credential and payload".into()));}
            return tx.ai_quote_read(actor,id);
        }
        let p = tx.ai_provider_policy()?;
        let m = tx
            .ai_models(true)?
            .into_iter()
            .find(|m| m["id"] == resolved.model_id)
            .ok_or(Error::NotFound)?;
        if !p.enabled
            || m["enabled"] != true
            || m[kind] != true
            || body["model"] != resolved.model_id
        {
            return Err(invalid("selected model does not support this operation"));
        }
        if resolved.funding == "credits" && p.usd_eur_ppm == 0 {
            return Err(invalid("managed AI pricing is not configured"));
        }
        let margin = if m["margin_bps"].as_i64().unwrap_or(-1) < 0 {
            p.margin_bps
        } else {
            m["margin_bps"].as_i64().unwrap_or(0)
        };
        let rate = |key: &str| {
            ceil(
                i128::from(m[key].as_i64().unwrap_or(0)) * 10000,
                i128::from(10000 - margin),
            )
        };
        let ir = rate("input_micro_per_million")?;
        let or = rate("output_micro_per_million")?;
        let cr = rate("cached_micro_per_million")?;
        let max = if resolved.funding == "credits" {
            ceil(
                i128::from(input_bound) * i128::from(ir)
                    + i128::from(output_bound) * i128::from(or),
                1_000_000,
            )?
        } else {
            0
        };
        tx.conn.execute("INSERT INTO ai_operations(id,user_id,actor,request_hash,kind,funding,model_id,status,max_micro,input_rate,output_rate,cached_rate,usd_eur_ppm,rate_version,provider_version,input_bound,output_bound,route_json,margin_bps,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,'quoted',?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,unixepoch()+600)",params![id,resolved.user_id,actor,fingerprint,kind,resolved.funding,resolved.model_id,max,ir,or,cr,p.usd_eur_ppm,m["version"].as_i64().unwrap_or(1),p.version,input_bound,output_bound,body["provider"].to_string(),margin])?;
        tx.conn.execute(
            "INSERT INTO ai_operation_keys VALUES(?1,?2,?3,?4)",
            params![resolved.user_id, kind, key, id],
        )?;
        let q = tx.ai_quote_read(actor, id)?;
        tx.commit()?;
        Ok(q)
    }
    fn ai_quote_read(&self, actor: &str, id: &str) -> Result<Quote, Error> {
        let user = actor_identity(&self.conn, actor)?.0;
        Ok(self.conn.query_row("SELECT id,model_id,funding,max_micro,expires_at,input_rate,output_rate,input_bound,output_bound,route_json FROM ai_operations WHERE id=?1 AND user_id=?2",params![id,user],|r|Ok(Quote{id:r.get(0)?,model_id:r.get(1)?,funding:r.get(2)?,max_micro:r.get(3)?,currency:"EUR",expires_at:r.get(4)?,input_rate:r.get(5)?,output_rate:r.get(6)?,provider_estimated_max_micro_usd:if r.get::<_,String>(2)?!="byok"{None}else{let route:String=r.get(9)?;let route:Value=serde_json::from_str(&route).unwrap_or(Value::Null);match (decimal_micro(&route["max_price"]["prompt"]),decimal_micro(&route["max_price"]["completion"])) {(Some(i),Some(o))=>ceil(i128::from(r.get::<_,i64>(7)?)*i128::from(i)+i128::from(r.get::<_,i64>(8)?)*i128::from(o),1_000_000).ok(),_=>None}}}))?)
    }
    pub fn ai_dispatch(
        &mut self,
        actor: &str,
        id: &str,
        accepted: &str,
        body: &Value,
    ) -> Result<crate::ai_policy::AiFunding, Error> {
        if id != accepted {
            return Err(invalid("accept the exact inference quote"));
        }
        let fingerprint = digest(body)?;
        let tx = StoreWrite::new(self)?;
        let resolved = tx.ai_resolve(actor)?;
        let user_policy = tx.ai_policy(actor)?;
        let (state,funding,model,hash,max,expires):(String,String,String,String,i64,i64)=tx.conn.query_row("SELECT status,funding,model_id,request_hash,max_micro,expires_at FROM ai_operations WHERE id=?1 AND user_id=?2",params![id,resolved.user_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
        let now = chrono::Utc::now().timestamp();
        if state != "quoted"
            || expires < now
            || funding != resolved.funding
            || model != resolved.model_id
            || hash != fingerprint
        {
            return Err(Error::Conflict(
                "quote expired, already dispatched, or settings changed".into(),
            ));
        }
        if funding == "credits" {
            if tx.ai_wallet_user(&resolved.user_id)?.available_micro < max {
                return Err(Error::Conflict("insufficient credits".into()));
            }
            let (model_cap,global_cap):(i64,i64)=tx.conn.query_row("SELECT m.daily_limit_micro,p.daily_limit_micro FROM ai_models m JOIN ai_provider_policy p ON p.id=m.provider WHERE m.id=?1",[&model],|r|Ok((r.get(0)?,r.get(1)?)))?;
            let user_spent:i64=tx.conn.query_row("SELECT COALESCE(sum(CASE WHEN status IN('dispatching','unknown') THEN max_micro ELSE customer_micro END),0) FROM ai_operations WHERE user_id=?1 AND funding='credits' AND created_at>=unixepoch('now','start of day')",[&resolved.user_id],|r|r.get(0))?;
            if user_spent.saturating_add(max) > user_policy.daily_limit_micro {
                return Err(Error::Conflict("account daily AI limit reached".into()));
            }
            for (scope, cap) in [(Some(model.as_str()), model_cap), (None, global_cap)] {
                let spent:i64=tx.conn.query_row("SELECT COALESCE(sum(CASE WHEN status IN('dispatching','unknown') THEN max_micro ELSE MAX(customer_micro,COALESCE(provider_micro_eur,0)) END),0) FROM ai_operations WHERE funding='credits' AND created_at>=unixepoch('now','start of day') AND (?1 IS NULL OR model_id=?1)",[scope],|r|r.get(0))?;
                if spent.saturating_add(max) > cap {
                    return Err(Error::Conflict("deployment AI spend limit reached".into()));
                }
            }
            let free_hold = tx
                .ai_wallet_user(&resolved.user_id)?
                .free_available_micro
                .min(max);
            tx.conn.execute("INSERT INTO ai_credit_holds(operation_id,user_id,amount_micro,free_micro,state) VALUES(?1,?2,?3,?4,'held')",params![id,resolved.user_id,max,free_hold])?;
        }
        tx.conn.execute(
            "UPDATE ai_operations SET status='dispatching',dispatched_at=unixepoch() WHERE id=?1",
            [id],
        )?;
        tx.commit()?;
        Ok(resolved)
    }
    pub fn ai_settle(
        &mut self,
        actor: &str,
        id: &str,
        usage: &Usage,
        usable: bool,
    ) -> Result<(), Error> {
        let user = actor_identity(&self.conn, actor)?.0;
        self.ai_settle_user(&user, id, usage, usable)
    }
    pub(crate) fn ai_settle_user(
        &mut self,
        user: &str,
        id: &str,
        usage: &Usage,
        usable: bool,
    ) -> Result<(), Error> {
        let mut tx = StoreWrite::nested(self)?;
        let (state,funding,model,max,ir,or,cr,fx,ib,ob,margin):(String,String,String,i64,i64,i64,i64,i64,i64,i64,i64)=tx.conn.query_row("SELECT status,funding,model_id,max_micro,input_rate,output_rate,cached_rate,usd_eur_ppm,input_bound,output_bound,margin_bps FROM ai_operations WHERE id=?1 AND user_id=?2",params![id,user],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?)))?;
        if state != "dispatching" && state != "unknown" {
            return Ok(());
        }
        let cost_eur = usage
            .cost_micro_usd
            .filter(|_| fx > 0)
            .map(|c| ceil(i128::from(c) * i128::from(fx), 1_000_000))
            .transpose()?;
        let mut charge = 0;
        let mut status = "released";
        if usable {
            match (usage.input_tokens, usage.output_tokens) {
                (Some(i), Some(o))
                    if i <= ib
                        && o <= ob
                        && usage.model.as_deref() == Some(model.as_str())
                        && usage.cost_micro_usd.is_some() =>
                {
                    let c = usage.cached_tokens.unwrap_or(0);
                    let reasoning = usage.reasoning_tokens.unwrap_or(0);
                    if c > i || reasoning > o {
                        status = "unknown";
                    } else {
                        charge = if funding == "credits" {
                            ceil(
                                i128::from(cost_eur.unwrap_or(0)) * 10000,
                                i128::from(10000 - margin),
                            )?
                            .min(ceil(
                                i128::from(i - c) * i128::from(ir)
                                    + i128::from(c) * i128::from(cr)
                                    + i128::from(o) * i128::from(or),
                                1_000_000,
                            )?)
                        } else {
                            0
                        };
                        if charge > max {
                            charge = 0;
                            status = "unknown";
                        } else {
                            status = "settled";
                        }
                    }
                }
                _ => status = "unknown",
            }
        }
        let free_used = if status == "settled" && funding == "credits" {
            let reserved = tx.conn.query_row(
                "SELECT free_micro FROM ai_credit_holds WHERE operation_id=?1",
                [id],
                |r| r.get::<_, i64>(0),
            )?;
            reserved.min(charge)
        } else {
            0
        };
        if status == "settled" && charge > 0 {
            if charge > free_used {
                tx.ai_credit(
                    user,
                    &format!("usage:{id}"),
                    -(charge - free_used),
                    "ai_usage",
                )?;
            }
            if free_used > 0 {
                tx.ai_credit(
                    user,
                    &format!("usage-free:{id}"),
                    -free_used,
                    "ai_usage_free",
                )?;
            }
        }
        let free_cost = if charge > 0 {
            ceil(
                i128::from(cost_eur.unwrap_or(0)) * i128::from(free_used),
                i128::from(charge),
            )?
        } else {
            0
        };
        tx.conn.execute("UPDATE ai_operations SET status=?2,provider=?3,input_tokens=?4,output_tokens=?5,cached_tokens=?6,reasoning_tokens=?7,customer_micro=?8,provider_micro_usd=?9,provider_micro_eur=?10,generation_id=?11,actual_model=?12,free_micro=?13,free_provider_cost_micro_eur=?14,settled_at=unixepoch() WHERE id=?1",params![id,status,usage.provider,usage.input_tokens,usage.output_tokens,usage.cached_tokens,usage.reasoning_tokens,charge,usage.cost_micro_usd,cost_eur,usage.generation_id,usage.model,free_used,free_cost])?;
        if status != "unknown" {
            tx.conn.execute(
                "UPDATE ai_credit_holds SET state=?2 WHERE operation_id=?1",
                params![
                    id,
                    if status == "settled" {
                        "settled"
                    } else {
                        "released"
                    }
                ],
            )?;
        }
        tx.commit()
    }
    pub fn ai_reconcile_expired(&mut self) -> Result<usize, Error> {
        let tx = StoreWrite::new(self)?;
        // Ambiguous calls are never resent; after one day resolve in the customer's favour.
        let n=tx.conn.execute("UPDATE ai_credit_holds SET state='released' WHERE state='held' AND operation_id IN(SELECT id FROM ai_operations WHERE status IN('dispatching','unknown') AND created_at<unixepoch()-86400)",[])?;
        tx.conn.execute("UPDATE ai_operations SET status='released',settled_at=unixepoch() WHERE status IN('dispatching','unknown') AND created_at<unixepoch()-86400",[])?;
        tx.conn.execute("UPDATE ai_operations SET status='released',settled_at=unixepoch() WHERE status='quoted' AND expires_at<unixepoch()",[])?;
        tx.commit()?;
        Ok(n)
    }
    pub fn ai_usage(
        &self,
        actor: &str,
        limit: i64,
        before: Option<&str>,
        funding: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<Value>, Error> {
        let user = actor_identity(&self.conn, actor)?.0;
        let cursor = before
            .map(|v| {
                v.split_once(':')
                    .ok_or_else(|| invalid("invalid usage cursor"))
            })
            .transpose()?;
        let (time, id) = if let Some((t, id)) = cursor {
            (
                t.parse::<i64>()
                    .map_err(|_| invalid("invalid usage cursor"))?,
                id,
            )
        } else {
            (i64::MAX, "~")
        };
        if funding.is_some_and(|v| !matches!(v, "credits" | "byok"))
            || status.is_some_and(|v| {
                !matches!(
                    v,
                    "quoted" | "dispatching" | "settled" | "released" | "unknown"
                )
            })
        {
            return Err(invalid("invalid usage filter"));
        }
        Ok(self.conn.prepare("SELECT id,kind,funding,model_id,provider,status,input_tokens,output_tokens,cached_tokens,reasoning_tokens,customer_micro,provider_micro_usd,created_at,max_micro,actual_model,input_rate,output_rate,free_micro FROM ai_operations WHERE user_id=?1 AND (created_at,id)<(?2,?3) AND (?4 IS NULL OR funding=?4) AND (?5 IS NULL OR status=?5) ORDER BY created_at DESC,id DESC LIMIT ?6")?.query_map(params![user,time,id,funding,status,limit.clamp(1,100)],|r|{let funding=r.get::<_,String>(2)?;Ok(json!({"id":r.get::<_,String>(0)?,"kind":r.get::<_,String>(1)?,"funding":funding,"model_id":r.get::<_,String>(3)?,"provider":r.get::<_,Option<String>>(4)?,"status":r.get::<_,String>(5)?,"input_tokens":r.get::<_,Option<i64>>(6)?,"output_tokens":r.get::<_,Option<i64>>(7)?,"cached_tokens":r.get::<_,Option<i64>>(8)?,"reasoning_tokens":r.get::<_,Option<i64>>(9)?,"credit_debit_micro":r.get::<_,i64>(10)?,"free_credit_micro":r.get::<_,i64>(17)?,"customer_micro":r.get::<_,i64>(10)?-r.get::<_,i64>(17)?,"byok_cost_micro_usd":if funding=="byok"{r.get::<_,Option<i64>>(11)?}else{None},"currency":"EUR","created_at":r.get::<_,i64>(12)?,"max_micro":r.get::<_,i64>(13)?,"actual_model":r.get::<_,Option<String>>(14)?,"input_rate":r.get::<_,i64>(15)?,"output_rate":r.get::<_,i64>(16)?}))})?.collect::<Result<_,_>>()?)
    }
}
impl Store {
    pub fn ai_grant(
        &mut self,
        admin: &str,
        user: &str,
        amount: i64,
        reference: &str,
        reason: &str,
    ) -> Result<(), Error> {
        crate::ai_policy::require_admin(&self.conn, admin)?;
        if !(1..=1_000_000_000).contains(&amount)
            || reference.is_empty()
            || reference.len() > 128
            || reason.trim().is_empty()
            || reason.len() > 500
        {
            return Err(invalid(
                "grant requires a positive bounded amount, reference and reason",
            ));
        }
        let mut tx = StoreWrite::new(self)?;
        if let Some((old_user, old_amount, old_reason)) = tx
            .conn
            .query_row(
                "SELECT user_id,amount_micro,reason FROM ai_free_grants WHERE reference=?1",
                [reference],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
        {
            if old_user != user || old_amount != amount || old_reason != reason {
                return Err(Error::Conflict(
                    "grant reference already used with different details".into(),
                ));
            }
            return Ok(());
        }
        tx.ai_credit(user, &format!("grant:{reference}"), amount, "free_grant")?;
        tx.conn.execute("INSERT INTO ai_free_grants(reference,user_id,admin_id,amount_micro,reason) VALUES(?1,?2,?3,?4,?5)",params![reference,user,admin,amount,reason])?;
        tx.conn.execute("INSERT INTO ai_admin_audit(actor,target,action,reason,before_json,after_json) VALUES(?1,?2,'credit.grant',?3,'{}',?4)",params![admin,user,reason,json!({"amount_micro":amount,"reference":reference}).to_string()])?;
        tx.commit()
    }
    pub fn ai_finance(&self, user: Option<&str>, from: i64, to: i64) -> Result<Value, Error> {
        let scalar = |sql: &str| -> Result<i64, Error> {
            Ok(self
                .conn
                .query_row(sql, params![user, from, to], |r| r.get(0))?)
        };
        let topups = scalar(
            "SELECT COALESCE(sum(gross_micro),0) FROM billing_purchases WHERE paid_at BETWEEN ?2 AND ?3 AND (?1 IS NULL OR user_id=?1)",
        )?;
        let conversion_deductions = scalar(
            "SELECT COALESCE(sum(gross_micro-credit_micro),0) FROM billing_purchases WHERE paid_at BETWEEN ?2 AND ?3 AND (?1 IS NULL OR user_id=?1)",
        )?;
        let conversion_reversals = scalar(
            "SELECT COALESCE(sum(conversion_reversal_micro),0) FROM billing_adjustments WHERE purchase_id IN(SELECT id FROM billing_purchases WHERE (?1 IS NULL OR user_id=?1)) AND created_at BETWEEN ?2 AND ?3",
        )?;
        let conversion_revenue = conversion_deductions - conversion_reversals;
        let earned = scalar(
            "SELECT COALESCE(sum(customer_micro-free_micro),0) FROM ai_operations WHERE funding='credits' AND (?1 IS NULL OR user_id=?1) AND settled_at BETWEEN ?2 AND ?3",
        )?;
        let expense = scalar(
            "SELECT COALESCE(sum(provider_micro_eur),0) FROM ai_operations WHERE funding='credits' AND (?1 IS NULL OR user_id=?1) AND dispatched_at BETWEEN ?2 AND ?3",
        )?;
        let unknown = scalar(
            "SELECT count(*) FROM ai_operations WHERE funding='credits' AND dispatched_at IS NOT NULL AND provider_micro_eur IS NULL AND (?1 IS NULL OR user_id=?1) AND dispatched_at BETWEEN ?2 AND ?3",
        )? > 0;
        let outstanding=self.conn.query_row("SELECT COALESCE(sum(amount_micro),0) FROM ai_wallet_entries WHERE (?1 IS NULL OR user_id=?1)",[user],|r|r.get::<_,i64>(0))?;
        let refunds = scalar(
            "SELECT COALESCE(sum(gross_refund_micro),0) FROM billing_adjustments WHERE purchase_id IN(SELECT id FROM billing_purchases WHERE (?1 IS NULL OR user_id=?1)) AND created_at BETWEEN ?2 AND ?3",
        )?;
        let fee_known:bool=self.conn.query_row("SELECT NOT EXISTS(SELECT 1 FROM billing_purchases WHERE paid_at BETWEEN ?2 AND ?3 AND payment_fee_micro_eur IS NULL AND (?1 IS NULL OR user_id=?1))",params![user,from,to],|r|r.get(0))?;
        let fees = scalar(
            "SELECT COALESCE(sum(payment_fee_micro_eur),0) FROM billing_purchases WHERE paid_at BETWEEN ?2 AND ?3 AND (?1 IS NULL OR user_id=?1)",
        )?;
        let free_issued = scalar(
            "SELECT COALESCE(sum(amount_micro),0) FROM ai_free_grants WHERE (?1 IS NULL OR user_id=?1) AND created_at BETWEEN ?2 AND ?3",
        )?;
        let free_used = scalar(
            "SELECT COALESCE(sum(free_micro),0) FROM ai_operations WHERE funding='credits' AND (?1 IS NULL OR user_id=?1) AND settled_at BETWEEN ?2 AND ?3",
        )?;
        let free_cost = scalar(
            "SELECT COALESCE(sum(free_provider_cost_micro_eur),0) FROM ai_operations WHERE funding='credits' AND (?1 IS NULL OR user_id=?1) AND dispatched_at BETWEEN ?2 AND ?3",
        )?;
        let free_remaining=self.conn.query_row("SELECT COALESCE(sum(amount_micro),0) FROM ai_wallet_entries WHERE kind IN('free_grant','ai_usage_free','free_revoke') AND (?1 IS NULL OR user_id=?1)",[user],|r|r.get::<_,i64>(0))?;
        let mut q=self.conn.prepare("SELECT model_id,provider,count(*),COALESCE(sum(customer_micro-free_micro),0),COALESCE(sum(provider_micro_eur),0),sum(provider_micro_eur IS NULL),COALESCE(sum(input_tokens),0),COALESCE(sum(output_tokens),0),COALESCE(sum(free_micro),0),COALESCE(sum(free_provider_cost_micro_eur),0) FROM ai_operations WHERE funding='credits' AND dispatched_at IS NOT NULL AND (?1 IS NULL OR user_id=?1) AND dispatched_at BETWEEN ?2 AND ?3 GROUP BY model_id,provider ORDER BY model_id,provider")?;
        let models=q.query_map(params![user,from,to],|r|{let revenue=r.get::<_,i64>(3)?;let cost=r.get::<_,i64>(4)?;Ok(json!({"model_id":r.get::<_,String>(0)?,"provider":r.get::<_,Option<String>>(1)?,"calls":r.get::<_,i64>(2)?,"usage_revenue_micro":revenue,"provider_cost_micro":cost,"provider_fx_unknown":r.get::<_,i64>(5)?>0,"contribution_micro":revenue-cost,"margin_bps":if revenue>0{Some((i128::from(revenue-cost)*10000/i128::from(revenue)) as i64)}else{None},"input_tokens":r.get::<_,i64>(6)?,"output_tokens":r.get::<_,i64>(7)?,"free_credits_used_micro":r.get::<_,i64>(8)?,"free_inference_cost_micro":r.get::<_,i64>(9)?}))})?.collect::<Result<Vec<_>,_>>()?;
        let byok_calls = scalar(
            "SELECT count(*) FROM ai_operations WHERE funding='byok' AND (?1 IS NULL OR user_id=?1) AND dispatched_at BETWEEN ?2 AND ?3",
        )?;
        let bad_debt = scalar(
            "SELECT COALESCE(sum(debt_micro),0) FROM billing_adjustments WHERE purchase_id IN(SELECT id FROM billing_purchases WHERE (?1 IS NULL OR user_id=?1)) AND created_at BETWEEN ?2 AND ?3",
        )?;
        let profit = if !unknown && fee_known {
            Some(earned + conversion_revenue - expense - fees - bad_debt)
        } else {
            None
        };
        Ok(
            json!({"currency":"EUR","topups_micro":topups,"conversion_deductions_micro":conversion_deductions,"conversion_revenue_micro":conversion_revenue,"refunds_micro":refunds,"bad_debt_micro":bad_debt,"net_cash_collected_micro":topups-refunds,"usage_revenue_micro":earned,"provider_cost_micro":expense,"outstanding_micro":outstanding,"paid_outstanding_micro":outstanding-free_remaining,"payment_fees_micro":if fee_known{Some(fees)}else{None},"profit_micro":profit,"margin_bps":profit.filter(|_|earned+conversion_revenue>0).map(|p|i128::from(p)*10000/i128::from(earned+conversion_revenue)),"contribution_micro":earned+conversion_revenue-expense,"provider_fx_unknown":unknown,"provisional":unknown||!fee_known,"models":models,"byok_calls":byok_calls,"free_credits_issued_micro":free_issued,"free_credits_used_micro":free_used,"free_credits_remaining_micro":free_remaining,"free_inference_cost_micro":free_cost,"from":from,"to":to}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Store {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        s.conn.execute("INSERT INTO ai_account_policy(user_id,user_ai_enabled,managed_ai_allowed,funding,model_id,daily_limit_micro) VALUES('dev',1,1,'credits','openai/gpt-4.1-mini',1000000)",[]).unwrap();
        s.conn.execute("UPDATE ai_models SET enabled=1,input_micro_per_million=1000000,output_micro_per_million=4000000,cached_micro_per_million=100000,margin_bps=2000 WHERE id='openai/gpt-4.1-mini'",[]).unwrap();
        s.conn
            .execute("UPDATE ai_provider_policy SET usd_eur_ppm=900000", [])
            .unwrap();
        s.ai_credit("dev", "fixture", 2_000_000, "topup").unwrap();
        s
    }
    #[test]
    fn model_catalog_requires_admin_and_removal_preserves_versioned_history() {
        let mut db = fixture();
        assert!(
            db.ai_model_create("dev", "vendor/new-model", "New model")
                .is_err()
        );
        db.conn
            .execute("INSERT INTO auth_admins VALUES('dev')", [])
            .unwrap();
        for id in [
            "https://evil.example/model",
            "vendor/../evil",
            "vendor/model?key=x",
            "vendor/a/b",
            "vendor/",
            "/model",
        ] {
            assert!(db.ai_model_create("dev", id, "Invalid").is_err());
        }
        db.ai_model_create("dev", "vendor/new-model:free", "New model")
            .unwrap();
        let created = db
            .ai_models(true)
            .unwrap()
            .into_iter()
            .find(|m| m["id"] == "vendor/new-model:free")
            .unwrap();
        assert_eq!(created["enabled"], false);
        assert_eq!(created["classify"], false);
        assert!(
            !db.ai_models(false)
                .unwrap()
                .iter()
                .any(|m| m["id"] == created["id"])
        );
        let mut model: Model = serde_json::from_value(created).unwrap();
        model.enabled = true;
        assert!(db.ai_model_update("dev", model.clone()).is_err());
        model.input_micro_per_million = 1000000;
        model.output_micro_per_million = 2000000;
        model.classify = true;
        db.ai_model_update("dev", model.clone()).unwrap();
        assert!(
            db.ai_models(false)
                .unwrap()
                .iter()
                .any(|m| m["id"] == model.id)
        );
        assert!(db.ai_model_remove("dev", &model.id, model.version).is_err());
        model.version += 1;
        db.ai_model_remove("dev", &model.id, model.version).unwrap();
        assert!(db.ai_model_update("dev", model.clone()).is_err());
        assert!(
            !db.ai_models(true)
                .unwrap()
                .iter()
                .any(|m| m["id"] == model.id)
        );
        let rows: i64 = db
            .conn
            .query_row(
                "SELECT count(*) FROM ai_models WHERE id=?1",
                [&model.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 1);
        db.ai_model_create("dev", &model.id, "Restored model")
            .unwrap();
        let restored = db
            .ai_models(true)
            .unwrap()
            .into_iter()
            .find(|m| m["id"] == model.id)
            .unwrap();
        assert_eq!(restored["enabled"], false);
        assert!(restored["version"].as_i64().unwrap() > model.version);
    }
    fn body() -> Value {
        json!({"model":"openai/gpt-4.1-mini","max_tokens":128,"messages":[{"role":"user","content":"synthetic"}]})
    }
    fn usage() -> Usage {
        Usage {
            generation_id: Some("gen-test".into()),
            model: Some("openai/gpt-4.1-mini".into()),
            provider: Some("OpenAI".into()),
            input_tokens: Some(100),
            output_tokens: Some(10),
            cached_tokens: Some(20),
            reasoning_tokens: Some(5),
            cost_micro_usd: Some(50),
        }
    }
    #[test]
    fn retail_quote_reservation_and_settlement_are_exact_and_idempotent() {
        let mut s = fixture();
        let b = body();
        let q = s.ai_quote("dev", "op", "draft", "key", &b).unwrap();
        assert_eq!(q.input_rate, 1_250_000);
        assert_eq!(q.output_rate, 5_000_000);
        assert!(s.ai_dispatch("dev", "op", "wrong", &b).is_err());
        s.ai_dispatch("dev", "op", "op", &b).unwrap();
        assert_eq!(s.ai_wallet("dev").unwrap().held_micro, q.max_micro);
        assert!(s.ai_dispatch("dev", "op", "op", &b).is_err());
        s.ai_settle("dev", "op", &usage(), true).unwrap();
        s.ai_settle("dev", "op", &usage(), true).unwrap();
        assert_eq!(s.ai_wallet("dev").unwrap().posted_micro, 2_000_000 - 57);
        assert_eq!(s.ai_wallet("dev").unwrap().held_micro, 0);
        let public = s.ai_usage("dev", 10, None, None, None).unwrap();
        let public = serde_json::to_string(&public).unwrap();
        assert!(!public.contains("provider_micro"));
        assert!(!public.contains("request_hash"));
        assert!(!public.contains("synthetic"));
        assert!(
            s.ai_usage("other", 10, None, None, None)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn failures_release_and_unknown_usage_is_never_charged_or_retried() {
        let mut s = fixture();
        let b = body();
        s.ai_quote("dev", "fail", "draft", "fail-key", &b).unwrap();
        s.ai_dispatch("dev", "fail", "fail", &b).unwrap();
        s.ai_settle("dev", "fail", &Usage::default(), false)
            .unwrap();
        assert_eq!(s.ai_wallet("dev").unwrap().posted_micro, 2_000_000);
        assert_eq!(s.ai_wallet("dev").unwrap().held_micro, 0);
        s.ai_quote("dev", "unknown", "draft", "unknown-key", &b)
            .unwrap();
        s.ai_dispatch("dev", "unknown", "unknown", &b).unwrap();
        s.ai_settle("dev", "unknown", &Usage::default(), true)
            .unwrap();
        assert!(s.ai_wallet("dev").unwrap().held_micro > 0);
        assert!(s.ai_dispatch("dev", "unknown", "unknown", &b).is_err());
        s.conn
            .execute(
                "UPDATE ai_operations SET created_at=unixepoch()-90000 WHERE id='unknown'",
                [],
            )
            .unwrap();
        assert_eq!(s.ai_reconcile_expired().unwrap(), 1);
        assert_eq!(s.ai_wallet("dev").unwrap().held_micro, 0);
    }
    #[test]
    fn current_policy_caps_and_balance_are_checked_before_each_dispatch() {
        let mut s = fixture();
        let b = body();
        s.ai_quote("dev", "op", "draft", "key", &b).unwrap();
        s.conn
            .execute("UPDATE ai_account_policy SET user_ai_enabled=0", [])
            .unwrap();
        assert!(s.ai_dispatch("dev", "op", "op", &b).is_err());
        s.conn
            .execute(
                "UPDATE ai_account_policy SET user_ai_enabled=1,daily_limit_micro=1",
                [],
            )
            .unwrap();
        assert!(s.ai_dispatch("dev", "op", "op", &b).is_err());
        s.conn
            .execute("UPDATE ai_account_policy SET daily_limit_micro=1000000", [])
            .unwrap();
        s.ai_credit("dev", "spend", -2_000_000, "usage").unwrap();
        assert!(s.ai_dispatch("dev", "op", "op", &b).is_err());
        assert_eq!(s.ai_wallet("dev").unwrap().held_micro, 0);
    }
    #[test]
    fn price_snapshots_do_not_change_when_admin_changes_the_rate_card() {
        let mut s = fixture();
        let b = body();
        let q = s.ai_quote("dev", "op", "draft", "key", &b).unwrap();
        s.conn
            .execute(
                "UPDATE ai_models SET input_micro_per_million=99000000,version=version+1",
                [],
            )
            .unwrap();
        let q2 = s.ai_quote("dev", "op", "draft", "key", &b).unwrap();
        assert_eq!(q.max_micro, q2.max_micro);
        s.ai_dispatch("dev", "op", "op", &b).unwrap();
        s.ai_settle("dev", "op", &usage(), true).unwrap();
        assert_eq!(s.ai_wallet("dev").unwrap().posted_micro, 2_000_000 - 57);
    }
    #[test]
    fn decimal_usage_and_overflow_validation_are_fixed_point() {
        assert_eq!(decimal_micro(&json!("0.0000012")), Some(2));
        assert_eq!(decimal_micro(&json!("1e-6")), Some(1));
        assert_eq!(decimal_micro(&json!(-1)), None);
        assert_eq!(decimal_micro(&json!("1e-2147483648")), None);
        assert_eq!(decimal_micro(&json!(null)), None);
        let mut s = fixture();
        assert!(
            s.ai_credit("dev", "overflow", i64::MIN, "adjustment")
                .is_err()
        );
        assert!(s.ai_credit("dev", "fixture", 1, "topup").is_err());
        assert!(s.conn.execute("DELETE FROM ai_wallet_entries", []).is_err());
    }
    #[test]
    fn stable_cursor_cannot_skip_operations_created_in_same_second() {
        let mut s = fixture();
        let b = body();
        for i in 0..3 {
            s.ai_quote("dev", &format!("op{i}"), "draft", &format!("key{i}"), &b)
                .unwrap();
        }
        let first = s
            .ai_usage("dev", 1, None, Some("credits"), Some("quoted"))
            .unwrap();
        let cursor = format!(
            "{}:{}",
            first[0]["created_at"],
            first[0]["id"].as_str().unwrap()
        );
        let rest = s.ai_usage("dev", 10, Some(&cursor), None, None).unwrap();
        assert_eq!(rest.len(), 2);
        assert!(rest.iter().all(|v| v["id"] != first[0]["id"]));
    }
}

#[cfg(test)]
mod grant_tests {
    use super::*;
    #[test]
    fn free_grants_are_audited_idempotent_and_never_purchase_revenue() {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        s.conn
            .execute("INSERT INTO auth_admins VALUES('dev')", [])
            .unwrap();
        assert!(
            s.ai_grant("other", "dev", 10_000_000, "grant", "welcome")
                .is_err()
        );
        assert!(s.ai_grant("dev", "dev", 10_000_000, "grant", "").is_err());
        s.ai_grant("dev", "dev", 10_000_000, "grant", "welcome")
            .unwrap();
        s.ai_grant("dev", "dev", 10_000_000, "grant", "welcome")
            .unwrap();
        assert!(
            s.ai_grant("dev", "dev", 20_000_000, "grant", "welcome")
                .is_err()
        );
        assert_eq!(s.ai_wallet("dev").unwrap().free_micro, 10_000_000);
        assert_eq!(s.ai_wallet("dev").unwrap().paid_micro, 0);
        let stats = s.ai_finance(None, 0, i64::MAX).unwrap();
        assert_eq!(stats["topups_micro"], 0);
        assert_eq!(stats["usage_revenue_micro"], 0);
        assert_eq!(stats["free_credits_issued_micro"], 10_000_000);
        let count: i64 = s
            .conn
            .query_row(
                "SELECT count(*) FROM ai_admin_audit WHERE action='credit.grant'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn free_first_reservations_do_not_double_allocate_and_record_inference_loss() {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        s.conn
            .execute("INSERT INTO auth_admins VALUES('dev')", [])
            .unwrap();
        s.conn.execute("INSERT INTO ai_account_policy(user_id,user_ai_enabled,managed_ai_allowed,funding,model_id,daily_limit_micro) VALUES('dev',1,1,'credits','openai/gpt-4.1-mini',1000000)",[]).unwrap();
        s.conn.execute("UPDATE ai_models SET enabled=1,input_micro_per_million=1000000,output_micro_per_million=4000000,margin_bps=2000",[]).unwrap();
        s.conn
            .execute("UPDATE ai_provider_policy SET usd_eur_ppm=900000", [])
            .unwrap();
        s.ai_grant("dev", "dev", 50, "free", "synthetic welcome")
            .unwrap();
        s.ai_credit("dev", "paid", 1000000, "topup").unwrap();
        let body = json!({"model":"openai/gpt-4.1-mini","max_tokens":128});
        for id in ["one", "two"] {
            s.ai_quote("dev", id, "draft", id, &body).unwrap();
            s.ai_dispatch("dev", id, id, &body).unwrap();
        }
        let held_free: i64 = s
            .conn
            .query_row("SELECT sum(free_micro) FROM ai_credit_holds", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(held_free, 50);
        let usage = Usage {
            generation_id: Some("gen-grant".into()),
            model: Some("openai/gpt-4.1-mini".into()),
            input_tokens: Some(100),
            output_tokens: Some(10),
            cost_micro_usd: Some(50),
            ..Usage::default()
        };
        s.ai_settle("dev", "one", &usage, true).unwrap();
        s.ai_settle("dev", "two", &Usage::default(), false).unwrap();
        let wallet = s.ai_wallet("dev").unwrap();
        assert_eq!(wallet.free_micro, 0);
        assert_eq!(wallet.paid_micro, 999993);
        let stats = s.ai_finance(None, 0, i64::MAX).unwrap();
        assert_eq!(stats["usage_revenue_micro"], 7);
        assert_eq!(stats["free_credits_used_micro"], 50);
        assert_eq!(stats["free_inference_cost_micro"], 40);
        assert_eq!(stats["provider_cost_micro"], 45);
        let public = s.ai_usage("dev", 10, None, None, None).unwrap();
        let row = public.iter().find(|v| v["id"] == "one").unwrap();
        assert_eq!(row["customer_micro"], 7);
        assert_eq!(row["credit_debit_micro"], 57);
        assert_eq!(row["free_credit_micro"], 50);
    }
    #[test]
    fn unused_expired_quotes_do_not_become_unknown_provider_expenses() {
        let mut s = Store::memory().unwrap();
        s.seed_dev().unwrap();
        s.conn.execute("INSERT INTO ai_account_policy(user_id,user_ai_enabled,managed_ai_allowed,funding,model_id) VALUES('dev',1,1,'credits','openai/gpt-4.1-mini')",[]).unwrap();
        s.conn.execute("UPDATE ai_models SET enabled=1,input_micro_per_million=1000000,output_micro_per_million=4000000",[]).unwrap();
        s.conn
            .execute("UPDATE ai_provider_policy SET usd_eur_ppm=900000", [])
            .unwrap();
        s.ai_quote(
            "dev",
            "unused",
            "draft",
            "unused",
            &json!({"model":"openai/gpt-4.1-mini","max_tokens":128}),
        )
        .unwrap();
        s.conn
            .execute("UPDATE ai_operations SET expires_at=0", [])
            .unwrap();
        s.ai_reconcile_expired().unwrap();
        let stats = s.ai_finance(None, 0, i64::MAX).unwrap();
        assert_eq!(stats["provider_fx_unknown"], false);
        assert!(stats["models"].as_array().unwrap().is_empty());
    }
}
