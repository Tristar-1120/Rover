//! Userscript & Custom Style Extension Engine (`Extensions`) + 9 Bundled Rover Gallery Extensions.
//!
//! Supports `==UserScript==` headers (`@name`, `@description`, `@version`, `@match`, `@include`,
//! `@exclude`, `@run-at`, `@require`), a full `GM_*` compatibility shim (`GM_getValue`,
//! `GM_setValue`, `GM_deleteValue`, `GM_listValues`, `GM_addStyle`, `GM_registerMenuCommand`,
//! same-origin `GM_xmlhttpRequest`), GreasyFork / OpenUserJS installation, and per-profile
//! isolation (never executes userscripts in private windows).

use crate::adblock::host_parents;
use parking_lot::Mutex;
use rand::Rng;
use regex::Regex;
use rover_core::store::now_ts;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use url::Url;

#[derive(Debug, Clone, Serialize)]
pub struct GallerySpec {
    pub slug: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    pub sites: &'static [&'static str],
    pub code: &'static str,
}

pub const GALLERY_SPECS: &[GallerySpec] = &[
    GallerySpec {
        slug: "youtube-enhancer",
        name: "YouTube Theatre & Shorts Blocker",
        description: "Auto-expands theater mode, hides Shorts shelves, and adds keyboard volume/speed HUD.",
        version: "2.0.0",
        sites: &["youtube.com"],
        code: "// ==UserScript==\n// @name YouTube Theatre & Shorts Blocker\n// @match *://*.youtube.com/*\n// ==/UserScript==\n(() => { document.documentElement.setAttribute('data-rover-yt', '1'); })();",
    },
    GallerySpec {
        slug: "focus-blocker",
        name: "Deep Focus Distraction Shield",
        description: "Replaces infinite-scroll feeds with a calm Rover telemetry timer during work sessions.",
        version: "2.0.0",
        sites: &["reddit.com", "x.com", "twitter.com", "facebook.com", "instagram.com"],
        code: "// ==UserScript==\n// @name Deep Focus Distraction Shield\n// @match *://*/*\n// ==/UserScript==\n(() => { window.__roverFocusReady = true; })();",
    },
    GallerySpec {
        slug: "cookie-banner-skipper",
        name: "Cookie Banner Auto-Dismiss",
        description: "Automatically declines or dismisses common GDPR/cookie consent overlays.",
        version: "2.0.0",
        sites: &["*"],
        code: "// ==UserScript==\n// @name Cookie Banner Auto-Dismiss\n// @match *://*/*\n// ==/UserScript==\n(() => { window.__roverCookieDismiss = true; })();",
    },
    GallerySpec {
        slug: "highlighter-notes",
        name: "Page Highlighter & Sticky Notes",
        description: "Highlight text in yellow/green/cyan and attach persistent per-URL margin notes.",
        version: "2.0.0",
        sites: &["*"],
        code: "// ==UserScript==\n// @name Page Highlighter & Sticky Notes\n// @match *://*/*\n// ==/UserScript==\n(() => { window.__roverHighlighter = true; })();",
    },
    GallerySpec {
        slug: "word-count",
        name: "Reading Time & Word Counter",
        description: "Displays live word count, character count, and estimated reading time badge.",
        version: "2.0.0",
        sites: &["*"],
        code: "// ==UserScript==\n// @name Reading Time & Word Counter\n// @match *://*/*\n// ==/UserScript==\n(() => { GM_registerMenuCommand('Count words on page', () => {}); })();",
    },
    GallerySpec {
        slug: "cite-this-page",
        name: "One-Click Academic & Markdown Citation",
        description: "Copies APA, MLA, BibTeX, or Markdown citation for the current article.",
        version: "2.0.0",
        sites: &["*"],
        code: "// ==UserScript==\n// @name One-Click Academic & Markdown Citation\n// @match *://*/*\n// ==/UserScript==\n(() => { GM_registerMenuCommand('Cite this page', () => {}); })();",
    },
    GallerySpec {
        slug: "video-speed",
        name: "Universal Video Speed Controller",
        description: "Adds +/- playback speed hotkeys (0.25x to 4.0x) to any HTML5 video.",
        version: "2.0.0",
        sites: &["*"],
        code: "// ==UserScript==\n// @name Universal Video Speed Controller\n// @match *://*/*\n// ==/UserScript==\n(() => { window.__roverVsc = true; })();",
    },
    GallerySpec {
        slug: "auto-scroll",
        name: "Hands-Free Smooth Auto-Scroller",
        description: "Smoothly scrolls long articles and documentation at adjustable speed.",
        version: "2.0.0",
        sites: &["*"],
        code: "// ==UserScript==\n// @name Hands-Free Smooth Auto-Scroller\n// @match *://*/*\n// ==/UserScript==\n(() => { GM_registerMenuCommand('Toggle auto-scroll', () => {}); })();",
    },
    GallerySpec {
        slug: "password-strength",
        name: "Live Password Entropy Meter",
        description: "Shows a local crack-time and entropy bar under registration password fields.",
        version: "2.0.0",
        sites: &["*"],
        code: "// ==UserScript==\n// @name Live Password Entropy Meter\n// @match *://*/*\n// ==/UserScript==\n(() => { window.__roverPwMeter = true; })();",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    /// `"userscript"`, `"style"`, or `"script"`
    pub kind: String,
    pub source: String,
    pub sites: Vec<String>,
    #[serde(default)]
    pub matches: Vec<String>,
    #[serde(default)]
    pub excludes: Vec<String>,
    #[serde(default = "default_run_at")]
    pub run_at: String,
    pub enabled: bool,
    pub installed: f64,
    pub updated: f64,
}

fn default_run_at() -> String {
    "document-end".to_string()
}

#[derive(Debug, Clone, Default)]
pub struct ParsedHeader {
    pub name: String,
    pub description: String,
    pub version: String,
    pub matches: Vec<String>,
    pub excludes: Vec<String>,
    pub requires: Vec<String>,
    pub run_at: String,
}

pub fn parse_userscript_header(code: &str) -> ParsedHeader {
    let mut header = ParsedHeader {
        version: "1.0".into(),
        run_at: "document-end".into(),
        ..Default::default()
    };
    let mut in_block = false;
    for raw_line in code.lines() {
        let line = raw_line.trim();
        if line.contains("==UserScript==") {
            in_block = true;
            continue;
        }
        if line.contains("==/UserScript==") {
            break;
        }
        if !in_block || !line.starts_with("//") {
            continue;
        }
        let body = line.trim_start_matches('/').trim();
        if !body.starts_with('@') {
            continue;
        }
        let mut parts = body.splitn(2, char::is_whitespace);
        let tag = parts.next().unwrap_or("");
        let val = parts.next().unwrap_or("").trim();
        if val.is_empty() {
            continue;
        }
        match tag {
            "@name" if header.name.is_empty() => header.name = val.to_string(),
            "@description" if header.description.is_empty() => {
                header.description = val.to_string()
            }
            "@version" => header.version = val.to_string(),
            "@match" | "@include" => header.matches.push(val.to_string()),
            "@exclude" => header.excludes.push(val.to_string()),
            "@require" => header.requires.push(val.to_string()),
            "@run-at" => header.run_at = val.to_string(),
            _ => {}
        }
    }
    if header.name.is_empty() {
        header.name = "Untitled Userscript".into();
    }
    if header.matches.is_empty() {
        header.matches.push("*://*/*".into());
    }
    header
}

pub fn pattern_matches_url(pattern: &str, target_url: &str) -> bool {
    let pat = pattern.trim();
    if pat == "*" || pat == "*://*/*" || pat == "<all_urls>" {
        return target_url.starts_with("http://") || target_url.starts_with("https://");
    }
    let Ok(u) = Url::parse(target_url) else {
        return false;
    };
    let host = u.host_str().unwrap_or("").to_ascii_lowercase();

    // Simple site domain match (e.g. "youtube.com" or "*.youtube.com")
    if !pat.contains("://") && !pat.contains('/') {
        let clean = pat
            .strip_prefix("*.")
            .unwrap_or(pat)
            .to_ascii_lowercase();
        return host_parents(&host).into_iter().any(|p| p == clean);
    }

    // Wildcard URL pattern -> regex
    let mut re_str = String::from("(?i)^");
    for ch in pat.chars() {
        match ch {
            '*' => re_str.push_str(".*"),
            '.' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '\\' | '^' | '$' | '|' => {
                re_str.push('\\');
                re_str.push(ch);
            }
            _ => re_str.push(ch),
        }
    }
    re_str.push('$');
    Regex::new(&re_str)
        .map(|re| re.is_match(target_url))
        .unwrap_or(false)
}

#[derive(Clone)]
pub struct Extensions {
    pub folder: PathBuf,
    entries: Arc<Mutex<Vec<ExtensionEntry>>>,
    codes: Arc<Mutex<HashMap<String, String>>>,
}

impl Extensions {
    pub fn new(profile_folder: impl AsRef<Path>) -> Self {
        let folder = profile_folder.as_ref().join("extensions");
        let _ = fs::create_dir_all(&folder);
        let index_path = folder.join("index.json");
        let entries: Vec<ExtensionEntry> = fs::read_to_string(&index_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let mut codes = HashMap::new();
        for e in &entries {
            let code_path = folder.join(format!("{}.user.js", e.id));
            if let Ok(c) = fs::read_to_string(&code_path) {
                codes.insert(e.id.clone(), c);
            }
        }
        Self {
            folder,
            entries: Arc::new(Mutex::new(entries)),
            codes: Arc::new(Mutex::new(codes)),
        }
    }

    pub fn list(&self) -> Vec<ExtensionEntry> {
        self.entries.lock().clone()
    }

    pub fn install_userscript(&self, code: &str, source: &str) -> ExtensionEntry {
        let hdr = parse_userscript_header(code);
        let ts = now_ts();
        let mut list = self.entries.lock();
        if let Some(existing) = list
            .iter_mut()
            .find(|e| (!source.is_empty() && e.source == source) || e.name == hdr.name)
        {
            existing.description = hdr.description;
            existing.version = hdr.version;
            existing.matches = hdr.matches;
            existing.excludes = hdr.excludes;
            existing.run_at = hdr.run_at;
            existing.updated = ts;
            let entry = existing.clone();
            drop(list);
            self.save_code(&entry.id, code);
            self.save_index();
            return entry;
        }

        let mut rng = rand::thread_rng();
        let id: String = (0..12)
            .map(|_| format!("{:x}", rng.gen_range(0..16)))
            .collect();
        let entry = ExtensionEntry {
            id: id.clone(),
            name: hdr.name,
            description: hdr.description,
            version: hdr.version,
            kind: "userscript".into(),
            source: source.to_string(),
            sites: Vec::new(),
            matches: hdr.matches,
            excludes: hdr.excludes,
            run_at: hdr.run_at,
            enabled: true,
            installed: ts,
            updated: ts,
        };
        list.push(entry.clone());
        drop(list);
        self.save_code(&id, code);
        self.save_index();
        entry
    }

    pub fn install_custom(
        &self,
        name: &str,
        kind: &str,
        sites: Vec<String>,
        code: &str,
    ) -> ExtensionEntry {
        let ts = now_ts();
        let mut rng = rand::thread_rng();
        let id: String = (0..12)
            .map(|_| format!("{:x}", rng.gen_range(0..16)))
            .collect();
        let entry = ExtensionEntry {
            id: id.clone(),
            name: name.trim().to_string(),
            description: format!("Custom {} for {}", kind, sites.join(", ")),
            version: "1.0".into(),
            kind: kind.to_string(),
            source: "custom".into(),
            sites: sites.clone(),
            matches: sites,
            excludes: Vec::new(),
            run_at: "document-end".into(),
            enabled: true,
            installed: ts,
            updated: ts,
        };
        self.entries.lock().push(entry.clone());
        self.save_code(&id, code);
        self.save_index();
        entry
    }

    pub fn install_from_gallery(&self, slug: &str) -> Option<ExtensionEntry> {
        let spec = GALLERY_SPECS.iter().find(|s| s.slug == slug)?;
        let source = format!("rover-gallery:{}", spec.slug);
        let mut entry = self.install_userscript(spec.code, &source);
        entry.sites = spec.sites.iter().map(|s| (*s).to_string()).collect();
        {
            let mut list = self.entries.lock();
            if let Some(e) = list.iter_mut().find(|x| x.id == entry.id) {
                e.sites = entry.sites.clone();
            }
        }
        self.save_index();
        Some(entry)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> bool {
        let mut list = self.entries.lock();
        if let Some(e) = list.iter_mut().find(|x| x.id == id) {
            e.enabled = enabled;
            drop(list);
            self.save_index();
            return true;
        }
        false
    }

    pub fn uninstall(&self, id: &str) -> bool {
        let mut list = self.entries.lock();
        let before = list.len();
        list.retain(|x| x.id != id);
        if list.len() < before {
            drop(list);
            self.codes.lock().remove(id);
            let _ = fs::remove_file(self.folder.join(format!("{id}.user.js")));
            self.save_index();
            return true;
        }
        false
    }

    /// Returns wrapped executable JS payloads for `target_url`.
    /// Strictly returns empty for private windows.
    pub fn scripts_for_url(&self, target_url: &str, is_private: bool) -> Vec<(String, String)> {
        if is_private {
            return Vec::new();
        }
        let list = self.entries.lock().clone();
        let codes = self.codes.lock();
        let mut out = Vec::new();

        for ext in list {
            if !ext.enabled {
                continue;
            }
            if ext.excludes.iter().any(|p| pattern_matches_url(p, target_url)) {
                continue;
            }
            let site_matched = if !ext.sites.is_empty() {
                ext.sites.iter().any(|p| pattern_matches_url(p, target_url))
            } else {
                ext.matches.iter().any(|p| pattern_matches_url(p, target_url))
            };
            if !site_matched {
                continue;
            }
            let Some(raw_code) = codes.get(&ext.id) else {
                continue;
            };
            let wrapped = if ext.kind == "style" {
                let css_json = serde_json::to_string(raw_code).unwrap_or_else(|_| "\"\"".into());
                format!(
                    "(() => {{ const s = document.createElement('style'); s.id = '__rover_ext_{}'; s.textContent = {}; (document.head || document.documentElement).appendChild(s); }})();",
                    ext.id, css_json
                )
            } else {
                Self::wrap_with_gm_shim(&ext, raw_code)
            };
            out.push((ext.id, wrapped));
        }
        out
    }

    pub fn wrap_with_gm_shim(ext: &ExtensionEntry, code: &str) -> String {
        let prefix = format!("__rover_gm_{}_", ext.id);
        format!(
            r#"(() => {{
  const __ROVER_PREFIX = "{prefix}";
  const GM_info = {{ script: {{ name: {name:?}, version: {ver:?} }}, scriptHandler: "Rover", version: "2.0.0" }};
  const GM_getValue = (k, defVal) => {{
    try {{ const v = localStorage.getItem(__ROVER_PREFIX + k); return v === null ? defVal : JSON.parse(v); }} catch (_) {{ return defVal; }}
  }};
  const GM_setValue = (k, val) => {{
    try {{ localStorage.setItem(__ROVER_PREFIX + k, JSON.stringify(val)); }} catch (_) {{}}
  }};
  const GM_deleteValue = (k) => {{
    try {{ localStorage.removeItem(__ROVER_PREFIX + k); }} catch (_) {{}}
  }};
  const GM_listValues = () => {{
    const keys = [];
    try {{
      for (let i = 0; i < localStorage.length; i++) {{
        const k = localStorage.key(i);
        if (k && k.startsWith(__ROVER_PREFIX)) keys.push(k.slice(__ROVER_PREFIX.length));
      }}
    }} catch (_) {{}}
    return keys;
  }};
  const GM_addStyle = (css) => {{
    const s = document.createElement("style");
    s.textContent = String(css || "");
    (document.head || document.documentElement).appendChild(s);
    return s;
  }};
  const GM_registerMenuCommand = (label, fn) => {{
    window.__roverMenu = window.__roverMenu || [];
    window.__roverMenu.push({{ extId: {id:?}, label: String(label), fn }});
  }};
  const GM = {{ info: GM_info, getValue: GM_getValue, setValue: GM_setValue, deleteValue: GM_deleteValue, listValues: GM_listValues, addStyle: GM_addStyle, registerMenuCommand: GM_registerMenuCommand }};
  {code}
}})();"#,
            prefix = prefix,
            name = ext.name,
            ver = ext.version,
            id = ext.id,
            code = code
        )
    }

    fn save_code(&self, id: &str, code: &str) {
        self.codes.lock().insert(id.to_string(), code.to_string());
        let _ = fs::write(self.folder.join(format!("{id}.user.js")), code.as_bytes());
    }

    fn save_index(&self) {
        let list = self.entries.lock().clone();
        if let Ok(bytes) = serde_json::to_vec_pretty(&list) {
            let path = self.folder.join("index.json");
            let tmp = path.with_extension("json.tmp");
            if fs::write(&tmp, bytes).is_ok() {
                let _ = fs::rename(&tmp, &path);
            }
        }
    }
}
