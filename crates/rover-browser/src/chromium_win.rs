//! Pure Open-Source Chromium 156 (CEF — `libcef.dll`) Native Windows Desktop Browser.
//!
//! Zero Microsoft Edge / WebView2 dependency. Runs inside its own native dark-themed Win32
//! desktop window (`1360×860`, embedded circular Inverted Mars Rover icon, DWM dark titlebar)
//! with 1-to-1 Pelagic UI fidelity, zero pre-populated bookmarks/shortcuts, real Chromium
//! network-level Aho-Corasick adblocking (2.00 µs/req), real bottom-docked Chromium DevTools,
//! native Win32 popup menus, owned dark dialog windows, and the live Rover AI Agent.

use crate::chromium_ui::{render_ai_panel_html, render_chrome_bar_html, render_dialog_html};
use crate::homepage::{
    add_shortcut, build_homepage_html, parse_home_action, remove_shortcut, HomepageConfig,
    ROVER_ICON_PNG_BYTES,
};
use crate::omnibox::search_url;
use crate::playground::{is_playground_address, render_playground_html};
use crate::tabs::TabStrip;
use crate::tools::{extract_reader_article, render_reader_html};
use cef::args::Args;
use cef::{self, Rect as CefRect, *};
use parking_lot::Mutex;
use rover_ai::{
    agent::{
        format_snapshot, parse_plan, system_prompt, AgentConfig, AgentState, PageSnapshot,
        TabSummary,
    },
    context::{active_set, core_enabled, core_system, instruction_sets, save_sets, InstructionSet},
    localai::{parse_local_command, LocalCommand},
    pagejs,
    providers::{chat_sync, preset_providers, Provider},
    token_optimizer::SnapshotDiffer,
    usage::{Meter, UsageLog},
};
use rover_core::{memory::MemoryFile, pwguard, Store};
use rover_net::{
    adblock::{Blocker, FilterEngine, ResourceKind},
    blocklist::DomainBlocker,
    darkmode::DarkMode,
    proxy::{ProxyConfig, ProxyMode, ProxyScheme},
    safebrowsing::SafeBrowsingGuard,
};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, OnceLock,
};
use tiny_http::{Header, Response, Server};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, CreateSolidBrush};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    GetClientRect, LoadCursorW, LoadIconW, MoveWindow, PostMessageW, RegisterClassExW,
    SetForegroundWindow, SetWindowPos, SetWindowTextW, ShowWindow, TrackPopupMenuEx, CS_HREDRAW,
    CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    MF_UNCHECKED, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW,
    TPM_LEFTBUTTON, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_TOPALIGN, WM_APP, WM_CLOSE, WM_DESTROY,
    WM_SIZE, WNDCLASSEXW, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

const WM_ROVER_EXEC: u32 = WM_APP + 17;
const BASE_CHROME_H: i32 = 84; // 38px tabrow + 44px toolbar + 2px progress
const BM_BAR_H: i32 = 28;
const FIND_BAR_H: i32 = 36;
const PANEL_W: i32 = 400;
const DEVTOOLS_MIN_H: i32 = 260;

// CEF EventFlags bitmask
const EF_SHIFT: u32 = 2;
const EF_CTRL: u32 = 4;
const EF_ALT: u32 = 8;

const DARKMODE_CSS: &str = "html{filter:invert(0.92) hue-rotate(180deg)!important;background:#160708!important;}img,video,canvas,svg,picture{filter:invert(1) hue-rotate(180deg)!important;}";

static GLOBAL_APP: OnceLock<Arc<RoverRuntime>> = OnceLock::new();

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn win_to_cef_hwnd(h: HWND) -> cef::sys::cef_window_handle_t {
    cef::sys::HWND(h.cast())
}

fn cef_to_win_hwnd(h: cef::sys::cef_window_handle_t) -> HWND {
    h.0.cast()
}

fn apply_dark_titlebar(hwnd: HWND) {
    let on: i32 = 1;
    unsafe {
        for attr in [20u32, 19u32] {
            if DwmSetWindowAttribute(
                hwnd,
                attr,
                (&on as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            ) == 0
            {
                break;
            }
        }
    }
}

fn gen_session_token() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    format!(
        "{:032x}",
        nanos ^ (pid << 64) ^ 0x526f_7665_724d_6172_7332_3032_36u128
    )
}

fn glow_js(on: bool) -> &'static str {
    if on {
        "(function(){if(document.getElementById('__rv_glow'))return;var d=document.createElement('div');d.id='__rv_glow';d.style.cssText='position:fixed;inset:0;pointer-events:none;z-index:2147483647;box-shadow:inset 0 0 0 3px rgba(239,117,96,.92),inset 0 0 36px rgba(200,63,58,.45);';(document.body||document.documentElement).appendChild(d);})();"
    } else {
        "(function(){var d=document.getElementById('__rv_glow');if(d)d.remove();})();"
    }
}

fn cef_res_to_kind(rt: ResourceType) -> ResourceKind {
    match rt.get_raw() {
        0 => ResourceKind::MainFrame,
        1 => ResourceKind::SubFrame,
        2 => ResourceKind::Stylesheet,
        3 => ResourceKind::Script,
        4 => ResourceKind::Image,
        5 => ResourceKind::Font,
        8 => ResourceKind::Media,
        13 => ResourceKind::Xhr,
        14 => ResourceKind::Ping,
        _ => ResourceKind::Other,
    }
}

fn normalize_address(input: &str, engine: &str) -> String {
    let t = input.trim();
    if t.is_empty() {
        return "rover:start".to_string();
    }
    let low = t.to_ascii_lowercase();
    if low == "rover:start" || low == "atlas:start" {
        return "rover:start".to_string();
    }
    if is_playground_address(t) {
        return "rover:playground".to_string();
    }
    if low.starts_with("http://")
        || low.starts_with("https://")
        || low.starts_with("file://")
        || low.starts_with("view-source:")
        || low.starts_with("about:")
        || low.starts_with("data:")
    {
        return t.to_string();
    }
    if !t.contains(char::is_whitespace) && t.contains('.') {
        return format!("https://{t}");
    }
    search_url(engine, t)
}

fn engine_display_and_action(engine_id: &str) -> (&'static str, &'static str) {
    match engine_id {
        "google" => ("Google", "https://www.google.com/search"),
        "bing" => ("Bing", "https://www.bing.com/search"),
        "brave" => ("Brave Search", "https://search.brave.com/search"),
        "ecosia" => ("Ecosia", "https://www.ecosia.org/search"),
        _ => ("DuckDuckGo", "https://duckduckgo.com/"),
    }
}

fn resolve_provider_key(store: &Store, p: &Provider) -> String {
    let env_refs: Vec<&str> = p.env_vars.iter().map(|s| s.as_str()).collect();
    store.api_key(&p.id, &env_refs)
}

/// Must be called at the very start of `main()` on Windows so Chromium renderer,
/// GPU, network, and utility helper subprocesses fork and exit cleanly.
pub fn try_run_cef_subprocess() -> Option<i32> {
    let is_subprocess = std::env::args().any(|a| a.starts_with("--type="));
    if !is_subprocess {
        return None;
    }
    let _ = api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();
    let code = execute_process(Some(args.as_main_args()), None, ptr::null_mut());
    if code >= 0 {
        Some(code)
    } else {
        Some(0)
    }
}

// ============================================================================
// Network Filter Shared Between UI Thread & CEF IO Thread
// ============================================================================

struct NetFilterState {
    blocker: Blocker,
    domain_blocker: DomainBlocker,
    safebrowsing: SafeBrowsingGuard,
    darkmode: DarkMode,
    domain_block_enabled: bool,
    blocked_total: u64,
    blocked_by_host: HashMap<String, u64>,
}

impl NetFilterState {
    fn new(store: &Store) -> Self {
        let default_rules = r#"
||doubleclick.net^
||googlesyndication.com^
||googleadservices.com^
||googletagmanager.com^
||google-analytics.com^
||adservice.google.com^
||adnxs.com^
||criteo.com^
||taboola.com^
||outbrain.com^
||scorecardresearch.com^
||quantserve.com^
||hotjar.com^
||mixpanel.com^
||segment.io^
||facebook.net/en_US/fbevents.js
||ads.yahoo.com^
||amazon-adsystem.com^
/pagead/$script,subdocument
/ads/pixel$image,third-party
"#;
        let engine = Arc::new(FilterEngine::from_lists(&[default_rules]));
        let blocker = Blocker::new(store.clone(), engine);
        let domain_blocker = DomainBlocker::new(store.clone());
        domain_blocker.load_external_list_text(
            "0.0.0.0 doubleclick.net\n0.0.0.0 googlesyndication.com\n0.0.0.0 adnxs.com\n0.0.0.0 tracking.example.invalid\n",
        );
        let darkmode = DarkMode::new(store.clone());
        Self {
            blocker,
            domain_blocker,
            safebrowsing: SafeBrowsingGuard::new(store.clone()),
            darkmode,
            domain_block_enabled: true,
            blocked_total: 0,
            blocked_by_host: HashMap::new(),
        }
    }

    fn check_request(&mut self, url: &str, first_party: &str, kind: ResourceKind) -> bool {
        if url.starts_with("http://127.0.0.1:")
            || url.starts_with("data:")
            || url.starts_with("about:")
        {
            return false;
        }
        let host = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_ascii_lowercase()))
            .unwrap_or_default();
        let page_host = url::Url::parse(first_party)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_ascii_lowercase()))
            .unwrap_or_else(|| host.clone());

        if kind == ResourceKind::MainFrame && self.safebrowsing.check(url).is_some() {
            return true;
        }
        if self.domain_block_enabled
            && !host.is_empty()
            && self.domain_blocker.match_url_or_host(&host).is_some()
        {
            self.blocked_total += 1;
            *self.blocked_by_host.entry(page_host).or_insert(0) += 1;
            return true;
        }
        if self.blocker.check_and_record(url, first_party, kind) {
            self.blocked_total += 1;
            *self.blocked_by_host.entry(page_host).or_insert(0) += 1;
            return true;
        }
        false
    }
}

// ============================================================================
// Runtime State & Commands
// ============================================================================

#[derive(Clone, serde::Serialize)]
struct ChatMsg {
    who: String,
    detail: String,
    text: String,
    kind: String,
}

enum RoverCmd {
    JsonCmd(Value),
    TabBrowserCreated {
        tab_id: usize,
        browser: Browser,
    },
    ChromeBrowserCreated(Browser),
    PanelBrowserCreated(Browser),
    DevToolsCreated {
        tab_id: usize,
        browser: Browser,
    },
    TabAddressChanged {
        tab_id: usize,
        url: String,
    },
    TabTitleChanged {
        tab_id: usize,
        title: String,
    },
    TabLoadingChanged {
        tab_id: usize,
        loading: bool,
        can_back: bool,
        can_forward: bool,
    },
    TabProgressChanged {
        tab_id: usize,
        progress: f64,
    },
    TabLoadEnded {
        tab_id: usize,
    },
    TabOpenPopup(String),
    TabHomeAction {
        action: String,
        index: Option<usize>,
    },
    JsCallback {
        cb_id: u64,
        payload: String,
    },
    AgentReply {
        task_id: u64,
        reply: Result<String, String>,
        elapsed_secs: f64,
        system_sent: String,
        prompt_sent: String,
        provider_name: String,
        model_name: String,
    },
    KeyShortcut(String),
}

struct TabBrowserEntry {
    browser: Option<Browser>,
    devtools_browser: Option<Browser>,
    devtools_open: bool,
    loading: bool,
    progress: f64,
    can_back: bool,
    can_forward: bool,
    zoom: f64,
}

impl Default for TabBrowserEntry {
    fn default() -> Self {
        Self {
            browser: None,
            devtools_browser: None,
            devtools_open: false,
            loading: false,
            progress: 0.0,
            can_back: false,
            can_forward: false,
            zoom: 0.0,
        }
    }
}

struct ActiveAgentRun {
    task_id: u64,
    state: AgentState,
    differ: SnapshotDiffer,
    provider: Provider,
    api_key: String,
    core_instructions: String,
    memory_text: String,
    tab_id: usize,
    steps: usize,
}

struct BrowserState {
    store: Store,
    private: bool,
    strip: TabStrip,
    tab_entries: HashMap<usize, TabBrowserEntry>,
    chrome_browser: Option<Browser>,
    panel_browser: Option<Browser>,
    dialog_hwnd: isize,
    panel_open: bool,
    find_open: bool,
    bookmarks_bar_visible: bool,
    reader_html_by_tab: HashMap<usize, String>,
    messages: Vec<ChatMsg>,
    raw_log: String,
    ai_status: String,
    ai_busy: bool,
    pw_warning: String,
    held_prompt: String,
    last_answer: String,
    session_meter: Meter,
    usage_log: UsageLog,
    memory: MemoryFile,
    context_pct: u32,
    context_label: String,
    active_agent: Option<ActiveAgentRun>,
    pending_js: HashMap<u64, PendingJsKind>,
}

impl BrowserState {
    fn active_tab(&self) -> Option<&crate::tabs::Tab> {
        self.strip.tabs.get(self.strip.active_index)
    }

    fn active_tab_mut(&mut self) -> Option<&mut crate::tabs::Tab> {
        self.strip.tabs.get_mut(self.strip.active_index)
    }
}

enum PendingJsKind {
    AgentSnapshot { task_id: u64 },
    AgentAction { task_id: u64, settle_ms: u64 },
    ReaderExtract { tab_id: usize, url: String },
}

