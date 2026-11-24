//! Tab Strip, Tab Groups, Pinned Tabs, Sleeping Tabs, Split View, Vertical Tabs, and Workspaces.

use once_cell::sync::Lazy;
use rover_core::store::now_ts;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

pub const TAB_MAX_WIDTH: usize = 240;
pub const TAB_MIN_WIDTH: usize = 140;
pub const PINNED_WIDTH: usize = 44;
pub const CLOSED_LIMIT: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GroupColor {
    pub name: &'static str,
    pub hex: &'static str,
}

pub const GROUP_COLORS: &[GroupColor] = &[
    GroupColor { name: "Red", hex: "#C83F3A" },
    GroupColor { name: "Orange", hex: "#EF7560" },
    GroupColor { name: "Yellow", hex: "#FDB78E" },
    GroupColor { name: "Pink", hex: "#DA5647" },
    GroupColor { name: "Cyan", hex: "#FED8B1" },
    GroupColor { name: "Purple", hex: "#9E2A2B" },
    GroupColor { name: "Blue", hex: "#D97757" },
    GroupColor { name: "Green", hex: "#C26D4A" },
    GroupColor { name: "Grey", hex: "#A88278" },
];

pub fn resolve_group_color(name_or_hex: &str) -> String {
    let trimmed = name_or_hex.trim();
    if trimmed.starts_with('#') {
        return trimmed.to_string();
    }
    for c in GROUP_COLORS {
        if c.name.eq_ignore_ascii_case(trimmed) {
            return c.hex.to_string();
        }
    }
    GROUP_COLORS[0].hex.to_string()
}

