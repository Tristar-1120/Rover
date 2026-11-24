//! Address bar & Homepage Search Bar autocomplete (`inline_completion`, live OpenSearch
//! suggestions with prefix cache, history/bookmark matching, and search engines).

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use rover_core::Store;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmniboxSuggestion {
    pub kind: String, // "visit", "search", "bookmark", "history", "internal"
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub fill_text: String,
}

static SUGGEST_CACHE: Lazy<Mutex<HashMap<(String, String), Vec<String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn search_engine_name(engine: &str) -> &'static str {
    match engine.trim().to_ascii_lowercase().as_str() {
        "google" => "Google",
        "bing" => "Bing",
        "brave" => "Brave Search",
        "ecosia" => "Ecosia",
        _ => "DuckDuckGo",
    }
}

pub fn search_url(engine: &str, query: &str) -> String {
    let q = utf8_percent_encode(query.trim(), NON_ALPHANUMERIC).to_string();
    match engine.trim().to_ascii_lowercase().as_str() {
        "google" => format!("https://www.google.com/search?q={q}"),
        "bing" => format!("https://www.bing.com/search?q={q}"),
        "brave" => format!("https://search.brave.com/search?q={q}"),
        "ecosia" => format!("https://www.ecosia.org/search?q={q}"),
        _ => format!("https://duckduckgo.com/?q={q}"),
    }
}

pub fn opensearch_url(engine: &str, query: &str) -> String {
    let q = utf8_percent_encode(query.trim(), NON_ALPHANUMERIC).to_string();
    match engine.trim().to_ascii_lowercase().as_str() {
        "google" => format!("https://suggestqueries.google.com/complete/search?client=firefox&q={q}"),
        "bing" => format!("https://api.bing.com/osjson.aspx?query={q}"),
        "brave" => format!("https://search.brave.com/api/suggest?q={q}"),
        "ecosia" => format!("https://ac.ecosia.org/autocomplete?q={q}&type=list"),
        _ => format!("https://duckduckgo.com/ac/?q={q}&type=list"),
    }
}

pub fn looks_like_url(input: &str) -> bool {
    let t = input.trim();
    if t.is_empty() {
        return false;
    }
    let low = t.to_ascii_lowercase();
    low.starts_with("http://")
        || low.starts_with("https://")
        || low.starts_with("file://")
        || low.starts_with("view-source:")
        || low.starts_with("about:")
        || low.starts_with("rover:")
        || low.starts_with("pelagic:")
        || low.starts_with("atlas:")
        || (!t.contains(char::is_whitespace) && t.contains('.'))
}

/// Parses OpenSearch JSON responses (`[query, [s1, s2, ...]]`) as well as
/// DuckDuckGo's `[{"phrase": "..."}]` object-array fallback.
pub fn parse_opensearch_suggestions(body: &str) -> Vec<String> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    if let Some(arr) = val.as_array() {
        // Standard OpenSearch tuple: ["query", ["sug1", "sug2", ...]]
        if arr.len() >= 2 {
            if let Some(items) = arr[1].as_array() {
                return items
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .take(8)
                    .collect();
            }
        }
        // DuckDuckGo fallback: [{"phrase": "..."}, ...]
        let phrases: Vec<String> = arr
            .iter()
            .filter_map(|v| {
                v.get("phrase")
                    .and_then(|p| p.as_str())
                    .map(|s| s.trim().to_string())
            })
            .filter(|s| !s.is_empty())
            .take(8)
            .collect();
        if !phrases.is_empty() {
            return phrases;
        }
    }
    Vec::new()
}

/// Returns cached live search suggestions for `(engine, query)` immediately (zero network),
/// falling back to prefix-filtered suggestions from shorter cached queries so typing never flickers.
pub fn cached_search_suggestions(engine: &str, query: &str) -> Vec<String> {
    let eng = engine.trim().to_ascii_lowercase();
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let cache = SUGGEST_CACHE.lock();
    if let Some(exact) = cache.get(&(eng.clone(), q.clone())) {
        return exact.clone();
    }
    // Check shorter prefixes (longest first) for instant non-flickering fallback
    let mut best: Vec<String> = Vec::new();
    let mut best_len = 0usize;
    for ((ce, cq), list) in cache.iter() {
        if ce == &eng && q.starts_with(cq.as_str()) && cq.len() > best_len {
            let filtered: Vec<String> = list
                .iter()
                .filter(|s| s.to_ascii_lowercase().starts_with(&q))
                .cloned()
                .collect();
            if !filtered.is_empty() {
                best = filtered;
                best_len = cq.len();
            }
        }
    }
    best
}