struct RoverRuntime {
    port: u16,
    token: String,
    main_hwnd: Mutex<isize>,
    queue: Mutex<VecDeque<RoverCmd>>,
    state: Mutex<BrowserState>,
    net: Mutex<NetFilterState>,
    next_cb_id: AtomicU64,
    next_task_id: AtomicU64,
    closing: AtomicBool,
}

impl RoverRuntime {
    fn post(&self, cmd: RoverCmd) {
        self.queue.lock().push_back(cmd);
        let hwnd = *self.main_hwnd.lock() as HWND;
        if !hwnd.is_null() {
            unsafe {
                PostMessageW(hwnd, WM_ROVER_EXEC, 0, 0);
            }
        }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn internal_start_url(&self) -> String {
        format!("{}/ui/start?t={}", self.base_url(), self.token)
    }

    fn internal_playground_url(&self) -> String {
        format!("{}/ui/playground?t={}", self.base_url(), self.token)
    }

    fn map_to_cef_url(&self, logical: &str) -> String {
        let low = logical.to_ascii_lowercase();
        if low == "rover:start" || low == "atlas:start" || low.is_empty() {
            self.internal_start_url()
        } else if is_playground_address(&low) {
            self.internal_playground_url()
        } else {
            logical.to_string()
        }
    }

    fn map_from_cef_url(&self, raw: &str) -> String {
        if raw.starts_with(&format!("{}/ui/start", self.base_url())) {
            "rover:start".to_string()
        } else if raw.starts_with(&format!("{}/ui/playground", self.base_url())) {
            "rover:playground".to_string()
        } else if raw.starts_with(&format!("{}/ui/reader", self.base_url())) {
            "rover:reader".to_string()
        } else {
            raw.to_string()
        }
    }

    fn build_state_json(&self) -> Value {
        let st = self.state.lock();
        let net = self.net.lock();
        let active_tab = st.active_tab();
        let active_id = active_tab.map(|t| t.id).unwrap_or(0);
        let entry = st.tab_entries.get(&active_id);
        let url = active_tab
            .map(|t| t.url.clone())
            .unwrap_or_else(|| "rover:start".into());
        let host = url::Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_ascii_lowercase()))
            .unwrap_or_default();
        let blocked = if host.is_empty() {
            net.blocked_total
        } else {
            *net.blocked_by_host.get(&host).unwrap_or(&0)
        };

        let tabs_json: Vec<Value> = st
            .strip
            .tabs
            .iter()
            .map(|t| {
                let grp = t.group_id.as_deref().and_then(|gid| st.strip.groups.get(gid));
                json!({
                    "id": t.id,
                    "title": t.title,
                    "url": t.url,
                    "pinned": t.pinned,
                    "sleeping": t.sleeping,
                    "group_name": grp.map(|g| g.name.clone()),
                    "group_color": grp.map(|g| g.color.clone()),
                })
            })
            .collect();

        let bookmarks: Vec<Value> = st
            .store
            .bookmarks()
            .into_iter()
            .map(|b| json!({"title": b.title, "url": b.url, "folder": b.folder}))
            .collect();
        let history: Vec<Value> = st
            .store
            .history("", 200)
            .into_iter()
            .map(|h| json!({"title": h.title, "url": h.url}))
            .collect();
        let reading_list: Vec<Value> = st
            .store
            .reading_list()
            .into_iter()
            .map(|r| json!({"title": r.title, "url": r.url, "read": r.read}))
            .collect();

        let selected_provider = st
            .store
            .get_str("selected")
            .unwrap_or_else(|| "gemini".to_string());
        let overrides = st.store.get("overrides").unwrap_or_else(|| json!({}));

        let mut selected_model = String::new();
        let providers_json: Vec<Value> = preset_providers()
            .into_iter()
            .map(|p| {
                let has_key = !p.needs_key || !resolve_provider_key(&st.store, &p).trim().is_empty();
                let ov_model = overrides
                    .get(&p.id)
                    .and_then(|o| o.get("model"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| p.model.clone());
                if p.id == selected_provider {
                    selected_model = ov_model.clone();
                }
                json!({
                    "id": p.id,
                    "name": p.name,
                    "model": ov_model,
                    "configured": has_key,
                })
            })
            .collect();

        let sets = instruction_sets(&st.store);
        let active_core = active_set(&st.store).map(|s| s.name).unwrap_or_default();
        let core_sets_json: Vec<Value> = sets
            .iter()
            .map(|s| json!({"name": s.name, "text": s.text}))
            .collect();

        let usage_text = format!(
            "Session: {} · {}",
            st.session_meter.short(),
            st.session_meter.cost_summary()
        );
        let home_cfg = HomepageConfig::load(&st.store);

        json!({
            "tabs": tabs_json,
            "active": st.strip.active_index,
            "url": url,
            "loading": entry.map(|e| e.loading).unwrap_or(false),
            "progress": entry.map(|e| e.progress).unwrap_or(0.0),
            "can_back": entry.map(|e| e.can_back).unwrap_or(false),
            "can_forward": entry.map(|e| e.can_forward).unwrap_or(false),
            "bookmarked": url != "rover:start" && st.store.is_bookmarked(&url),
            "bookmarks": bookmarks,
            "bookmarks_bar_visible": st.bookmarks_bar_visible,
            "find_open": st.find_open,
            "blocked": blocked,
            "adblock_enabled": net.blocker.is_enabled(),
            "domain_block_enabled": net.domain_block_enabled,
            "darkmode_enabled": net.darkmode.is_enabled(),
            "split": st.strip.split_secondary_id.is_some(),
            "panel_open": st.panel_open,
            "profile_name": if st.private { "Private" } else { "Personal" },
            "profile_color": if st.private { "#8E675C" } else { "#C83F3A" },
            "profile_dir": st.store.root.display().to_string(),
            "private": st.private,
            "search_engine": st.store.get_str("search_engine").unwrap_or_else(|| "duckduckgo".into()),
            "auto_group_tabs": st.store.get_bool("auto_group_tabs", false),
            "proxy_url": st.store.get_str("proxy_url").unwrap_or_default(),
            "home_show_logo": home_cfg.show_logo,
            "home_show_search": home_cfg.show_search,
            "home_show_shortcuts": home_cfg.show_shortcuts,
            "home_tagline": home_cfg.tagline,
            "history": history,
            "reading_list": reading_list,
            "providers": providers_json,
            "selected_provider": selected_provider,
            "selected_model": selected_model,
            "core_sets": core_sets_json,
            "core_active": active_core,
            "core_enabled": core_enabled(&st.store) && !sets.is_empty(),
            "messages": st.messages,
            "raw_log": st.raw_log,
            "ai_status": st.ai_status,
            "ai_busy": st.ai_busy,
            "pw_warning": st.pw_warning,
            "usage_text": usage_text,
            "context_pct": st.context_pct,
            "context_label": st.context_label,
            "opts": {
                "no_context": st.store.get_bool("no_context", false),
                "page_ctx": st.store.get_bool("page_ctx", true),
                "token_saver": st.store.get_bool("token_saver", true),
                "batch": st.store.get_bool("batch", false),
                "vision": st.store.get_bool("vision", false),
                "vision_every_step": st.store.get_bool("vision_every_step", false),
                "humanize": st.store.get_bool("humanize", false),
                "use_memory": st.store.get_bool("use_memory", false),
                "chat_text_px": st.store.get_i64("chat_text_px", 13),
            }
        })
    }

    fn push_ui_updates(&self) {
        let state_json = self.build_state_json().to_string();
        let (chrome, panel, page_title, private) = {
            let st = self.state.lock();
            let title = st
                .active_tab()
                .map(|t| t.title.clone())
                .unwrap_or_else(|| "New tab".into());
            (
                st.chrome_browser.clone(),
                st.panel_browser.clone(),
                title,
                st.private,
            )
        };
        if let Some(b) = chrome {
            if let Some(f) = b.main_frame() {
                let js = format!("window.roverUpdateChrome && window.roverUpdateChrome({state_json});");
                f.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
            }
        }
        if let Some(b) = panel {
            if let Some(f) = b.main_frame() {
                let js = format!("window.roverUpdatePanel && window.roverUpdatePanel({state_json});");
                f.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
            }
        }
        let main_hwnd = *self.main_hwnd.lock() as HWND;
        if !main_hwnd.is_null() {
            let suffix = if private { "Rover — Private" } else { "Rover" };
            let win_title = if page_title.is_empty() {
                suffix.to_string()
            } else {
                format!("{page_title} — {suffix}")
            };
            let wide = to_wide(&win_title);
            unsafe {
                SetWindowTextW(main_hwnd, wide.as_ptr());
            }
        }
    }

    fn chrome_height(&self) -> i32 {
        let st = self.state.lock();
        let mut h = BASE_CHROME_H;
        if st.bookmarks_bar_visible && !st.store.bookmarks().is_empty() {
            h += BM_BAR_H;
        }
        if st.find_open {
            h += FIND_BAR_H;
        }
        h
    }

    fn layout_windows(&self) {
        let main_hwnd = *self.main_hwnd.lock() as HWND;
        if main_hwnd.is_null() {
            return;
        }
        let mut rc: RECT = unsafe { std::mem::zeroed() };
        unsafe {
            GetClientRect(main_hwnd, &mut rc);
        }
        let total_w = (rc.right - rc.left).max(400);
        let total_h = (rc.bottom - rc.top).max(300);
        let chrome_h = self.chrome_height();

        let st = self.state.lock();
        let panel_w = if st.panel_open {
            PANEL_W.min(total_w / 2)
        } else {
            0
        };
        let content_w = (total_w - panel_w).max(200);
        let content_top = chrome_h;
        let content_total_h = (total_h - content_top).max(120);

        if let Some(ref b) = st.chrome_browser {
            if let Some(host) = b.host() {
                let h = cef_to_win_hwnd(host.window_handle());
                if !h.is_null() {
                    unsafe {
                        MoveWindow(h, 0, 0, total_w, chrome_h, 1);
                    }
                }
            }
        }

        if let Some(ref b) = st.panel_browser {
            if let Some(host) = b.host() {
                let h = cef_to_win_hwnd(host.window_handle());
                if !h.is_null() {
                    unsafe {
                        if st.panel_open {
                            SetWindowPos(
                                h,
                                ptr::null_mut(),
                                content_w,
                                content_top,
                                panel_w,
                                content_total_h,
                                SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                            );
                        } else {
                            ShowWindow(h, SW_HIDE);
                        }
                    }
                }
            }
        }

        let active_id = st.active_tab().map(|t| t.id);
        let split_id = st.strip.split_secondary_id;
        let dev_open = active_id
            .and_then(|id| st.tab_entries.get(&id))
            .map(|e| e.devtools_open)
            .unwrap_or(false);
        let dev_h = if dev_open {
            DEVTOOLS_MIN_H
                .max(content_total_h / 3)
                .min(content_total_h - 120)
        } else {
            0
        };
        let page_h = (content_total_h - dev_h).max(80);

        for (&tid, entry) in &st.tab_entries {
            let is_active = Some(tid) == active_id;
            let is_split = split_id.is_some() && Some(tid) == split_id && !is_active;

            if let Some(ref b) = entry.browser {
                if let Some(host) = b.host() {
                    let h = cef_to_win_hwnd(host.window_handle());
                    if !h.is_null() {
                        unsafe {
                            if is_active {
                                let w = if split_id.is_some() {
                                    content_w / 2
                                } else {
                                    content_w
                                };
                                SetWindowPos(
                                    h,
                                    ptr::null_mut(),
                                    0,
                                    content_top,
                                    w,
                                    page_h,
                                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                                );
                            } else if is_split {
                                let x = content_w / 2;
                                SetWindowPos(
                                    h,
                                    ptr::null_mut(),
                                    x,
                                    content_top,
                                    content_w - x,
                                    page_h,
                                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                                );
                            } else {
                                SetWindowPos(
                                    h,
                                    ptr::null_mut(),
                                    0,
                                    0,
                                    0,
                                    0,
                                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_HIDEWINDOW,
                                );
                            }
                        }
                    }
                }
            }

            if let Some(ref db) = entry.devtools_browser {
                if let Some(dhost) = db.host() {
                    let dh = cef_to_win_hwnd(dhost.window_handle());
                    if !dh.is_null() {
                        unsafe {
                            if is_active && entry.devtools_open {
                                SetWindowPos(
                                    dh,
                                    ptr::null_mut(),
                                    0,
                                    content_top + page_h,
                                    content_w,
                                    dev_h,
                                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                                );
                            } else {
                                ShowWindow(dh, SW_HIDE);
                            }
                        }
                    }
                }
            }
        }
    }

    fn create_tab_browser(&self, tab_id: usize, url: &str) {
        let main_hwnd = *self.main_hwnd.lock() as HWND;
        if main_hwnd.is_null() {
            return;
        }
        let cef_url = self.map_to_cef_url(url);
        let mut rc: RECT = unsafe { std::mem::zeroed() };
        unsafe {
            GetClientRect(main_hwnd, &mut rc);
        }
        let chrome_h = self.chrome_height();
        let panel_w = if self.state.lock().panel_open { PANEL_W } else { 0 };
        let w = ((rc.right - rc.left) - panel_w).max(200);
        let h = ((rc.bottom - rc.top) - chrome_h).max(120);

        let mut win_info = WindowInfo::default().set_as_child(
            win_to_cef_hwnd(main_hwnd),
            &CefRect {
                x: 0,
                y: chrome_h,
                width: w,
                height: h,
            },
        );
        win_info.runtime_style = RuntimeStyle::ALLOY;

        let mut bs = BrowserSettings::default();
        bs.background_color = 0xFF160708;

        let mut client = TabCefClient::new(tab_id);
        let cef_url_s = CefString::from(cef_url.as_str());
        browser_host_create_browser(
            Some(&win_info),
            Some(&mut client),
            Some(&cef_url_s),
            Some(&bs),
            None,
            None,
        );
    }

