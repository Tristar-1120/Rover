//! Conservative USD token pricing engine (`pricing`).
//!
//! Unknown or gateway models never imply a free `$0.00` call. Supports OpenAI cached-input
//! discounts, Anthropic 5m/1h cache write rates, Gemini long-context tiers (`> 200,000` tokens),
//! date-based promotional rate transitions, local `$0` providers (`ollama`, `lmstudio`), and
//! user custom rate overrides.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PRICING_CHECKED: &str = "2026-10-05";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RateTable {
    pub input: f64,
    pub output: f64,
    pub cache_input: f64,
    pub cache_write: f64,
    pub cache_write_long: f64,
}

impl RateTable {
    pub const fn new(
        input: f64,
        output: f64,
        cache_input: Option<f64>,
        cache_write: Option<f64>,
        cache_write_long: Option<f64>,
    ) -> Self {
        Self {
            input,
            output,
            cache_input: match cache_input {
                Some(v) => v,
                None => input,
            },
            cache_write: match cache_write {
                Some(v) => v,
                None => input,
            },
            cache_write_long: match cache_write_long {
                Some(v) => v,
                None => input,
            },
        }
    }
}

/// Rejects booleans, NaN, infinity, negatives, and malformed JSON numbers.
pub fn nonnegative_number(val: &Value) -> Option<f64> {
    match val {
        Value::Number(n) => {
            let f = n.as_f64()?;
            if f.is_finite() && f >= 0.0 {
                Some(f)
            } else {
                None
            }
        }
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return None;
            }
            let f = trimmed.parse::<f64>().ok()?;
            if f.is_finite() && f >= 0.0 {
                Some(f)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn normalize_rates(val: &Value) -> Option<RateTable> {
    let obj = val.as_object()?;
    let inp = nonnegative_number(obj.get("input")?)?;
    let out = nonnegative_number(obj.get("output")?)?;
    let cache_input = match obj.get("cache_input") {
        Some(v) => nonnegative_number(v)?,
        None => inp,
    };
    let cache_write = match obj.get("cache_write") {
        Some(v) => nonnegative_number(v)?,
        None => inp,
    };
    let cache_write_long = match obj.get("cache_write_long") {
        Some(v) => nonnegative_number(v)?,
        None => inp,
    };
    Some(RateTable {
        input: inp,
        output: out,
        cache_input,
        cache_write,
        cache_write_long,
    })
}

pub fn lookup_rates(
    pid: &str,
    model: &str,
    inp_tokens: u64,
    on_date: Option<&str>,
) -> Option<RateTable> {
    let m = model.trim();
    if matches!(pid, "ollama" | "lmstudio") {
        return Some(RateTable::new(0.0, 0.0, Some(0.0), Some(0.0), Some(0.0)));
    }
    match pid {
        "openai" => match m {
            "gpt-4o-mini" | "gpt-4o-mini-2024-07-18" => {
                Some(RateTable::new(0.15, 0.60, Some(0.075), None, None))
            }
            "gpt-4o" => Some(RateTable::new(2.50, 10.0, Some(1.25), None, None)),
            "gpt-4.1" | "gpt-4.1-2025-04-14" => {
                Some(RateTable::new(2.0, 8.0, Some(0.50), None, None))
            }
            "gpt-4.1-mini" | "gpt-4.1-mini-2025-04-14" => {
                Some(RateTable::new(0.40, 1.60, Some(0.10), None, None))
            }
            _ => None,
        },
        "anthropic" => match m {
            "claude-fable-5-1" => Some(RateTable::new(
                10.0,
                50.0,
                Some(0.25),
                Some(12.50),
                Some(20.0),
            )),
            "claude-fable-5" => Some(RateTable::new(
                10.0,
                50.0,
                Some(1.00),
                Some(12.50),
                Some(20.0),
            )),
            "claude-opus-5"
            | "claude-opus-4-8"
            | "claude-opus-4-7"
            | "claude-opus-4-6"
            | "claude-opus-4-5" => Some(RateTable::new(
                5.0,
                25.0,
                Some(0.50),
                Some(6.25),
                Some(10.0),
            )),
            "claude-opus-5-5" => {
                Some(RateTable::new(4.0, 20.0, Some(0.20), Some(5.0), Some(8.0)))
            }
            "claude-haiku-5-5" => Some(RateTable::new(
                0.10,
                0.50,
                Some(0.01),
                Some(0.125),
                Some(0.20),
            )),
            "claude-sonnet-5-5" | "claude-sonnet-5" => {
                Some(RateTable::new(2.0, 10.0, Some(0.20), Some(2.50), Some(4.0)))
            }
            "claude-sonnet-4-6" | "claude-sonnet-4-5" => {
                Some(RateTable::new(3.0, 15.0, Some(0.30), Some(3.75), Some(6.0)))
            }
            "claude-haiku-4-5" | "claude-haiku-4-5-20251001" => {
                Some(RateTable::new(1.0, 5.0, Some(0.10), Some(1.25), Some(2.0)))
            }
            _ => None,
        },
        "gemini" => {
            let clean = m.strip_prefix("models/").unwrap_or(m);
            match clean {
                "gemini-2.5-pro" => {
                    if inp_tokens > 200_000 {
                        Some(RateTable::new(2.50, 15.0, Some(0.25), None, None))
                    } else {
                        Some(RateTable::new(1.25, 10.0, Some(0.125), None, None))
                    }
                }
                "gemini-3.8-flash" | "gemini-3.7-flash" | "gemini-3.6-flash" => {
                    let cutoff = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
                    let date = match on_date {
                        Some(s) if s.len() >= 10 => {
                            NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d").ok()?
                        }
                        Some(_) => return None,
                        None => chrono::Local::now().date_naive(),
                    };
                    if date < cutoff {
                        Some(RateTable::new(0.75, 3.75, Some(0.075), None, None))
                    } else {
                        Some(RateTable::new(1.50, 7.50, Some(0.15), None, None))
                    }
                }
                "gemini-3.5-flash" => Some(RateTable::new(1.50, 9.0, Some(0.15), None, None)),
                "gemini-3.5-flash-lite" | "gemini-2.5-flash" => {
                    Some(RateTable::new(0.30, 2.50, Some(0.03), None, None))
                }
                "gemini-3.1-flash-lite" => {
                    Some(RateTable::new(0.25, 1.50, Some(0.025), None, None))
                }
                "gemini-2.5-flash-lite" => {
                    Some(RateTable::new(0.10, 0.40, Some(0.01), None, None))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn estimate_cost(
    pid: &str,
    model: &str,
    inp: u64,
    out: u64,
    cached_in: u64,
    cache_write: u64,
    cache_write_long: u64,
    reported_cost: Option<&Value>,
    custom_rates: Option<&Value>,
    on_date: Option<&str>,
) -> (Option<f64>, bool) {
    if let Some(rep_val) = reported_cost {
        if !rep_val.is_null() {
            if let Some(c) = nonnegative_number(rep_val) {
                return (Some(c), false);
            } else {
                return (None, false);
            }
        }
    }

    let price = match custom_rates {
        Some(v) if !v.is_null() => match normalize_rates(v) {
            Some(r) => r,
            None => return (None, false),
        },
        _ => match lookup_rates(pid, model, inp, on_date) {
            Some(r) => r,
            None => return (None, false),
        },
    };

    if cached_in.saturating_add(cache_write) > inp || cache_write_long > cache_write {
        return (None, false);
    }

    let uncached_in = (inp - cached_in - cache_write) as f64;
    let short_write = (cache_write - cache_write_long) as f64;
    let cost = (uncached_in * price.input
        + (out as f64) * price.output
        + (cached_in as f64) * price.cache_input
        + short_write * price.cache_write
        + (cache_write_long as f64) * price.cache_write_long)
        / 1_000_000.0;

    if !cost.is_finite() {
        return (None, false);
    }
    let is_local_default =
        matches!(pid, "ollama" | "lmstudio") && custom_rates.map(|v| v.is_null()).unwrap_or(true);
    (Some(cost), !is_local_default)
}
