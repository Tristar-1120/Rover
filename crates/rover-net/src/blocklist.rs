//! Pi-hole style whole-domain and subdomain blocker (`DomainBlocker`).
//!
//! Parses plain domains (`example.com`), wildcards (`*.example.com`), Adblock rules (`||example.com^`),
//! and `/etc/hosts` lines (`0.0.0.0 example.com`, `127.0.0.1 ads.example.com`) at >1M lines/sec.
//! Separate from the ad blocker so blocked domains remain blocked even when adblock is paused.

use crate::adblock::host_parents;
use parking_lot::RwLock;
use rover_core::Store;
use serde::Serialize;
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use url::Url;

#[derive(Debug, Clone, Serialize)]
pub struct PresetList {
    pub name: &'static str,
    pub url: &'static str,
    pub description: &'static str,
}

pub const BLOCKLIST_PRESETS: &[PresetList] = &[
    PresetList {
        name: "StevenBlack Hosts (Ads + Malware)",
        url: "https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts",
        description: "Unified hosts file blocking adware and malware domains",
    },
    PresetList {
        name: "OISD Small",
        url: "https://small.oisd.nl/domainswild",
        description: "Curated low-false-positive domain blocklist",
    },
    PresetList {
        name: "HaGeZi Multi Light",
        url: "https://raw.githubusercontent.com/hagezi/dns-blocklists/main/domains/light.txt",
        description: "Lightweight DNS blocklist for ads, trackers, and telemetry",
    },
];

/// Fast zero-regex domain parser for hosts files, adblock `||domain^`, wildcards `*.domain`, and plain domains.
pub fn parse_domain_line(raw: &str) -> Option<String> {
    let line = match raw.find('#') {
        Some(i) => &raw[..i],
        None => raw,
    };
    let line = match line.find('!') {
        Some(i) => &line[..i],
        None => line,
    }
    .trim();
    if line.is_empty() || line.starts_with("@@") || line.contains('$') || line.contains('/') {
        return None;
    }

    // Handle hosts file format: 0.0.0.0 domain.com or 127.0.0.1 domain.com
    let candidate = if line.starts_with("0.0.0.0")
        || line.starts_with("127.0.0.1")
        || line.starts_with("::1")
        || line.starts_with("::")
    {
        line.split_whitespace().nth(1)?
    } else {
        line
    };

    let c1 = candidate.strip_prefix("||").unwrap_or(candidate);
    let c2 = c1.strip_prefix("*.").unwrap_or(c1);
    let c3 = c2.strip_prefix('.').unwrap_or(c2);
    let candidate = c3
        .trim_end_matches('^')
        .trim_end_matches('.')
        .to_ascii_lowercase();

    if candidate.is_empty()
        || matches!(
            candidate.as_str(),
            "localhost"
                | "localhost.localdomain"
                | "local"
                | "broadcasthost"
                | "ip6-localhost"
                | "ip6-loopback"
                | "0.0.0.0"
        )
    {
        return None;
    }

    if !candidate.contains('.') {
        return None;
    }
    if candidate
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
    {
        Some(candidate)
    } else {
        None
    }
}

pub fn parse_domain_list(text: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in text.lines() {
        if let Some(d) = parse_domain_line(line) {
            set.insert(d);
        }
    }
    set
}

#[derive(Clone)]
pub struct DomainBlocker {
    pub store: Store,
    blocked: Arc<RwLock<HashSet<String>>>,
    allowed: Arc<RwLock<HashSet<String>>>,
    external_lists: Arc<RwLock<HashSet<String>>>,
}

impl DomainBlocker {
    pub fn new(store: Store) -> Self {
        let blocker = Self {
            store,
            blocked: Arc::new(RwLock::new(HashSet::new())),
            allowed: Arc::new(RwLock::new(HashSet::new())),
            external_lists: Arc::new(RwLock::new(HashSet::new())),
        };
        blocker.reload();
        blocker
    }

    pub fn reload(&self) {
        let user_blocked: Vec<String> = self
            .store
            .get("blocked_domains")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let user_allowed: Vec<String> = self
            .store
            .get("allowed_domains")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let mut bset = self.external_lists.read().clone();
        for item in user_blocked {
            if let Some(d) = parse_domain_line(&item) {
                bset.insert(d);
            }
        }
        let mut aset = HashSet::new();
        for item in user_allowed {
            if let Some(d) = parse_domain_line(&item) {
                aset.insert(d);
            }
        }
        *self.blocked.write() = bset;
        *self.allowed.write() = aset;
    }