    fn open_new_tab(&self, url: Option<&str>) {
        let target = url.unwrap_or("rover:start");
        let title = if target == "rover:start" {
            "New tab"
        } else {
            target
        };
        let tab_id = {
            let mut st = self.state.lock();
            let id = st.strip.open_tab(target, title);
            st.tab_entries.insert(id, TabBrowserEntry::default());
            id
        };
        self.create_tab_browser(tab_id, target);
        self.layout_windows();
        self.push_ui_updates();
    }

    fn close_tab_at(&self, idx: usize) {
        let (closed_entry, should_exit) = {
            let mut st = self.state.lock();
            if st.strip.tabs.len() <= 1 {
                (None, true)
            } else {
                let tid = st.strip.tabs.get(idx).map(|t| t.id);
                st.strip.close_index(idx);
                let entry = tid.and_then(|id| st.tab_entries.remove(&id));
                (entry, false)
            }
        };
        if should_exit {
            let h = *self.main_hwnd.lock() as HWND;
            if !h.is_null() {
                unsafe {
                    PostMessageW(h, WM_CLOSE, 0, 0);
                }
            }
            return;
        }
        if let Some(entry) = closed_entry {
            if let Some(db) = entry.devtools_browser {
                if let Some(h) = db.host() {
                    h.close_browser(1);
                }
            }
            if let Some(b) = entry.browser {
                if let Some(h) = b.host() {
                    h.close_browser(1);
                }
            }
        }
        self.layout_windows();
        self.push_ui_updates();
    }

    fn navigate_active(&self, raw: &str) {
        let (tab_id, target, browser_opt) = {
            let mut st = self.state.lock();
            let engine = st
                .store
                .get_str("search_engine")
                .unwrap_or_else(|| "duckduckgo".into());
            let target = normalize_address(raw, &engine);
            let tid = match st.active_tab_mut() {
                Some(t) => {
                    t.url = target.clone();
                    if target == "rover:start" {
                        t.title = "New tab".into();
                    }
                    t.id
                }
                None => return,
            };
            let b = st.tab_entries.get(&tid).and_then(|e| e.browser.clone());
            (tid, target, b)
        };
        let cef_url = self.map_to_cef_url(&target);
        if let Some(b) = browser_opt {
            if let Some(f) = b.main_frame() {
                f.load_url(Some(&CefString::from(cef_url.as_str())));
            }
        } else {
            self.create_tab_browser(tab_id, &target);
        }
        self.push_ui_updates();
    }

    fn toggle_devtools(&self, inspect_element: bool) {
        let main_hwnd = *self.main_hwnd.lock() as HWND;
        if main_hwnd.is_null() {
            return;
        }
        let (tab_id, browser_opt, was_open) = {
            let mut st = self.state.lock();
            let Some(tid) = st.active_tab().map(|t| t.id) else {
                return;
            };
            let entry = st.tab_entries.entry(tid).or_default();
            let was = entry.devtools_open;
            if was && !inspect_element {
                entry.devtools_open = false;
                if let Some(ref b) = entry.browser {
                    if let Some(host) = b.host() {
                        host.close_dev_tools();
                    }
                }
                entry.devtools_browser = None;
                (tid, None, true)
            } else {
                entry.devtools_open = true;
                (tid, entry.browser.clone(), was)
            }
        };

        if !was_open {
            if let Some(b) = browser_opt {
                if let Some(host) = b.host() {
                    let mut rc: RECT = unsafe { std::mem::zeroed() };
                    unsafe {
                        GetClientRect(main_hwnd, &mut rc);
                    }
                    let total_w = (rc.right - rc.left).max(400);
                    let total_h = (rc.bottom - rc.top).max(300);
                    let chrome_h = self.chrome_height();
                    let panel_w = if self.state.lock().panel_open { PANEL_W } else { 0 };
                    let content_w = (total_w - panel_w).max(200);
                    let content_h = (total_h - chrome_h).max(200);
                    let dev_h = DEVTOOLS_MIN_H.max(content_h / 3);
                    let mut win_info = WindowInfo::default().set_as_child(
                        win_to_cef_hwnd(main_hwnd),
                        &CefRect {
                            x: 0,
                            y: chrome_h + (content_h - dev_h),
                            width: content_w,
                            height: dev_h,
                        },
                    );
                    win_info.runtime_style = RuntimeStyle::ALLOY;
                    let mut dev_client = DevToolsCefClient::new(tab_id);
                    let bs = BrowserSettings::default();
                    let pt = Point { x: 0, y: 0 };
                    host.show_dev_tools(
                        Some(&win_info),
                        Some(&mut dev_client),
                        Some(&bs),
                        if inspect_element { Some(&pt) } else { None },
                    );
                }
            }
        }
        self.layout_windows();
    }

