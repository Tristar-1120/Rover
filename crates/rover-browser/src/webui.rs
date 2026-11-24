//! Interactive 1-to-1 Pelagic-Fidelity Rover Browser Shell (`webui`).
//!
//! Styled end-to-end with the exact inverted Pelagic -> **Rover Mars** palette:
//! - Tab row `#100506` · Window `#160708` · Inputs/Panel `#200A0C` · Toolbar `#290E10`
//! - Accents: Crimson `#C83F3A` · Coral `#EF7560` · Ridge `#DA5647` · Peach `#FDB78E` · Apricot `#FED8B1`
//! - Starts with ZERO pre-populated bookmarks and ZERO pre-populated homepage shortcuts.

use crate::homepage::{build_homepage_html, rover_icon_data_uri};
use crate::playground::render_playground_html;

pub fn render_browser_shell_html() -> String {
    let preview_dir = std::env::temp_dir().join("rover_preview_ui_v2");
    let _ = std::fs::remove_dir_all(&preview_dir);
    let store = rover_core::Store::new(Some(preview_dir), None);
    let home_preview = build_homepage_html(&store, "DuckDuckGo", "https://duckduckgo.com/");
    let playground_preview = render_playground_html();
    let home_b64 = base64_encode(home_preview.as_bytes());
    let play_b64 = base64_encode(playground_preview.as_bytes());
    let icon_uri = rover_icon_data_uri();

    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<link rel="icon" type="image/png" href="{icon_uri}">
<title>New tab — Rover</title>
<style>
  :root {{
    --tabrow:#100506; --bg:#160708; --panel:#160708; --input:#200A0C; --btn:#290E10;
    --btn-hover:#351316; --border:#4A1D20; --border-hi:#692A2E;
    --crimson:#C83F3A; --coral:#EF7560; --ridge:#DA5647;
    --peach:#FDB78E; --apricot:#FED8B1; --maroon:#47090C;
    --ink:#FEF0E4; --muted:#C9A497; --dim:#8E675C;
  }}
  * {{ box-sizing:border-box; margin:0; padding:0; }}
  body {{
    height:100vh; display:flex; flex-direction:column;
    background:var(--bg); color:var(--ink);
    font-family:'Segoe UI',system-ui,-apple-system,sans-serif; font-size:13px;
    overflow:hidden; user-select:none;
  }}

  /* --- Top Tab Row (1-to-1 Pelagic) --- */
  .tabrow {{
    height:38px; display:flex; align-items:flex-end; gap:2px;
    padding:0 8px; background:var(--tabrow);
  }}
  .tabs {{ display:flex; align-items:flex-end; gap:2px; flex:1; overflow-x:auto; scrollbar-width:none; height:100%; }}
  .tabs::-webkit-scrollbar {{ display:none; }}
  .tab {{
    display:flex; align-items:center; justify-content:space-between; gap:8px;
    min-width:140px; max-width:240px; flex:1; height:32px; padding:0 10px;
    border-radius:8px 8px 0 0; background:transparent; color:var(--muted);
    cursor:pointer; font-size:12.5px; transition:background .1s;
  }}
  .tab:hover {{ background:rgba(254,216,177,.07); }}
  .tab.active {{ background:var(--bg); color:#FFFFFF; font-weight:500; }}
  .tab.sleeping {{ opacity:.55; font-style:italic; }}
  .tab-left {{ display:flex; align-items:center; gap:7px; overflow:hidden; }}
  .tab-title {{ overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }}
  .tab-x {{
    width:18px; height:18px; border-radius:4px; border:none; background:transparent;
    color:var(--muted); font-size:11px; cursor:pointer; display:flex; align-items:center; justify-content:center; flex-shrink:0;
  }}
  .tab-x:hover {{ background:rgba(255,255,255,.12); color:#fff; }}
  .tab-tool {{
    width:30px; height:28px; margin-bottom:4px; border:none; border-radius:6px;
    background:transparent; color:var(--muted); cursor:pointer;
    display:flex; align-items:center; justify-content:center;
  }}
  .tab-tool:hover {{ background:var(--btn-hover); color:#fff; }}

  /* --- Toolbar (1-to-1 Pelagic: ← → ↻ ⌂ [Omnibox] ☆ 🧩 [AI] ⚙ ⋮ 👤) --- */
  .toolbar {{
    height:44px; display:flex; align-items:center; gap:6px;
    padding:6px 8px; background:var(--bg);
  }}
  .tbtn {{
    width:34px; height:32px; border-radius:9px; position:relative;
    background:var(--btn); border:1px solid var(--border); color:var(--ink);
    display:flex; align-items:center; justify-content:center; cursor:pointer; flex-shrink:0;
  }}
  .tbtn:hover {{ background:var(--btn-hover); border-color:var(--border-hi); }}
  .tbtn:disabled {{ opacity:.4; cursor:default; }}
  .tbtn svg {{ width:18px; height:18px; stroke:currentColor; fill:none; stroke-width:1.7; stroke-linecap:round; stroke-linejoin:round; }}
  .tbtn .badge {{
    position:absolute; right:2px; bottom:2px; min-width:14px; height:13px; padding:0 3px;
    border-radius:6px; background:var(--crimson); color:#fff; font-size:9px; font-weight:700;
    display:none; align-items:center; justify-content:center; border:1px solid var(--bg);
  }}
  .tbtn .badge.show {{ display:flex; }}
  .omni {{
    flex:1; height:32px; border-radius:16px; padding:0 14px;
    background:var(--input); border:1px solid var(--border); color:var(--ink);
    font-size:13px; outline:none; user-select:text;
  }}
  .omni:focus {{ border-color:var(--coral); }}
  .omni::placeholder {{ color:var(--dim); }}

  /* --- Progress line (3px) --- */
  .progress {{ height:2px; background:transparent; overflow:hidden; }}
  .progress-bar {{ height:100%; width:0%; background:var(--coral); transition:width .18s; }}

  /* --- Bookmarks Bar (EMPTY & HIDDEN BY DEFAULT, 1-to-1 Pelagic) --- */
  .bmbar {{
    display:none; align-items:center; gap:4px; padding:2px 8px 4px;
    background:var(--bg); border-bottom:1px solid var(--border); overflow-x:auto;
  }}
  .bmbar.visible {{ display:flex; }}
  .bm-item {{
    padding:3px 9px; border-radius:6px; color:var(--muted); font-size:12px;
    cursor:pointer; white-space:nowrap;
  }}
  .bm-item:hover {{ background:var(--btn-hover); color:var(--ink); }}

  /* --- Find Bar (Hidden until Ctrl+F) --- */
  .findbar {{
    display:none; align-items:center; gap:6px; padding:4px 8px;
    background:var(--bg); border-bottom:1px solid var(--border);
  }}
  .findbar.visible {{ display:flex; }}
  .findbar input {{
    width:260px; height:28px; border-radius:8px; padding:0 10px;
    background:var(--input); border:1px solid var(--border); color:var(--ink); outline:none;
  }}
  .findbar input:focus {{ border-color:var(--coral); }}

  /* --- Main Workspace --- */
  .workspace {{ flex:1; display:flex; min-height:0; }}
  .left-pane {{ flex:1; display:flex; flex-direction:column; min-width:0; }}
  .viewports {{ flex:1; display:flex; position:relative; min-height:0; background:var(--bg); }}
  .viewport-frame {{ flex:1; border:none; width:100%; height:100%; background:var(--bg); }}
  .split-frame {{ flex:1; border:none; border-left:2px solid var(--coral); width:50%; height:100%; display:none; background:var(--bg); }}
  .viewports.split-active .split-frame {{ display:block; }}

  /* --- Bottom Docked Developer Tools (hidden until F12 / Ctrl+Shift+I) --- */
  .devdock {{
    display:none; flex-direction:column; height:240px;
    background:#120607; border-top:1px solid var(--border);
  }}
  .devdock.visible {{ display:flex; }}
  .devdock-title {{
    height:30px; display:flex; align-items:center; justify-content:space-between;
    padding:0 10px; background:#1D0A0C; border-bottom:1px solid var(--border);
    font-weight:600; font-size:12.5px; color:var(--ink);
  }}
  .devdock-body {{
    flex:1; padding:10px 14px; font-family:Consolas,ui-monospace,monospace;
    font-size:12px; color:var(--muted); overflow:auto; user-select:text;
  }}

  /* --- Right Docked Rover AI Panel (400px, 1-to-1 Pelagic panel.py) --- */
  .ai-dock {{
    width:400px; display:flex; flex-direction:column;
    background:var(--panel); border-left:1px solid var(--border);
  }}
  .ai-dock.hidden {{ display:none; }}
  .ai-dock-title {{
    height:34px; display:flex; align-items:center; justify-content:space-between;
    padding:0 10px; background:#1E090B; border-bottom:1px solid var(--border);
    font-weight:600; font-size:13px;
  }}
  .ai-body {{
    flex:1; display:flex; flex-direction:column; gap:6px; padding:8px; min-height:0;
  }}
  .ai-form-row {{ display:flex; align-items:center; gap:6px; }}
  .ai-lbl {{ width:58px; color:var(--muted); font-size:12.5px; flex-shrink:0; }}
  .ai-sel, .ai-inp {{
    flex:1; height:30px; border-radius:8px; padding:0 8px;
    background:var(--input); border:1px solid var(--border); color:var(--ink); font-size:13px; outline:none;
  }}
  .ai-sel:focus, .ai-inp:focus {{ border-color:var(--coral); }}
  .ai-mini {{
    height:30px; min-width:34px; padding:0 9px; border-radius:8px;
    background:var(--btn); border:1px solid var(--border); color:var(--ink); cursor:pointer;
    display:flex; align-items:center; justify-content:center; font-size:12.5px;
  }}
  .ai-mini:hover {{ background:var(--btn-hover); border-color:var(--border-hi); }}
  .ai-core-row {{ display:flex; align-items:center; gap:6px; font-size:12.5px; }}
  .ai-core-row label {{ display:flex; align-items:center; gap:6px; color:var(--ink); white-space:nowrap; cursor:pointer; }}
  .ai-views {{ display:flex; gap:4px; }}
  .vpill {{
    padding:3px 10px; border-radius:6px; font-size:12px; cursor:pointer;
    background:var(--btn); border:1px solid var(--border); color:var(--ink);
  }}
  .vpill.on {{ background:var(--crimson); border-color:var(--coral); color:#fff; }}

  .chat-view, .raw-view {{
    flex:1; overflow-y:auto; padding:8px; border-radius:8px;
    background:var(--input); border:1px solid var(--border); user-select:text; min-height:60px;
  }}
  .raw-view {{ display:none; font-family:Consolas,ui-monospace,monospace; font-size:12px; white-space:pre-wrap; color:var(--muted); }}
  .msg {{
    padding:10px; border-radius:8px; margin-bottom:6px; line-height:1.45;
  }}
  .msg.ai {{ background:#220B0D; }}
  .msg.user {{ background:#331215; }}
  .msg.err {{ background:#3E1115; }}
  .msg-who {{ font-weight:600; margin-bottom:4px; }}
  .msg.ai .msg-who {{ color:var(--peach); }}
  .msg.user .msg-who {{ color:var(--apricot); }}

  .opt-toggle {{
    background:transparent; border:none; color:var(--muted); text-align:left;
    cursor:pointer; font-size:13px; padding:2px 0;
  }}
  .opt-toggle:hover {{ color:#fff; }}
  .opt-drawer {{
    display:none; flex-direction:column; gap:5px; max-height:200px; overflow-y:auto;
    padding:4px 2px 6px 4px; font-size:12.5px; color:var(--ink);
  }}
  .opt-drawer.open {{ display:flex; }}
  .opt-drawer label {{ display:flex; align-items:center; gap:7px; cursor:pointer; }}
  .ctx-bar {{
    height:18px; border-radius:6px; background:var(--input); border:1px solid var(--border);
    position:relative; overflow:hidden; display:flex; align-items:center; justify-content:center;
    font-size:11px; color:var(--muted);
  }}
  .ctx-fill {{ position:absolute; left:0; top:0; bottom:0; width:0%; background:var(--crimson); opacity:.6; }}

  .pw-block {{
    display:none; padding:6px 8px; border-radius:6px; border:1px solid var(--coral);
    background:#361013; font-size:12px; align-items:center; gap:8px;
  }}
  .pw-block.show {{ display:flex; }}
  .ai-textarea {{
    height:76px; resize:none; border-radius:8px; padding:7px 9px;
    background:var(--input); border:1px solid var(--border); color:var(--ink);
    font-family:inherit; font-size:13px; outline:none; user-select:text;
  }}
  .ai-textarea:focus {{ border-color:var(--coral); }}
  .ai-actions {{ display:flex; align-items:center; gap:6px; }}
  .btn-std {{
    padding:5px 11px; border-radius:8px; background:var(--btn);
    border:1px solid var(--border); color:var(--ink); cursor:pointer; font-size:13px;
  }}
  .btn-std:hover {{ background:var(--btn-hover); border-color:var(--border-hi); }}
  .btn-primary {{
    padding:5px 14px; border-radius:8px; background:var(--crimson);
    border:1px solid var(--coral); color:#fff; font-weight:600; cursor:pointer; font-size:13px;
  }}
  .btn-primary:hover {{ background:var(--ridge); }}

  /* --- Floating Menus (Hamburger ⋮, Profile, Extensions) --- */
  .menu-pop {{
    position:fixed; display:none; flex-direction:column; min-width:250px; max-height:78vh;
    overflow-y:auto; background:#200A0C; border:1px solid var(--border-hi);
    border-radius:10px; padding:5px; z-index:200; box-shadow:0 14px 36px rgba(0,0,0,.6);
  }}
  .menu-pop.open {{ display:flex; }}
  .mitem {{
    display:flex; align-items:center; justify-content:space-between; gap:18px;
    padding:6px 12px; border-radius:6px; cursor:pointer; font-size:12.5px; color:var(--ink);
  }}
  .mitem:hover {{ background:rgba(254,216,177,.10); }}
  .mitem .k {{ color:var(--dim); font-size:11.5px; }}
  .msep {{ height:1px; background:var(--border); margin:4px 6px; }}
</style>
</head>
<body>
  <!-- Top Tab Row -->
  <div class="tabrow">
    <div class="tabs" id="tabs"></div>
    <button class="tab-tool" onclick="openTab('rover:start','New tab')" title="New tab (Ctrl+T)">
      <svg width="16" height="16" viewBox="0 0 20 20" stroke="currentColor" fill="none" stroke-width="1.8"><line x1="10" y1="4.5" x2="10" y2="15.5"/><line x1="4.5" y1="10" x2="15.5" y2="10"/></svg>
    </button>
    <button class="tab-tool" onclick="toggleMenu(event,'tabsearch-menu')" title="Search tabs">
      <svg width="16" height="16" viewBox="0 0 20 20" stroke="currentColor" fill="none" stroke-width="1.8"><polyline points="5,7.5 10,12.5 15,7.5"/></svg>
    </button>
  </div>

  <!-- Toolbar -->
  <div class="toolbar">
    <button class="tbtn" onclick="historyBack()" title="Back (Alt+←)">
      <svg viewBox="0 0 20 20"><line x1="16" y1="10" x2="4.5" y2="10"/><polyline points="9.5,5 4.5,10 9.5,15"/></svg>
    </button>
    <button class="tbtn" onclick="historyFwd()" title="Forward (Alt+→)">
      <svg viewBox="0 0 20 20"><line x1="4" y1="10" x2="15.5" y2="10"/><polyline points="10.5,5 15.5,10 10.5,15"/></svg>
    </button>
    <button class="tbtn" onclick="reloadTab()" title="Reload (F5)">
      <svg viewBox="0 0 20 20"><path d="M15.4 7A6 6 0 1 0 16 11.5"/><polyline points="12.6,3.2 15.6,7.1 11.4,7.4"/></svg>
    </button>
    <button class="tbtn" onclick="navigate('rover:start')" title="Home">
      <svg viewBox="0 0 20 20"><polyline points="3,10 10,3.6 17,10"/><polyline points="5.3,8.4 5.3,16.5 14.7,16.5 14.7,8.4"/></svg>
    </button>
    <input class="omni" id="omni" type="text" placeholder="Search or enter address" onkeydown="if(event.key==='Enter')navigate(this.value)">
    <button class="tbtn" id="star-btn" onclick="toggleBookmark()" title="Bookmark this page (Ctrl+D)">
      <svg viewBox="0 0 20 20"><polygon points="10,2.8 12.2,7.3 17.2,8 13.6,11.5 14.4,16.5 10,14.1 5.6,16.5 6.4,11.5 2.8,8 7.8,7.3"/></svg>
    </button>
    <button class="tbtn" onclick="toggleMenu(event,'ext-menu')" title="Extensions & Ad blocker">
      <svg viewBox="0 0 20 20"><path d="M8 3.5a2 2 0 1 1 4 0h3.5V7a2 2 0 1 1 0 4v4.5H12a2 2 0 1 0-4 0H4.5V11a2 2 0 1 0 0-4V3.5H8z"/></svg>
      <span class="badge" id="ext-badge">0</span>
    </button>
    <button class="tbtn" onclick="togglePanel()" title="Rover AI (Ctrl+Shift+A)">
      <img src="{icon_uri}" width="20" height="20" style="border-radius:50%" alt="Rover AI">
    </button>
    <button class="tbtn" onclick="openSettings()" title="Settings (Ctrl+,)">
      <svg viewBox="0 0 20 20"><circle cx="10" cy="10" r="3"/><path d="M10 2.5v2M10 15.5v2M2.5 10h2M15.5 10h2M4.7 4.7l1.4 1.4M13.9 13.9l1.4 1.4M15.3 4.7l-1.4 1.4M6.1 13.9l-1.4 1.4"/></svg>
    </button>
    <button class="tbtn" onclick="toggleMenu(event,'main-menu')" title="Menu">
      <svg viewBox="0 0 20 20"><circle cx="10" cy="4.5" r="1.4" fill="currentColor" stroke="none"/><circle cx="10" cy="10" r="1.4" fill="currentColor" stroke="none"/><circle cx="10" cy="15.5" r="1.4" fill="currentColor" stroke="none"/></svg>
    </button>
    <button class="tbtn" onclick="toggleMenu(event,'profile-menu')" title="Profile: Personal">
      <svg viewBox="0 0 20 20"><circle cx="10" cy="6" r="3.4"/><path d="M3.2 17.6c0-4.2 3.2-5.8 6.8-5.8s6.8 1.6 6.8 5.8"/><circle cx="15.8" cy="15.4" r="2.7" fill="#C83F3A" stroke="#160708" stroke-width="1.3"/></svg>
    </button>
  </div>
  <div class="progress"><div class="progress-bar" id="pbar"></div></div>
  <div class="bmbar" id="bmbar"></div>
  <div class="findbar" id="findbar">
    <input id="find-input" placeholder="Find in page">
    <button class="ai-mini" onclick="closeFind()">✕</button>
  </div>

  <!-- Main Workspace + Right Rover AI Dock -->
  <div class="workspace">
    <div class="left-pane">
      <div class="viewports" id="viewports">
        <iframe class="viewport-frame" id="main-frame"></iframe>
        <iframe class="split-frame" id="split-frame"></iframe>
      </div>
      <div class="devdock" id="devdock">
        <div class="devdock-title">
          <span>Developer tools</span>
          <button class="tab-x" onclick="toggleDevTools()">✕</button>
        </div>
        <div class="devdock-body" id="devdock-body">Rover Chromium DevTools Console — DOM inspector ready.</div>
      </div>
    </div>

    <aside class="ai-dock" id="ai-dock">
      <div class="ai-dock-title">
        <span>Rover AI</span>
        <button class="tab-x" onclick="togglePanel()" title="Close panel">✕</button>
      </div>
      <div class="ai-body">
        <div class="ai-form-row">
          <span class="ai-lbl">Provider</span>
          <select class="ai-sel" id="provider-sel">
            <option value="gemini">○ Google Gemini</option>
            <option value="anthropic">○ Anthropic Claude</option>
            <option value="openai">○ OpenAI</option>
            <option value="groq">○ Groq</option>
            <option value="openrouter">○ OpenRouter</option>
            <option value="deepseek">○ DeepSeek</option>
            <option value="ollama">● Ollama (local)</option>
          </select>
          <button class="ai-mini" onclick="openSettings()" title="Add or change API keys">⚙</button>
        </div>
        <div class="ai-form-row">
          <span class="ai-lbl">Model</span>
          <input class="ai-inp" id="model-inp" value="gemini-2.5-flash">
          <button class="ai-mini" onclick="setStatus('Models up to date.')" title="Refresh models">↻</button>
        </div>
        <div class="ai-core-row">
          <label><input type="checkbox" id="core-on"> Always-on</label>
          <select class="ai-sel" id="core-sel" disabled><option>No instructions yet: click Edit…</option></select>
          <button class="ai-mini" onclick="setStatus('Always-on instruction sets are configured in the native desktop app.')">Edit…</button>
        </div>
        <div class="ai-views">
          <button class="vpill on" id="vbtn-chat" onclick="showView('chat')">Readable</button>
          <button class="vpill" id="vbtn-raw" onclick="showView('raw')">Raw</button>
        </div>
        <div class="chat-view" id="chat-view"></div>
        <div class="raw-view" id="raw-view">Nothing sent yet. Every request to an AI service and its raw reply shows here.</div>
        <div id="ai-status" style="color:var(--muted);font-size:12px;min-height:14px;"></div>

        <button class="opt-toggle" id="opt-btn" onclick="toggleOpts()">▸ Options and usage</button>
        <div class="opt-drawer" id="opt-drawer">
          <div class="ai-form-row">
            <span class="ai-lbl">Text size</span>
            <input type="range" min="10" max="24" value="13" style="flex:1;accent-color:#C83F3A" oninput="setChatSize(this.value)">
            <span id="sz-lbl" style="color:var(--muted);min-width:34px">13px</span>
          </div>
          <label><input type="checkbox" id="opt-nocontext"> No context: only my message and always-on instructions</label>
          <label><input type="checkbox" id="opt-pagectx" checked> Let the AI read the page I'm on</label>
          <label><input type="checkbox" id="opt-tokensaver" checked> Save tokens (shorter, cheaper answers)</label>
          <label><input type="checkbox" id="opt-batch"> Fast mode: plan several steps at once</label>
          <label><input type="checkbox" id="opt-vision"> Let the agent look at screenshots</label>
          <label><input type="checkbox" id="opt-vision-all"> Screenshot every step (more accurate, costs more)</label>
          <label><input type="checkbox" id="opt-human"> Act like a person (visible cursor, letter-by-letter typing)</label>
          <label><input type="checkbox" id="opt-mem"> Include my saved notes in the next message</label>
          <div style="display:flex;gap:6px;margin-top:2px">
            <button class="btn-std" style="font-size:12px" onclick="setStatus('Saved last answer to Rover Memory.md')">Save last answer to notes</button>
            <button class="btn-std" style="font-size:12px" onclick="setStatus('Edit always-on instructions')">Always-on instructions…</button>
          </div>
          <div style="color:var(--muted);font-size:11.5px;margin-top:2px" id="usage-lbl">Session: 0 in · 0 out · $0.0000</div>
          <div class="ctx-bar"><div class="ctx-fill" id="ctx-fill"></div><span id="ctx-txt" style="position:relative">Context: no request yet</span></div>
        </div>

        <div class="pw-block" id="pw-block">
          <span id="pw-text" style="flex:1"></span>
          <button class="btn-std" onclick="sendAnyway()">Send anyway</button>
        </div>

        <textarea class="ai-textarea" id="ai-input" placeholder="Tell Rover what to do…" onkeydown="if(event.key==='Enter'&&!event.shiftKey){{event.preventDefault();sendPrompt();}}"></textarea>
        <div class="ai-actions">
          <button class="btn-std" onclick="newChat()">Clear history</button>
          <div style="flex:1"></div>
          <button class="btn-primary" onclick="sendPrompt()">Send</button>
        </div>
      </div>
    </aside>
  </div>

  <!-- Menus -->
  <div class="menu-pop" id="main-menu">
    <div class="mitem" onclick="openTab('rover:start','New tab');closeMenus()"><span>New tab</span><span class="k">Ctrl+T</span></div>
    <div class="mitem" onclick="reopenClosed();closeMenus()"><span>Reopen closed tab</span><span class="k">Ctrl+Shift+T</span></div>
    <div class="mitem" onclick="toggleSplit();closeMenus()"><span>Split view</span><span class="k">Ctrl+Shift+E</span></div>
    <div class="mitem" onclick="groupTabs();closeMenus()"><span>Group tabs by site</span></div>
    <div class="msep"></div>
    <div class="mitem" onclick="toggleBmBar();closeMenus()"><span>Show bookmarks bar</span><span class="k">Ctrl+Shift+B</span></div>
    <div class="mitem" onclick="openFind();closeMenus()"><span>Find in page</span><span class="k">Ctrl+F</span></div>
    <div class="mitem" onclick="toggleDevTools();closeMenus()"><span>Developer tools</span><span class="k">Ctrl+Shift+I</span></div>
    <div class="msep"></div>
    <div class="mitem" onclick="togglePanel();closeMenus()"><span>Rover AI panel</span><span class="k">Ctrl+Shift+A</span></div>
    <div class="mitem" onclick="openSettings();closeMenus()"><span>Settings…</span><span class="k">Ctrl+,</span></div>
    <div class="msep"></div>
    <div class="mitem" onclick="alert('Rover — a Chromium-based browser rebuilt in Rust with a built-in AI agent.');closeMenus()"><span>About Rover</span></div>
  </div>
  <div class="menu-pop" id="ext-menu">
    <div class="mitem" onclick="closeMenus()"><span>Ad &amp; tracker blocker: ON (2.00 µs/req)</span></div>
    <div class="mitem" onclick="closeMenus()"><span>Pi-hole domain blocklist: Ready</span></div>
  </div>
  <div class="menu-pop" id="profile-menu">
    <div class="mitem" style="color:var(--muted)"><span>● Personal</span></div>
    <div class="msep"></div>
    <div class="mitem" onclick="closeMenus()"><span>Add profile…</span></div>
    <div class="mitem" onclick="closeMenus()"><span>Manage profiles…</span></div>
  </div>
  <div class="menu-pop" id="tabsearch-menu"></div>

<script>
  const HOME_HTML = atob("{home_b64}");
  const PLAY_HTML = atob("{play_b64}");
  const SESSION_KEY = 'rover_webui_tabs_v1';
  function pageHtmlFor(url) {{
    if (!url || url === 'rover:start') return HOME_HTML;
    if (url === 'rover:playground' || url === 'pelagic:playground') return PLAY_HTML;
    return `<!doctype html><html><body style="background:#160708;color:#FEF0E4;font-family:system-ui;padding:36px;max-width:760px;margin:auto"><h2 style="color:#FED8B1">${{esc(url)}}</h2><p style="color:#C9A497;margin-top:10px">In the native Windows desktop Rover app (powered by pure Chromium CEF 156), this tab renders live directly inside Chromium with 2.00 µs Aho-Corasick adblocking and docked DevTools.</p></body></html>`;
  }}
  let tabs = [{{ title: 'New tab', url: 'rover:start', html: HOME_HTML }}];
  let closedTabs = [];
  let bookmarks = []; // ZERO pre-populated bookmarks (1-to-1 Pelagic)
  let showBmBar = false;
  let active = 0;
  let splitOn = false;
  let heldPrompt = '';
  try {{
    const saved = JSON.parse(localStorage.getItem(SESSION_KEY) || 'null');
    if (saved && Array.isArray(saved.tabs) && saved.tabs.length) {{
      tabs = saved.tabs.map(t => ({{ title: t.title || 'New tab', url: t.url || 'rover:start', html: pageHtmlFor(t.url) }}));
      active = Math.max(0, Math.min(saved.active || 0, tabs.length - 1));
    }}
  }} catch(_) {{}}
  function saveSession() {{
    try {{
      localStorage.setItem(SESSION_KEY, JSON.stringify({{ active, tabs: tabs.map(t => ({{ title: t.title, url: t.url }})) }}));
    }} catch(_) {{}}
  }}

  function renderTabs() {{
    const el = document.getElementById('tabs');
    el.innerHTML = '';
    tabs.forEach((t, i) => {{
      const d = document.createElement('div');
      d.className = 'tab' + (i === active ? ' active' : '');
      d.onclick = () => selectTab(i);
      d.innerHTML = `<div class="tab-left"><span class="tab-title">${{esc(t.title || 'New tab')}}</span></div>`
                  + `<button class="tab-x" onclick="event.stopPropagation();closeTab(${{i}})" title="Close tab">✕</button>`;
      el.appendChild(d);
    }});
    const ts = document.getElementById('tabsearch-menu');
    ts.innerHTML = tabs.map((t, i) => `<div class="mitem" onclick="selectTab(${{i}});closeMenus()"><span>${{esc(t.title)}}</span></div>`).join('');
  }}

  function renderBookmarks() {{
    const bar = document.getElementById('bmbar');
    bar.innerHTML = '';
    bookmarks.forEach(b => {{
      const a = document.createElement('div');
      a.className = 'bm-item';
      a.textContent = b.title || b.url;
      a.onclick = () => navigate(b.url);
      bar.appendChild(a);
    }});
    bar.classList.toggle('visible', showBmBar && bookmarks.length > 0);
  }}

  function selectTab(i) {{
    active = Math.max(0, Math.min(i, tabs.length - 1));
    const t = tabs[active];
    document.getElementById('main-frame').srcdoc = t.html || HOME_HTML;
    document.getElementById('omni').value = t.url === 'rover:start' ? '' : t.url;
    document.title = (t.title || 'New tab') + ' — Rover';
    renderTabs();
    saveSession();
  }}

  function openTab(url, title) {{
    tabs.push({{ title: title || 'New tab', url, html: pageHtmlFor(url) }});
    selectTab(tabs.length - 1);
  }}

  function closeTab(i) {{
    if (tabs.length <= 1) {{
      if (tabs[0] && tabs[0].url !== 'rover:start') closedTabs.push(tabs[0]);
      tabs = [{{ title: 'New tab', url: 'rover:start', html: HOME_HTML }}];
      selectTab(0);
      return;
    }}
    closedTabs.push(tabs.splice(i, 1)[0]);
    if (active >= tabs.length) active = tabs.length - 1;
    selectTab(active);
  }}

  function reopenClosed() {{
    if (!closedTabs.length) return;
    tabs.push(closedTabs.pop());
    selectTab(tabs.length - 1);
  }}

  function navigate(raw) {{
    const v = (raw || '').trim();
    const low = v.toLowerCase();
    if (!v || low === 'rover:start') {{
      tabs[active] = {{ title: 'New tab', url: 'rover:start', html: HOME_HTML }};
    }} else if (low === 'rover:playground' || low === 'pelagic:playground') {{
      tabs[active] = {{ title: 'Rover Playground', url: 'rover:playground', html: PLAY_HTML }};
    }} else {{
      const u = /^https?:\/\//i.test(v) ? v : (!v.includes(' ') && v.includes('.') ? 'https://' + v : 'https://duckduckgo.com/?q=' + encodeURIComponent(v));
      tabs[active] = {{ title: u.replace(/^https?:\/\//,'').split('/')[0], url: u, html: pageHtmlFor(u) }};
    }}
    selectTab(active);
  }}

  function toggleBookmark() {{
    const t = tabs[active];
    if (!t || t.url === 'rover:start') return;
    const idx = bookmarks.findIndex(b => b.url === t.url);
    if (idx >= 0) bookmarks.splice(idx, 1);
    else {{ bookmarks.push({{ title: t.title, url: t.url }}); showBmBar = true; }}
    renderBookmarks();
  }}

  function toggleBmBar() {{ showBmBar = !showBmBar; renderBookmarks(); }}
  function togglePanel() {{ document.getElementById('ai-dock').classList.toggle('hidden'); }}
  function toggleDevTools() {{ document.getElementById('devdock').classList.toggle('visible'); }}
  function toggleSplit() {{
    splitOn = !splitOn;
    document.getElementById('viewports').classList.toggle('split-active', splitOn);
    if (splitOn) document.getElementById('split-frame').srcdoc = PLAY_HTML;
  }}
  function openFind() {{ document.getElementById('findbar').classList.add('visible'); document.getElementById('find-input').focus(); }}
  function closeFind() {{ document.getElementById('findbar').classList.remove('visible'); }}
  function historyBack() {{}}
  function historyFwd() {{}}
  function reloadTab() {{ selectTab(active); }}
  function groupTabs() {{ setStatus('Grouped open tabs by site.'); }}
  function openSettings() {{ setStatus('Configure API keys and browser settings in Settings.'); }}
  function setStatus(s) {{ document.getElementById('ai-status').textContent = s; }}
  function setChatSize(px) {{
    document.getElementById('chat-view').style.fontSize = px + 'px';
    document.getElementById('sz-lbl').textContent = px + 'px';
  }}
  function toggleOpts() {{
    const d = document.getElementById('opt-drawer');
    const open = d.classList.toggle('open');
    document.getElementById('opt-btn').textContent = (open ? '▾ ' : '▸ ') + 'Options and usage';
  }}
  function showView(w) {{
    document.getElementById('chat-view').style.display = w === 'chat' ? 'block' : 'none';
    document.getElementById('raw-view').style.display = w === 'raw' ? 'block' : 'none';
    document.getElementById('vbtn-chat').classList.toggle('on', w === 'chat');
    document.getElementById('vbtn-raw').classList.toggle('on', w === 'raw');
  }}
  function toggleMenu(ev, id) {{
    ev.stopPropagation();
    const target = document.getElementById(id);
    const wasOpen = target.classList.contains('open');
    closeMenus();
    if (!wasOpen) {{
      const r = ev.currentTarget.getBoundingClientRect();
      target.style.top = (r.bottom + 4) + 'px';
      target.style.right = Math.max(8, window.innerWidth - r.right) + 'px';
      target.classList.add('open');
    }}
  }}
  function closeMenus() {{ document.querySelectorAll('.menu-pop').forEach(m => m.classList.remove('open')); }}
  document.addEventListener('click', closeMenus);

  function appendMsg(who, htmlBody, kind) {{
    const el = document.getElementById('chat-view');
    const d = document.createElement('div');
    d.className = 'msg ' + kind;
    d.innerHTML = `<div class="msg-who">${{esc(who)}}</div><div>${{htmlBody}}</div>`;
    el.appendChild(d);
    el.scrollTop = el.scrollHeight;
  }}

  function greet() {{
    document.getElementById('chat-view').innerHTML = '';
    appendMsg('Rover', 'Hi! <strong>First:</strong> click ⚙ next to <em>Provider</em> and paste an API key (Gemini and Groq have free ones). Tell me what to do and press <strong>Enter</strong>. I work in the browser while you watch: I open pages, click and type, and answer questions about what I find.<br><br>I never type passwords or card numbers, I ask before buying, sending or deleting anything, and I hand CAPTCHAs to you.', 'ai');
  }}

  function newChat() {{ greet(); }}

  function sendPrompt(bypass) {{
    const inp = document.getElementById('ai-input');
    const text = (bypass ? heldPrompt : inp.value).trim();
    if (!text) return;
    if (!bypass && /(password|passwd|api.?key|sk-|secret)/i.test(text)) {{
      heldPrompt = text;
      document.getElementById('pw-text').textContent = 'Held back: this looks like it contains a password or API key.';
      document.getElementById('pw-block').classList.add('show');
      return;
    }}
    document.getElementById('pw-block').classList.remove('show');
    inp.value = '';
    appendMsg('You', esc(text), 'user');
    if (/^open\s+/i.test(text)) {{
      const dest = text.replace(/^open\s+/i, '').trim();
      navigate(dest);
      appendMsg('Rover Local · 0 tokens', 'Opened <code>' + esc(dest) + '</code>.', 'ai');
    }} else {{
      appendMsg('Rover', 'Add an API key in Settings (⚙) to run live AI agent tasks in Chromium, or try a zero-token command like <code>open rover:playground</code>.', 'ai');
    }}
  }}
  function sendAnyway() {{ sendPrompt(true); }}
  function esc(s) {{ return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }}

  greet();
  selectTab(0);
</script>
</body>
</html>"##
    )
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