static SITE_NAMES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    [
        ("github.com", "GitHub"),
        ("wikipedia.org", "Wikipedia"),
        ("youtube.com", "YouTube"),
        ("google.com", "Google"),
        ("reddit.com", "Reddit"),
        ("stackoverflow.com", "Stack Overflow"),
        ("news.ycombinator.com", "Hacker News"),
        ("nasa.gov", "NASA"),
        ("x.com", "X"),
        ("amazon.com", "Amazon"),
    ]
    .into_iter()
    .collect()
});

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabGroup {
    pub id: String,
    pub name: String,
    pub color: String,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tab {
    pub id: usize,
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub muted: bool,
    pub audible: bool,
    pub sleeping: bool,
    pub loading: bool,
    pub devtools_open: bool,
    pub agent_active: bool,
    pub group_id: Option<String>,
    pub last_active: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Workspace {
    pub name: String,
    pub tabs: Vec<Tab>,
    pub groups: HashMap<String, TabGroup>,
    pub active_index: usize,
    pub split_secondary_id: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabStrip {
    pub tabs: Vec<Tab>,
    pub groups: HashMap<String, TabGroup>,
    pub active_index: usize,
    pub closed_stack: Vec<Tab>,
    pub split_secondary_id: Option<usize>,
    pub vertical_tabs: bool,
    pub workspaces: HashMap<String, Workspace>,
    pub current_workspace: String,
    next_tab_id: usize,
    next_group_id: usize,
}

impl Default for TabStrip {
    fn default() -> Self {
        Self::new()
    }
}

impl TabStrip {
    pub fn new() -> Self {
        let mut strip = Self {
            tabs: Vec::new(),
            groups: HashMap::new(),
            active_index: 0,
            closed_stack: Vec::new(),
            split_secondary_id: None,
            vertical_tabs: false,
            workspaces: HashMap::new(),
            current_workspace: "Main".into(),
            next_tab_id: 1,
            next_group_id: 1,
        };
        strip.open_tab("rover:start", "New Tab");
        strip
    }

    /// Computes equal tab width for unpinned tabs inside `available_px` strip width.
    pub fn compute_tab_width(&self, available_px: usize) -> usize {
        let pinned_count = self.tabs.iter().filter(|t| t.pinned).count();
        let unpinned_count = self.tabs.len().saturating_sub(pinned_count);
        if unpinned_count == 0 {
            return TAB_MAX_WIDTH;
        }
        let rem = available_px.saturating_sub(pinned_count * PINNED_WIDTH);
        (rem / unpinned_count).clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH)
    }

    pub fn open_tab(&mut self, url: &str, title: &str) -> usize {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let clean_url = if url.trim().is_empty() {
            "rover:start"
        } else {
            url.trim()
        };
        let clean_title = if title.trim().is_empty() {
            "New Tab"
        } else {
            title.trim()
        };
        self.tabs.push(Tab {
            id,
            url: clean_url.to_string(),
            title: clean_title.to_string(),
            pinned: false,
            muted: false,
            audible: false,
            sleeping: false,
            loading: false,
            devtools_open: false,
            agent_active: false,
            group_id: None,
            last_active: now_ts(),
        });
        self.active_index = self.tabs.len() - 1;
        id
    }

    pub fn select_index(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.active_index = index;
        let ts = now_ts();
        self.tabs[index].sleeping = false;
        self.tabs[index].last_active = ts;
        true
    }

    pub fn close_index(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return false;
        }
        let removed = self.tabs.remove(index);
        if self.split_secondary_id == Some(removed.id) {
            self.split_secondary_id = None;
        }
        if !removed.url.starts_with("rover:start") && !removed.url.starts_with("atlas:start") {
            self.closed_stack.push(removed);
            if self.closed_stack.len() > CLOSED_LIMIT {
                self.closed_stack.remove(0);
            }
        }
        if self.active_index >= self.tabs.len() {
            self.active_index = self.tabs.len() - 1;
        }
        self.cleanup_empty_groups();
        true
    }

    pub fn reopen_closed(&mut self) -> Option<usize> {
        let mut tab = self.closed_stack.pop()?;
        tab.id = self.next_tab_id;
        self.next_tab_id += 1;
        tab.sleeping = false;
        tab.last_active = now_ts();
        let id = tab.id;
        self.tabs.push(tab);
        self.active_index = self.tabs.len() - 1;
        Some(id)
    }

    pub fn set_pinned(&mut self, index: usize, pinned: bool) {
        if index >= self.tabs.len() {
            return;
        }
        let active_id = self.tabs[self.active_index].id;
        self.tabs[index].pinned = pinned;
        if pinned {
            self.tabs[index].group_id = None;
            self.tabs[index].sleeping = false;
        }
        // Keep all pinned tabs at the front of the strip
        self.tabs.sort_by_key(|t| !t.pinned);
        if let Some(pos) = self.tabs.iter().position(|t| t.id == active_id) {
            self.active_index = pos;
        }
    }

    pub fn duplicate_tab(&mut self, index: usize) -> Option<usize> {
        let src = self.tabs.get(index)?.clone();
        let new_id = self.next_tab_id;
        self.next_tab_id += 1;
        let dup = Tab {
            id: new_id,
            url: src.url,
            title: src.title,
            pinned: false,
            muted: src.muted,
            audible: false,
            sleeping: false,
            loading: false,
            devtools_open: false,
            agent_active: false,
            group_id: src.group_id,
            last_active: now_ts(),
        };
        self.tabs.insert(index + 1, dup);
        self.active_index = index + 1;
        Some(new_id)
    }

    // --------------------------------------------------------- Tab Groups
    pub fn group_tabs(&mut self, indexes: &[usize], name: &str, color: Option<&str>) -> String {
        let clean_name = name.trim();
        let existing_gid = self
            .groups
            .values()
            .find(|g| !clean_name.is_empty() && g.name.eq_ignore_ascii_case(clean_name))
            .map(|g| g.id.clone());

        let gid = match existing_gid {
            Some(id) => {
                if let Some(c) = color {
                    if let Some(g) = self.groups.get_mut(&id) {
                        g.color = resolve_group_color(c);
                    }
                }
                id
            }
            None => {
                let id = format!("g{}", self.next_group_id);
                self.next_group_id += 1;
                let chosen_color = color
                    .map(resolve_group_color)
                    .unwrap_or_else(|| GROUP_COLORS[self.groups.len() % GROUP_COLORS.len()].hex.to_string());
                self.groups.insert(
                    id.clone(),
                    TabGroup {
                        id: id.clone(),
                        name: clean_name.to_string(),
                        color: chosen_color,
                        collapsed: false,
                    },
                );
                id
            }
        };

        for &idx in indexes {
            if let Some(t) = self.tabs.get_mut(idx) {
                if !t.pinned {
                    t.group_id = Some(gid.clone());
                }
            }
        }
        gid
    }

    pub fn ungroup_by_name(&mut self, name: &str) -> usize {
        let target_ids: Vec<String> = self
            .groups
            .values()
            .filter(|g| g.name.eq_ignore_ascii_case(name.trim()))
            .map(|g| g.id.clone())
            .collect();
        let mut count = 0;
        for gid in &target_ids {
            self.groups.remove(gid);
            for t in &mut self.tabs {
                if t.group_id.as_ref() == Some(gid) {
                    t.group_id = None;
                    count += 1;
                }
            }
        }
        count
    }

    pub fn group_by_site(&mut self) -> usize {
        let mut by_domain: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, t) in self.tabs.iter().enumerate() {
            if t.pinned {
                continue;
            }
            if let Ok(u) = Url::parse(&t.url) {
                if matches!(u.scheme(), "http" | "https") {
                    if let Some(host) = u.host_str() {
                        let dom = rover_net::adblock::registrable_domain(host);
                        if !dom.is_empty() {
                            by_domain.entry(dom).or_default().push(idx);
                        }
                    }
                }
            }
        }

        let mut moved = 0usize;
        for (dom, idxs) in by_domain {
            if idxs.len() < 2 {
                continue;
            }
            let label = SITE_NAMES
                .get(dom.as_str())
                .map(|s| (*s).to_string())
                .unwrap_or_else(|| {
                    let base = dom.split('.').next().unwrap_or(&dom);
                    let mut chars = base.chars();
                    match chars.next() {
                        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                        None => dom.clone(),
                    }
                });
            self.group_tabs(&idxs, &label, None);
            moved += idxs.len();
        }
        moved
    }

    fn cleanup_empty_groups(&mut self) {
        let used: std::collections::HashSet<String> = self
            .tabs
            .iter()
            .filter_map(|t| t.group_id.clone())
            .collect();
        self.groups.retain(|id, _| used.contains(id));
    }

    // --------------------------------------------------------- Sleeping Tabs
    pub fn can_sleep_tab(&self, index: usize) -> bool {
        let Some(t) = self.tabs.get(index) else {
            return false;
        };
        if index == self.active_index
            || self.split_secondary_id == Some(t.id)
            || t.pinned
            || t.audible
            || t.devtools_open
            || t.agent_active
            || t.loading
        {
            return false;
        }
        true
    }

    pub fn sleep_tab(&mut self, index: usize) -> bool {
        if !self.can_sleep_tab(index) {
            return false;
        }
        self.tabs[index].sleeping = true;
        true
    }

    pub fn wake_tab(&mut self, index: usize) {
        if let Some(t) = self.tabs.get_mut(index) {
            t.sleeping = false;
            t.last_active = now_ts();
        }
    }

    pub fn auto_sleep_idle(&mut self, idle_minutes: f64, now: f64) -> usize {
        let cutoff = now - idle_minutes * 60.0;
        let mut slept = 0;
        for idx in 0..self.tabs.len() {
            if self.can_sleep_tab(idx) && !self.tabs[idx].sleeping && self.tabs[idx].last_active < cutoff {
                self.tabs[idx].sleeping = true;
                slept += 1;
            }
        }
        slept
    }

    // --------------------------------------------------------- Split View & Search
    pub fn set_split(&mut self, secondary_index: usize) -> bool {
        if secondary_index >= self.tabs.len() || secondary_index == self.active_index {
            return false;
        }
        self.tabs[secondary_index].sleeping = false;
        self.split_secondary_id = Some(self.tabs[secondary_index].id);
        true
    }

    pub fn end_split(&mut self) {
        self.split_secondary_id = None;
    }

    pub fn search_tabs(&self, query: &str) -> Vec<&Tab> {
        let q = query.trim().to_ascii_lowercase();
        self.tabs
            .iter()
            .filter(|t| {
                q.is_empty()
                    || t.title.to_ascii_lowercase().contains(&q)
                    || t.url.to_ascii_lowercase().contains(&q)
            })
            .collect()
    }

    // --------------------------------------------------------- Workspaces
    pub fn switch_workspace(&mut self, target_name: &str) {
        let clean = target_name.trim();
        if clean.is_empty() || clean == self.current_workspace {
            return;
        }
        // Save current workspace
        self.workspaces.insert(
            self.current_workspace.clone(),
            Workspace {
                name: self.current_workspace.clone(),
                tabs: self.tabs.clone(),
                groups: self.groups.clone(),
                active_index: self.active_index,
                split_secondary_id: self.split_secondary_id,
            },
        );
        self.current_workspace = clean.to_string();
        if let Some(ws) = self.workspaces.get(clean).cloned() {
            self.tabs = ws.tabs;
            self.groups = ws.groups;
            self.active_index = ws.active_index.min(self.tabs.len().saturating_sub(1));
            self.split_secondary_id = ws.split_secondary_id;
        } else {
            self.tabs.clear();
            self.groups.clear();
            self.active_index = 0;
            self.split_secondary_id = None;
            self.open_tab("rover:start", "New Tab");
        }
    }

    /// Replaces the sole remaining tab with a fresh `rover:start` tab instead of
    /// exiting the browser, remembering the previous page in `closed_stack` so
    /// `Ctrl+Shift+T` can still reopen it. Returns `(old_tab_id, new_tab_id)`.
    pub fn reset_last_tab_to_home(&mut self) -> (Option<usize>, usize) {
        let old = self.tabs.pop();
        let old_id = old.as_ref().map(|t| t.id);
        if let Some(removed) = old {
            if !removed.url.starts_with("rover:start") && !removed.url.starts_with("atlas:start") {
                self.closed_stack.push(removed);
                if self.closed_stack.len() > CLOSED_LIMIT {
                    self.closed_stack.remove(0);
                }
            }
        }
        self.split_secondary_id = None;
        self.groups.clear();
        let new_id = self.open_tab("rover:start", "New tab");
        (old_id, new_id)
    }

    /// Closes all unpinned tabs except `keep_index`. Returns the IDs of closed tabs.
    pub fn close_other_tabs(&mut self, keep_index: usize) -> Vec<usize> {
        if keep_index >= self.tabs.len() {
            return Vec::new();
        }
        let keep_id = self.tabs[keep_index].id;
        let mut closed_ids = Vec::new();
        let mut kept = Vec::with_capacity(self.tabs.len());
        for t in self.tabs.drain(..) {
            if t.id == keep_id || t.pinned {
                kept.push(t);
            } else {
                closed_ids.push(t.id);
                if self.split_secondary_id == Some(t.id) {
                    self.split_secondary_id = None;
                }
                if !t.url.starts_with("rover:start") && !t.url.starts_with("atlas:start") {
                    self.closed_stack.push(t);
                    if self.closed_stack.len() > CLOSED_LIMIT {
                        self.closed_stack.remove(0);
                    }
                }
            }
        }
        self.tabs = kept;
        self.active_index = self
            .tabs
            .iter()
            .position(|t| t.id == keep_id)
            .unwrap_or(0);
        self.cleanup_empty_groups();
        closed_ids
    }

    // --------------------------------------------------------- Session Persistence
    pub fn to_session(&self) -> SavedSession {
        let split_secondary_index = self
            .split_secondary_id
            .and_then(|sid| self.tabs.iter().position(|t| t.id == sid));
        SavedSession {
            version: 1,
            tabs: self.tabs.iter().map(SavedTab::from_tab).collect(),
            active_index: self.active_index.min(self.tabs.len().saturating_sub(1)),
            groups: self.groups.clone(),
            closed_tabs: self.closed_stack.iter().map(SavedTab::from_tab).collect(),
            split_secondary_index,
            workspaces: self.workspaces.clone(),
            current_workspace: self.current_workspace.clone(),
        }
    }

    pub fn from_session(session: SavedSession) -> Self {
        let mut strip = Self {
            tabs: Vec::new(),
            groups: session.groups,
            active_index: 0,
            closed_stack: Vec::new(),
            split_secondary_id: None,
            vertical_tabs: false,
            workspaces: session.workspaces,
            current_workspace: if session.current_workspace.trim().is_empty() {
                "Main".into()
            } else {
                session.current_workspace
            },
            next_tab_id: 1,
            next_group_id: 1,
        };

        // Ensure next_tab_id stays higher than any ID stored inside workspaces
        for ws in strip.workspaces.values() {
            for t in &ws.tabs {
                if t.id >= strip.next_tab_id {
                    strip.next_tab_id = t.id + 1;
                }
            }
        }

        // Ensure next_group_id stays higher than any existing group key ("gN")
        for gid in strip.groups.keys() {
            if let Some(num) = gid.strip_prefix('g').and_then(|s| s.parse::<usize>().ok()) {
                if num >= strip.next_group_id {
                    strip.next_group_id = num + 1;
                }
            }
        }

        let now = now_ts();
        for st in session.tabs {
            let clean_url = st.url.trim();
            if clean_url.is_empty() || clean_url == "about:blank" {
                continue;
            }
            let id = strip.next_tab_id;
            strip.next_tab_id += 1;
            let title = if st.title.trim().is_empty() {
                if clean_url == "rover:start" {
                    "New tab".to_string()
                } else {
                    clean_url.to_string()
                }
            } else {
                st.title
            };
            let group_id = st.group_id.filter(|gid| strip.groups.contains_key(gid));
            strip.tabs.push(Tab {
                id,
                url: clean_url.to_string(),
                title,
                pinned: st.pinned,
                muted: false,
                audible: false,
                sleeping: false,
                loading: false,
                devtools_open: false,
                agent_active: false,
                group_id,
                last_active: now,
            });
        }

        if strip.tabs.is_empty() {
            strip.open_tab("rover:start", "New tab");
        } else {
            strip.active_index = session.active_index.min(strip.tabs.len() - 1);
            if let Some(sidx) = session.split_secondary_index {
                if sidx < strip.tabs.len() && sidx != strip.active_index {
                    strip.split_secondary_id = Some(strip.tabs[sidx].id);
                }
            }
        }

        for ct in session.closed_tabs.into_iter().rev().take(CLOSED_LIMIT).rev() {
            let clean_url = ct.url.trim();
            if clean_url.is_empty() || clean_url.starts_with("rover:start") {
                continue;
            }
            let id = strip.next_tab_id;
            strip.next_tab_id += 1;
            strip.closed_stack.push(Tab {
                id,
                url: clean_url.to_string(),
                title: if ct.title.trim().is_empty() {
                    clean_url.to_string()
                } else {
                    ct.title
                },
                pinned: false,
                muted: false,
                audible: false,
                sleeping: false,
                loading: false,
                devtools_open: false,
                agent_active: false,
                group_id: None,
                last_active: now,
            });
        }

        strip.cleanup_empty_groups();
        strip
    }
}