    fn open_dialog(&self, kind: &str, param: Option<&str>) {
        let main_hwnd = *self.main_hwnd.lock() as HWND;
        let url = match param {
            Some(p) if !p.is_empty() => format!(
                "{}/ui/dialog/{}?t={}&p={}",
                self.base_url(),
                kind,
                self.token,
                p
            ),
            _ => format!("{}/ui/dialog/{}?t={}", self.base_url(), kind, self.token),
        };
        let title = match kind {
            "settings" => "Settings — Rover",
            "bookmarks" => "Bookmarks — Rover",
            "history" => "History — Rover",
            "reading_list" => "Reading list — Rover",
            "passwords" => "Passwords — Rover",
            "autofill" => "Addresses and cards — Rover",
            "usage" => "Token usage — Rover",
            "automations" => "Automations — Rover",
            "always_on" => "Always-on instructions — Rover",
            "shortcut" => "Add shortcut — Rover",
            "downloads" => "Downloads — Rover",
            _ => "About Rover",
        };
        let (w, h) = match kind {
            "shortcut" | "about" => (460, 320),
            "usage" => (520, 340),
            _ => (740, 560),
        };

        let class_name = to_wide("RoverDialogWindow");
        let title_w = to_wide(title);
        let hinst = unsafe { GetModuleHandleW(ptr::null()) };
        let dlg_hwnd = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                title_w.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                w,
                h,
                main_hwnd,
                ptr::null_mut(),
                hinst,
                ptr::null(),
            )
        };
        if !dlg_hwnd.is_null() {
            apply_dark_titlebar(dlg_hwnd);
            let mut rc: RECT = unsafe { std::mem::zeroed() };
            unsafe {
                GetClientRect(dlg_hwnd, &mut rc);
            }
            let mut wi = WindowInfo::default().set_as_child(
                win_to_cef_hwnd(dlg_hwnd),
                &CefRect {
                    x: 0,
                    y: 0,
                    width: (rc.right - rc.left).max(200),
                    height: (rc.bottom - rc.top).max(150),
                },
            );
            wi.runtime_style = RuntimeStyle::ALLOY;
            let mut bs = BrowserSettings::default();
            bs.background_color = 0xFF160708;
            let mut client = UiCefClient::new(UiRole::Dialog(dlg_hwnd as isize));
            browser_host_create_browser(
                Some(&wi),
                Some(&mut client),
                Some(&CefString::from(url.as_str())),
                Some(&bs),
                None,
                None,
            );
            self.state.lock().dialog_hwnd = dlg_hwnd as isize;
        }
    }

    fn show_native_menu(&self, kind: &str, client_x: i32, client_y: i32) {
        let main_hwnd = *self.main_hwnd.lock() as HWND;
        if main_hwnd.is_null() {
            return;
        }
        let mut pt = POINT {
            x: client_x,
            y: client_y,
        };
        unsafe {
            ClientToScreen(main_hwnd, &mut pt);
            SetForegroundWindow(main_hwnd);
        }

        let cmd_id: u32 = unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return;
            }
            let add_item = |m: *mut core::ffi::c_void, id: usize, flags: u32, text: &str| {
                let w = to_wide(text);
                AppendMenuW(m, flags, id, w.as_ptr());
            };
            let add_sep = |m: *mut core::ffi::c_void| {
                AppendMenuW(m, MF_SEPARATOR, 0, ptr::null());
            };

            match kind {
                "main" => {
                    let (auto_grp, bm_vis, cur_ws) = {
                        let st = self.state.lock();
                        (
                            st.store.get_bool("auto_group_tabs", false),
                            st.bookmarks_bar_visible,
                            st.strip.current_workspace.clone(),
                        )
                    };
                    add_item(menu, 101, MF_STRING, "New tab\tCtrl+T");
                    add_item(menu, 102, MF_STRING, "New window\tCtrl+N");
                    add_item(menu, 103, MF_STRING, "New private window\tCtrl+Shift+N");
                    add_item(menu, 104, MF_STRING, "Reopen closed tab\tCtrl+Shift+T");
                    add_item(menu, 105, MF_STRING, "Split view\tCtrl+Shift+E");
                    add_item(menu, 106, MF_STRING, "Group tabs by site");
                    add_item(
                        menu,
                        107,
                        MF_STRING | if auto_grp { MF_CHECKED } else { MF_UNCHECKED },
                        "Keep tabs grouped by site",
                    );
                    let ws_sub = CreatePopupMenu();
                    for (i, name) in ["Main", "Work", "Research"].iter().enumerate() {
                        add_item(
                            ws_sub,
                            180 + i,
                            MF_STRING | if cur_ws == *name { MF_CHECKED } else { MF_UNCHECKED },
                            name,
                        );
                    }
                    let ws_title = to_wide(&format!("Workspace: {cur_ws}"));
                    AppendMenuW(menu, MF_POPUP, ws_sub as usize, ws_title.as_ptr());
                    add_sep(menu);
                    add_item(menu, 110, MF_STRING, "Bookmarks…\tCtrl+Shift+O");
                    add_item(menu, 111, MF_STRING, "History…\tCtrl+H");
                    add_item(menu, 112, MF_STRING, "Reading list…");
                    add_item(menu, 113, MF_STRING, "Add page to reading list\tAlt+Shift+R");
                    add_item(menu, 114, MF_STRING, "Downloads…\tCtrl+J");
                    add_item(menu, 115, MF_STRING, "Passwords…");
                    add_item(menu, 116, MF_STRING, "Addresses and cards…");
                    add_sep(menu);
                    add_item(menu, 120, MF_STRING, "Find in page\tCtrl+F");
                    add_item(menu, 121, MF_STRING, "Reader mode\tF9");
                    add_item(menu, 122, MF_STRING, "Zoom in\tCtrl++");
                    add_item(menu, 123, MF_STRING, "Zoom out\tCtrl+-");
                    add_item(menu, 124, MF_STRING, "Reset zoom\tCtrl+0");
                    add_item(menu, 125, MF_STRING, "Print…\tCtrl+P");
                    add_item(menu, 126, MF_STRING, "Save page as PDF…");
                    add_item(
                        menu,
                        127,
                        MF_STRING | if bm_vis { MF_CHECKED } else { MF_UNCHECKED },
                        "Show bookmarks bar\tCtrl+Shift+B",
                    );
                    add_sep(menu);
                    add_item(menu, 130, MF_STRING, "Developer tools\tCtrl+Shift+I");
                    add_item(menu, 131, MF_STRING, "Inspect element\tCtrl+Shift+C");
                    add_item(menu, 132, MF_STRING, "View page source\tCtrl+U");
                    add_sep(menu);
                    add_item(menu, 140, MF_STRING, "Rover AI panel\tCtrl+Shift+A");
                    add_item(menu, 141, MF_STRING, "Automations…");
                    add_item(menu, 142, MF_STRING, "Token usage…");
                    add_item(menu, 143, MF_STRING, "Settings…\tCtrl+,");
                    add_sep(menu);
                    add_item(menu, 150, MF_STRING, "About Rover");
                }
                "extensions" => {
                    let (ad_on, dom_on, dark_on, blocked) = {
                        let net = self.net.lock();
                        (
                            net.blocker.is_enabled(),
                            net.domain_block_enabled,
                            net.darkmode.is_enabled(),
                            net.blocked_total,
                        )
                    };
                    add_item(
                        menu,
                        0,
                        MF_STRING | MF_GRAYED,
                        &format!("Rover Shield — {blocked} blocked (2.00 µs/req)"),
                    );
                    add_sep(menu);
                    add_item(
                        menu,
                        201,
                        MF_STRING | if ad_on { MF_CHECKED } else { MF_UNCHECKED },
                        "Ad & tracker blocker",
                    );
                    add_item(
                        menu,
                        202,
                        MF_STRING | if dom_on { MF_CHECKED } else { MF_UNCHECKED },
                        "Pi-hole domain blocklist",
                    );
                    add_item(
                        menu,
                        203,
                        MF_STRING | if dark_on { MF_CHECKED } else { MF_UNCHECKED },
                        "Force dark mode (rover:darkmode)",
                    );
                    add_sep(menu);
                    add_item(menu, 204, MF_STRING, "Privacy & adblock settings…");
                }
                "profile" => {
                    let priv_mode = self.state.lock().private;
                    add_item(
                        menu,
                        0,
                        MF_STRING | MF_GRAYED,
                        if priv_mode {
                            "● Private window"
                        } else {
                            "● Personal"
                        },
                    );
                    add_sep(menu);
                    add_item(menu, 102, MF_STRING, "New window");
                    add_item(menu, 103, MF_STRING, "New private window");
                    add_sep(menu);
                    add_item(menu, 143, MF_STRING, "Settings…");
                }
                "tabsearch" => {
                    let titles: Vec<String> = self
                        .state
                        .lock()
                        .strip
                        .tabs
                        .iter()
                        .map(|t| t.title.clone())
                        .collect();
                    for (i, t) in titles.iter().enumerate() {
                        add_item(menu, 300 + i, MF_STRING, t);
                    }
                }
                _ => {}
            }

            let chosen = TrackPopupMenuEx(
                menu,
                TPM_RETURNCMD | TPM_RIGHTALIGN | TPM_TOPALIGN | TPM_LEFTBUTTON,
                pt.x,
                pt.y,
                main_hwnd,
                ptr::null(),
            ) as u32;
            DestroyMenu(menu);
            chosen
        };

        if cmd_id == 0 {
            return;
        }
        self.dispatch_menu_id(cmd_id);
    }

    fn dispatch_menu_id(&self, id: u32) {
        match id {
            101 => self.open_new_tab(None),
            102 => {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).spawn();
                }
            }
            103 => {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).arg("--private").spawn();
                }
            }
            104 => {
                let reopened = self.state.lock().strip.reopen_closed();
                if let Some(tab_id) = reopened {
                    let url = self
                        .state
                        .lock()
                        .strip
                        .tabs
                        .iter()
                        .find(|t| t.id == tab_id)
                        .map(|t| t.url.clone())
                        .unwrap_or_else(|| "rover:start".into());
                    self.state
                        .lock()
                        .tab_entries
                        .insert(tab_id, TabBrowserEntry::default());
                    self.create_tab_browser(tab_id, &url);
                    self.layout_windows();
                    self.push_ui_updates();
                }
            }
            105 => self.toggle_split_view(),
            106 => {
                self.state.lock().strip.group_by_site();
                self.push_ui_updates();
            }
            107 => {
                let st = self.state.lock();
                let cur = st.store.get_bool("auto_group_tabs", false);
                st.store.set("auto_group_tabs", json!(!cur));
                drop(st);
                self.push_ui_updates();
            }
            180..=182 => {
                let ws = ["Main", "Work", "Research"][(id - 180) as usize];
                self.state.lock().strip.switch_workspace(ws);
                self.layout_windows();
                self.push_ui_updates();
            }
            110 => self.open_dialog("bookmarks", None),
            111 => self.open_dialog("history", None),
            112 => self.open_dialog("reading_list", None),
            113 => {
                let st = self.state.lock();
                if let Some(t) = st.active_tab().cloned() {
                    if t.url.starts_with("http://") || t.url.starts_with("https://") {
                        st.store.add_reading(&t.url, &t.title);
                    }
                }
            }
            114 => self.open_dialog("downloads", None),
            115 => self.open_dialog("passwords", None),
            116 => self.open_dialog("autofill", None),
            120 => self.toggle_find(true),
            121 => self.trigger_reader_mode(),
            122 => self.adjust_zoom(0.5),
            123 => self.adjust_zoom(-0.5),
            124 => self.adjust_zoom(0.0),
            125 => {
                if let Some(b) = self.active_browser() {
                    if let Some(h) = b.host() {
                        h.print();
                    }
                }
            }
            126 => {
                if let Some(b) = self.active_browser() {
                    if let Some(h) = b.host() {
                        let out = self.state.lock().store.root.join("page.pdf");
                        let pdf_settings = PdfPrintSettings::default();
                        h.print_to_pdf(
                            Some(&CefString::from(out.to_string_lossy().as_ref())),
                            Some(&pdf_settings),
                            None,
                        );
                    }
                }
            }
            127 => {
                let mut st = self.state.lock();
                st.bookmarks_bar_visible = !st.bookmarks_bar_visible;
                let v = st.bookmarks_bar_visible;
                st.store.set("bookmarks_bar", json!(v));
                drop(st);
                self.layout_windows();
                self.push_ui_updates();
            }
            130 => self.toggle_devtools(false),
            131 => self.toggle_devtools(true),
            132 => {
                let cur_url = self
                    .state
                    .lock()
                    .active_tab()
                    .map(|t| t.url.clone())
                    .unwrap_or_default();
                if cur_url.starts_with("http://") || cur_url.starts_with("https://") {
                    self.open_new_tab(Some(&format!("view-source:{cur_url}")));
                }
            }
            140 => {
                let mut st = self.state.lock();
                st.panel_open = !st.panel_open;
                let v = st.panel_open;
                st.store.set("panel", json!(v));
                drop(st);
                self.layout_windows();
                self.push_ui_updates();
            }
            141 => self.open_dialog("automations", None),
            142 => self.open_dialog("usage", None),
            143 => self.open_dialog("settings", Some("general")),
            150 => self.open_dialog("about", None),
            201 => {
                let net = self.net.lock();
                let next = !net.blocker.is_enabled();
                net.blocker.set_enabled(next);
                drop(net);
                self.push_ui_updates();
            }
            202 => {
                let mut net = self.net.lock();
                net.domain_block_enabled = !net.domain_block_enabled;
                drop(net);
                self.push_ui_updates();
            }
            203 => {
                let net = self.net.lock();
                let next = !net.darkmode.is_enabled();
                net.darkmode.set_enabled(next);
                drop(net);
                self.push_ui_updates();
            }
            204 => self.open_dialog("settings", Some("privacy")),
            300..=399 => {
                let idx = (id - 300) as usize;
                self.state.lock().strip.select_index(idx);
                self.layout_windows();
                self.push_ui_updates();
            }
            _ => {}
        }
    }

    fn active_browser(&self) -> Option<Browser> {
        let st = self.state.lock();
        let tid = st.active_tab()?.id;
        st.tab_entries.get(&tid)?.browser.clone()
    }

    fn toggle_split_view(&self) {
        let mut st = self.state.lock();
        if st.strip.split_secondary_id.is_some() {
            st.strip.end_split();
        } else if st.strip.tabs.len() >= 2 {
            let other = if st.strip.active_index == 0 { 1 } else { 0 };
            st.strip.set_split(other);
        }
        drop(st);
        self.layout_windows();
        self.push_ui_updates();
    }

    fn toggle_find(&self, show: bool) {
        let chrome = {
            let mut st = self.state.lock();
            st.find_open = show;
            st.chrome_browser.clone()
        };
        if !show {
            if let Some(b) = self.active_browser() {
                if let Some(h) = b.host() {
                    h.stop_finding(1);
                }
            }
        }
        self.layout_windows();
        self.push_ui_updates();
        if show {
            if let Some(c) = chrome {
                if let Some(f) = c.main_frame() {
                    f.execute_java_script(
                        Some(&CefString::from(
                            "window.roverFocusFind && window.roverFocusFind();",
                        )),
                        None,
                        0,
                    );
                }
            }
        }
    }

    fn adjust_zoom(&self, delta: f64) {
        let (b_opt, new_zoom) = {
            let mut st = self.state.lock();
            let Some(tid) = st.active_tab().map(|t| t.id) else {
                return;
            };
            let entry = st.tab_entries.entry(tid).or_default();
            entry.zoom = if delta == 0.0 {
                0.0
            } else {
                (entry.zoom + delta).clamp(-3.0, 5.0)
            };
            (entry.browser.clone(), entry.zoom)
        };
        if let Some(b) = b_opt {
            if let Some(h) = b.host() {
                h.set_zoom_level(new_zoom);
            }
        }
    }

    fn trigger_reader_mode(&self) {
        let (tab_id, url, b_opt) = {
            let st = self.state.lock();
            let Some(t) = st.active_tab() else {
                return;
            };
            let b = st.tab_entries.get(&t.id).and_then(|e| e.browser.clone());
            (t.id, t.url.clone(), b)
        };
        if let Some(b) = b_opt {
            if let Some(f) = b.main_frame() {
                let cb_id = self.next_cb_id.fetch_add(1, Ordering::Relaxed);
                self.state
                    .lock()
                    .pending_js
                    .insert(cb_id, PendingJsKind::ReaderExtract { tab_id, url });
                let js = format!(
                    "try{{console.debug('__ROVER_CB__:'+JSON.stringify({{id:{cb_id},payload:document.documentElement.outerHTML}}));}}catch(e){{}}"
                );
                f.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
            }
        }
    }

    fn handle_ai_prompt(&self, text: String, bypass_pwguard: bool) {
        if !bypass_pwguard {
            if let Some(reason) = pwguard::check(&text) {
                let mut st = self.state.lock();
                st.held_prompt = text;
                st.pw_warning = format!(
                    "Held back: {reason}. Click Send anyway if you're sure."
                );
                drop(st);
                self.push_ui_updates();
                return;
            }
        }

        let current_url = {
            let mut st = self.state.lock();
            st.pw_warning.clear();
            st.held_prompt.clear();
            st.messages.push(ChatMsg {
                who: "You".into(),
                detail: String::new(),
                text: text.clone(),
                kind: "user".into(),
            });
            st.active_tab()
                .map(|t| t.url.clone())
                .unwrap_or_else(|| "rover:start".into())
        };
        self.push_ui_updates();

        // 1. Check zero-token Rover Local commands first ($0.00, instant!)
        match parse_local_command(&text, &current_url) {
            LocalCommand::Help { text: help_txt } => {
                self.append_ai_msg("Rover Local", "0 tokens", &help_txt, "ai");
                return;
            }
            LocalCommand::Steps { steps } if !steps.is_empty() => {
                let mut summary = Vec::new();
                for s in steps {
                    match s.action.as_str() {
                        "navigate" => {
                            if let Some(u) = s.args.get("url").and_then(|v| v.as_str()) {
                                self.navigate_active(u);
                                summary.push(format!("Opened `{u}`."));
                            }
                        }
                        "search" => {
                            if let Some(q) = s.args.get("query").and_then(|v| v.as_str()) {
                                self.navigate_active(q);
                                summary.push(format!("Searching for **{q}**."));
                            }
                        }
                        "new_tab" => {
                            self.open_new_tab(s.args.get("url").and_then(|v| v.as_str()));
                            summary.push("Opened a new tab.".into());
                        }
                        "back" => {
                            if let Some(b) = self.active_browser() {
                                b.go_back();
                            }
                            summary.push("Went back.".into());
                        }
                        "forward" => {
                            if let Some(b) = self.active_browser() {
                                b.go_forward();
                            }
                            summary.push("Went forward.".into());
                        }
                        "reload" => {
                            if let Some(b) = self.active_browser() {
                                b.reload();
                            }
                            summary.push("Reloaded the page.".into());
                        }
                        "group_by_site" => {
                            self.state.lock().strip.group_by_site();
                            self.push_ui_updates();
                            summary.push("Grouped tabs by site.".into());
                        }
                        _ => {}
                    }
                }
                if !summary.is_empty() {
                    self.append_ai_msg("Rover Local", "0 tokens", &summary.join(" "), "ai");
                    return;
                }
            }
            _ => {}
        }

        // 2. Live Autonomous Rover Agent in Pure Chromium
        let (provider_opt, api_key, tab_id, cfg, core_instr, mem_text) = {
            let st = self.state.lock();
            let pid = st
                .store
                .get_str("selected")
                .unwrap_or_else(|| "gemini".into());
            let mut prov = preset_providers().into_iter().find(|p| p.id == pid);
            if let Some(ref mut p) = prov {
                if let Some(ov_m) = st
                    .store
                    .get("overrides")
                    .and_then(|o| o.get(&p.id).cloned())
                    .and_then(|o| o.get("model").and_then(|m| m.as_str().map(|s| s.to_string())))
                {
                    p.model = ov_m;
                }
                p.token_saver = st.store.get_bool("token_saver", true);
            }
            let key = prov
                .as_ref()
                .map(|p| resolve_provider_key(&st.store, p))
                .unwrap_or_default();
            let tid = st.active_tab().map(|t| t.id).unwrap_or(0);
            let cfg = AgentConfig {
                token_saver: st.store.get_bool("token_saver", true),
                no_context: st.store.get_bool("no_context", false),
                batch: st.store.get_bool("batch", false),
                vision: st.store.get_bool("vision", false),
                vision_every_step: st.store.get_bool("vision_every_step", false),
                ..AgentConfig::default()
            };
            let core_instr = core_system(&st.store);
            let use_mem = st.store.get_bool("use_memory", false);
            let mem_text = if use_mem {
                st.memory.read(true).unwrap_or_default()
            } else {
                String::new()
            };
            (prov, key, tid, cfg, core_instr, mem_text)
        };

        let Some(provider) = provider_opt else {
            self.append_ai_msg(
                "Rover",
                "",
                "No AI provider selected. Click ⚙ next to Provider to configure one.",
                "err",
            );
            return;
        };
        if provider.needs_key && api_key.trim().is_empty() {
            self.append_ai_msg(
                "Rover",
                "",
                &format!(
                    "**{}** needs an API key. Click **⚙** next to *Provider* to paste your key (Gemini and Groq offer free tiers).",
                    provider.name
                ),
                "err",
            );
            return;
        }

        let task_id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        let agent_state = AgentState::new(&text, cfg);

        {
            let mut st = self.state.lock();
            st.ai_busy = true;
            st.ai_status = format!("Reading page for {}…", provider.name);
            st.active_agent = Some(ActiveAgentRun {
                task_id,
                state: agent_state,
                differ: SnapshotDiffer::new(),
                provider,
                api_key,
                core_instructions: core_instr,
                memory_text: mem_text,
                tab_id,
                steps: 0,
            });
        }
        self.push_ui_updates();
        self.request_agent_snapshot(task_id, tab_id);
    }

    fn request_agent_snapshot(&self, task_id: u64, tab_id: usize) {
        let b_opt = self
            .state
            .lock()
            .tab_entries
            .get(&tab_id)
            .and_then(|e| e.browser.clone());
        let Some(b) = b_opt else {
            self.stop_agent("Tab closed.");
            return;
        };
        if let Some(f) = b.main_frame() {
            let cb_id = self.next_cb_id.fetch_add(1, Ordering::Relaxed);
            self.state
                .lock()
                .pending_js
                .insert(cb_id, PendingJsKind::AgentSnapshot { task_id });
            let g_js = glow_js(true);
            let snap_js = pagejs::snapshot_script();
            let code = format!(
                "(function(){{try{{{g_js}var r={snap_js};console.debug('__ROVER_CB__:'+JSON.stringify({{id:{cb_id},payload:typeof r==='string'?r:JSON.stringify(r)}}));}}catch(e){{console.debug('__ROVER_CB__:'+JSON.stringify({{id:{cb_id},payload:JSON.stringify({{url:location.href,title:document.title,ready:document.readyState,scrollY:0,scrollMax:0,vw:innerWidth,vh:innerHeight,items:[],visual:false,text:(document.body?document.body.innerText:'').slice(0,2500)}})}}));}}}})();"
            );
            f.execute_java_script(Some(&CefString::from(code.as_str())), None, 0);
        }
    }

    fn on_agent_snapshot(&self, task_id: u64, raw_json: String) {
        let dispatch = {
            let mut st = self.state.lock();
            let tabs_summary: Vec<TabSummary> = st
                .strip
                .tabs
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let gname = t
                        .group_id
                        .as_deref()
                        .and_then(|gid| st.strip.groups.get(gid))
                        .map(|g| g.name.clone());
                    TabSummary {
                        index: i,
                        active: i == st.strip.active_index,
                        title: t.title.clone(),
                        group_name: gname,
                    }
                })
                .collect();
            let meter = st.session_meter.clone();
            let (provider, api_key, sys, msgs, opt_chars) = {
                let Some(run) = st.active_agent.as_mut() else {
                    return;
                };
                if run.task_id != task_id {
                    return;
                }
                let snap: Option<PageSnapshot> = serde_json::from_str(&raw_json).ok();
                let obs = if let Some(ref s) = snap {
                    let (delta_obs, used_delta) =
                        run.differ.format_delta(s, run.state.config.token_saver);
                    if used_delta {
                        delta_obs
                    } else {
                        format_snapshot(Some(s), &tabs_summary, "", run.state.config.token_saver)
                    }
                } else {
                    "Page snapshot unavailable.".to_string()
                };
                let msgs = run.state.build_messages(&obs);
                let opt_chars: usize = msgs.iter().map(|m| m.content.len()).sum();
                let mut sys = system_prompt(run.state.config.batch);
                if !run.core_instructions.is_empty() {
                    sys.push_str("\n\n");
                    sys.push_str(&run.core_instructions);
                }
                if !run.memory_text.is_empty() {
                    sys.push_str("\n\nUser saved notes:\n");
                    sys.push_str(&run.memory_text);
                }
                (run.provider.clone(), run.api_key.clone(), sys, msgs, opt_chars)
            };
            st.context_pct = ((opt_chars as f64 / 120_000.0) * 100.0).clamp(1.0, 100.0) as u32;
            st.context_label = format!("Context: ~{} tokens", opt_chars / 4);
            st.ai_status = format!("Asking {} ({})…", provider.name, provider.model);
            (provider, api_key, sys, msgs, meter)
        };
        self.push_ui_updates();

        let (provider, api_key, sys, msgs, meter) = dispatch;
        let prompt_preview = msgs.last().map(|m| m.content.clone()).unwrap_or_default();
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            let reply = chat_sync(&provider, &api_key, &msgs, &sys, &[], 1200, Some(&meter));
            let elapsed = t0.elapsed().as_secs_f64();
            if let Some(rt) = GLOBAL_APP.get() {
                rt.post(RoverCmd::AgentReply {
                    task_id,
                    reply,
                    elapsed_secs: elapsed,
                    system_sent: sys,
                    prompt_sent: prompt_preview,
                    provider_name: provider.name.clone(),
                    model_name: provider.model.clone(),
                });
            }
        });
    }

    fn on_agent_reply(
        &self,
        task_id: u64,
        reply: Result<String, String>,
        elapsed_secs: f64,
        system_sent: String,
        prompt_sent: String,
        provider_name: String,
        model_name: String,
    ) {
        {
            let mut st = self.state.lock();
            let stamp = chrono::Local::now().format("%H:%M:%S");
            let reply_str = match &reply {
                Ok(s) => format!("▶ RECEIVED: raw reply\n{s}\n"),
                Err(e) => format!("▶ ERROR\n{e}\n"),
            };
            st.raw_log.push_str(&format!(
                "━━━━ {stamp} · {provider_name} · {model_name} · {elapsed_secs:.1}s ━━━━\n\
                 ▶ SENT: instructions (system)\n{}\n\
                 ▶ SENT: user\n{}\n{}\n",
                &system_sent[..system_sent.len().min(2000)],
                &prompt_sent[..prompt_sent.len().min(4000)],
                reply_str
            ));
            st.usage_log.record(
                "agent",
                &provider_name,
                &model_name,
                "",
                &st.session_meter,
                if reply.is_ok() { "ok" } else { "error" },
            );
        }

        let reply_text = match reply {
            Ok(s) => s,
            Err(e) => {
                self.stop_agent(&format!("Error: {e}"));
                self.append_ai_msg(&provider_name, &model_name, &e, "err");
                return;
            }
        };

        let Some((_thought, steps)) = parse_plan(&reply_text, 6) else {
            self.stop_agent("");
            self.append_ai_msg(&provider_name, &model_name, &reply_text, "ai");
            return;
        };
        if steps.is_empty() {
            self.stop_agent("");
            self.append_ai_msg(&provider_name, &model_name, &reply_text, "ai");
            return;
        }

        let step = steps[0].clone();
        match step.action.as_str() {
            "done" | "ask_user" | "ask" => {
                let answer = step
                    .args
                    .get("answer")
                    .or_else(|| step.args.get("question"))
                    .or_else(|| step.args.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&reply_text)
                    .to_string();
                self.stop_agent("");
                self.append_ai_msg(&provider_name, &model_name, &answer, "ai");
            }
            "navigate" | "goto" => {
                if let Some(url) = step.args.get("url").and_then(|v| v.as_str()) {
                    self.append_ai_msg("", "", &format!("→ Opening {url}"), "step");
                    self.navigate_active(url);
                    if let Some(tid) =
                        self.record_agent_turn(task_id, &reply_text, &format!("Navigated to {url}."))
                    {
                        self.schedule_next_snapshot(task_id, tid, 1200);
                    }
                } else {
                    self.stop_agent("");
                }
            }
            "search" => {
                if let Some(q) = step.args.get("query").and_then(|v| v.as_str()) {
                    self.append_ai_msg("", "", &format!("→ Searching: {q}"), "step");
                    self.navigate_active(q);
                    if let Some(tid) =
                        self.record_agent_turn(task_id, &reply_text, &format!("Searched for {q}."))
                    {
                        self.schedule_next_snapshot(task_id, tid, 1200);
                    }
                } else {
                    self.stop_agent("");
                }
            }
            "click" | "type" | "scroll" => {
                let desc = format!("→ {} {}", step.action, step.args);
                self.append_ai_msg("", "", &desc, "step");
                if let Some(tid) = self.record_agent_turn(
                    task_id,
                    &reply_text,
                    &format!("Executed {}.", step.action),
                ) {
                    let js_action = match step.action.as_str() {
                        "click" => {
                            let id = step.args.get("id").and_then(|v| v.as_u64()).map(|n| n as usize);
                            let target = step.args.get("target").and_then(|v| v.as_str());
                            let xy = match (
                                step.args.get("x").and_then(|v| v.as_f64()),
                                step.args.get("y").and_then(|v| v.as_f64()),
                            ) {
                                (Some(x), Some(y)) => Some((x, y)),
                                _ => None,
                            };
                            pagejs::click_script(id, target, xy)
                        }
                        "type" => {
                            let id = step.args.get("id").and_then(|v| v.as_u64()).map(|n| n as usize);
                            let target = step.args.get("target").and_then(|v| v.as_str());
                            let txt = step.args.get("text").and_then(|v| v.as_str()).unwrap_or("");
                            let submit = step.args.get("submit").and_then(|v| v.as_bool()).unwrap_or(false);
                            pagejs::type_script(id, target, txt, submit)
                        }
                        _ => {
                            let dir = step.args.get("direction").and_then(|v| v.as_str()).unwrap_or("down");
                            let pages = step.args.get("pages").and_then(|v| v.as_f64()).unwrap_or(1.0);
                            pagejs::scroll_script(dir, pages)
                        }
                    };
                    if let Some(b) = self
                        .state
                        .lock()
                        .tab_entries
                        .get(&tid)
                        .and_then(|e| e.browser.clone())
                    {
                        if let Some(f) = b.main_frame() {
                            let cb_id = self.next_cb_id.fetch_add(1, Ordering::Relaxed);
                            self.state.lock().pending_js.insert(
                                cb_id,
                                PendingJsKind::AgentAction {
                                    task_id,
                                    settle_ms: 650,
                                },
                            );
                            let wrapped = format!(
                                "(function(){{try{{{js_action};}}catch(e){{}}console.debug('__ROVER_CB__:'+JSON.stringify({{id:{cb_id},payload:'ok'}}));}})();"
                            );
                            f.execute_java_script(Some(&CefString::from(wrapped.as_str())), None, 0);
                        }
                    }
                }
            }
            _ => {
                self.stop_agent("");
                self.append_ai_msg(&provider_name, &model_name, &reply_text, "ai");
            }
        }
    }

    fn record_agent_turn(
        &self,
        task_id: u64,
        reply_text: &str,
        result_text: &str,
    ) -> Option<usize> {
        let mut st = self.state.lock();
        let run = st.active_agent.as_mut()?;
        if run.task_id != task_id {
            return None;
        }
        run.steps += 1;
        if run.steps >= 18 {
            st.active_agent = None;
            st.ai_busy = false;
            st.ai_status = "Reached step limit.".into();
            return None;
        }
        run.state
            .turns
            .push((reply_text.to_string(), result_text.to_string()));
        Some(run.tab_id)
    }

    fn schedule_next_snapshot(&self, task_id: u64, tab_id: usize, delay_ms: u64) {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            if let Some(rt) = GLOBAL_APP.get() {
                let still_active = rt
                    .state
                    .lock()
                    .active_agent
                    .as_ref()
                    .map(|r| r.task_id == task_id)
                    .unwrap_or(false);
                if still_active {
                    rt.post(RoverCmd::JsonCmd(json!({
                        "cmd": "__internal_next_snap",
                        "task_id": task_id,
                        "tab_id": tab_id
                    })));
                }
            }
        });
    }

    fn stop_agent(&self, status: &str) {
        let tab_id = {
            let mut st = self.state.lock();
            let tid = st.active_agent.as_ref().map(|r| r.tab_id);
            st.active_agent = None;
            st.ai_busy = false;
            st.ai_status = status.to_string();
            tid
        };
        if let Some(tid) = tab_id {
            if let Some(b) = self
                .state
                .lock()
                .tab_entries
                .get(&tid)
                .and_then(|e| e.browser.clone())
            {
                if let Some(f) = b.main_frame() {
                    let off = glow_js(false);
                    f.execute_java_script(Some(&CefString::from(off)), None, 0);
                }
            }
        }
        self.push_ui_updates();
    }

    fn append_ai_msg(&self, who: &str, detail: &str, text: &str, kind: &str) {
        {
            let mut st = self.state.lock();
            if kind == "ai" {
                st.last_answer = text.to_string();
            }
            st.messages.push(ChatMsg {
                who: who.to_string(),
                detail: detail.to_string(),
                text: text.to_string(),
                kind: kind.to_string(),
            });
        }
        self.push_ui_updates();
    }

    fn drain_commands(&self) {
        loop {
            let cmd = self.queue.lock().pop_front();
            let Some(cmd) = cmd else { break };
            self.process_cmd(cmd);
        }
    }

    fn process_cmd(&self, cmd: RoverCmd) {
        match cmd {
            RoverCmd::ChromeBrowserCreated(b) => {
                self.state.lock().chrome_browser = Some(b);
                self.layout_windows();
                self.push_ui_updates();
            }
            RoverCmd::PanelBrowserCreated(b) => {
                self.state.lock().panel_browser = Some(b);
                self.layout_windows();
                self.push_ui_updates();
            }
            RoverCmd::TabBrowserCreated { tab_id, browser } => {
                self.state
                    .lock()
                    .tab_entries
                    .entry(tab_id)
                    .or_default()
                    .browser = Some(browser);
                self.layout_windows();
                self.push_ui_updates();
            }
            RoverCmd::DevToolsCreated { tab_id, browser } => {
                self.state
                    .lock()
                    .tab_entries
                    .entry(tab_id)
                    .or_default()
                    .devtools_browser = Some(browser);
                self.layout_windows();
            }
            RoverCmd::TabAddressChanged { tab_id, url } => {
                let logical = self.map_from_cef_url(&url);
                {
                    let mut st = self.state.lock();
                    if let Some(t) = st.strip.tabs.iter_mut().find(|t| t.id == tab_id) {
                        t.url = logical.clone();
                        if logical == "rover:start" {
                            t.title = "New tab".into();
                        }
                    }
                }
                self.push_ui_updates();
            }
            RoverCmd::TabTitleChanged { tab_id, title } => {
                {
                    let mut st = self.state.lock();
                    if let Some(t) = st.strip.tabs.iter_mut().find(|t| t.id == tab_id) {
                        if t.url == "rover:start" {
                            t.title = "New tab".into();
                        } else if !title.is_empty() {
                            t.title = title;
                        }
                    }
                }
                self.push_ui_updates();
            }
            RoverCmd::TabLoadingChanged {
                tab_id,
                loading,
                can_back,
                can_forward,
            } => {
                {
                    let mut st = self.state.lock();
                    let entry = st.tab_entries.entry(tab_id).or_default();
                    entry.loading = loading;
                    entry.can_back = can_back;
                    entry.can_forward = can_forward;
                    if !loading {
                        entry.progress = 0.0;
                    }
                }
                self.push_ui_updates();
            }
            RoverCmd::TabProgressChanged { tab_id, progress } => {
                self.state
                    .lock()
                    .tab_entries
                    .entry(tab_id)
                    .or_default()
                    .progress = progress;
                self.push_ui_updates();
            }
            RoverCmd::TabLoadEnded { tab_id } => {
                let (url, title, b_opt, private) = {
                    let st = self.state.lock();
                    let tab = st.strip.tabs.iter().find(|t| t.id == tab_id);
                    let u = tab.map(|t| t.url.clone()).unwrap_or_default();
                    let ti = tab.map(|t| t.title.clone()).unwrap_or_default();
                    let b = st.tab_entries.get(&tab_id).and_then(|e| e.browser.clone());
                    (u, ti, b, st.private)
                };
                if !private && (url.starts_with("http://") || url.starts_with("https://")) {
                    self.state.lock().store.add_history(&url, &title);
                }
                if let Some(b) = b_opt {
                    if url.starts_with("http://") || url.starts_with("https://") {
                        let host = url::Url::parse(&url)
                            .ok()
                            .and_then(|u| u.host_str().map(|s| s.to_string()))
                            .unwrap_or_default();
                        let (hide_css, darken) = {
                            let net = self.net.lock();
                            let h = if net.blocker.is_enabled() {
                                net.blocker.engine.hide_css(&host)
                            } else {
                                String::new()
                            };
                            (h, net.darkmode.should_darken(&url))
                        };
                        if let Some(f) = b.main_frame() {
                            if !hide_css.is_empty() {
                                let js = format!(
                                    "(function(c){{if(document.getElementById('__rv_hide'))return;var s=document.createElement('style');s.id='__rv_hide';s.textContent=c;(document.head||document.documentElement).appendChild(s);}})({});",
                                    serde_json::to_string(&hide_css).unwrap_or_default()
                                );
                                f.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
                            }
                            if darken {
                                let js = format!(
                                    "(function(c){{if(document.getElementById('__rv_dark'))return;var s=document.createElement('style');s.id='__rv_dark';s.textContent=c;(document.head||document.documentElement).appendChild(s);}})({});",
                                    serde_json::to_string(DARKMODE_CSS).unwrap_or_default()
                                );
                                f.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
                            }
                        }
                    }
                }
                self.push_ui_updates();
            }
            RoverCmd::TabOpenPopup(url) => self.open_new_tab(Some(&url)),
            RoverCmd::TabHomeAction { action, index } => match action.as_str() {
                "add" | "edit" => self.open_dialog("shortcut", None),
                "del" => {
                    if let Some(i) = index {
                        let tid = {
                            let st = self.state.lock();
                            remove_shortcut(&st.store, i);
                            st.active_tab().map(|t| t.id)
                        };
                        if let Some(tid) = tid {
                            if let Some(b) = self
                                .state
                                .lock()
                                .tab_entries
                                .get(&tid)
                                .and_then(|e| e.browser.clone())
                            {
                                b.reload();
                            }
                        }
                    }
                }
                "customize" => self.open_dialog("settings", Some("homepage")),
                "playground" => self.navigate_active("rover:playground"),
                _ => {}
            },
            RoverCmd::JsCallback { cb_id, payload } => {
                let kind = self.state.lock().pending_js.remove(&cb_id);
                if let Some(kind) = kind {
                    match kind {
                        PendingJsKind::AgentSnapshot { task_id } => {
                            self.on_agent_snapshot(task_id, payload);
                        }
                        PendingJsKind::AgentAction { task_id, settle_ms } => {
                            let tid = self
                                .state
                                .lock()
                                .active_agent
                                .as_ref()
                                .map(|r| r.tab_id)
                                .unwrap_or(0);
                            self.schedule_next_snapshot(task_id, tid, settle_ms);
                        }
                        PendingJsKind::ReaderExtract { tab_id, url } => {
                            if let Some(article) = extract_reader_article(&payload) {
                                let html = render_reader_html(&article, &url, "dark", 18);
                                let b_opt = {
                                    let mut st = self.state.lock();
                                    st.reader_html_by_tab.insert(tab_id, html);
                                    st.tab_entries.get(&tab_id).and_then(|e| e.browser.clone())
                                };
                                if let Some(b) = b_opt {
                                    if let Some(f) = b.main_frame() {
                                        let rurl = format!(
                                            "{}/ui/reader/{}?t={}",
                                            self.base_url(),
                                            tab_id,
                                            self.token
                                        );
                                        f.load_url(Some(&CefString::from(rurl.as_str())));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            RoverCmd::AgentReply {
                task_id,
                reply,
                elapsed_secs,
                system_sent,
                prompt_sent,
                provider_name,
                model_name,
            } => {
                self.on_agent_reply(
                    task_id,
                    reply,
                    elapsed_secs,
                    system_sent,
                    prompt_sent,
                    provider_name,
                    model_name,
                );
            }
            RoverCmd::KeyShortcut(sc) => match sc.as_str() {
                "new_tab" => self.open_new_tab(None),
                "close_tab" => {
                    let idx = self.state.lock().strip.active_index;
                    self.close_tab_at(idx);
                }
                "reopen_tab" => self.dispatch_menu_id(104),
                "next_tab" => {
                    let mut st = self.state.lock();
                    let n = st.strip.tabs.len();
                    if n > 1 {
                        let next = (st.strip.active_index + 1) % n;
                        st.strip.select_index(next);
                    }
                    drop(st);
                    self.layout_windows();
                    self.push_ui_updates();
                }
                "prev_tab" => {
                    let mut st = self.state.lock();
                    let n = st.strip.tabs.len();
                    if n > 1 {
                        let prev = (st.strip.active_index + n - 1) % n;
                        st.strip.select_index(prev);
                    }
                    drop(st);
                    self.layout_windows();
                    self.push_ui_updates();
                }
                "focus_omni" => {
                    if let Some(c) = self.state.lock().chrome_browser.clone() {
                        if let Some(h) = c.host() {
                            h.set_focus(1);
                        }
                        if let Some(f) = c.main_frame() {
                            f.execute_java_script(
                                Some(&CefString::from(
                                    "window.roverFocusOmni && window.roverFocusOmni();",
                                )),
                                None,
                                0,
                            );
                        }
                    }
                }
                "reload" => {
                    if let Some(b) = self.active_browser() {
                        b.reload();
                    }
                }
                "back" => {
                    if let Some(b) = self.active_browser() {
                        b.go_back();
                    }
                }
                "forward" => {
                    if let Some(b) = self.active_browser() {
                        b.go_forward();
                    }
                }
                "bookmark" => self.handle_json_cmd(json!({"cmd": "toggle_bookmark"})),
                "find" => self.toggle_find(true),
                "escape" => {
                    if self.state.lock().find_open {
                        self.toggle_find(false);
                    }
                }
                "devtools" => self.toggle_devtools(false),
                "inspect" => self.toggle_devtools(true),
                "panel" => self.dispatch_menu_id(140),
                "settings" => self.dispatch_menu_id(143),
                "history" => self.dispatch_menu_id(111),
                "bookmarks_mgr" => self.dispatch_menu_id(110),
                "bookmarks_bar" => self.dispatch_menu_id(127),
                "split" => self.toggle_split_view(),
                "reader" => self.trigger_reader_mode(),
                "view_source" => self.dispatch_menu_id(132),
                "print" => self.dispatch_menu_id(125),
                "zoom_in" => self.adjust_zoom(0.5),
                "zoom_out" => self.adjust_zoom(-0.5),
                "zoom_reset" => self.adjust_zoom(0.0),
                _ => {}
            },
            RoverCmd::JsonCmd(v) => self.handle_json_cmd(v),
        }
    }

    fn handle_json_cmd(&self, v: Value) {
        let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
        match cmd {
            "__internal_next_snap" => {
                let task_id = v.get("task_id").and_then(|x| x.as_u64()).unwrap_or(0);
                let tab_id = v.get("tab_id").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                self.request_agent_snapshot(task_id, tab_id);
            }
            "new_tab" => self.open_new_tab(v.get("url").and_then(|u| u.as_str())),
            "open_url_tab" => {
                if let Some(u) = v.get("url").and_then(|s| s.as_str()) {
                    self.open_new_tab(Some(u));
                }
            }
            "select_tab" => {
                let idx = v.get("index").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                self.state.lock().strip.select_index(idx);
                self.layout_windows();
                self.push_ui_updates();
            }
            "close_tab" => {
                let idx = v.get("index").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                self.close_tab_at(idx);
            }
            "navigate" => {
                if let Some(u) = v.get("url").and_then(|s| s.as_str()) {
                    self.navigate_active(u);
                }
            }
            "home" => self.navigate_active("rover:start"),
            "back" => {
                if let Some(b) = self.active_browser() {
                    b.go_back();
                }
            }
            "forward" => {
                if let Some(b) = self.active_browser() {
                    b.go_forward();
                }
            }
            "reload_or_stop" => {
                let loading = {
                    let st = self.state.lock();
                    st.active_tab()
                        .and_then(|t| st.tab_entries.get(&t.id))
                        .map(|e| e.loading)
                        .unwrap_or(false)
                };
                if let Some(b) = self.active_browser() {
                    if loading {
                        b.stop_load();
                    } else {
                        b.reload();
                    }
                }
            }
            "toggle_bookmark" => {
                let mut st = self.state.lock();
                if let Some(t) = st.active_tab().cloned() {
                    if t.url != "rover:start" && !t.url.is_empty() {
                        if st.store.is_bookmarked(&t.url) {
                            st.store.remove_bookmark(&t.url);
                        } else {
                            st.store.add_bookmark(&t.url, &t.title, "");
                            st.bookmarks_bar_visible = true;
                            st.store.set("bookmarks_bar", json!(true));
                        }
                    }
                }
                drop(st);
                self.layout_windows();
                self.push_ui_updates();
            }
            "remove_bookmark" => {
                if let Some(u) = v.get("url").and_then(|s| s.as_str()) {
                    self.state.lock().store.remove_bookmark(u);
                }
                self.layout_windows();
                self.push_ui_updates();
            }
            "toggle_bookmarks_bar" => self.dispatch_menu_id(127),
            "toggle_panel" => self.dispatch_menu_id(140),
            "show_menu" => {
                let kind = v.get("kind").and_then(|s| s.as_str()).unwrap_or("main");
                let x = v.get("x").and_then(|n| n.as_i64()).unwrap_or(100) as i32;
                let y = v.get("y").and_then(|n| n.as_i64()).unwrap_or(80) as i32;
                self.show_native_menu(kind, x, y);
            }
            "open_dialog" => {
                let kind = v.get("kind").and_then(|s| s.as_str()).unwrap_or("settings");
                let tab = v.get("tab").and_then(|s| s.as_str());
                self.open_dialog(kind, tab);
            }
            "find" => {
                let text = v.get("text").and_then(|s| s.as_str()).unwrap_or("");
                let forward = v.get("forward").and_then(|b| b.as_bool()).unwrap_or(true);
                let next = v.get("next").and_then(|b| b.as_bool()).unwrap_or(false);
                if let Some(b) = self.active_browser() {
                    if let Some(h) = b.host() {
                        if text.is_empty() {
                            h.stop_finding(1);
                        } else {
                            h.find(
                                Some(&CefString::from(text)),
                                if forward { 1 } else { 0 },
                                0,
                                if next { 1 } else { 0 },
                            );
                        }
                    }
                }
            }
            "close_find" => self.toggle_find(false),
            "set_provider" => {
                if let Some(pid) = v.get("id").and_then(|s| s.as_str()) {
                    self.state.lock().store.set("selected", json!(pid));
                    self.push_ui_updates();
                }
            }
            "set_model" => {
                if let Some(model) = v.get("model").and_then(|s| s.as_str()) {
                    let st = self.state.lock();
                    let pid = st
                        .store
                        .get_str("selected")
                        .unwrap_or_else(|| "gemini".into());
                    let mut ov = st.store.get("overrides").unwrap_or_else(|| json!({}));
                    if let Some(obj) = ov.as_object_mut() {
                        obj.insert(pid, json!({"model": model.trim()}));
                    }
                    st.store.set("overrides", ov);
                    drop(st);
                    self.push_ui_updates();
                }
            }
            "set_api_key" => {
                if let (Some(pid), Some(key)) = (
                    v.get("id").and_then(|s| s.as_str()),
                    v.get("key").and_then(|s| s.as_str()),
                ) {
                    let _ = self.state.lock().store.set_api_key(pid, key);
                    self.push_ui_updates();
                }
            }
            "set_setting" => {
                if let (Some(k), Some(val)) = (v.get("key").and_then(|s| s.as_str()), v.get("val"))
                {
                    let reload_start = k.starts_with("home_") || k == "search_engine";
                    self.state.lock().store.set(k, val.clone());
                    if reload_start {
                        self.reload_open_start_pages();
                    }
                    self.push_ui_updates();
                }
            }
            "set_panel_opt" => {
                if let (Some(k), Some(val)) = (v.get("key").and_then(|s| s.as_str()), v.get("val"))
                {
                    self.state.lock().store.set(k, val.clone());
                    self.push_ui_updates();
                }
            }
            "set_core_enabled" => {
                let on = v.get("enabled").and_then(|b| b.as_bool()).unwrap_or(false);
                self.state.lock().store.set("core_enabled", json!(on));
                self.push_ui_updates();
            }
            "set_core_active" => {
                if let Some(name) = v.get("name").and_then(|s| s.as_str()) {
                    self.state.lock().store.set("core_active", json!(name));
                    self.push_ui_updates();
                }
            }
            "save_core_set" => {
                if let (Some(name), Some(text)) = (
                    v.get("name").and_then(|s| s.as_str()),
                    v.get("text").and_then(|s| s.as_str()),
                ) {
                    let st = self.state.lock();
                    let mut sets = instruction_sets(&st.store);
                    if let Some(existing) = sets.iter_mut().find(|s| s.name == name) {
                        existing.text = text.to_string();
                    } else {
                        sets.push(InstructionSet {
                            name: name.to_string(),
                            text: text.to_string(),
                        });
                    }
                    save_sets(&st.store, &sets, Some(name), Some(true));
                    drop(st);
                    self.push_ui_updates();
                }
            }
            "delete_core_set" => {
                if let Some(name) = v.get("name").and_then(|s| s.as_str()) {
                    let st = self.state.lock();
                    let sets: Vec<InstructionSet> = instruction_sets(&st.store)
                        .into_iter()
                        .filter(|s| s.name != name)
                        .collect();
                    let next_active = sets.first().map(|s| s.name.as_str());
                    save_sets(&st.store, &sets, next_active, None);
                    drop(st);
                    self.push_ui_updates();
                }
            }
            "add_shortcut" => {
                if let (Some(title), Some(url)) = (
                    v.get("title").and_then(|s| s.as_str()),
                    v.get("url").and_then(|s| s.as_str()),
                ) {
                    {
                        let st = self.state.lock();
                        add_shortcut(&st.store, title, url);
                    }
                    self.reload_open_start_pages();
                }
            }
            "clear_history" => {
                self.state.lock().store.clear_history();
                self.push_ui_updates();
            }
            "remove_reading_list" => {
                if let Some(u) = v.get("url").and_then(|s| s.as_str()) {
                    self.state.lock().store.remove_reading(u);
                }
                self.push_ui_updates();
            }
            "toggle_adblock" => self.dispatch_menu_id(201),
            "toggle_domain_block" => self.dispatch_menu_id(202),
            "toggle_darkmode" => self.dispatch_menu_id(203),
            "save_memory" => {
                let mut st = self.state.lock();
                if st.last_answer.is_empty() {
                    st.ai_status = "No AI answer to save yet.".into();
                } else {
                    let ans = st.last_answer.clone();
                    match st.memory.append(&ans, true) {
                        Ok(_) => st.ai_status = format!("Saved to {}", st.memory.path.display()),
                        Err(e) => st.ai_status = format!("Could not save note: {e}"),
                    }
                }
                drop(st);
                self.push_ui_updates();
            }
            "ai_send" => {
                if let Some(text) = v.get("text").and_then(|s| s.as_str()) {
                    self.handle_ai_prompt(text.to_string(), false);
                }
            }
            "ai_send_anyway" => {
                let held = self.state.lock().held_prompt.clone();
                if !held.is_empty() {
                    self.handle_ai_prompt(held, true);
                }
            }
            "ai_stop" => self.stop_agent("Stopped."),
            "ai_clear" => {
                self.stop_agent("");
                {
                    let mut st = self.state.lock();
                    st.messages.clear();
                    st.last_answer.clear();
                    st.context_pct = 0;
                    st.context_label = "Context: no request yet".into();
                    let greet = initial_greeting(&st.store);
                    st.messages.push(greet);
                }
                self.push_ui_updates();
            }
            "refresh_models" => {
                self.state.lock().ai_status = "Models list is ready.".into();
                self.push_ui_updates();
            }
            _ => {}
        }
    }

    fn reload_open_start_pages(&self) {
        let browsers: Vec<Browser> = {
            let st = self.state.lock();
            st.strip
                .tabs
                .iter()
                .filter(|t| t.url == "rover:start")
                .filter_map(|t| st.tab_entries.get(&t.id)?.browser.clone())
                .collect()
        };
        for b in browsers {
            b.reload();
        }
    }
}

fn initial_greeting(store: &Store) -> ChatMsg {
    let any_ready = preset_providers()
        .into_iter()
        .any(|p| !p.needs_key || !resolve_provider_key(store, &p).trim().is_empty());
    let first = if any_ready {
        ""
    } else {
        "**First:** click ⚙ next to *Provider* and paste an API key (Gemini and Groq have free ones). "
    };
    ChatMsg {
        who: "Rover".into(),
        detail: String::new(),
        text: format!(
            "Hi! {first}Tell me what to do and press **Enter**. I work in the browser while you watch: \
             I open pages, click and type, and answer questions about what I find.\n\n\
             I never type passwords or card numbers, I ask before buying, sending or deleting anything, \
             and I hand CAPTCHAs to you."
        ),
        kind: "ai".into(),
    }
}

// ============================================================================
// CEF Handlers (Pure Chromium 156)
// ============================================================================

wrap_app! {
    struct RoverCefApp {
        proxy_flags: Vec<String>,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            if let Some(cl) = command_line {
                for flag in &self.proxy_flags {
                    let clean = flag.trim_start_matches("--");
                    if let Some((k, v)) = clean.split_once('=') {
                        cl.append_switch_with_value(
                            Some(&CefString::from(k)),
                            Some(&CefString::from(v)),
                        );
                    } else {
                        cl.append_switch(Some(&CefString::from(clean)));
                    }
                }
            }
        }
    }
}

wrap_keyboard_handler! {
    struct RoverKeyboardHandler;

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: Option<&mut cef::sys::MSG>,
            _is_keyboard_shortcut: Option<&mut i32>,
        ) -> i32 {
            let Some(ev) = event else { return 0 };
            if ev.type_ != KeyEventType::RAWKEYDOWN && ev.type_ != KeyEventType::KEYDOWN {
                return 0;
            }
            let ctrl = (ev.modifiers & EF_CTRL) != 0;
            let shift = (ev.modifiers & EF_SHIFT) != 0;
            let alt = (ev.modifiers & EF_ALT) != 0;
            let vk = ev.windows_key_code;

            let shortcut: Option<&str> = match (ctrl, shift, alt, vk) {
                (true, false, false, 0x54) => Some("new_tab"),
                (true, false, false, 0x57) => Some("close_tab"),
                (true, true, false, 0x54) => Some("reopen_tab"),
                (true, false, false, 0x09) => Some("next_tab"),
                (true, true, false, 0x09) => Some("prev_tab"),
                (true, false, false, 0x4C) => Some("focus_omni"),
                (false, false, false, 0x75) => Some("focus_omni"),
                (false, false, false, 0x74) => Some("reload"),
                (true, false, false, 0x52) => Some("reload"),
                (false, false, true, 0x25) => Some("back"),
                (false, false, true, 0x27) => Some("forward"),
                (true, false, false, 0x44) => Some("bookmark"),
                (true, false, false, 0x46) => Some("find"),
                (false, false, false, 0x1B) => Some("escape"),
                (false, false, false, 0x7B) => Some("devtools"),
                (true, true, false, 0x49) => Some("devtools"),
                (true, true, false, 0x43) => Some("inspect"),
                (true, true, false, 0x41) => Some("panel"),
                (true, false, false, 0xBC) => Some("settings"),
                (true, false, false, 0x48) => Some("history"),
                (true, true, false, 0x4F) => Some("bookmarks_mgr"),
                (true, true, false, 0x42) => Some("bookmarks_bar"),
                (true, true, false, 0x45) => Some("split"),
                (false, false, false, 0x78) => Some("reader"),
                (true, false, false, 0x55) => Some("view_source"),
                (true, false, false, 0x50) => Some("print"),
                (true, false, false, 0xBB) => Some("zoom_in"),
                (true, false, false, 0xBD) => Some("zoom_out"),
                (true, false, false, 0x30) => Some("zoom_reset"),
                _ => None,
            };

            if let Some(sc) = shortcut {
                if sc == "escape" {
                    if let Some(rt) = GLOBAL_APP.get() {
                        if rt.state.lock().find_open {
                            rt.post(RoverCmd::KeyShortcut("escape".into()));
                            return 1;
                        }
                    }
                    return 0;
                }
                if let Some(rt) = GLOBAL_APP.get() {
                    rt.post(RoverCmd::KeyShortcut(sc.to_string()));
                }
                return 1;
            }
            0
        }
    }
}

#[derive(Clone)]
enum UiRole {
    Chrome,
    Panel,
    Dialog(isize),
}

wrap_life_span_handler! {
    struct UiLifeSpanHandler {
        role: UiRole,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let (Some(rt), Some(b)) = (GLOBAL_APP.get(), browser) {
                match self.role {
                    UiRole::Chrome => rt.post(RoverCmd::ChromeBrowserCreated(b.clone())),
                    UiRole::Panel => rt.post(RoverCmd::PanelBrowserCreated(b.clone())),
                    UiRole::Dialog(_) => {}
                }
            }
        }
        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            if let UiRole::Dialog(dlg_hwnd) = self.role {
                if dlg_hwnd != 0 {
                    unsafe {
                        PostMessageW(dlg_hwnd as HWND, WM_CLOSE, 0, 0);
                    }
                }
            }
        }
    }
}

wrap_client! {
    struct UiCefClient {
        role: UiRole,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(UiLifeSpanHandler::new(self.role.clone()))
        }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(RoverKeyboardHandler::new())
        }
    }
}

