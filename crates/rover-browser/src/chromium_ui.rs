//! 1-to-1 Pelagic-Fidelity HTML/CSS/JS Surfaces for Pure Chromium CEF (`chromium_ui`).
//!
//! Renders the Top Chrome Bar, Right-Docked Rover AI Panel, and Native Dialog views
//! (Settings, Bookmarks, History, Reading List, Passwords, Autofill, Token Usage,
//! Automations, Always-on Instructions, Add/Edit Shortcut, Downloads, About)
//! using the exact inverted Pelagic -> **Rover Mars** palette and ZERO pre-populated
//! bookmarks or shortcuts.

use crate::homepage::rover_icon_data_uri;

/// Shared Inverted Mars Rover CSS tokens matching Pelagic's exact geometry & typography.
const MARS_VARS_CSS: &str = r#"
  :root {
    --tabrow:#100506; --bg:#160708; --panel:#160708; --input:#200A0C; --btn:#290E10;
    --btn-hover:#351316; --btn-press:#42181C; --border:#4A1D20; --border-hi:#692A2E;
    --crimson:#C83F3A; --coral:#EF7560; --ridge:#DA5647;
    --peach:#FDB78E; --apricot:#FED8B1; --maroon:#47090C;
    --ink:#FEF0E4; --muted:#C9A497; --dim:#8E675C;
  }
  * { box-sizing:border-box; margin:0; padding:0; }
  ::-webkit-scrollbar { width:9px; height:9px; }
  ::-webkit-scrollbar-track { background:transparent; }
  ::-webkit-scrollbar-thumb { background:#532024; border-radius:5px; }
  ::-webkit-scrollbar-thumb:hover { background:#6E2B30; }
"#;

/// Exact 1-to-1 Pelagic Top Chrome Bar:
/// - Row 1 (`38px`, `#100506`): Clean equal-width tabs, tab group color chips, `+` new tab, `⌄` search tabs.
/// - Row 2 (`44px`, `#160708`): `←` `→` `↻` `⌂` | rounded pill Omnibox (`Search or enter address`) | `☆` `🧩` (with blocked badge) [Rover AI icon] `⚙` `⋮` `👤`.
/// - Row 3 (`2px`): Loading progress line (`#EF7560`).
/// - Optional Row 4 (`28px`): Bookmarks bar — **EMPTY and HIDDEN by default**, appears only when user adds bookmarks and bookmarks bar is shown.
/// - Optional Row 5 (`36px`): Find-in-page bar (`Ctrl+F`).
pub fn render_chrome_bar_html(token: &str) -> String {
    let icon_uri = rover_icon_data_uri();
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<style>
{MARS_VARS_CSS}
  body {{
    background:var(--bg); color:var(--ink);
    font-family:'Segoe UI',system-ui,-apple-system,sans-serif; font-size:13px;
    overflow:hidden; user-select:none; display:flex; flex-direction:column;
  }}

  /* --- Top Tab Row --- */
  .tabrow {{
    height:38px; display:flex; align-items:flex-end; gap:2px;
    padding:0 8px; background:var(--tabrow);
  }}
  .tabs {{
    display:flex; align-items:flex-end; gap:2px; flex:1;
    overflow-x:auto; scrollbar-width:none; height:100%;
  }}
  .tabs::-webkit-scrollbar {{ display:none; }}
  .gchip {{
    height:22px; margin:0 4px 5px 2px; padding:0 8px; border-radius:6px;
    font-size:11.5px; font-weight:600; color:#FEF0E4; display:flex; align-items:center;
    white-space:nowrap; cursor:pointer; flex-shrink:0;
  }}
  .tab {{
    display:flex; align-items:center; justify-content:space-between; gap:6px;
    min-width:130px; max-width:240px; flex:1; height:32px; padding:0 10px;
    border-radius:8px 8px 0 0; background:transparent; color:var(--muted);
    cursor:pointer; font-size:12.5px; transition:background .08s;
  }}
  .tab.pinned {{ min-width:38px; max-width:44px; flex:0 0 auto; justify-content:center; padding:0 6px; }}
  .tab:hover {{ background:rgba(254,216,177,.07); }}
  .tab.active {{ background:var(--bg); color:#FFFFFF; font-weight:500; }}
  .tab.sleeping {{ opacity:.52; font-style:italic; }}
  .tab-left {{ display:flex; align-items:center; gap:6px; overflow:hidden; min-width:0; }}
  .tab-dot {{ width:7px; height:7px; border-radius:50%; flex-shrink:0; }}
  .tab-title {{ overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }}
  .tab-x {{
    width:18px; height:18px; border-radius:4px; border:none; background:transparent;
    color:var(--muted); font-size:11px; cursor:pointer;
    display:flex; align-items:center; justify-content:center; flex-shrink:0;
  }}
  .tab-x:hover {{ background:rgba(255,255,255,.14); color:#fff; }}
  .tab-tool {{
    width:30px; height:28px; margin-bottom:4px; border:none; border-radius:6px;
    background:transparent; color:var(--muted); cursor:pointer;
    display:flex; align-items:center; justify-content:center; flex-shrink:0;
  }}
  .tab-tool:hover {{ background:var(--btn-hover); color:#fff; }}

  /* --- Main Toolbar --- */
  .toolbar {{
    height:44px; display:flex; align-items:center; gap:6px;
    padding:6px 8px; background:var(--bg);
  }}
  .tbtn {{
    width:34px; height:32px; border-radius:9px; position:relative;
    background:var(--btn); border:1px solid var(--border); color:var(--ink);
    display:flex; align-items:center; justify-content:center; cursor:pointer; flex-shrink:0;
  }}
  .tbtn:hover:not(:disabled) {{ background:var(--btn-hover); border-color:var(--border-hi); }}
  .tbtn:active:not(:disabled) {{ background:var(--btn-press); }}
  .tbtn:disabled {{ opacity:.38; cursor:default; }}
  .tbtn svg {{
    width:18px; height:18px; stroke:currentColor; fill:none;
    stroke-width:1.7; stroke-linecap:round; stroke-linejoin:round;
  }}
  .tbtn.starred svg {{ stroke:var(--peach); fill:var(--peach); }}
  .tbtn .badge {{
    position:absolute; right:2px; bottom:2px; min-width:14px; height:13px; padding:0 3px;
    border-radius:6px; background:var(--crimson); color:#fff; font-size:9px; font-weight:700;
    display:none; align-items:center; justify-content:center; border:1px solid var(--bg);
    line-height:1;
  }}
  .tbtn .badge.show {{ display:flex; }}
  .omni {{
    flex:1; height:32px; border-radius:16px; padding:0 14px;
    background:var(--input); border:1px solid var(--border); color:var(--ink);
    font-size:13px; outline:none; user-select:text; min-width:120px;
  }}
  .omni:focus {{ border-color:var(--coral); }}
  .omni::placeholder {{ color:var(--dim); }}

  /* --- Progress bar --- */
  .progress {{ height:2px; background:transparent; }}
  .progress-fill {{ height:100%; width:0%; background:var(--coral); transition:width .15s; }}

  /* --- Bookmarks Bar (hidden when empty / disabled) --- */
  .bmbar {{
    height:28px; display:none; align-items:center; gap:4px;
    padding:0 8px 2px; background:var(--bg); border-bottom:1px solid var(--border);
    overflow-x:auto; scrollbar-width:none;
  }}
  .bmbar::-webkit-scrollbar {{ display:none; }}
  .bmbar.show {{ display:flex; }}
  .bm-item {{
    padding:3px 9px; border-radius:6px; color:var(--muted); font-size:12px;
    cursor:pointer; white-space:nowrap;
  }}
  .bm-item:hover {{ background:var(--btn-hover); color:var(--ink); }}

  /* --- Find Bar --- */
  .findbar {{
    height:36px; display:none; align-items:center; gap:6px;
    padding:4px 8px; background:var(--bg); border-bottom:1px solid var(--border);
  }}
  .findbar.show {{ display:flex; }}
  .findbar input {{
    width:270px; height:28px; border-radius:8px; padding:0 10px;
    background:var(--input); border:1px solid var(--border); color:var(--ink);
    font-size:13px; outline:none; user-select:text;
  }}
  .findbar input:focus {{ border-color:var(--coral); }}
  .fbtn {{
    width:28px; height:28px; border-radius:7px; background:var(--btn);
    border:1px solid var(--border); color:var(--ink); cursor:pointer;
    display:flex; align-items:center; justify-content:center;
  }}
  .fbtn:hover {{ background:var(--btn-hover); }}
</style>
</head>
<body>
  <div class="tabrow">
    <div class="tabs" id="tabs"></div>
    <button class="tab-tool" onclick="cmd({{cmd:'new_tab'}})" title="New tab (Ctrl+T)">
      <svg width="16" height="16" viewBox="0 0 20 20" stroke="currentColor" fill="none" stroke-width="1.8" stroke-linecap="round"><line x1="10" y1="4.5" x2="10" y2="15.5"/><line x1="4.5" y1="10" x2="15.5" y2="10"/></svg>
    </button>
    <button class="tab-tool" id="tabsearch-btn" onclick="openNativeMenu('tabsearch', this)" title="Search tabs">
      <svg width="16" height="16" viewBox="0 0 20 20" stroke="currentColor" fill="none" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="5,7.5 10,12.5 15,7.5"/></svg>
    </button>
  </div>

  <div class="toolbar">
    <button class="tbtn" id="back-btn" onclick="cmd({{cmd:'back'}})" title="Back (Alt+←)" disabled>
      <svg viewBox="0 0 20 20"><line x1="16" y1="10" x2="4.5" y2="10"/><polyline points="9.5,5 4.5,10 9.5,15"/></svg>
    </button>
    <button class="tbtn" id="fwd-btn" onclick="cmd({{cmd:'forward'}})" title="Forward (Alt+→)" disabled>
      <svg viewBox="0 0 20 20"><line x1="4" y1="10" x2="15.5" y2="10"/><polyline points="10.5,5 15.5,10 10.5,15"/></svg>
    </button>
    <button class="tbtn" id="reload-btn" onclick="cmd({{cmd:'reload_or_stop'}})" title="Reload (F5)">
      <svg id="reload-svg" viewBox="0 0 20 20"><path d="M15.4 7A6 6 0 1 0 16 11.5"/><polyline points="12.6,3.2 15.6,7.1 11.4,7.4"/></svg>
    </button>
    <button class="tbtn" onclick="cmd({{cmd:'home'}})" title="Home">
      <svg viewBox="0 0 20 20"><polyline points="3,10 10,3.6 17,10"/><polyline points="5.3,8.4 5.3,16.5 14.7,16.5 14.7,8.4"/></svg>
    </button>
    <input class="omni" id="omni" type="text" placeholder="Search or enter address"
           onkeydown="if(event.key==='Enter'){{cmd({{cmd:'navigate',url:this.value}});this.blur();}}"
           onfocus="this.select()">
    <button class="tbtn" id="star-btn" onclick="cmd({{cmd:'toggle_bookmark'}})" title="Bookmark this page (Ctrl+D)">
      <svg viewBox="0 0 20 20"><polygon points="10,2.8 12.2,7.3 17.2,8 13.6,11.5 14.4,16.5 10,14.1 5.6,16.5 6.4,11.5 2.8,8 7.8,7.3"/></svg>
    </button>
    <button class="tbtn" id="ext-btn" onclick="openNativeMenu('extensions', this)" title="Extensions &amp; Ad blocker">
      <svg viewBox="0 0 20 20"><path d="M8 3.5a2 2 0 1 1 4 0h3.5V7a2 2 0 1 1 0 4v4.5H12a2 2 0 1 0-4 0H4.5V11a2 2 0 1 0 0-4V3.5H8z"/></svg>
      <span class="badge" id="ext-badge">0</span>
    </button>
    <button class="tbtn" onclick="cmd({{cmd:'toggle_panel'}})" title="Rover AI (Ctrl+Shift+A)">
      <img src="{icon_uri}" width="20" height="20" style="border-radius:50%" alt="Rover AI">
    </button>
    <button class="tbtn" onclick="cmd({{cmd:'open_dialog',kind:'settings'}})" title="Settings (Ctrl+,)">
      <svg viewBox="0 0 20 20"><circle cx="10" cy="10" r="3"/><path d="M10 2.5v2M10 15.5v2M2.5 10h2M15.5 10h2M4.7 4.7l1.4 1.4M13.9 13.9l1.4 1.4M15.3 4.7l-1.4 1.4M6.1 13.9l-1.4 1.4"/></svg>
    </button>
    <button class="tbtn" id="menu-btn" onclick="openNativeMenu('main', this)" title="Menu">
      <svg viewBox="0 0 20 20"><circle cx="10" cy="4.5" r="1.4" fill="currentColor" stroke="none"/><circle cx="10" cy="10" r="1.4" fill="currentColor" stroke="none"/><circle cx="10" cy="15.5" r="1.4" fill="currentColor" stroke="none"/></svg>
    </button>
    <button class="tbtn" id="profile-btn" onclick="openNativeMenu('profile', this)" title="Profile: Personal">
      <svg viewBox="0 0 20 20"><circle cx="10" cy="6" r="3.4"/><path d="M3.2 17.6c0-4.2 3.2-5.8 6.8-5.8s6.8 1.6 6.8 5.8"/><circle id="profile-dot" cx="15.8" cy="15.4" r="2.7" fill="#C83F3A" stroke="#160708" stroke-width="1.3"/></svg>
    </button>
  </div>

  <div class="progress"><div class="progress-fill" id="pbar"></div></div>
  <div class="bmbar" id="bmbar"></div>
  <div class="findbar" id="findbar">
    <input id="find-input" placeholder="Find in page"
           oninput="cmd({{cmd:'find',text:this.value,forward:true,next:false}})"
           onkeydown="if(event.key==='Enter')cmd({{cmd:'find',text:this.value,forward:!event.shiftKey,next:true}});else if(event.key==='Escape')cmd({{cmd:'close_find'}});">
    <button class="fbtn" onclick="cmd({{cmd:'find',text:document.getElementById('find-input').value,forward:false,next:true}})" title="Previous">▲</button>
    <button class="fbtn" onclick="cmd({{cmd:'find',text:document.getElementById('find-input').value,forward:true,next:true}})" title="Next">▼</button>
    <button class="fbtn" onclick="cmd({{cmd:'close_find'}})" title="Close">✕</button>
  </div>

<script>
  const TOKEN = "{token}";
  function cmd(payload) {{
    fetch('/api/cmd?t=' + TOKEN, {{
      method:'POST',
      headers:{{'Content-Type':'application/json'}},
      body:JSON.stringify(payload)
    }}).catch(()=>{{}});
  }}
  function openNativeMenu(kind, el) {{
    const r = el.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    cmd({{ cmd:'show_menu', kind, x: Math.round(r.right * dpr), y: Math.round(r.bottom * dpr) }});
  }}
  window.roverFocusOmni = () => {{
    const o = document.getElementById('omni');
    o.focus(); o.select();
  }};
  window.roverFocusFind = () => {{
    const f = document.getElementById('find-input');
    f.focus(); f.select();
  }};
  window.roverUpdateChrome = (st) => {{
    const tabsEl = document.getElementById('tabs');
    tabsEl.innerHTML = '';
    let lastGroup = null;
    (st.tabs || []).forEach((t, i) => {{
      if (t.group_name && t.group_name !== lastGroup) {{
        const chip = document.createElement('div');
        chip.className = 'gchip';
        chip.style.background = t.group_color || '#C83F3A';
        chip.textContent = t.group_name;
        tabsEl.appendChild(chip);
      }}
      lastGroup = t.group_name || null;
      const d = document.createElement('div');
      d.className = 'tab' + (i === st.active ? ' active' : '') + (t.sleeping ? ' sleeping' : '') + (t.pinned ? ' pinned' : '');
      d.onclick = () => cmd({{ cmd:'select_tab', index: i }});
      d.onauxclick = (e) => {{ if (e.button === 1) {{ e.preventDefault(); cmd({{ cmd:'close_tab', index: i }}); }} }};
      const dot = t.group_color ? `<span class="tab-dot" style="background:${{t.group_color}}"></span>` : '';
      const title = (t.sleeping ? '☾ ' : '') + (t.title || 'New tab');
      d.innerHTML = t.pinned
        ? `<span class="tab-title" title="${{esc(title)}}">${{esc(title.slice(0,2))}}</span>`
        : `<div class="tab-left">${{dot}}<span class="tab-title">${{esc(title)}}</span></div>`
          + `<button class="tab-x" onclick="event.stopPropagation();cmd({{cmd:'close_tab',index:${{i}}}})" title="Close tab">✕</button>`;
      tabsEl.appendChild(d);
    }});

    const omni = document.getElementById('omni');
    if (document.activeElement !== omni) {{
      omni.value = (!st.url || st.url === 'rover:start' || st.url === 'about:blank') ? '' : st.url;
    }}
    document.getElementById('back-btn').disabled = !st.can_back;
    document.getElementById('fwd-btn').disabled = !st.can_forward;
    const star = document.getElementById('star-btn');
    star.classList.toggle('starred', !!st.bookmarked);
    document.getElementById('reload-btn').title = st.loading ? 'Stop' : 'Reload (F5)';
    document.getElementById('reload-svg').innerHTML = st.loading
      ? '<line x1="5" y1="5" x2="15" y2="15"/><line x1="15" y1="5" x2="5" y2="15"/>'
      : '<path d="M15.4 7A6 6 0 1 0 16 11.5"/><polyline points="12.6,3.2 15.6,7.1 11.4,7.4"/>';
    document.getElementById('pbar').style.width = st.loading ? Math.max(8, Math.round((st.progress || 0.1) * 100)) + '%' : '0%';

    const badge = document.getElementById('ext-badge');
    const n = st.blocked || 0;
    badge.textContent = n > 99 ? '99+' : String(n);
    badge.classList.toggle('show', n > 0);

    const pbtn = document.getElementById('profile-btn');
    pbtn.title = 'Profile: ' + (st.profile_name || 'Personal') + (st.private ? ' (private window)' : '');
    document.getElementById('profile-dot').setAttribute('fill', st.profile_color || '#C83F3A');

    const bmEl = document.getElementById('bmbar');
    bmEl.innerHTML = '';
    const bms = st.bookmarks || [];
    bms.forEach(b => {{
      const a = document.createElement('div');
      a.className = 'bm-item';
      a.textContent = b.title || b.url;
      a.title = b.url;
      a.onclick = () => cmd({{ cmd:'navigate', url: b.url }});
      bmEl.appendChild(a);
    }});
    bmEl.classList.toggle('show', !!st.bookmarks_bar_visible && bms.length > 0);
    document.getElementById('findbar').classList.toggle('show', !!st.find_open);
  }};
  function esc(s) {{ return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }}
  fetch('/api/state?t=' + TOKEN).then(r=>r.json()).then(st => window.roverUpdateChrome(st)).catch(()=>{{}});
</script>
</body>
</html>"##
    )
}

/// Exact 1-to-1 Pelagic `panel.py` Right-Docked Rover AI Panel (`400px`).
pub fn render_ai_panel_html(token: &str) -> String {
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<style>
{MARS_VARS_CSS}
  body {{
    height:100vh; display:flex; flex-direction:column;
    background:var(--panel); color:var(--ink);
    font-family:'Segoe UI',system-ui,-apple-system,sans-serif; font-size:13px;
    overflow:hidden; user-select:none; border-left:1px solid var(--border);
  }}
  .dock-head {{
    height:34px; display:flex; align-items:center; justify-content:space-between;
    padding:0 10px; background:#1E090B; border-bottom:1px solid var(--border);
    font-weight:600; font-size:13px; flex-shrink:0;
  }}
  .xbtn {{
    width:22px; height:22px; border:none; border-radius:5px; background:transparent;
    color:var(--muted); cursor:pointer; font-size:12px;
  }}
  .xbtn:hover {{ background:rgba(255,255,255,.12); color:#fff; }}
  .body {{
    flex:1; display:flex; flex-direction:column; gap:6px; padding:8px; min-height:0;
  }}
  .frow {{ display:flex; align-items:center; gap:6px; }}
  .lbl {{ width:56px; color:var(--muted); font-size:12.5px; flex-shrink:0; }}
  .sel, .inp {{
    flex:1; height:30px; border-radius:8px; padding:0 8px;
    background:var(--input); border:1px solid var(--border); color:var(--ink);
    font-size:13px; outline:none; min-width:0;
  }}
  .sel:focus, .inp:focus {{ border-color:var(--coral); }}
  .mbtn {{
    height:30px; min-width:34px; padding:0 9px; border-radius:8px;
    background:var(--btn); border:1px solid var(--border); color:var(--ink);
    cursor:pointer; font-size:12.5px; flex-shrink:0;
  }}
  .mbtn:hover {{ background:var(--btn-hover); border-color:var(--border-hi); }}
  .core-row {{ display:flex; align-items:center; gap:5px; font-size:12.5px; }}
  .core-row label {{ display:flex; align-items:center; gap:6px; white-space:nowrap; cursor:pointer; }}
  .views {{ display:flex; gap:4px; }}
  .vpill {{
    padding:3px 10px; border-radius:6px; font-size:12px; cursor:pointer;
    background:var(--btn); border:1px solid var(--border); color:var(--ink);
  }}
  .vpill.on {{ background:var(--crimson); border-color:var(--coral); color:#fff; }}

  .chat-view, .raw-view {{
    flex:1; overflow-y:auto; padding:8px; border-radius:8px;
    background:var(--input); border:1px solid var(--border); user-select:text; min-height:60px;
  }}
  .raw-view {{
    display:none; font-family:Consolas,ui-monospace,monospace; font-size:12px;
    white-space:pre-wrap; color:var(--muted); line-height:1.4;
  }}
  .msg {{ padding:10px; border-radius:8px; margin-bottom:6px; line-height:1.45; word-wrap:break-word; }}
  .msg.ai {{ background:#220B0D; }}
  .msg.user {{ background:#341215; }}
  .msg.err {{ background:#421015; border:1px solid #8A252C; }}
  .msg.step {{ background:transparent; padding:2px 4px; color:var(--muted); font-size:11.5px; margin-bottom:2px; }}
  .msg-who {{ font-weight:600; margin-bottom:4px; display:flex; gap:8px; align-items:baseline; }}
  .msg.ai .msg-who {{ color:var(--peach); }}
  .msg.user .msg-who {{ color:var(--apricot); }}
  .msg.err .msg-who {{ color:#FF9B9B; }}
  .msg-detail {{ color:var(--dim); font-size:11px; font-weight:400; }}
  .msg code {{ background:#160708; padding:1px 4px; border-radius:4px; font-family:Consolas,monospace; font-size:.92em; }}
  .msg pre {{ background:#160708; padding:8px; border-radius:6px; overflow-x:auto; margin:6px 0; }}
  .msg a {{ color:var(--peach); }}

  .status {{ color:var(--muted); font-size:12px; min-height:15px; max-height:48px; overflow:hidden; }}
  .opt-btn {{
    background:transparent; border:none; color:var(--muted); text-align:left;
    cursor:pointer; font-size:13px; padding:2px 0;
  }}
  .opt-btn:hover {{ color:#fff; }}
  .opt-drawer {{
    display:none; flex-direction:column; gap:5px; max-height:210px; overflow-y:auto;
    padding:4px 4px 6px 4px; font-size:12.5px;
  }}
  .opt-drawer.open {{ display:flex; }}
  .opt-drawer label {{ display:flex; align-items:center; gap:7px; cursor:pointer; }}
  .ctx-bar {{
    height:18px; border-radius:6px; background:var(--input); border:1px solid var(--border);
    position:relative; overflow:hidden; display:flex; align-items:center; justify-content:center;
    font-size:11px; color:var(--muted); flex-shrink:0;
  }}
  .ctx-fill {{ position:absolute; left:0; top:0; bottom:0; width:0%; background:var(--crimson); opacity:.65; transition:width .2s; }}

  .pw-block {{
    display:none; padding:6px 8px; border-radius:6px; border:1px solid var(--coral);
    background:#381013; font-size:12px; align-items:center; gap:8px;
  }}
  .pw-block.show {{ display:flex; }}

  .prompt-box {{
    height:76px; resize:none; border-radius:8px; padding:7px 9px;
    background:var(--input); border:1px solid var(--border); color:var(--ink);
    font-family:inherit; font-size:13px; outline:none; user-select:text; flex-shrink:0;
  }}
  .prompt-box:focus {{ border-color:var(--coral); }}
  .actions {{ display:flex; align-items:center; gap:6px; flex-shrink:0; }}
  .btn {{
    padding:5px 10px; border-radius:8px; background:var(--btn);
    border:1px solid var(--border); color:var(--ink); cursor:pointer; font-size:13px;
  }}
  .btn:hover {{ background:var(--btn-hover); border-color:var(--border-hi); }}
  .btn-primary {{
    padding:5px 14px; border-radius:8px; background:var(--crimson);
    border:1px solid var(--coral); color:#fff; font-weight:600; cursor:pointer; font-size:13px;
  }}
  .btn-primary:hover {{ background:var(--ridge); }}
  .btn-stop {{ display:none; padding:5px 10px; border-radius:8px; background:#5A191E; border:1px solid var(--coral); color:#fff; cursor:pointer; }}
  .btn-stop.show {{ display:inline-block; }}
</style>
</head>
<body>
  <div class="dock-head">
    <span>Rover AI</span>
    <button class="xbtn" onclick="cmd({{cmd:'toggle_panel'}})" title="Close Rover AI panel">✕</button>
  </div>
  <div class="body">
    <div class="frow">
      <span class="lbl">Provider</span>
      <select class="sel" id="provider-sel" onchange="onProviderChange()" title="Which AI service answers. ● = ready to use, ○ = needs an API key (click ⚙)."></select>
      <button class="mbtn" onclick="cmd({{cmd:'open_dialog',kind:'settings',tab:'ai'}})" title="Add or change API keys">⚙</button>
    </div>
    <div class="frow">
      <span class="lbl">Model</span>
      <input class="inp" id="model-inp" onchange="cmd({{cmd:'set_model',model:this.value}})" title="The model this service uses.">
      <button class="mbtn" onclick="cmd({{cmd:'refresh_models'}})" title="Get the latest list of models from this service">↻</button>
    </div>
    <div class="core-row">
      <label title="Send your always-on instructions with every request."><input type="checkbox" id="core-on" onchange="cmd({{cmd:'set_core_enabled',enabled:this.checked}})"> Always-on</label>
      <select class="sel" id="core-sel" onchange="cmd({{cmd:'set_core_active',name:this.value}})"></select>
      <button class="mbtn" onclick="cmd({{cmd:'open_dialog',kind:'always_on'}})" title="Add, rename, edit or delete instruction sets">Edit…</button>
    </div>
    <div class="views">
      <button class="vpill on" id="v-chat" onclick="showView('chat')">Readable</button>
      <button class="vpill" id="v-raw" onclick="showView('raw')">Raw</button>
    </div>

    <div class="chat-view" id="chat-view"></div>
    <div class="raw-view" id="raw-view">Nothing sent yet. Every request to an AI service and its raw reply shows here.</div>
    <div class="status" id="ai-status"></div>

    <button class="opt-btn" id="opt-btn" onclick="toggleOpts()">▸ Options and usage</button>
    <div class="opt-drawer" id="opt-drawer">
      <div class="frow">
        <span class="lbl">Text size</span>
        <input type="range" id="size-slider" min="10" max="24" value="13" style="flex:1;accent-color:#C83F3A" oninput="setChatSize(this.value)">
        <span id="size-lbl" style="color:var(--muted);min-width:34px">13px</span>
      </div>
      <label><input type="checkbox" id="opt-nocontext" onchange="saveOpt('no_context',this.checked)"> No context: only my message and always-on instructions</label>
      <label><input type="checkbox" id="opt-pagectx" checked onchange="saveOpt('page_ctx',this.checked)"> Let the AI read the page I'm on</label>
      <label><input type="checkbox" id="opt-tokensaver" checked onchange="saveOpt('token_saver',this.checked)"> Save tokens (shorter, cheaper answers)</label>
      <label><input type="checkbox" id="opt-batch" onchange="saveOpt('batch',this.checked)"> Fast mode: plan several steps at once</label>
      <label><input type="checkbox" id="opt-vision" onchange="saveOpt('vision',this.checked)"> Let the agent look at screenshots</label>
      <label><input type="checkbox" id="opt-vision-all" onchange="saveOpt('vision_every_step',this.checked)"> Screenshot every step (more accurate, costs more)</label>
      <label><input type="checkbox" id="opt-human" onchange="saveOpt('humanize',this.checked)"> Act like a person (visible cursor, letter-by-letter typing)</label>
      <label><input type="checkbox" id="opt-mem" onchange="saveOpt('use_memory',this.checked)"> Include my saved notes in the next message</label>
      <div style="display:flex;gap:6px;margin-top:2px">
        <button class="btn" style="font-size:12px" onclick="cmd({{cmd:'save_memory'}})">Save last answer to notes</button>
        <button class="btn" style="font-size:12px" onclick="cmd({{cmd:'open_dialog',kind:'always_on'}})">Always-on instructions…</button>
      </div>
      <div id="usage-lbl" style="color:var(--muted);font-size:11.5px;margin-top:2px">Session: 0 in · 0 out · $0.0000</div>
      <div class="ctx-bar"><div class="ctx-fill" id="ctx-fill"></div><span id="ctx-txt" style="position:relative">Context: no request yet</span></div>
    </div>

    <div class="pw-block" id="pw-block">
      <span id="pw-text" style="flex:1"></span>
      <button class="btn" style="font-size:12px" onclick="cmd({{cmd:'ai_send_anyway'}})">Send anyway</button>
    </div>

    <textarea class="prompt-box" id="prompt-box" placeholder="Tell Rover what to do…"
              onkeydown="if(event.key==='Enter'&&!event.shiftKey){{event.preventDefault();sendPrompt();}}"></textarea>
    <div class="actions">
      <button class="btn" onclick="cmd({{cmd:'ai_clear'}})" title="Clear this conversation with the agent and start fresh">Clear history</button>
      <div style="flex:1"></div>
      <button class="btn-stop" id="stop-btn" onclick="cmd({{cmd:'ai_stop'}})">■ Stop</button>
      <button class="btn-primary" id="send-btn" onclick="sendPrompt()" title="Send (Enter). Shift+Enter starts a new line.">Send</button>
    </div>
  </div>

<script>
  const TOKEN = "{token}";
  let lastMsgCount = -1;
  let lastRawLen = -1;
  function cmd(payload) {{
    fetch('/api/cmd?t=' + TOKEN, {{
      method:'POST',
      headers:{{'Content-Type':'application/json'}},
      body:JSON.stringify(payload)
    }}).catch(()=>{{}});
  }}
  function saveOpt(key, val) {{ cmd({{ cmd:'set_panel_opt', key, val }}); }}
  function onProviderChange() {{
    const pid = document.getElementById('provider-sel').value;
    cmd({{ cmd:'set_provider', id: pid }});
  }}
  function sendPrompt() {{
    const box = document.getElementById('prompt-box');
    const text = box.value.trim();
    if (!text) return;
    box.value = '';
    cmd({{ cmd:'ai_send', text }});
  }}
  function setChatSize(px) {{
    document.getElementById('chat-view').style.fontSize = px + 'px';
    document.getElementById('size-lbl').textContent = px + 'px';
    saveOpt('chat_text_px', Number(px));
  }}
  function toggleOpts() {{
    const d = document.getElementById('opt-drawer');
    const open = d.classList.toggle('open');
    document.getElementById('opt-btn').textContent = (open ? '▾ ' : '▸ ') + 'Options and usage';
  }}
  function showView(w) {{
    document.getElementById('chat-view').style.display = w === 'chat' ? 'block' : 'none';
    document.getElementById('raw-view').style.display = w === 'raw' ? 'block' : 'none';
    document.getElementById('v-chat').classList.toggle('on', w === 'chat');
    document.getElementById('v-raw').classList.toggle('on', w === 'raw');
  }}
  function renderMd(t) {{
    let s = String(t).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
    s = s.replace(/```([\s\S]*?)```/g, '<pre><code>$1</code></pre>');
    s = s.replace(/`([^`]+)`/g, '<code>$1</code>');
    s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    s = s.replace(/\*([^*]+)\*/g, '<em>$1</em>');
    s = s.replace(/\n/g, '<br>');
    return s;
  }}
  window.roverFocusPrompt = () => document.getElementById('prompt-box').focus();
  window.roverUpdatePanel = (st) => {{
    const ps = document.getElementById('provider-sel');
    if (st.providers && st.providers.length) {{
      const cur = st.selected_provider || '';
      ps.innerHTML = st.providers.map(p => `<option value="${{p.id}}"${{p.id===cur?' selected':''}}>${{p.configured?'● ':'○ '}}${{p.name}}</option>`).join('');
    }}
    const mi = document.getElementById('model-inp');
    if (document.activeElement !== mi && st.selected_model) mi.value = st.selected_model;

    const cs = document.getElementById('core-sel');
    const sets = st.core_sets || [];
    if (sets.length) {{
      cs.disabled = false;
      cs.innerHTML = sets.map(s => `<option value="${{esc(s.name)}}"${{s.name===st.core_active?' selected':''}}>${{esc(s.name)}}</option>`).join('');
      document.getElementById('core-on').disabled = false;
      document.getElementById('core-on').checked = !!st.core_enabled;
    }} else {{
      cs.disabled = true;
      cs.innerHTML = '<option>No instructions yet: click Edit…</option>';
      document.getElementById('core-on').disabled = true;
      document.getElementById('core-on').checked = false;
    }}

    if (st.opts) {{
      document.getElementById('opt-nocontext').checked = !!st.opts.no_context;
      document.getElementById('opt-pagectx').checked = st.opts.page_ctx !== false;
      document.getElementById('opt-tokensaver').checked = st.opts.token_saver !== false;
      document.getElementById('opt-batch').checked = !!st.opts.batch;
      document.getElementById('opt-vision').checked = !!st.opts.vision;
      document.getElementById('opt-vision-all').checked = !!st.opts.vision_every_step;
      document.getElementById('opt-human').checked = !!st.opts.humanize;
      document.getElementById('opt-mem').checked = !!st.opts.use_memory;
      const px = st.opts.chat_text_px || 13;
      document.getElementById('size-slider').value = px;
      document.getElementById('size-lbl').textContent = px + 'px';
      document.getElementById('chat-view').style.fontSize = px + 'px';
    }}

    const msgs = st.messages || [];
    if (msgs.length !== lastMsgCount) {{
      lastMsgCount = msgs.length;
      const cv = document.getElementById('chat-view');
      cv.innerHTML = msgs.map(m => {{
        if (m.kind === 'step') return `<div class="msg step">${{esc(m.text)}}</div>`;
        return `<div class="msg ${{m.kind||'ai'}}"><div class="msg-who"><span>${{esc(m.who||'Rover')}}</span>`
             + (m.detail ? `<span class="msg-detail">${{esc(m.detail)}}</span>` : '')
             + `</div><div>${{renderMd(m.text||'')}}</div></div>`;
      }}).join('');
      cv.scrollTop = cv.scrollHeight;
    }}

    const raw = st.raw_log || '';
    if (raw.length !== lastRawLen) {{
      lastRawLen = raw.length;
      const rv = document.getElementById('raw-view');
      rv.textContent = raw || 'Nothing sent yet. Every request to an AI service and its raw reply shows here.';
      rv.scrollTop = rv.scrollHeight;
    }}

    document.getElementById('ai-status').textContent = st.ai_status || '';
    document.getElementById('usage-lbl').textContent = st.usage_text || 'Session: 0 in · 0 out · $0.0000';
    document.getElementById('ctx-fill').style.width = (st.context_pct || 0) + '%';
    document.getElementById('ctx-txt').textContent = st.context_label || 'Context: no request yet';
    document.getElementById('stop-btn').classList.toggle('show', !!st.ai_busy);

    const pwb = document.getElementById('pw-block');
    if (st.pw_warning) {{
      document.getElementById('pw-text').textContent = st.pw_warning;
      pwb.classList.add('show');
    }} else {{
      pwb.classList.remove('show');
    }}
  }};
  function esc(s) {{ return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }}
  fetch('/api/state?t=' + TOKEN).then(r=>r.json()).then(st => window.roverUpdatePanel(st)).catch(()=>{{}});
</script>
</body>
</html>"##
    )
}

/// Native Owned Dark Dialog Renderer for all Pelagic dialogs (Settings, Bookmarks, History,
/// Reading List, Passwords, Autofill, Token Usage, Automations, Always-on, Shortcut, Downloads, About).
pub fn render_dialog_html(kind: &str, token: &str, param: Option<&str>) -> String {
    let icon_uri = rover_icon_data_uri();
    let kind_esc = kind.replace('"', "");
    let param_esc = param.unwrap_or("").replace('"', "");
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Rover — {kind_esc}</title>
<style>
{MARS_VARS_CSS}
  body {{
    background:var(--bg); color:var(--ink);
    font-family:'Segoe UI',system-ui,-apple-system,sans-serif; font-size:13px;
    padding:18px 20px; line-height:1.5;
  }}
  h1 {{ font-size:18px; font-weight:600; color:var(--apricot); margin-bottom:14px; display:flex; align-items:center; gap:10px; }}
  h2 {{ font-size:14px; font-weight:600; color:var(--peach); margin:16px 0 8px; }}
  .tabs {{ display:flex; gap:6px; border-bottom:1px solid var(--border); padding-bottom:8px; margin-bottom:14px; flex-wrap:wrap; }}
  .tab {{ padding:5px 12px; border-radius:7px; background:var(--btn); border:1px solid var(--border); color:var(--muted); cursor:pointer; font-size:12.5px; }}
  .tab.on {{ background:var(--crimson); border-color:var(--coral); color:#fff; font-weight:600; }}
  .card {{ background:var(--input); border:1px solid var(--border); border-radius:10px; padding:12px 14px; margin-bottom:10px; }}
  .row {{ display:flex; align-items:center; gap:8px; margin-bottom:8px; }}
  .row label {{ min-width:140px; color:var(--muted); }}
  input[type=text], input[type=password], select, textarea {{
    flex:1; background:var(--bg); border:1px solid var(--border); color:var(--ink);
    border-radius:8px; padding:6px 10px; font-size:13px; font-family:inherit; outline:none;
  }}
  input:focus, select:focus, textarea:focus {{ border-color:var(--coral); }}
  .btn {{ padding:6px 12px; border-radius:8px; background:var(--btn); border:1px solid var(--border); color:var(--ink); cursor:pointer; font-size:12.5px; }}
  .btn:hover {{ background:var(--btn-hover); border-color:var(--border-hi); }}
  .btn-primary {{ padding:6px 14px; border-radius:8px; background:var(--crimson); border:1px solid var(--coral); color:#fff; font-weight:600; cursor:pointer; font-size:12.5px; }}
  .btn-primary:hover {{ background:var(--ridge); }}
  .list-item {{ display:flex; align-items:center; justify-content:space-between; gap:10px; padding:7px 10px; border-bottom:1px solid var(--border); }}
  .list-item:last-child {{ border-bottom:none; }}
  .muted {{ color:var(--muted); font-size:12px; }}
  .link {{ color:var(--peach); cursor:pointer; text-decoration:none; }}
  .link:hover {{ text-decoration:underline; }}
  .empty {{ color:var(--dim); padding:20px; text-align:center; }}
  .toast {{ color:var(--peach); font-size:12.5px; margin-top:8px; min-height:18px; }}
</style>
</head>
<body>
  <div id="root">Loading…</div>
  <div class="toast" id="toast"></div>
<script>
  const TOKEN = "{token}";
  const KIND = "{kind_esc}";
  const PARAM = "{param_esc}";
  const ICON = "{icon_uri}";
  let STATE = null;
  let ACTIVE_TAB = PARAM || 'general';

  async function cmd(payload) {{
    await fetch('/api/cmd?t=' + TOKEN, {{
      method:'POST', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify(payload)
    }});
    await load();
  }}
  function toast(msg) {{ document.getElementById('toast').textContent = msg; }}
  function esc(s) {{ return String(s||'').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }}

  async function load() {{
    const r = await fetch('/api/state?t=' + TOKEN);
    STATE = await r.json();
    render();
  }}

  function render() {{
    const el = document.getElementById('root');
    if (KIND === 'settings') {{
      const tabs = [['general','General'],['homepage','Homepage'],['ai','AI & API Keys'],['privacy','Privacy & Adblock'],['proxy','Connection'],['sync','Sync']];
      let html = `<h1>Settings</h1><div class="tabs">`
        + tabs.map(([k,l]) => `<button class="tab${{ACTIVE_TAB===k?' on':''}}" onclick="ACTIVE_TAB='${{k}}';render()">${{l}}</button>`).join('')
        + `</div>`;
      if (ACTIVE_TAB === 'general') {{
        const se = STATE.search_engine || 'duckduckgo';
        html += `<div class="card">
          <div class="row"><label>Search engine</label>
            <select id="se" onchange="cmd({{cmd:'set_setting',key:'search_engine',val:this.value}});toast('Search engine updated.')">
              <option value="duckduckgo"${{se==='duckduckgo'?' selected':''}}>DuckDuckGo</option>
              <option value="google"${{se==='google'?' selected':''}}>Google</option>
              <option value="bing"${{se==='bing'?' selected':''}}>Bing</option>
              <option value="brave"${{se==='brave'?' selected':''}}>Brave Search</option>
              <option value="ecosia"${{se==='ecosia'?' selected':''}}>Ecosia</option>
            </select></div>
          <div class="row"><label>Bookmarks bar</label>
            <input type="checkbox"${{STATE.bookmarks_bar_visible?' checked':''}} onchange="cmd({{cmd:'toggle_bookmarks_bar'}})"> Show bookmarks bar (when bookmarks exist)</div>
          <div class="row"><label>Tab grouping</label>
            <input type="checkbox"${{STATE.auto_group_tabs?' checked':''}} onchange="cmd({{cmd:'set_setting',key:'auto_group_tabs',val:this.checked}})"> Keep tabs grouped by site</div>
        </div>`;
      }} else if (ACTIVE_TAB === 'homepage') {{
        html += `<div class="card">
          <p class="muted" style="margin-bottom:10px">Shortcuts start empty (1-to-1 with Pelagic). Use "+ Add shortcut" on the New Tab page or below to add your own.</p>
          <div class="row"><label>Show Rover logo</label>
            <input type="checkbox"${{STATE.home_show_logo!==false?' checked':''}} onchange="cmd({{cmd:'set_setting',key:'home_show_logo',val:this.checked}});toast('Homepage updated.')"></div>
          <div class="row"><label>Show search box</label>
            <input type="checkbox"${{STATE.home_show_search!==false?' checked':''}} onchange="cmd({{cmd:'set_setting',key:'home_show_search',val:this.checked}});toast('Homepage updated.')"></div>
          <div class="row"><label>Show shortcuts</label>
            <input type="checkbox"${{STATE.home_show_shortcuts!==false?' checked':''}} onchange="cmd({{cmd:'set_setting',key:'home_show_shortcuts',val:this.checked}});toast('Homepage updated.')"></div>
          <div class="row"><label>Tagline text</label>
            <input type="text" id="h-greet" value="${{esc(STATE.home_tagline||'')}}" placeholder="Optional tagline">
            <button class="btn-primary" onclick="cmd({{cmd:'set_setting',key:'home_tagline',val:document.getElementById('h-greet').value}});toast('Tagline saved.')">Save</button></div>
        </div>`;
      }} else if (ACTIVE_TAB === 'ai') {{
        html += `<div class="card"><p class="muted" style="margin-bottom:10px">API keys are encrypted at rest with ChaCha20-Poly1305 AEAD and never sent to web pages.</p>`;
        (STATE.providers || []).forEach(p => {{
          html += `<div class="row">
            <label style="min-width:170px">${{p.configured?'● ':'○ '}}${{esc(p.name)}}</label>
            <input type="password" id="k-${{p.id}}" placeholder="${{p.configured?'•••••••• (saved)':'Paste API key…'}}">
            <button class="btn-primary" onclick="saveKey('${{p.id}}')">Save</button>
          </div>`;
        }});
        html += `</div>`;
      }} else if (ACTIVE_TAB === 'privacy') {{
        html += `<div class="card">
          <div class="row"><label>Ad &amp; tracker blocker</label>
            <input type="checkbox"${{STATE.adblock_enabled?' checked':''}} onchange="cmd({{cmd:'toggle_adblock'}})"> Enabled (Rust Aho-Corasick · 2.00 µs/req · ${{STATE.blocked||0}} blocked)</div>
          <div class="row"><label>Pi-hole domain list</label>
            <input type="checkbox"${{STATE.domain_block_enabled?' checked':''}} onchange="cmd({{cmd:'toggle_domain_block'}})"> Enabled</div>
          <div class="row"><label>Force dark mode</label>
            <input type="checkbox"${{STATE.darkmode_enabled?' checked':''}} onchange="cmd({{cmd:'toggle_darkmode'}})"> rover:darkmode on all sites</div>
        </div>`;
      }} else if (ACTIVE_TAB === 'proxy') {{
        html += `<div class="card">
          <div class="row"><label>Proxy server</label>
            <input type="text" id="px-url" value="${{esc(STATE.proxy_url||'')}}" placeholder="socks5://127.0.0.1:1080 or http://host:port">
            <button class="btn-primary" onclick="cmd({{cmd:'set_setting',key:'proxy_url',val:document.getElementById('px-url').value}});toast('Proxy saved (applies on next launch).')">Save</button></div>
          <p class="muted">WebRTC UDP IP leak protection is enabled automatically whenever a proxy is active.</p>
        </div>`;
      }} else if (ACTIVE_TAB === 'sync') {{
        html += `<div class="card">
          <p>Serverless encrypted folder sync (<strong>Rover Sync</strong>) uses ChaCha20-Poly1305 AEAD and LWW CRDT merging with zero cloud account required.</p>
          <p class="muted" style="margin-top:8px">Profile data path: <code>${{esc(STATE.profile_dir||'')}}</code></p>
        </div>`;
      }}
      el.innerHTML = html;
    }} else if (KIND === 'bookmarks') {{
      const bms = STATE.bookmarks || [];
      el.innerHTML = `<h1>Bookmarks</h1><div class="card">`
        + (bms.length ? bms.map(b => `<div class="list-item"><div><a class="link" onclick="cmd({{cmd:'open_url_tab',url:'${{esc(b.url)}}'}})">${{esc(b.title||b.url)}}</a><div class="muted">${{esc(b.url)}}</div></div><button class="btn" onclick="cmd({{cmd:'remove_bookmark',url:'${{esc(b.url)}}'}})">Remove</button></div>`).join('')
          : `<div class="empty">No bookmarks yet. Click ☆ in the address bar (or press Ctrl+D) to bookmark a page.</div>`)
        + `</div>`;
    }} else if (KIND === 'history') {{
      const items = STATE.history || [];
      el.innerHTML = `<h1>History</h1><div style="margin-bottom:10px;display:flex;justify-content:flex-end"><button class="btn" onclick="cmd({{cmd:'clear_history'}});toast('History cleared.')">Clear history</button></div><div class="card">`
        + (items.length ? items.map(h => `<div class="list-item"><div><a class="link" onclick="cmd({{cmd:'open_url_tab',url:'${{esc(h.url)}}'}})">${{esc(h.title||h.url)}}</a><div class="muted">${{esc(h.url)}}</div></div></div>`).join('')
          : `<div class="empty">No browsing history yet.</div>`)
        + `</div>`;
    }} else if (KIND === 'reading_list') {{
      const items = STATE.reading_list || [];
      el.innerHTML = `<h1>Reading list</h1><div class="card">`
        + (items.length ? items.map(r => `<div class="list-item"><div><a class="link" onclick="cmd({{cmd:'open_url_tab',url:'${{esc(r.url)}}'}})">${{esc(r.title||r.url)}}</a><div class="muted">${{esc(r.url)}}</div></div><button class="btn" onclick="cmd({{cmd:'remove_reading_list',url:'${{esc(r.url)}}'}})">Remove</button></div>`).join('')
          : `<div class="empty">Your reading list is empty. Press Alt+Shift+R on any page to save it for later.</div>`)
        + `</div>`;
    }} else if (KIND === 'shortcut') {{
      el.innerHTML = `<h1>Add shortcut</h1><div class="card">
        <div class="row"><label>Name</label><input type="text" id="sc-name" placeholder="Example"></div>
        <div class="row"><label>URL</label><input type="text" id="sc-url" placeholder="https://example.com"></div>
        <div style="display:flex;justify-content:flex-end;gap:8px;margin-top:10px">
          <button class="btn-primary" onclick="addSc()">Save shortcut</button>
        </div></div>`;
    }} else if (KIND === 'always_on') {{
      const sets = STATE.core_sets || [];
      el.innerHTML = `<h1>Always-on instructions</h1>
        <div class="card">
          <div class="row"><label>Set name</label><input type="text" id="ao-name" placeholder="e.g. Research"></div>
          <div class="row"><label>Instructions</label><textarea id="ao-text" rows="4" placeholder="Instructions the AI follows in every request…"></textarea></div>
          <div style="display:flex;justify-content:flex-end"><button class="btn-primary" onclick="saveAo()">Add / Save set</button></div>
        </div>
        <div class="card">`
        + (sets.length ? sets.map(s => `<div class="list-item"><div><strong>${{esc(s.name)}}</strong><div class="muted">${{esc(s.text.slice(0,120))}}</div></div><button class="btn" onclick="cmd({{cmd:'delete_core_set',name:'${{esc(s.name)}}'}})">Delete</button></div>`).join('')
          : `<div class="empty">No always-on instruction sets yet.</div>`)
        + `</div>`;
    }} else if (KIND === 'usage') {{
      el.innerHTML = `<h1>Token usage</h1><div class="card">
        <p><strong>${{esc(STATE.usage_text||'Session: 0 in · 0 out · $0.0000')}}</strong></p>
        <p class="muted" style="margin-top:8px">Rover's Rust Token Optimizer (DOM Delta Snapshots + Compact DSL + Semantic Pruner + Anthropic/OpenAI Prompt Caching) reduces multi-step agent token volume by up to 86.9%.</p>
      </div>`;
    }} else if (KIND === 'passwords' || KIND === 'autofill' || KIND === 'automations' || KIND === 'downloads') {{
      const titles = {{ passwords:'Passwords', autofill:'Addresses and cards', automations:'Automations', downloads:'Downloads' }};
      el.innerHTML = `<h1>${{titles[KIND]||KIND}}</h1><div class="card">
        <p>Protected by Rover's ChaCha20-Poly1305 AEAD vault (<code>RV2</code>) and local SQLite WAL profile storage.</p>
        <p class="muted" style="margin-top:8px">No entries saved yet.</p>
      </div>`;
    }} else {{
      el.innerHTML = `<h1><img src="${{ICON}}" width="34" height="34" style="border-radius:50%"> About Rover</h1>
        <div class="card">
          <p><strong>Rover 2.0.0</strong> — a pure open-source <strong>Chromium (CEF 156)</strong> browser rebuilt in Rust with an integrated low-token AI agent.</p>
          <p class="muted" style="margin-top:10px">• Engine: Pure Chromium 156 (libcef.dll) — zero Microsoft Edge / WebView2.<br>
          • Speed: 2.00 µs Aho-Corasick adblock · 2.0M lines/sec Pi-hole parser · ChaCha20-Poly1305 AEAD.<br>
          • Token Efficiency: DOM Delta Snapshots · Compact DSL · Semantic Pruner · Zero-token Rover Local.<br>
          • Works with Gemini, Claude, OpenAI, OpenRouter, Groq, Mistral, DeepSeek, xAI, Ollama and 26+ providers.</p>
        </div>`;
    }}
  }}

  async function saveKey(pid) {{
    const v = document.getElementById('k-' + pid).value.trim();
    if (!v) return;
    await cmd({{ cmd:'set_api_key', id: pid, key: v }});
    toast('Saved encrypted API key for ' + pid + '.');
  }}
  async function addSc() {{
    const name = document.getElementById('sc-name').value.trim();
    const url = document.getElementById('sc-url').value.trim();
    if (!url) return;
    await cmd({{ cmd:'add_shortcut', title: name || url, url }});
    toast('Shortcut added to New Tab page.');
    window.close();
  }}
  async function saveAo() {{
    const name = document.getElementById('ao-name').value.trim();
    const text = document.getElementById('ao-text').value.trim();
    if (!name) return;
    await cmd({{ cmd:'save_core_set', name, text }});
    toast('Saved instruction set "' + name + '".');
  }}

  load();
</script>
</body>
</html>"##
    )
}
