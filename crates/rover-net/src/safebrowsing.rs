//! Local Threat Intelligence & Safe Browsing Guard (`SafeBrowsingGuard`).
//!
//! Checks Phishing Army, OpenPhish, and URLhaus threat lists locally on the user's machine
//! without sending visited URLs to any server. Protects shared hosting domains (`github.io`,
//! `vercel.app`, `pages.dev`, `netlify.app`, etc.) from blanket host blocks while enforcing
//! exact URL-prefix matching on them.

use crate::adblock::host_parents;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rover_core::Store;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use url::Url;

static SHARED_HOSTS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "github.io",
        "gitlab.io",
        "pages.dev",
        "vercel.app",
        "netlify.app",
        "web.app",
        "firebaseapp.com",
        "appspot.com",
        "herokuapp.com",
        "googleusercontent.com",
        "google.com",
        "docs.google.com",
        "drive.google.com",
        "sites.google.com",
        "dropbox.com",
        "onedrive.live.com",
        "amazonaws.com",
        "s3.amazonaws.com",
        "blob.core.windows.net",
        "workers.dev",
        "render.com",
        "onrender.com",
    ]
    .into_iter()
    .collect()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatKind {
    Phishing,
    Malware,
}

impl ThreatKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Phishing => "phishing",
            Self::Malware => "malware",
        }
    }
}

#[derive(Clone)]
pub struct SafeBrowsingGuard {
    pub store: Store,
    hosts: Arc<RwLock<HashMap<String, ThreatKind>>>,
    url_prefixes: Arc<RwLock<HashMap<String, Vec<(String, ThreatKind)>>>>,
    session_allowed: Arc<RwLock<HashSet<String>>>,
}

impl SafeBrowsingGuard {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            hosts: Arc::new(RwLock::new(HashMap::new())),
            url_prefixes: Arc::new(RwLock::new(HashMap::new())),
            session_allowed: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.store.get_bool("safebrowsing", true)
    }

    pub fn add_host_list(&self, list_text: &str, kind: ThreatKind) {
        let mut hosts = self.hosts.write();
        for raw_line in list_text.lines() {
            let line = raw_line
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if line.is_empty() {
                continue;
            }
            let host = if line.starts_with("0.0.0.0") || line.starts_with("127.0.0.1") {
                line.split_whitespace().nth(1).unwrap_or("")
            } else {
                line.as_str()
            };
            let bare = host.strip_prefix("www.").unwrap_or(host);
            if !bare.is_empty()
                && bare.contains('.')
                && !SHARED_HOSTS.contains(bare)
                && !host_parents(bare)
                    .into_iter()
                    .any(|p| SHARED_HOSTS.contains(p))
            {
                hosts.insert(bare.to_string(), kind);
            }
        }
    }

    pub fn add_url_list(&self, list_text: &str, kind: ThreatKind) {
        let mut urls = self.url_prefixes.write();
        let mut hosts = self.hosts.write();
        for raw_line in list_text.lines() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Ok(u) = Url::parse(line) else {
                continue;
            };
            let Some(host_str) = u.host_str() else {
                continue;
            };
            let bare = host_str
                .to_ascii_lowercase()
                .strip_prefix("www.")
                .unwrap_or(&host_str.to_ascii_lowercase())
                .to_string();
            let norm = normalize_threat_url(&u);
            urls.entry(bare.clone()).or_default().push((norm, kind));

            let is_shared = host_parents(&bare)
                .into_iter()
                .any(|p| SHARED_HOSTS.contains(p));
            if !is_shared && (u.path() == "/" || u.path().is_empty()) && u.query().is_none() {
                hosts.entry(bare).or_insert(kind);
            }
        }
    }

    pub fn allow_for_session(&self, url_or_host: &str) {
        let host = Url::parse(url_or_host)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_ascii_lowercase()))
            .unwrap_or_else(|| url_or_host.trim().to_ascii_lowercase());
        let bare = host.strip_prefix("www.").unwrap_or(&host).to_string();
        if !bare.is_empty() {
            self.session_allowed.write().insert(bare);
        }
    }

    pub fn check(&self, target_url: &str) -> Option<ThreatKind> {
        if !self.is_enabled() {
            return None;
        }
        let Ok(u) = Url::parse(target_url) else {
            return None;
        };
        if !matches!(u.scheme(), "http" | "https") {
            return None;
        }
        let host = u.host_str()?.to_ascii_lowercase();
        let bare = host.strip_prefix("www.").unwrap_or(&host);
        if self.session_allowed.read().contains(bare) {
            return None;
        }
        let norm = normalize_threat_url(&u);
        if let Some(prefixes) = self.url_prefixes.read().get(bare) {
            for (prefix, kind) in prefixes {
                if norm.starts_with(prefix) {
                    return Some(*kind);
                }
            }
        }
        let hosts = self.hosts.read();
        for p in host_parents(bare) {
            if let Some(&kind) = hosts.get(p) {
                return Some(kind);
            }
        }
        None
    }

    pub fn render_warning_page(&self, url: &str, kind: ThreatKind) -> String {
        let title = match kind {
            ThreatKind::Phishing => "Deceptive site ahead",
            ThreatKind::Malware => "Dangerous malware site ahead",
        };
        format!(
            r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{title} — Rover Security</title>
<style>
  body {{ background:#450a0a; color:#fef2f2; font-family:system-ui,-apple-system,sans-serif; display:flex; align-items:center; justify-content:center; height:100vh; margin:0; }}
  .card {{ background:#7f1d1d; border:1px solid #ef4444; border-radius:14px; max-width:560px; padding:32px; box-shadow:0 20px 50px rgba(0,0,0,.6); }}
  h1 {{ margin:0 0 12px; font-size:24px; color:#fecaca; }}
  p {{ line-height:1.55; margin:0 0 18px; color:#fee2e2; word-break:break-all; }}
  .actions {{ display:flex; gap:12px; align-items:center; }}
  a.safe {{ background:#fef2f2; color:#7f1d1d; text-decoration:none; padding:10px 18px; border-radius:8px; font-weight:700; }}
  a.proceed {{ color:#fca5a5; font-size:13px; }}
</style>
</head>
<body>
  <div class="card">
    <h1>{title}</h1>
    <p>Rover blocked <strong>{url}</strong> because it appears on public {kind_str} threat intelligence lists.</p>
    <div class="actions">
      <a class="safe" href="?rover=unsafe-back">Back to safety</a>
      <a class="proceed" href="?rover=unsafe-continue&amp;url={url}">Visit this unsafe site anyway (for this session)</a>
    </div>
  </div>
</body>
</html>"#,
            kind_str = kind.as_str()
        )
    }
}

fn normalize_threat_url(u: &Url) -> String {
    let host = u
        .host_str()
        .unwrap_or("")
        .to_ascii_lowercase()
        .strip_prefix("www.")
        .unwrap_or(&u.host_str().unwrap_or("").to_ascii_lowercase())
        .to_string();
    let path = u.path().trim_end_matches('/');
    match u.query() {
        Some(q) => format!("{host}{path}?{q}"),
        None => format!("{host}{path}"),
    }
}