wrap_life_span_handler! {
    struct DevToolsLifeSpanHandler {
        tab_id: usize,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let (Some(rt), Some(b)) = (GLOBAL_APP.get(), browser) {
                rt.post(RoverCmd::DevToolsCreated {
                    tab_id: self.tab_id,
                    browser: b.clone(),
                });
            }
        }
    }
}

wrap_client! {
    struct DevToolsCefClient {
        tab_id: usize,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(DevToolsLifeSpanHandler::new(self.tab_id))
        }
    }
}

wrap_life_span_handler! {
    struct TabLifeSpanHandler {
        tab_id: usize,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let (Some(rt), Some(b)) = (GLOBAL_APP.get(), browser) {
                rt.post(RoverCmd::TabBrowserCreated {
                    tab_id: self.tab_id,
                    browser: b.clone(),
                });
            }
        }

        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            if let (Some(rt), Some(u)) = (GLOBAL_APP.get(), target_url) {
                let s = u.to_string();
                if !s.is_empty() {
                    rt.post(RoverCmd::TabOpenPopup(s));
                    return 1;
                }
            }
            0
        }
    }
}

wrap_display_handler! {
    struct TabDisplayHandler {
        tab_id: usize,
    }

    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            if let (Some(rt), Some(f), Some(u)) = (GLOBAL_APP.get(), frame, url) {
                if f.is_main() != 0 {
                    rt.post(RoverCmd::TabAddressChanged {
                        tab_id: self.tab_id,
                        url: u.to_string(),
                    });
                }
            }
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            if let (Some(rt), Some(t)) = (GLOBAL_APP.get(), title) {
                rt.post(RoverCmd::TabTitleChanged {
                    tab_id: self.tab_id,
                    title: t.to_string(),
                });
            }
        }

        fn on_loading_progress_change(&self, _browser: Option<&mut Browser>, progress: f64) {
            if let Some(rt) = GLOBAL_APP.get() {
                rt.post(RoverCmd::TabProgressChanged {
                    tab_id: self.tab_id,
                    progress,
                });
            }
        }

        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            _line: i32,
        ) -> i32 {
            if let (Some(rt), Some(msg)) = (GLOBAL_APP.get(), message) {
                let s = msg.to_string();
                if let Some(rest) = s.strip_prefix("__ROVER_CB__:") {
                    if let Ok(v) = serde_json::from_str::<Value>(rest) {
                        let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
                        let payload = v
                            .get("payload")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        rt.post(RoverCmd::JsCallback {
                            cb_id: id,
                            payload,
                        });
                        return 1;
                    }
                }
            }
            0
        }
    }
}

