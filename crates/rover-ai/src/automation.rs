//! Local Automation & Two-Tier Background Watchers (`automation`).
//!
//! Evaluates conditions (`text`, `gone`, `url`, `title`, `element`, `number` with `near` anchor,
//! `changed` SHA-1 digest, plus `all`/`any` combinators) 100% locally with zero AI tokens.
//!
//! Includes Rover's **Two-Tier Watcher Architecture**:
//! - **Tier 1 (Native Rust Fast HTTP Poll)**: Watchers without interactive click/type steps can
//!   poll static pages in ~50 KB of RAM without spawning a 150 MB Chromium renderer tab.
//! - **Tier 2 (Headless Browser Tab)**: Watchers with pre-check steps (`click`, `type`, `select`)
//!   or dynamic SPA pages execute inside an invisible background browser tab.

use crate::agent::{PageSnapshot, PlannedStep};
use once_cell::sync::Lazy;
use rand::Rng;
use regex::Regex;
use rover_core::store::now_ts;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::HashSet;

pub const MIN_INTERVAL_USER: f64 = 5.0;
pub const MIN_INTERVAL_AI: f64 = 15.0;
pub const AI_MAX_HOURS: f64 = 24.0;
pub const MAX_STEPS: usize = 12;
pub const MAX_WATCHERS: usize = 10;

static SAFE_WATCH_ACTIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "navigate",
        "reload",
        "wait",
        "scroll",
        "click",
        "type",
        "select",
        "press_key",
    ]
    .into_iter()
    .collect()
});

static NUMBER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[$€£]?\s?(\d{1,3}(?:,\d{3})+|\d+)(?:\.(\d+))?").unwrap()
});

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Condition {
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Text {
        value: String,
        #[serde(default)]
        regex: bool,
    },
    Gone {
        value: String,
        #[serde(default)]
        regex: bool,
    },
    Url {
        value: String,
    },
    Title {
        value: String,
    },
    Element {
        value: String,
    },
    Number {
        op: String,
        num: f64,
        #[serde(default)]
        near: Option<String>,
    },
    Changed {
        #[serde(default)]
        near: Option<String>,
    },
}

pub fn normalize_condition(raw: &Value) -> Result<Condition, String> {
    let obj = raw
        .as_object()
        .ok_or_else(|| "Condition must be a JSON object".to_string())?;

    if let Some(all_arr) = obj.get("all").and_then(|v| v.as_array()) {
        if all_arr.is_empty() || all_arr.len() > 6 {
            return Err("'all' condition must have 1..=6 sub-conditions".into());
        }
        let mut subs = Vec::new();
        for item in all_arr {
            subs.push(normalize_condition(item)?);
        }
        return Ok(Condition::All(subs));
    }

    if let Some(any_arr) = obj.get("any").and_then(|v| v.as_array()) {
        if any_arr.is_empty() || any_arr.len() > 6 {
            return Err("'any' condition must have 1..=6 sub-conditions".into());
        }
        let mut subs = Vec::new();
        for item in any_arr {
            subs.push(normalize_condition(item)?);
        }
        return Ok(Condition::Any(subs));
    }

    let ctype = obj
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    match ctype.as_str() {
        "text" | "gone" => {
            let val = obj
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if val.is_empty() {
                return Err(format!("'{ctype}' condition requires a non-empty 'value'"));
            }
            let is_re = obj.get("regex").and_then(|v| v.as_bool()).unwrap_or(false);
            if is_re && Regex::new(&val).is_err() {
                return Err("Invalid regular expression in condition".into());
            }
            if ctype == "text" {
                Ok(Condition::Text {
                    value: val,
                    regex: is_re,
                })
            } else {
                Ok(Condition::Gone {
                    value: val,
                    regex: is_re,
                })
            }
        }
        "url" | "title" | "element" => {
            let val = obj
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if val.is_empty() {
                return Err(format!("'{ctype}' condition requires a non-empty 'value'"));
            }
            match ctype.as_str() {
                "url" => Ok(Condition::Url { value: val }),
                "title" => Ok(Condition::Title { value: val }),
                _ => Ok(Condition::Element { value: val }),
            }
        }
        "number" => {
            let op = obj
                .get("op")
                .and_then(|v| v.as_str())
                .unwrap_or("<")
                .trim()
                .to_string();
            if !matches!(op.as_str(), "<" | "<=" | ">" | ">=" | "==" | "!=") {
                return Err(format!("Unsupported numeric operator '{op}'"));
            }
            let num = obj
                .get("num")
                .or_else(|| obj.get("value"))
                .and_then(|v| {
                    v.as_f64()
                        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
                })
                .filter(|n| n.is_finite())
                .ok_or_else(|| "'number' condition requires a finite 'num'".to_string())?;
            let near = obj
                .get("near")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            Ok(Condition::Number { op, num, near })
        }
        "changed" => {
            let near = obj
                .get("near")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            Ok(Condition::Changed { near })
        }
        other => Err(format!("Unknown condition type '{other}'")),
    }
}

