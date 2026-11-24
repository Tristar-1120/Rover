//! Thread-safe hierarchical token and USD accounting (`Meter`, `UsageLog`).

use crate::pricing::{estimate_cost, nonnegative_number, PRICING_CHECKED};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderCostRow {
    pub cost_usd: Option<f64>,
    pub priced_calls: u64,
    pub unpriced_calls: u64,
    pub cost_estimated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeterSnapshot {
    pub calls: u64,
    #[serde(rename = "in")]
    pub inp: u64,
    pub out: u64,
    pub total: u64,
    pub estimated: bool,
    pub by: HashMap<String, [u64; 3]>,
    pub cost_usd: Option<f64>,
    pub priced_calls: u64,
    pub unpriced_calls: u64,
    pub cost_estimated: bool,
    pub by_cost: HashMap<String, ProviderCostRow>,
}

pub fn format_cost(
    cost_usd: Option<f64>,
    unpriced_calls: u64,
    cost_estimated: bool,
    compact: bool,
) -> String {
    let Some(cost) = cost_usd.filter(|c| c.is_finite() && *c >= 0.0) else {
        return if compact {
            "USD unavailable".to_string()
        } else {
            "USD cost unavailable (pricing needed)".to_string()
        };
    };
    let approx = if cost_estimated { "~" } else { "" };
    let amount = if cost > 0.0 && cost < 0.00000001 {
        "<$0.00000001".to_string()
    } else if cost >= 0.0001 || cost == 0.0 {
        format!("${cost:.4}")
    } else {
        let s = format!("${cost:.8}");
        s.trim_end_matches('0').to_string()
    };
    let plus = if unpriced_calls > 0 { "+" } else { "" };
    let mut res = format!("{approx}{amount}{plus} USD");
    if unpriced_calls > 0 && !compact {
        let word = if unpriced_calls == 1 { "call" } else { "calls" };
        res.push_str(&format!(" (partial; {unpriced_calls} unpriced {word})"));
    }
    res
}

#[derive(Default)]
struct MeterInner {
    calls: u64,
    inp: u64,
    out: u64,
    estimated: bool,
    by: HashMap<String, [u64; 3]>,
    cost_usd: f64,
    priced_calls: u64,
    unpriced_calls: u64,
    cost_estimated: bool,
    by_cost: HashMap<String, ProviderCostRow>,
}

#[derive(Clone, Default)]
pub struct Meter {
    parent: Option<Arc<Meter>>,
    inner: Arc<Mutex<MeterInner>>,
}

impl Meter {
    pub fn new(parent: Option<Arc<Meter>>) -> Self {
        Self {
            parent,
            inner: Arc::new(Mutex::new(MeterInner::default())),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &self,
        pid: &str,
        inp: u64,
        out: u64,
        estimated: bool,
        model: &str,
        cached_in: u64,
        cache_write: u64,
        cache_write_long: u64,
        reported_cost: Option<&Value>,
        custom_rates: Option<&Value>,
    ) {
        let (cost, approx_cost) = estimate_cost(
            pid,
            model,
            inp,
            out,
            cached_in,
            cache_write,
            cache_write_long,
            reported_cost,
            custom_rates,
            None,
        );
        {
            let mut g = self.inner.lock();
            g.calls += 1;
            g.inp += inp;
            g.out += out;
            g.estimated |= estimated;
            let row = g.by.entry(pid.to_string()).or_insert([0, 0, 0]);
            row[0] += 1;
            row[1] += inp;
            row[2] += out;

            match cost {
                None => {
                    g.unpriced_calls += 1;
                    let billing = g.by_cost.entry(pid.to_string()).or_insert(ProviderCostRow {
                        cost_usd: Some(0.0),
                        priced_calls: 0,
                        unpriced_calls: 0,
                        cost_estimated: false,
                    });
                    billing.unpriced_calls += 1;
                }
                Some(c) => {
                    g.cost_usd += c;
                    g.priced_calls += 1;
                    g.cost_estimated |= approx_cost;
                    let billing = g.by_cost.entry(pid.to_string()).or_insert(ProviderCostRow {
                        cost_usd: Some(0.0),
                        priced_calls: 0,
                        unpriced_calls: 0,
                        cost_estimated: false,
                    });
                    *billing.cost_usd.get_or_insert(0.0) += c;
                    billing.priced_calls += 1;
                    billing.cost_estimated |= approx_cost;
                }
            }
        }
        if let Some(ref p) = self.parent {
            p.add(
                pid,
                inp,
                out,
                estimated,
                model,
                cached_in,
                cache_write,
                cache_write_long,
                reported_cost,
                custom_rates,
            );
        }
    }

    pub fn snapshot(&self) -> MeterSnapshot {
        let g = self.inner.lock();
        let mut by_cost = g.by_cost.clone();
        for row in by_cost.values_mut() {
            if row.priced_calls == 0 && row.unpriced_calls > 0 {
                row.cost_usd = None;
            }
        }
        let cost_usd = if g.priced_calls > 0 || g.calls == 0 {
            Some(g.cost_usd)
        } else {
            None
        };
        MeterSnapshot {
            calls: g.calls,
            inp: g.inp,
            out: g.out,
            total: g.inp + g.out,
            estimated: g.estimated,
            by: g.by.clone(),
            cost_usd,
            priced_calls: g.priced_calls,
            unpriced_calls: g.unpriced_calls,
            cost_estimated: g.cost_estimated,
            by_cost,
        }
    }

    pub fn cost_usd(&self) -> Option<f64> {
        self.snapshot().cost_usd
    }

    pub fn total(&self) -> u64 {
        self.snapshot().total
    }

    pub fn cost_summary(&self) -> String {
        let s = self.snapshot();
        format_cost(s.cost_usd, s.unpriced_calls, s.cost_estimated, false)
    }

    pub fn short(&self) -> String {
        let s = self.snapshot();
        let approx = if s.estimated { "~" } else { "" };
        let cost_str = format_cost(s.cost_usd, s.unpriced_calls, s.cost_estimated, true);
        format!("{approx}{} tokens ({} calls) · {cost_str}", s.total, s.calls)
    }
}

#[derive(Clone)]
pub struct UsageLog {
    pub path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl UsageLog {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn record(
        &self,
        mode: &str,
        provider: &str,
        model: &str,
        task: &str,
        meter: &Meter,
        status: &str,
    ) {
        let snap = meter.snapshot();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let task_clipped: String = task.chars().take(80).collect();
        let entry = json!({
            "time": now,
            "mode": mode,
            "provider": provider,
            "model": model,
            "task": task_clipped,
            "status": status,
            "usage_version": 2,
            "pricing_checked": PRICING_CHECKED,
            "calls": snap.calls,
            "in": snap.inp,
            "out": snap.out,
            "estimated": snap.estimated,
            "by": snap.by,
            "cost_usd": snap.cost_usd,
            "priced_calls": snap.priced_calls,
            "unpriced_calls": snap.unpriced_calls,
            "cost_estimated": snap.cost_estimated,
            "by_cost": snap.by_cost,
        });
        let _g = self.lock.lock();
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.path) {
            if let Ok(line) = serde_json::to_string(&entry) {
                let _ = writeln!(file, "{line}");
            }
        }
    }

    fn read_entries(&self) -> Vec<Value> {
        let _g = self.lock.lock();
        let Ok(raw) = fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for line in raw.lines() {
            if let Ok(val) = serde_json::from_str::<Value>(line) {
                if let Some(obj) = val.as_object() {
                    if obj.contains_key("calls") && obj.contains_key("in") && obj.contains_key("out")
                    {
                        out.push(val);
                    }
                }
            }
        }
        out
    }

    pub fn recent(&self, limit: usize) -> Vec<Value> {
        if limit == 0 {
            return Vec::new();
        }
        let mut entries = self.read_entries();
        entries.reverse();
        entries.truncate(limit);
        entries
    }

    pub fn cost_data(entry: &Value) -> ProviderCostRow {
        let calls = entry.get("calls").and_then(|v| v.as_u64()).unwrap_or(0);
        if let Some(cost_val) = entry.get("cost_usd") {
            let cost = nonnegative_number(cost_val);
            let raw_priced = entry
                .get("priced_calls")
                .and_then(|v| v.as_u64())
                .unwrap_or(if cost.is_some() { calls } else { 0 });
            let priced = if cost.is_some() {
                calls.min(raw_priced)
            } else {
                0
            };
            let raw_unpriced = entry
                .get("unpriced_calls")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let unpriced = (calls - priced).max(raw_unpriced);
            return ProviderCostRow {
                cost_usd: cost,
                priced_calls: priced,
                unpriced_calls: unpriced,
                cost_estimated: entry
                    .get("cost_estimated")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            };
        }
        if calls == 0 {
            return ProviderCostRow {
                cost_usd: Some(0.0),
                priced_calls: 0,
                unpriced_calls: 0,
                cost_estimated: false,
            };
        }
        let inp = entry.get("in").and_then(|v| v.as_u64()).unwrap_or(0);
        let out = entry.get("out").and_then(|v| v.as_u64()).unwrap_or(0);
        let provider = entry.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        let model = entry.get("model").and_then(|v| v.as_str()).unwrap_or("");
        let clean_model = model.strip_prefix("models/").unwrap_or(model);
        let tier_unknown =
            provider == "gemini" && clean_model == "gemini-2.5-pro" && calls > 1 && inp > 200_000;
        let on_date = entry.get("time").and_then(|v| v.as_str());
        let (cost, estimated) = if tier_unknown {
            (None, false)
        } else {
            estimate_cost(provider, model, inp, out, 0, 0, 0, None, None, on_date)
        };
        ProviderCostRow {
            cost_usd: cost,
            priced_calls: if cost.is_some() { calls } else { 0 },
            unpriced_calls: if cost.is_none() { calls } else { 0 },
            cost_estimated: estimated,
        }
    }

    pub fn totals(&self) -> Value {
        let entries = self.read_entries();
        let mut calls = 0u64;
        let mut inp = 0u64;
        let mut out = 0u64;
        let mut priced_calls = 0u64;
        let mut unpriced_calls = 0u64;
        let mut cost_sum = 0.0f64;
        let mut cost_estimated = false;
        let mut estimated = false;

        for e in &entries {
            let billing = Self::cost_data(e);
            calls += e.get("calls").and_then(|v| v.as_u64()).unwrap_or(0);
            inp += e.get("in").and_then(|v| v.as_u64()).unwrap_or(0);
            out += e.get("out").and_then(|v| v.as_u64()).unwrap_or(0);
            priced_calls += billing.priced_calls;
            unpriced_calls += billing.unpriced_calls;
            cost_sum += billing.cost_usd.unwrap_or(0.0);
            cost_estimated |= billing.cost_estimated;
            estimated |= e.get("estimated").and_then(|v| v.as_bool()).unwrap_or(false);
        }

        let final_cost = if calls > 0 && priced_calls == 0 {
            None
        } else {
            Some(cost_sum)
        };

        json!({
            "tasks": entries.len(),
            "calls": calls,
            "in": inp,
            "out": out,
            "total": inp + out,
            "cost_usd": final_cost,
            "priced_calls": priced_calls,
            "unpriced_calls": unpriced_calls,
            "cost_estimated": cost_estimated,
            "estimated": estimated,
        })
    }
}