    pub fn load_external_list_text(&self, text: &str) {
        let parsed = parse_domain_list(text);
        self.external_lists.write().extend(parsed);
        self.reload();
    }

    pub fn block_domain(&self, domain: &str) {
        let Some(d) = parse_domain_line(domain) else {
            return;
        };
        let mut list: Vec<String> = self
            .store
            .get("blocked_domains")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        if !list.iter().any(|x| x.eq_ignore_ascii_case(&d)) {
            list.push(d);
            self.store.set("blocked_domains", json!(list));
        }
        // Also remove from allowed_domains if present
        let mut allow: Vec<String> = self
            .store
            .get("allowed_domains")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let before = allow.len();
        allow.retain(|x| !x.eq_ignore_ascii_case(domain));
        if allow.len() != before {
            self.store.set("allowed_domains", json!(allow));
        }
        self.reload();
    }

    pub fn unblock_domain(&self, domain: &str) {
        let Some(d) = parse_domain_line(domain) else {
            return;
        };
        let mut list: Vec<String> = self
            .store
            .get("blocked_domains")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        list.retain(|x| !x.eq_ignore_ascii_case(&d));
        self.store.set("blocked_domains", json!(list));

        let mut allow: Vec<String> = self
            .store
            .get("allowed_domains")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        if !allow.iter().any(|x| x.eq_ignore_ascii_case(&d)) {
            allow.push(d);
            self.store.set("allowed_domains", json!(allow));
        }
        self.reload();
    }

    /// Returns the matched blocked root domain if `url_or_host` is blocked (and not in `allowed_domains`).
    pub fn match_url_or_host(&self, url_or_host: &str) -> Option<String> {
        let host = Url::parse(url_or_host)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_ascii_lowercase()))
            .unwrap_or_else(|| url_or_host.trim().to_ascii_lowercase());
        if host.is_empty() {
            return None;
        }
        let parents = host_parents(&host);
        let allowed = self.allowed.read();
        for p in &parents {
            if allowed.contains(*p) {
                return None;
            }
        }
        let blocked = self.blocked.read();
        for p in &parents {
            if blocked.contains(*p) {
                return Some((*p).to_string());
            }
        }
        None
    }

    pub fn render_blocked_page(&self, blocked_url: &str, matched_rule: &str) -> String {
        format!(
            r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Site blocked — Rover</title>
<style>
  body {{ background:#160708; color:#FEF0E4; font-family:system-ui,-apple-system,sans-serif; display:flex; align-items:center; justify-content:center; height:100vh; margin:0; }}
  .card {{ background:#220B0D; border:1px solid #531E21; border-radius:14px; max-width:520px; padding:32px; box-shadow:0 16px 40px rgba(0,0,0,.55); }}
  h1 {{ margin:0 0 12px; font-size:22px; color:#EF7560; }}
  p {{ line-height:1.55; color:#D9A798; margin:0 0 20px; word-break:break-all; }}
  code {{ background:#160708; color:#FED8B1; padding:2px 6px; border-radius:5px; }}
  .actions {{ display:flex; gap:12px; }}
  a.btn {{ text-decoration:none; padding:9px 16px; border-radius:8px; font-weight:600; font-size:14px; }}
  a.primary {{ background:#C83F3A; color:#FEF0E4; }}
  a.secondary {{ background:#341215; color:#FED8B1; border:1px solid #6B2629; }}
</style>
</head>
<body>
  <div class="card">
    <h1>This site is blocked</h1>
    <p>Rover blocked <code>{blocked_url}</code> because <code>{matched_rule}</code> is in your blocked domains list.</p>
    <div class="actions">
      <a class="btn primary" href="?rover=blocked-back">Go back</a>
      <a class="btn secondary" href="?rover=blocked-unblock&amp;domain={matched_rule}&amp;url={blocked_url}">Unblock {matched_rule}</a>
    </div>
  </div>
</body>
</html>"#
        )
    }
}