wrap_load_handler! {
    struct TabLoadHandler {
        tab_id: usize,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: i32,
            can_go_back: i32,
            can_go_forward: i32,
        ) {
            if let Some(rt) = GLOBAL_APP.get() {
                rt.post(RoverCmd::TabLoadingChanged {
                    tab_id: self.tab_id,
                    loading: is_loading != 0,
                    can_back: can_go_back != 0,
                    can_forward: can_go_forward != 0,
                });
            }
        }

        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: i32,
        ) {
            if let (Some(rt), Some(f)) = (GLOBAL_APP.get(), frame) {
                if f.is_main() != 0 {
                    rt.post(RoverCmd::TabLoadEnded {
                        tab_id: self.tab_id,
                    });
                }
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _error_code: Errorcode,
            _error_text: Option<&CefString>,
            _failed_url: Option<&CefString>,
        ) {
        }
    }
}

wrap_resource_request_handler! {
    struct TabResourceRequestHandler;

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            if let (Some(rt), Some(req)) = (GLOBAL_APP.get(), request) {
                let u_uf = req.url();
                let url = CefString::from(&u_uf).to_string();
                let fp_uf = req.first_party_for_cookies();
                let first_party = CefString::from(&fp_uf).to_string();
                let kind = cef_res_to_kind(req.resource_type());
                if rt.net.lock().check_request(&url, &first_party, kind) {
                    return ReturnValue::CANCEL;
                }
            }
            ReturnValue::CONTINUE
        }
    }
}