/// Evaluates `cond` against `snap` and updates `state` (used for `Changed` baseline SHA-1 digests).
/// Returns `(matched, human_reason)`.
pub fn check_condition(
    cond: &Condition,
    snap: &PageSnapshot,
    state: &mut Value,
) -> (bool, String) {
    match cond {
        Condition::All(subs) => {
            let mut reasons = Vec::new();
            for s in subs {
                let (ok, why) = check_condition(s, snap, state);
                if !ok {
                    return (false, why);
                }
                reasons.push(why);
            }
            (true, reasons.join(" AND "))
        }
        Condition::Any(subs) => {
            let mut last_why = String::new();
            for s in subs {
                let (ok, why) = check_condition(s, snap, state);
                if ok {
                    return (true, why);
                }
                last_why = why;
            }
            (false, last_why)
        }
        Condition::Text { value, regex } => {
            let found = if *regex {
                Regex::new(&format!("(?i){value}"))
                    .map(|re| re.is_match(&snap.text))
                    .unwrap_or(false)
            } else {
                snap.text
                    .to_ascii_lowercase()
                    .contains(&value.to_ascii_lowercase())
            };
            if found {
                (true, format!("“{value}” appeared on the page"))
            } else {
                (false, format!("“{value}” is not on the page yet"))
            }
        }
        Condition::Gone { value, regex } => {
            let found = if *regex {
                Regex::new(&format!("(?i){value}"))
                    .map(|re| re.is_match(&snap.text))
                    .unwrap_or(false)
            } else {
                snap.text
                    .to_ascii_lowercase()
                    .contains(&value.to_ascii_lowercase())
            };
            if !found {
                (true, format!("“{value}” is no longer on the page"))
            } else {
                (false, format!("“{value}” is still present"))
            }
        }
        Condition::Url { value } => {
            let ok = snap
                .url
                .to_ascii_lowercase()
                .contains(&value.to_ascii_lowercase());
            (
                ok,
                if ok {
                    format!("URL contains “{value}”")
                } else {
                    format!("URL does not contain “{value}”")
                },
            )
        }
        Condition::Title { value } => {
            let ok = snap
                .title
                .to_ascii_lowercase()
                .contains(&value.to_ascii_lowercase());
            (
                ok,
                if ok {
                    format!("Title contains “{value}”")
                } else {
                    format!("Title does not contain “{value}”")
                },
            )
        }
        Condition::Element { value } => {
            let want = value.to_ascii_lowercase();
            let ok = snap.items.iter().any(|it| {
                it.text.to_ascii_lowercase().contains(&want)
                    || it
                        .placeholder
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&want)
            });
            (
                ok,
                if ok {
                    format!("Control “{value}” is visible")
                } else {
                    format!("Control “{value}” is not visible")
                },
            )
        }
        Condition::Number { op, num, near } => {
            let window = match near {
                Some(anchor) => {
                    let low_text = snap.text.to_ascii_lowercase();
                    let low_anchor = anchor.to_ascii_lowercase();
                    match low_text.find(&low_anchor) {
                        Some(idx) => {
                            let end = (idx + anchor.len() + 90).min(snap.text.len());
                            &snap.text[idx..end]
                        }
                        None => return (false, format!("“{anchor}” was not found on the page")),
                    }
                }
                None => snap.text.as_str(),
            };
            for caps in NUMBER_RE.captures_iter(window) {
                let int_part = caps[1].replace(',', "");
                let frac_part = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                let num_str = if frac_part.is_empty() {
                    int_part
                } else {
                    format!("{int_part}.{frac_part}")
                };
                if let Ok(found_val) = num_str.parse::<f64>() {
                    let matched = match op.as_str() {
                        "<" => found_val < *num,
                        "<=" => found_val <= *num,
                        ">" => found_val > *num,
                        ">=" => found_val >= *num,
                        "==" => (found_val - *num).abs() < 1e-9,
                        "!=" => (found_val - *num).abs() >= 1e-9,
                        _ => false,
                    };
                    if matched {
                        return (true, format!("Found {found_val} ({op} {num})"));
                    }
                }
            }
            (false, format!("No number matching {op} {num} yet"))
        }
        Condition::Changed { near } => {
            let norm: String = snap.text.split_whitespace().collect::<Vec<_>>().join(" ");
            let slice = match near {
                Some(anchor) => {
                    let low = norm.to_ascii_lowercase();
                    match low.find(&anchor.to_ascii_lowercase()) {
                        Some(idx) => {
                            let end = (idx + 300).min(norm.len());
                            &norm[idx..end]
                        }
                        None => norm.as_str(),
                    }
                }
                None => norm.as_str(),
            };
            let mut h = Sha1::new();
            h.update(slice.as_bytes());
            let digest = hex::encode(h.finalize());

            if !state.is_object() {
                *state = json!({});
            }
            let prev = state
                .get("digest")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            state["digest"] = json!(digest);
            match prev {
                None => (false, "Recorded initial page state".to_string()),
                Some(old) if old != digest => (true, "The page content changed".to_string()),
                Some(_) => (false, "No change since last check".to_string()),
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatcherSpec {
    pub id: String,
    pub name: String,
    pub url: String,
    pub every: f64,
    pub condition: Condition,
    pub steps: Vec<PlannedStep>,
    pub then: Vec<String>,
    pub ask: String,
    pub task: String,
    pub repeat: bool,
    pub reload: bool,
    pub source: String,
    pub max_runs: usize,
    pub created: f64,
    pub until: Option<f64>,
}

impl WatcherSpec {
    /// Returns true if this watcher can execute in Tier-1 Fast Native HTTP mode (~50 KB RAM)
    /// without spinning up a full Chromium renderer tab.
    pub fn can_use_fast_http_tier(&self) -> bool {
        self.steps.is_empty()
            && !matches!(self.condition, Condition::Element { .. })
    }
}

#[allow(clippy::too_many_arguments)]
pub fn new_watcher_spec(
    name: &str,
    url: &str,
    every_secs: f64,
    raw_condition: &Value,
    steps: Vec<PlannedStep>,
    then: Vec<String>,
    ask: &str,
    task: &str,
    repeat: bool,
    reload: bool,
    source: &str,
) -> Result<WatcherSpec, String> {
    let clean_url = url.trim();
    if !clean_url.to_ascii_lowercase().starts_with("http://")
        && !clean_url.to_ascii_lowercase().starts_with("https://")
    {
        return Err("Watcher requires an http:// or https:// URL".into());
    }
    let min_iv = if source == "ai" {
        MIN_INTERVAL_AI
    } else {
        MIN_INTERVAL_USER
    };
    if !every_secs.is_finite() || every_secs < min_iv {
        return Err(format!(
            "Check interval must be at least {min_iv} seconds"
        ));
    }
    if steps.len() > MAX_STEPS {
        return Err(format!("Watcher cannot have more than {MAX_STEPS} steps"));
    }
    for s in &steps {
        if !SAFE_WATCH_ACTIONS.contains(s.action.as_str()) {
            return Err(format!(
                "Action '{}' is not allowed inside a background watcher",
                s.action
            ));
        }
    }
    let condition = normalize_condition(raw_condition)?;
    let ts = now_ts();
    let until = if source == "ai" {
        Some(ts + AI_MAX_HOURS * 3600.0)
    } else {
        None
    };
    let mut rng = rand::thread_rng();
    let id: String = std::iter::once('w')
        .chain((0..11).map(|_| char::from_digit(rng.gen_range(0..10), 10).unwrap()))
        .collect();

    let final_then = if then.is_empty() {
        vec!["notify".to_string()]
    } else {
        then
    };

    Ok(WatcherSpec {
        id,
        name: if name.trim().is_empty() {
            "Watcher".into()
        } else {
            name.trim().to_string()
        },
        url: clean_url.to_string(),
        every: every_secs,
        condition,
        steps,
        then: final_then,
        ask: ask.trim().to_string(),
        task: task.trim().to_string(),
        repeat,
        reload,
        source: source.to_string(),
        max_runs: 2000,
        created: ts,
        until,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Watcher {
    pub spec: WatcherSpec,
    pub enabled: bool,
    pub runs: usize,
    pub errors: usize,
    pub was_met: bool,
    pub last_status: String,
    pub state: Value,
}

impl Watcher {
    pub fn new(spec: WatcherSpec) -> Self {
        Self {
            spec,
            enabled: true,
            runs: 0,
            errors: 0,
            was_met: false,
            last_status: "Waiting for first check".into(),
            state: json!({}),
        }
    }

    /// Evaluates one check cycle against `snap`.
    /// Returns `true` ONLY on a rising edge (`met && (repeat || !was_met)`) so AI is never polled on non-matches.
    pub fn evaluate_snapshot(&mut self, snap: &PageSnapshot, now: f64) -> (bool, String) {
        if !self.enabled {
            return (false, "Paused".into());
        }
        if let Some(until) = self.spec.until {
            if now > until {
                self.enabled = false;
                self.last_status = "Expired (24h AI limit)".into();
                return (false, self.last_status.clone());
            }
        }
        if self.runs >= self.spec.max_runs {
            self.enabled = false;
            self.last_status = "Stopped (reached max runs)".into();
            return (false, self.last_status.clone());
        }

        self.runs += 1;
        self.errors = 0;
        let (met, reason) = check_condition(&self.spec.condition, snap, &mut self.state);
        self.last_status = reason.clone();

        let fire = met && (self.spec.repeat || !self.was_met);
        self.was_met = met;
        if fire && !self.spec.repeat {
            self.enabled = false;
        }
        (fire, reason)
    }
}