/// Fetches live search suggestions from the active search engine's OpenSearch endpoint
/// (with a short 1.8s timeout and in-memory cache).
pub fn fetch_search_suggestions(engine: &str, query: &str) -> Vec<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() || looks_like_url(trimmed) {
        return Vec::new();
    }
    let eng = engine.trim().to_ascii_lowercase();
    let key = (eng.clone(), trimmed.to_ascii_lowercase());
    if let Some(hit) = SUGGEST_CACHE.lock().get(&key).cloned() {
        return hit;
    }

    let url = opensearch_url(&eng, trimmed);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(1200))
        .timeout_read(Duration::from_millis(1600))
        .timeout(Duration::from_millis(1800))
        .build();

    let results = match agent
        .get(&url)
        .set("User-Agent", "Mozilla/5.0 Rover/2.0")
        .set("Accept", "application/json, text/javascript, */*")
        .call()
    {
        Ok(resp) => resp
            .into_string()
            .map(|body| parse_opensearch_suggestions(&body))
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };

    if !results.is_empty() {
        let mut cache = SUGGEST_CACHE.lock();
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(key, results.clone());
    }
    results
}

fn clean_host_path(raw_url: &str) -> Option<String> {
    let u = Url::parse(raw_url).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    let host = u.host_str()?.to_ascii_lowercase();
    let bare = host.strip_prefix("www.").unwrap_or(&host);
    let path = u.path().trim_end_matches('/');
    if path.is_empty() {
        Some(bare.to_string())
    } else {
        Some(format!("{bare}{path}"))
    }
}

/// Computes inline suffix completion for `typed` against visited history and bookmarks.
pub fn inline_completion(store: &Store, typed: &str) -> Option<String> {
    if !store.get_bool("autocomplete", true) {
        return None;
    }
    let t = typed.trim_start();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return None;
    }
    let s1 = t
        .strip_prefix("https://")
        .or_else(|| t.strip_prefix("http://"))
        .unwrap_or(t);
    let low = s1.strip_prefix("www.").unwrap_or(s1).to_ascii_lowercase();
    if low.is_empty() {
        return None;
    }

    // Check internal Rover pages first
    for internal in ["rover:playground", "rover:start", "pelagic:playground"] {
        if internal.starts_with(&low) && internal.len() > low.len() {
            return Some(format!("{t}{}", &internal[low.len()..]));
        }
    }

    // Check most visited history, then bookmarks
    for h in store.most_visited(200) {
        if let Some(hp) = clean_host_path(&h.url) {
            let candidate = if !low.contains('/') {
                hp.split('/').next().unwrap_or(&hp)
            } else {
                hp.as_str()
            };
            if candidate.starts_with(&low) && candidate.len() > low.len() {
                return Some(format!("{t}{}", &candidate[low.len()..]));
            }
        }
    }
    for b in store.bookmarks() {
        if let Some(hp) = clean_host_path(&b.url) {
            let candidate = if !low.contains('/') {
                hp.split('/').next().unwrap_or(&hp)
            } else {
                hp.as_str()
            };
            if candidate.starts_with(&low) && candidate.len() > low.len() {
                return Some(format!("{t}{}", &candidate[low.len()..]));
            }
        }
    }
    None
}

fn collect_local_site_matches(
    store: &Store,
    typed: &str,
    limit: usize,
    seen_urls: &mut HashSet<String>,
) -> Vec<OmniboxSuggestion> {
    let low = typed.to_ascii_lowercase();
    let mut prefix_hits = Vec::new();
    let mut other_hits = Vec::new();

    for b in store.bookmarks() {
        let title_low = b.title.to_ascii_lowercase();
        let url_low = b.url.to_ascii_lowercase();
        if title_low.contains(&low) || url_low.contains(&low) {
            if seen_urls.insert(b.url.clone()) {
                let hp = clean_host_path(&b.url).unwrap_or_else(|| b.url.clone());
                let is_prefix = hp.starts_with(&low) || title_low.starts_with(&low);
                let item = OmniboxSuggestion {
                    kind: "bookmark".into(),
                    title: if b.title.is_empty() { hp.clone() } else { b.title },
                    fill_text: b.url.clone(),
                    url: b.url,
                };
                if is_prefix {
                    prefix_hits.push(item);
                } else {
                    other_hits.push(item);
                }
            }
        }
    }

    for h in store.history(typed, 40) {
        if seen_urls.insert(h.url.clone()) {
            let hp = clean_host_path(&h.url).unwrap_or_else(|| h.url.clone());
            let title_low = h.title.to_ascii_lowercase();
            let is_prefix = hp.starts_with(&low) || title_low.starts_with(&low);
            let item = OmniboxSuggestion {
                kind: "history".into(),
                title: if h.title.is_empty() { hp.clone() } else { h.title },
                fill_text: h.url.clone(),
                url: h.url,
            };
            if is_prefix {
                prefix_hits.push(item);
            } else {
                other_hits.push(item);
            }
        }
    }

    prefix_hits.extend(other_hits);
    prefix_hits.truncate(limit);
    prefix_hits
}

