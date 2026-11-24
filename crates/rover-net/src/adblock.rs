//! High-speed Ad & Tracker Filter Engine (`FilterEngine`, `Blocker`).
//!
//! Replaces Pelagic's Python per-rule `re.search` loop with:
//! 1. O(1) Domain Suffix Hash Lookups for pure `||domain^` rules.
//! 2. An `AhoCorasick` multi-pattern automaton over rule tokens so only rules whose token
//!    actually appears in the URL are evaluated.
//! 3. Binary cache serialization (`bincode`) for instant startup.

use aho_corasick::AhoCorasick;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use regex::Regex;
use rover_core::Store;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use url::Url;

static TOKEN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[a-z0-9_-]{3,}").unwrap());
static PURE_DOMAIN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\|\|([a-z0-9.-]+\.[a-z]{2,})\^?$").unwrap());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    MainFrame,
    SubFrame,
    Stylesheet,
    Script,
    Image,
    Font,
    Media,
    Xhr,
    Ping,
    Other,
}

impl ResourceKind {
    pub fn from_option(opt: &str) -> Option<Self> {
        match opt {
            "script" => Some(Self::Script),
            "image" | "imageset" => Some(Self::Image),
            "stylesheet" | "css" => Some(Self::Stylesheet),
            "xmlhttprequest" | "xhr" | "fetch" => Some(Self::Xhr),
            "subdocument" | "frame" => Some(Self::SubFrame),
            "font" => Some(Self::Font),
            "media" => Some(Self::Media),
            "ping" | "beacon" => Some(Self::Ping),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledRule {
    pub literal: Option<String>,
    pub pattern_regex: String,
    pub types: HashSet<ResourceKind>,
    pub exclude_types: HashSet<ResourceKind>,
    pub third_party: Option<bool>,
    pub include_domains: HashSet<String>,
    pub exclude_domains: HashSet<String>,
}

impl CompiledRule {
    pub fn is_unconditional(&self) -> bool {
        self.types.is_empty()
            && self.exclude_types.is_empty()
            && self.third_party.is_none()
            && self.include_domains.is_empty()
            && self.exclude_domains.is_empty()
    }

    pub fn matches_context(
        &self,
        kind: ResourceKind,
        is_third_party: bool,
        page_host: &str,
    ) -> bool {
        if let Some(tp) = self.third_party {
            if tp != is_third_party {
                return false;
            }
        }
        if !self.types.is_empty() && !self.types.contains(&kind) {
            return false;
        }
        if self.exclude_types.contains(&kind) {
            return false;
        }
        if !self.exclude_domains.is_empty()
            && host_parents(page_host)
                .into_iter()
                .any(|p| self.exclude_domains.contains(p))
        {
            return false;
        }
        if !self.include_domains.is_empty()
            && !host_parents(page_host)
                .into_iter()
                .any(|p| self.include_domains.contains(p))
        {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterSnapshot {
    pub block_uncond_domains: HashSet<String>,
    pub block_cond_domains: HashMap<String, Vec<CompiledRule>>,
    pub allow_uncond_domains: HashSet<String>,
    pub allow_cond_domains: HashMap<String, Vec<CompiledRule>>,
    pub block_tokens: Vec<String>,
    pub block_token_rules: Vec<Vec<CompiledRule>>,
    pub block_any: Vec<CompiledRule>,
    pub allow_tokens: Vec<String>,
    pub allow_token_rules: Vec<Vec<CompiledRule>>,
    pub allow_any: Vec<CompiledRule>,
    pub cosmetic: HashMap<String, Vec<String>>,
    pub cosmetic_except: HashMap<String, HashSet<String>>,
}

pub struct FilterEngine {
    snap: FilterSnapshot,
    block_ac: Option<AhoCorasick>,
    allow_ac: Option<AhoCorasick>,
    regex_cache: RwLock<HashMap<String, Regex>>,
}

pub fn host_parents(host: &str) -> Vec<&str> {
    let h = host.trim().trim_end_matches('.');
    if h.is_empty() {
        return Vec::new();
    }
    let mut out = vec![h];
    for (idx, ch) in h.char_indices() {
        if ch == '.' && idx + 1 < h.len() {
            out.push(&h[idx + 1..]);
        }
    }
    out
}

pub fn registrable_domain(host: &str) -> String {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let parts: Vec<&str> = h.split('.').collect();
    if parts.len() <= 2 {
        return h;
    }
    let last2 = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
    if matches!(
        last2.as_str(),
        "co.uk" | "org.uk" | "ac.uk" | "com.au" | "co.jp" | "co.nz" | "com.br"
    ) && parts.len() >= 3
    {
        return format!("{}.{}", parts[parts.len() - 3], last2);
    }
    last2
}

pub fn is_third_party_hosts(req_host: &str, page_host: &str) -> bool {
    if req_host.is_empty() || page_host.is_empty() {
        return false;
    }
    registrable_domain(req_host) != registrable_domain(page_host)
}

fn pattern_to_regex_str(pat: &str) -> Option<String> {
    let p = pat.trim();
    if p.is_empty() {
        return None;
    }
    if p.starts_with('/') && p.ends_with('/') && p.len() > 2 {
        return Some(p[1..p.len() - 1].to_string());
    }
    let mut out = String::from("(?i)");
    let mut slice = p;
    if let Some(rest) = slice.strip_prefix("||") {
        out.push_str(r"^https?://(?:[a-z0-9-]+\.)*");
        slice = rest;
    } else if let Some(rest) = slice.strip_prefix('|') {
        out.push('^');
        slice = rest;
    }
    let anchor_end = if let Some(rest) = slice.strip_suffix('|') {
        slice = rest;
        true
    } else {
        false
    };
    for ch in slice.chars() {
        match ch {
            '*' => out.push_str(".*"),
            '^' => out.push_str(r"(?:[/?#&:;]|$)"),
            '.' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '\\' | '$' | '|' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    if anchor_end {
        out.push('$');
    }
    Some(out)
}

impl Default for FilterEngine {
    fn default() -> Self {
        Self::from_snapshot(FilterSnapshot::default())
    }
}

impl FilterEngine {
    pub fn from_snapshot(snap: FilterSnapshot) -> Self {
        let block_ac = if snap.block_tokens.is_empty() {
            None
        } else {
            AhoCorasick::new(&snap.block_tokens).ok()
        };
        let allow_ac = if snap.allow_tokens.is_empty() {
            None
        } else {
            AhoCorasick::new(&snap.allow_tokens).ok()
        };
        Self {
            snap,
            block_ac,
            allow_ac,
            regex_cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn from_lists(lists: &[&str]) -> Self {
        let mut snap = FilterSnapshot::default();
        let mut block_tok_map: HashMap<String, usize> = HashMap::new();
        let mut allow_tok_map: HashMap<String, usize> = HashMap::new();

        for list in lists {
            for raw_line in list.lines() {
                Self::parse_line(
                    raw_line,
                    &mut snap,
                    &mut block_tok_map,
                    &mut allow_tok_map,
                );
            }
        }
        Self::from_snapshot(snap)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(&self.snap).unwrap_or_default()
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let snap: FilterSnapshot = bincode::deserialize(bytes).ok()?;
        Some(Self::from_snapshot(snap))
    }

    fn parse_line(
        raw_line: &str,
        snap: &mut FilterSnapshot,
        block_tok_map: &mut HashMap<String, usize>,
        allow_tok_map: &mut HashMap<String, usize>,
    ) {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('!') || line.starts_with('[') {
            return;
        }
        // Cosmetic rules: host##selector or host#@#selector
        if let Some((domains_str, selector)) = line.split_once("#@#") {
            let sel = selector.trim();
            if sel.is_empty() {
                return;
            }
            for d in domains_str.split(',') {
                let d = d.trim().to_ascii_lowercase();
                if !d.is_empty() && !d.starts_with('~') {
                    snap.cosmetic_except
                        .entry(d)
                        .or_default()
                        .insert(sel.to_string());
                }
            }
            return;
        }
        if let Some((domains_str, selector)) = line.split_once("##") {
            if line.contains("#?#") || line.contains("#$#") || selector.starts_with("+js") {
                return;
            }
            let sel = selector.trim();
            if sel.is_empty() || sel.contains(":has-text") || sel.contains(":-abp-") {
                return;
            }
            for d in domains_str.split(',') {
                let d = d.trim().to_ascii_lowercase();
                if !d.is_empty() && !d.starts_with('~') && !d.ends_with('*') {
                    snap.cosmetic.entry(d).or_default().push(sel.to_string());
                }
            }
            return;
        }
        if line.contains('#') {
            return;
        }

        let (is_allow, body) = if let Some(rest) = line.strip_prefix("@@") {
            (true, rest)
        } else {
            (false, line)
        };

        let (pat_part, opt_part) = match body.rfind('$') {
            Some(idx) if !(body[..idx].starts_with('/') && body[..idx].ends_with('/')) => {
                (&body[..idx], Some(&body[idx + 1..]))
            }
            _ => (body, None),
        };

        let mut types = HashSet::new();
        let mut exclude_types = HashSet::new();
        let mut third_party = None;
        let mut include_domains = HashSet::new();
        let mut exclude_domains = HashSet::new();

        if let Some(opts) = opt_part {
            for raw_opt in opts.split(',') {
                let opt = raw_opt.trim().to_ascii_lowercase();
                if opt.is_empty() {
                    continue;
                }
                if matches!(
                    opt.as_str(),
                    "csp"
                        | "rewrite"
                        | "redirect"
                        | "redirect-rule"
                        | "removeparam"
                        | "popup"
                        | "Badfilter"
                        | "badfilter"
                ) {
                    return;
                }
                if opt == "third-party" || opt == "3p" {
                    third_party = Some(true);
                } else if opt == "~third-party" || opt == "1p" || opt == "first-party" {
                    third_party = Some(false);
                } else if let Some(doms) = opt.strip_prefix("domain=") {
                    for d in doms.split('|') {
                        let d = d.trim();
                        if let Some(ex) = d.strip_prefix('~') {
                            if !ex.is_empty() {
                                exclude_domains.insert(ex.to_string());
                            }
                        } else if !d.is_empty() {
                            include_domains.insert(d.to_string());
                        }
                    }
                } else if let Some(neg_opt) = opt.strip_prefix('~') {
                    if let Some(rk) = ResourceKind::from_option(neg_opt) {
                        exclude_types.insert(rk);
                    }
                } else if let Some(rk) = ResourceKind::from_option(&opt) {
                    types.insert(rk);
                }
            }
        }

        let pat_low = pat_part.trim().to_ascii_lowercase();
        let is_plain_literal = !pat_low.is_empty()
            && !pat_low.contains('*')
            && !pat_low.contains('^')
            && !pat_low.contains('|')
            && !(pat_low.starts_with('/') && pat_low.ends_with('/') && pat_low.len() > 2);
        let literal = if is_plain_literal {
            Some(pat_low.clone())
        } else {
            None
        };
        let Some(regex_str) = pattern_to_regex_str(pat_part) else {
            return;
        };
        let rule = CompiledRule {
            literal,
            pattern_regex: regex_str,
            types,
            exclude_types,
            third_party,
            include_domains,
            exclude_domains,
        };

        // Pure ||domain^ shortcut
        if let Some(caps) = PURE_DOMAIN_RE.captures(&pat_low) {
            let dom = caps[1].to_string();
            if is_allow {
                if rule.is_unconditional() {
                    snap.allow_uncond_domains.insert(dom);
                } else {
                    snap.allow_cond_domains.entry(dom).or_default().push(rule);
                }
            } else if rule.is_unconditional() {
                snap.block_uncond_domains.insert(dom);
            } else {
                snap.block_cond_domains.entry(dom).or_default().push(rule);
            }
            return;
        }

        // Longest 3+ char alphanumeric token for Aho-Corasick prefiltering
        let best_tok = TOKEN_RE
            .find_iter(&pat_low)
            .map(|m| m.as_str())
            .filter(|t| !matches!(*t, "http" | "https" | "www" | "com" | "net" | "org"))
            .max_by_key(|t| t.len());

        if is_allow {
            if let Some(tok) = best_tok {
                let idx = *allow_tok_map.entry(tok.to_string()).or_insert_with(|| {
                    snap.allow_tokens.push(tok.to_string());
                    snap.allow_token_rules.push(Vec::new());
                    snap.allow_tokens.len() - 1
                });
                snap.allow_token_rules[idx].push(rule);
            } else {
                snap.allow_any.push(rule);
            }
        } else if let Some(tok) = best_tok {
            let idx = *block_tok_map.entry(tok.to_string()).or_insert_with(|| {
                snap.block_tokens.push(tok.to_string());
                snap.block_token_rules.push(Vec::new());
                snap.block_tokens.len() - 1
            });
            snap.block_token_rules[idx].push(rule);
        } else {
            snap.block_any.push(rule);
        }
    }

    fn rule_matches_url(&self, rule: &CompiledRule, url_low: &str) -> bool {
        if let Some(ref lit) = rule.literal {
            return url_low.contains(lit.as_str());
        }
        if let Some(re) = self.regex_cache.read().get(&rule.pattern_regex) {
            return re.is_match(url_low);
        }
        if let Ok(re) = Regex::new(&rule.pattern_regex) {
            let matched = re.is_match(url_low);
            self.regex_cache
                .write()
                .insert(rule.pattern_regex.clone(), re);
            return matched;
        }
        false
    }

    pub fn should_block(&self, req_url: &str, page_url: &str, kind: ResourceKind) -> bool {
        if kind == ResourceKind::MainFrame {
            return false;
        }
        let Ok(u) = Url::parse(req_url) else {
            return false;
        };
        if !matches!(u.scheme(), "http" | "https") {
            return false;
        }
        let req_host = u.host_str().unwrap_or("").to_ascii_lowercase();
        let page_host = Url::parse(page_url)
            .ok()
            .and_then(|p| p.host_str().map(|s| s.to_ascii_lowercase()))
            .unwrap_or_default();
        let is_3p = is_third_party_hosts(&req_host, &page_host);
        let url_low = req_url.to_ascii_lowercase();

        if !self.matches_set(
            &url_low,
            &req_host,
            &page_host,
            kind,
            is_3p,
            &self.snap.block_uncond_domains,
            &self.snap.block_cond_domains,
            &self.block_ac,
            &self.snap.block_token_rules,
            &self.snap.block_any,
        ) {
            return false;
        }

        !self.matches_set(
            &url_low,
            &req_host,
            &page_host,
            kind,
            is_3p,
            &self.snap.allow_uncond_domains,
            &self.snap.allow_cond_domains,
            &self.allow_ac,
            &self.snap.allow_token_rules,
            &self.snap.allow_any,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn matches_set(
        &self,
        url_low: &str,
        req_host: &str,
        page_host: &str,
        kind: ResourceKind,
        is_3p: bool,
        uncond_domains: &HashSet<String>,
        cond_domains: &HashMap<String, Vec<CompiledRule>>,
        ac: &Option<AhoCorasick>,
        token_rules: &[Vec<CompiledRule>],
        any_rules: &[CompiledRule],
    ) -> bool {
        for parent in host_parents(req_host) {
            if uncond_domains.contains(parent) {
                return true;
            }
            if let Some(rules) = cond_domains.get(parent) {
                for r in rules {
                    if r.matches_context(kind, is_3p, page_host) {
                        return true;
                    }
                }
            }
        }

        if let Some(automaton) = ac {
            let mut visited_buckets = HashSet::new();
            for mat in automaton.find_overlapping_iter(url_low) {
                let pid = mat.pattern().as_usize();
                if visited_buckets.insert(pid) {
                    for r in &token_rules[pid] {
                        if r.matches_context(kind, is_3p, page_host)
                            && self.rule_matches_url(r, url_low)
                        {
                            return true;
                        }
                    }
                }
            }
        }

        for r in any_rules {
            if r.matches_context(kind, is_3p, page_host) && self.rule_matches_url(r, url_low) {
                return true;
            }
        }
        false
    }

    pub fn hide_css(&self, page_host: &str) -> String {
        let host = page_host.to_ascii_lowercase();
        let parents = host_parents(&host);
        let mut exceptions = HashSet::new();
        for p in &parents {
            if let Some(ex) = self.snap.cosmetic_except.get(*p) {
                for s in ex {
                    exceptions.insert(s.as_str());
                }
            }
        }
        let mut selectors = Vec::new();
        for p in &parents {
            if let Some(sels) = self.snap.cosmetic.get(*p) {
                for s in sels {
                    if !exceptions.contains(s.as_str()) && !selectors.contains(&s.as_str()) {
                        selectors.push(s.as_str());
                    }
                }
            }
        }
        if selectors.is_empty() {
            String::new()
        } else {
            format!("{} {{ display: none !important; }}", selectors.join(", "))
        }
    }
}

/// Per-profile ad blocker wrapping a shared `FilterEngine` and tracking blocked counts + paused sites.
#[derive(Clone)]
pub struct Blocker {
    pub store: Store,
    pub engine: Arc<FilterEngine>,
    blocked_by_host: Arc<RwLock<HashMap<String, usize>>>,
    total_blocked: Arc<RwLock<usize>>,
}

impl Blocker {
    pub fn new(store: Store, engine: Arc<FilterEngine>) -> Self {
        Self {
            store,
            engine,
            blocked_by_host: Arc::new(RwLock::new(HashMap::new())),
            total_blocked: Arc::new(RwLock::new(0)),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.store.get_bool("adblock", true)
    }

    pub fn set_enabled(&self, on: bool) {
        self.store.set("adblock", serde_json::json!(on));
    }

    pub fn paused_sites(&self) -> Vec<String> {
        self.store
            .get("adblock_paused")
            .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
            .unwrap_or_default()
    }

    pub fn is_paused_for(&self, page_url_or_host: &str) -> bool {
        let host = Url::parse(page_url_or_host)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_ascii_lowercase()))
            .unwrap_or_else(|| page_url_or_host.trim().to_ascii_lowercase());
        if host.is_empty() {
            return false;
        }
        let paused = self.paused_sites();
        host_parents(&host)
            .into_iter()
            .any(|p| paused.iter().any(|s| s.eq_ignore_ascii_case(p)))
    }

    pub fn set_paused_for(&self, page_url_or_host: &str, paused: bool) {
        let host = Url::parse(page_url_or_host)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_ascii_lowercase()))
            .unwrap_or_else(|| page_url_or_host.trim().to_ascii_lowercase());
        if host.is_empty() {
            return;
        }
        let mut list = self.paused_sites();
        list.retain(|x| !x.eq_ignore_ascii_case(&host));
        if paused {
            list.push(host);
        }
        self.store.set("adblock_paused", serde_json::json!(list));
    }

    pub fn check_and_record(&self, req_url: &str, page_url: &str, kind: ResourceKind) -> bool {
        if !self.is_enabled() || self.is_paused_for(page_url) {
            return false;
        }
        if self.engine.should_block(req_url, page_url, kind) {
            let page_host = Url::parse(page_url)
                .ok()
                .and_then(|u| u.host_str().map(|s| s.to_ascii_lowercase()))
                .unwrap_or_default();
            *self.total_blocked.write() += 1;
            if !page_host.is_empty() {
                *self.blocked_by_host.write().entry(page_host).or_insert(0) += 1;
            }
            return true;
        }
        false
    }

    pub fn blocked_count(&self, page_url_or_host: &str) -> usize {
        let host = Url::parse(page_url_or_host)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_ascii_lowercase()))
            .unwrap_or_else(|| page_url_or_host.trim().to_ascii_lowercase());
        self.blocked_by_host
            .read()
            .get(&host)
            .copied()
            .unwrap_or(0)
    }

    pub fn total_blocked(&self) -> usize {
        *self.total_blocked.read()
    }
}
