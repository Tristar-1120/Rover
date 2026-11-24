//! Address bar Omnibox autocomplete (`inline_completion`, history/bookmark suggestions, search engines).

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use rover_core::Store;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmniboxSuggestion {
    pub kind: String, // "visit", "search", "bookmark", "history", "internal"
    pub title: String,
    pub url: String,
}

pub fn search_url(engine: &str, query: &str) -> String {
    let q = utf8_percent_encode(query.trim(), NON_ALPHANUMERIC).to_string();
    match engine.trim().to_ascii_lowercase().as_str() {
        "duckduckgo" | "ddg" => format!("https://duckduckgo.com/?q={q}"),
        "bing" => format!("https://www.bing.com/search?q={q}"),
        "brave" => format!("https://search.brave.com/search?q={q}"),
        "ecosia" => format!("https://www.ecosia.org/search?q={q}"),
        _ => format!("https://www.google.com/search?q={q}"),
    }
}

pub fn opensearch_url(engine: &str, query: &str) -> String {
    let q = utf8_percent_encode(query.trim(), NON_ALPHANUMERIC).to_string();
    match engine.trim().to_ascii_lowercase().as_str() {
        "duckduckgo" | "ddg" => format!("https://duckduckgo.com/ac/?q={q}&type=list"),
        "bing" => format!("https://api.bing.com/osjson.aspx?query={q}"),
        "brave" => format!("https://search.brave.com/api/suggest?q={q}"),
        "ecosia" => format!("https://ac.ecosia.org/autocomplete?q={q}&type=list"),
        _ => format!("https://suggestqueries.google.com/complete/search?client=firefox&q={q}"),
    }
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

/// Builds the dropdown suggestion rows for `typed`.
pub fn suggest(store: &Store, typed: &str, search_suggestions: &[String]) -> Vec<OmniboxSuggestion> {
    let t = typed.trim();
    if t.is_empty() {
        return Vec::new();
    }
    let engine = store
        .get_str("search_engine")
        .unwrap_or_else(|| "google".into());
    let mut rows = Vec::new();

    let looks_like_url = t.starts_with("http://")
        || t.starts_with("https://")
        || t.starts_with("rover:")
        || t.starts_with("pelagic:")
        || (!t.contains(char::is_whitespace) && t.contains('.'));

    if looks_like_url {
        let url = if t.contains(':') {
            t.to_string()
        } else {
            format!("https://{t}")
        };
        rows.push(OmniboxSuggestion {
            kind: "visit".into(),
            title: format!("Visit {t}"),
            url,
        });
    } else {
        rows.push(OmniboxSuggestion {
            kind: "search".into(),
            title: format!("Search for “{t}”"),
            url: search_url(&engine, t),
        });
    }

    if !store.get_bool("autocomplete", true) {
        return rows;
    }

    let low = t.to_ascii_lowercase();
    let mut seen_urls = std::collections::HashSet::new();
    if let Some(first) = rows.first() {
        seen_urls.insert(first.url.clone());
    }

    // Matching bookmarks
    for b in store.bookmarks() {
        if rows.len() >= 6 {
            break;
        }
        if (b.title.to_ascii_lowercase().contains(&low)
            || b.url.to_ascii_lowercase().contains(&low))
            && seen_urls.insert(b.url.clone())
        {
            rows.push(OmniboxSuggestion {
                kind: "bookmark".into(),
                title: b.title,
                url: b.url,
            });
        }
    }

    // Matching history
    for h in store.history(t, 10) {
        if rows.len() >= 6 {
            break;
        }
        if seen_urls.insert(h.url.clone()) {
            rows.push(OmniboxSuggestion {
                kind: "history".into(),
                title: if h.title.is_empty() {
                    h.url.clone()
                } else {
                    h.title
                },
                url: h.url,
            });
        }
    }

    // Search engine live suggestions
    for s in search_suggestions.iter().take(5) {
        let u = search_url(&engine, s);
        if seen_urls.insert(u.clone()) {
            rows.push(OmniboxSuggestion {
                kind: "search".into(),
                title: s.clone(),
                url: u,
            });
        }
    }

    rows
}