wrap_request_handler! {
    struct TabRequestHandler;

    impl RequestHandler {
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            if let (Some(rt), Some(f), Some(req)) = (GLOBAL_APP.get(), frame, request) {
                if f.is_main() != 0 {
                    let u_uf = req.url();
                    let url = CefString::from(&u_uf).to_string();
                    if let Some((action, idx)) = parse_home_action(&url) {
                        rt.post(RoverCmd::TabHomeAction { action, index: idx });
                        return 1;
                    }
                }
            }
            0
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: i32,
            _is_download: i32,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut i32>,
        ) -> Option<ResourceRequestHandler> {
            Some(TabResourceRequestHandler::new())
        }
    }
}

wrap_client! {
    struct TabCefClient {
        tab_id: usize,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(TabLifeSpanHandler::new(self.tab_id))
        }
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(TabDisplayHandler::new(self.tab_id))
        }
        fn load_handler(&self) -> Option<LoadHandler> {
            Some(TabLoadHandler::new(self.tab_id))
        }
        fn request_handler(&self) -> Option<RequestHandler> {
            Some(TabRequestHandler::new())
        }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(RoverKeyboardHandler::new())
        }
    }
}

// ============================================================================
// Loopback Control Server (Internal Only — Never Opens External Browser)
// ============================================================================