fn default_workspace_name() -> String {
    "Main".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedTab {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub group_id: Option<String>,
}

impl SavedTab {
    fn from_tab(t: &Tab) -> Self {
        Self {
            url: t.url.clone(),
            title: t.title.clone(),
            pinned: t.pinned,
            group_id: t.group_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedSession {
    #[serde(default)]
    pub version: u32,
    pub tabs: Vec<SavedTab>,
    #[serde(default)]
    pub active_index: usize,
    #[serde(default)]
    pub groups: HashMap<String, TabGroup>,
    #[serde(default)]
    pub closed_tabs: Vec<SavedTab>,
    #[serde(default)]
    pub split_secondary_index: Option<usize>,
    #[serde(default)]
    pub workspaces: HashMap<String, Workspace>,
    #[serde(default = "default_workspace_name")]
    pub current_workspace: String,
}

/// Atomically persists the open tab strip session to `path` (`session.json`).
pub fn save_session(path: &std::path::Path, strip: &TabStrip) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let session = strip.to_session();
    let bytes = serde_json::to_vec_pretty(&session)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, &bytes).is_ok() {
        if std::fs::rename(&tmp, path).is_ok() {
            return Ok(());
        }
        let _ = std::fs::remove_file(&tmp);
    }
    std::fs::write(path, &bytes)
}

/// Loads a previously saved tab session from `path` (`session.json`).
pub fn load_session(path: &std::path::Path) -> Option<SavedSession> {
    let raw = std::fs::read_to_string(path).ok()?;
    let session: SavedSession = serde_json::from_str(&raw).ok()?;
    if session.tabs.is_empty() {
        return None;
    }
    Some(session)
}