/// Builds the top address bar dropdown suggestion rows for `typed`.
pub fn suggest(store: &Store, typed: &str, search_suggestions: &[String]) -> Vec<OmniboxSuggestion> {
    let t = typed.trim();
    if t.is_empty() {
        return Vec::new();
    }
    let engine = store
        .get_str("search_engine")
        .unwrap_or_else(|| "duckduckgo".into());
    let eng_label = search_engine_name(&engine);
    let mut rows = Vec::new();

    if looks_like_url(t) {
        let url = if t.contains(':') {
            t.to_string()
        } else {
            format!("https://{t}")
        };
        rows.push(OmniboxSuggestion {
            kind: "visit".into(),
            title: format!("Visit {t}"),
            fill_text: t.to_string(),
            url,
        });
    } else {
        rows.push(OmniboxSuggestion {
            kind: "search".into(),
            title: format!("{t}  —  Search {eng_label}"),
            fill_text: t.to_string(),
            url: search_url(&engine, t),
        });
    }

    if !store.get_bool("autocomplete", true) {
        return rows;
    }

    let low = t.to_ascii_lowercase();
    let mut seen_urls = HashSet::new();
    let mut seen_queries = HashSet::new();
    seen_queries.insert(low.clone());
    if let Some(first) = rows.first() {
        seen_urls.insert(first.url.clone());
    }

    // Internal pages matching prefix
    for (addr, label) in [
        ("rover:playground", "Rover Playground — 21 AI Agent Challenges"),
        ("rover:start", "Rover New Tab"),
    ] {
        if addr.starts_with(&low) && addr != low && seen_urls.insert(addr.to_string()) {
            rows.push(OmniboxSuggestion {
                kind: "internal".into(),
                title: label.into(),
                fill_text: addr.into(),
                url: addr.into(),
            });
        }
    }

    // Matching bookmarks & history (up to 5, prefix matches ranked first)
    let local = collect_local_site_matches(store, t, 5, &mut seen_urls);
    rows.extend(local);

    // Search engine live suggestions (up to 5)
    let mut added_searches = 0usize;
    for s in search_suggestions {
        if added_searches >= 5 {
            break;
        }
        let clean = s.trim();
        let sl = clean.to_ascii_lowercase();
        if clean.is_empty() || !seen_queries.insert(sl) {
            continue;
        }
        let u = search_url(&engine, clean);
        if seen_urls.insert(u.clone()) {
            rows.push(OmniboxSuggestion {
                kind: "search".into(),
                title: clean.to_string(),
                fill_text: clean.to_string(),
                url: u,
            });
            added_searches += 1;
        }
    }

    rows
}

/// Builds autocomplete rows specifically for the New Tab homepage search bar:
/// prioritizes live search engine suggestions first, plus matching bookmarks/history.
pub fn suggest_for_search_bar(
    store: &Store,
    typed: &str,
    search_suggestions: &[String],
) -> Vec<OmniboxSuggestion> {
    let t = typed.trim();
    if t.is_empty() || !store.get_bool("autocomplete", true) {
        return Vec::new();
    }
    let engine = store
        .get_str("search_engine")
        .unwrap_or_else(|| "duckduckgo".into());
    let mut rows = Vec::new();
    let mut seen_urls = HashSet::new();
    let mut seen_queries = HashSet::new();

    if looks_like_url(t) {
        let url = if t.contains(':') {
            t.to_string()
        } else {
            format!("https://{t}")
        };
        seen_urls.insert(url.clone());
        rows.push(OmniboxSuggestion {
            kind: "visit".into(),
            title: format!("Visit {t}"),
            fill_text: t.to_string(),
            url,
        });
    }

    for s in search_suggestions.iter().take(6) {
        let clean = s.trim();
        if clean.is_empty() || !seen_queries.insert(clean.to_ascii_lowercase()) {
            continue;
        }
        let u = search_url(&engine, clean);
        if seen_urls.insert(u.clone()) {
            rows.push(OmniboxSuggestion {
                kind: "search".into(),
                title: clean.to_string(),
                fill_text: clean.to_string(),
                url: u,
            });
        }
    }

    let local = collect_local_site_matches(store, t, 4, &mut seen_urls);
    rows.extend(local);

    rows
}