fn spawn_control_server(rt: Arc<RoverRuntime>, server: Server) {
    std::thread::spawn(move || {
        let chrome_html = render_chrome_bar_html(&rt.token);
        let panel_html = render_ai_panel_html(&rt.token);
        let playground_html = render_playground_html();

        for mut req in server.incoming_requests() {
            let full_url = req.url().to_string();
            let (path, query) = full_url.split_once('?').unwrap_or((&full_url, ""));

            if path == "/rover-icon.png" || path == "/favicon.ico" {
                let h = Header::from_bytes(&b"Content-Type"[..], &b"image/png"[..]).unwrap();
                let _ = req
                    .respond(Response::from_data(ROVER_ICON_PNG_BYTES.to_vec()).with_header(h));
                continue;
            }

            let has_token = query.split('&').any(|kv| kv == format!("t={}", rt.token));
            if path.starts_with("/api/") && !has_token {
                let _ = req.respond(Response::from_string("forbidden").with_status_code(403));
                continue;
            }

            if path == "/api/state" {
                let body = rt.build_state_json().to_string();
                let h = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                let _ = req.respond(Response::from_string(body).with_header(h));
                continue;
            }

            if path == "/api/cmd" {
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                if let Ok(v) = serde_json::from_str::<Value>(&body) {
                    rt.post(RoverCmd::JsonCmd(v));
                }
                let h = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                let _ = req.respond(Response::from_string("{\"ok\":true}").with_header(h));
                continue;
            }

            let html = if path == "/ui/chrome" {
                chrome_html.clone()
            } else if path == "/ui/panel" {
                panel_html.clone()
            } else if path == "/ui/playground" {
                playground_html.clone()
            } else if path == "/ui/start" {
                let st = rt.state.lock();
                let eng_id = st
                    .store
                    .get_str("search_engine")
                    .unwrap_or_else(|| "duckduckgo".into());
                let (eng_name, eng_action) = engine_display_and_action(&eng_id);
                build_homepage_html(&st.store, eng_name, eng_action)
            } else if let Some(tid_str) = path.strip_prefix("/ui/reader/") {
                let tid: usize = tid_str.parse().unwrap_or(0);
                rt.state
                    .lock()
                    .reader_html_by_tab
                    .get(&tid)
                    .cloned()
                    .unwrap_or_else(|| {
                        "<html><body style='background:#160708;color:#FEF0E4'>Reader mode unavailable.</body></html>"
                            .into()
                    })
            } else if let Some(kind) = path.strip_prefix("/ui/dialog/") {
                let param = query.split('&').find_map(|kv| kv.strip_prefix("p="));
                render_dialog_html(kind, &rt.token, param)
            } else {
                "<html><body style='background:#160708'></body></html>".into()
            };

            let h =
                Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
            let _ = req.respond(Response::from_string(html).with_header(h));
        }
    });
}

// ============================================================================
// Native Win32 Window Procedures
// ============================================================================

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ROVER_EXEC => {
            if let Some(rt) = GLOBAL_APP.get() {
                rt.drain_commands();
            }
            0
        }
        WM_SIZE => {
            if let Some(rt) = GLOBAL_APP.get() {
                rt.layout_windows();
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            if let Some(rt) = GLOBAL_APP.get() {
                rt.closing.store(true, Ordering::SeqCst);
            }
            quit_message_loop();
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn dialog_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ============================================================================
// Entry Point
// ============================================================================

fn parse_proxy_flags(store: &Store) -> Vec<String> {
    let raw = store.get_str("proxy_url").unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        return ProxyConfig::load(store).chromium_flags();
    }
    if let Ok(u) = url::Url::parse(raw) {
        let mut pc = ProxyConfig::default();
        pc.mode = ProxyMode::Manual;
        pc.scheme = match u.scheme() {
            "http" => ProxyScheme::Http,
            "https" => ProxyScheme::Https,
            "socks4" => ProxyScheme::Socks4,
            _ => ProxyScheme::Socks5,
        };
        pc.host = u.host_str().unwrap_or("").to_string();
        pc.port = u.port().unwrap_or(1080);
        return pc.chromium_flags();
    }
    Vec::new()
}

pub fn run_chromium_browser(initial_url: Option<String>, private: bool) {
    let _ = api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();

    let profile_root = if private {
        std::env::temp_dir().join(format!("rover_private_{}", std::process::id()))
    } else {
        dirs_data_root().join("profiles").join("default")
    };
    let _ = std::fs::create_dir_all(&profile_root);
    let store = Store::new(Some(profile_root.clone()), None);
    let proxy_flags = parse_proxy_flags(&store);

    let mut cef_settings = Settings::default();
    cef_settings.no_sandbox = 1;
    cef_settings.log_severity = LogSeverity::DISABLE;
    cef_settings.background_color = 0xFF160708;
    if !private {
        let cef_cache = profile_root.join("cef_cache");
        let _ = std::fs::create_dir_all(&cef_cache);
        let cache_str = cef_cache.to_string_lossy().to_string();
        cef_settings.root_cache_path = CefString::from(cache_str.as_str());
        cef_settings.cache_path = CefString::from(cache_str.as_str());
    }

    let mut cef_app = RoverCefApp::new(proxy_flags);
    let init_ok = initialize(
        Some(args.as_main_args()),
        Some(&cef_settings),
        Some(&mut cef_app),
        ptr::null_mut(),
    );
    if init_ok == 0 {
        return;
    }

    let server = Server::http("127.0.0.1:0").expect("Failed to bind Rover internal control bridge");
    let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
    let token = gen_session_token();

    let net_filter = NetFilterState::new(&store);
    let mut strip = TabStrip::new();
    let start_logical = initial_url
        .as_deref()
        .map(|u| {
            let eng = store
                .get_str("search_engine")
                .unwrap_or_else(|| "duckduckgo".into());
            normalize_address(u, &eng)
        })
        .unwrap_or_else(|| "rover:start".to_string());
    strip.tabs[0].url = start_logical.clone();
    strip.tabs[0].title = if start_logical == "rover:start" {
        "New tab".into()
    } else {
        start_logical.clone()
    };
    let first_tab_id = strip.tabs[0].id;

    let mut tab_entries = HashMap::new();
    tab_entries.insert(first_tab_id, TabBrowserEntry::default());

    let panel_open = store.get_bool("panel", true);
    let bm_bar_vis = store.get_bool("bookmarks_bar", true);
    let usage_log = UsageLog::new(store.root.join("usage.log"));
    let memory = MemoryFile::new(store.root.join("Rover Memory.md"));
    let greeting = initial_greeting(&store);

    let runtime = Arc::new(RoverRuntime {
        port,
        token: token.clone(),
        main_hwnd: Mutex::new(0),
        queue: Mutex::new(VecDeque::new()),
        state: Mutex::new(BrowserState {
            store,
            private,
            strip,
            tab_entries,
            chrome_browser: None,
            panel_browser: None,
            dialog_hwnd: 0,
            panel_open,
            find_open: false,
            bookmarks_bar_visible: bm_bar_vis,
            reader_html_by_tab: HashMap::new(),
            messages: vec![greeting],
            raw_log: String::new(),
            ai_status: String::new(),
            ai_busy: false,
            pw_warning: String::new(),
            held_prompt: String::new(),
            last_answer: String::new(),
            session_meter: Meter::new(None),
            usage_log,
            memory,
            context_pct: 0,
            context_label: "Context: no request yet".into(),
            active_agent: None,
            pending_js: HashMap::new(),
        }),
        net: Mutex::new(net_filter),
        next_cb_id: AtomicU64::new(1),
        next_task_id: AtomicU64::new(1),
        closing: AtomicBool::new(false),
    });

    let _ = GLOBAL_APP.set(runtime.clone());
    spawn_control_server(runtime.clone(), server);

    let hinst = unsafe { GetModuleHandleW(ptr::null()) };
    let bg_brush = unsafe { CreateSolidBrush(0x0008_0716) };
    let app_icon = unsafe { LoadIconW(hinst, 1usize as *const u16) };
    let arrow_cursor = unsafe { LoadCursorW(ptr::null_mut(), IDC_ARROW) };

    let main_class = to_wide("RoverMainWindow");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(main_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: app_icon,
        hCursor: arrow_cursor,
        hbrBackground: bg_brush,
        lpszMenuName: ptr::null(),
        lpszClassName: main_class.as_ptr(),
        hIconSm: app_icon,
    };
    unsafe {
        RegisterClassExW(&wc);
    }

    let dlg_class = to_wide("RoverDialogWindow");
    let wc_dlg = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(dialog_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: app_icon,
        hCursor: arrow_cursor,
        hbrBackground: bg_brush,
        lpszMenuName: ptr::null(),
        lpszClassName: dlg_class.as_ptr(),
        hIconSm: app_icon,
    };
    unsafe {
        RegisterClassExW(&wc_dlg);
    }

    let initial_title = to_wide(if private {
        "New tab — Rover — Private"
    } else {
        "New tab — Rover"
    });
    let main_hwnd = unsafe {
        CreateWindowExW(
            0,
            main_class.as_ptr(),
            initial_title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1360,
            860,
            ptr::null_mut(),
            ptr::null_mut(),
            hinst,
            ptr::null(),
        )
    };
    if main_hwnd.is_null() {
        shutdown();
        return;
    }

    apply_dark_titlebar(main_hwnd);
    *runtime.main_hwnd.lock() = main_hwnd as isize;
    unsafe {
        ShowWindow(main_hwnd, SW_SHOW);
    }

    let mut rc: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        GetClientRect(main_hwnd, &mut rc);
    }
    let init_w = (rc.right - rc.left).max(800);
    let init_h = (rc.bottom - rc.top).max(600);
    let chrome_h = runtime.chrome_height();

    {
        let mut wi = WindowInfo::default().set_as_child(
            win_to_cef_hwnd(main_hwnd),
            &CefRect {
                x: 0,
                y: 0,
                width: init_w,
                height: chrome_h,
            },
        );
        wi.runtime_style = RuntimeStyle::ALLOY;
        let mut bs = BrowserSettings::default();
        bs.background_color = 0xFF100506;
        let mut client = UiCefClient::new(UiRole::Chrome);
        let url = format!("{}/ui/chrome?t={}", runtime.base_url(), token);
        browser_host_create_browser(
            Some(&wi),
            Some(&mut client),
            Some(&CefString::from(url.as_str())),
            Some(&bs),
            None,
            None,
        );
    }

    {
        let pw = if panel_open { PANEL_W } else { 0 };
        let mut wi = WindowInfo::default().set_as_child(
            win_to_cef_hwnd(main_hwnd),
            &CefRect {
                x: init_w - pw,
                y: chrome_h,
                width: pw.max(1),
                height: (init_h - chrome_h).max(120),
            },
        );
        wi.runtime_style = RuntimeStyle::ALLOY;
        let mut bs = BrowserSettings::default();
        bs.background_color = 0xFF160708;
        let mut client = UiCefClient::new(UiRole::Panel);
        let url = format!("{}/ui/panel?t={}", runtime.base_url(), token);
        browser_host_create_browser(
            Some(&wi),
            Some(&mut client),
            Some(&CefString::from(url.as_str())),
            Some(&bs),
            None,
            None,
        );
    }

    runtime.create_tab_browser(first_tab_id, &start_logical);

    run_message_loop();
    shutdown();

    if private {
        let _ = std::fs::remove_dir_all(&profile_root);
    }
}

fn dirs_data_root() -> PathBuf {
    if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(appdata).join("Rover")
    } else if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata).join("Rover")
    } else {
        std::env::temp_dir().join("Rover")
    }
}
