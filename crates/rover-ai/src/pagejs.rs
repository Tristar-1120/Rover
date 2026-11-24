//! Isolated-world JavaScript DOM instrumentation snippets (`pagejs`).
//!
//! Includes `SNAPSHOT`, `FULL_TEXT`, `SELECTION_AND_TEXT`, `PREPARE_CLICK`,
//! `PREPARE_COORDINATE_CLICK`, `CHECK_STATE`, `TOGGLE_LAST`, `RESOLVE_TARGET`,
//! `FALLBACK_CLICK`, `TYPE_TEXT`, `SELECT_OPTION`, `SCROLL`, `AGENT_BADGE`,
//! `AGENT_BADGE_OFF`, `ELEMENT_RECT`, `FOCUS_CLEAR`, `READ_VALUE`, `INSERT_CHAR`,
//! `FOCUSED_FIELD`, `PASTE_HTML`, `CURSOR_MOVE`, `CURSOR_CLICK`, `CAPTCHA`, and
//! Rover's new `EXTRACT_STRUCTURED` action script.
//!
//! Uses `__rover*` globals while mirroring `__atlas*` aliases for 100% compatibility.

use serde_json::Value;

pub const NORMALIZE_TEXT: &str = r#"
  const cleanText = (raw, limit) => {
    const lines = String(raw || '').replace(/\r/g, '').split('\n')
      .map(s => s.replace(/[ \t]+/g, ' ').trim()).filter(Boolean);
    const out = [];
    for (let i = 0; i < lines.length;) {
      let end = i + 1;
      while (end < lines.length && lines[end] === lines[i]) end++;
      out.push(lines[i] + (end - i > 1 ? ' [repeated x' + (end - i) + ']' : ''));
      i = end;
    }
    const text = out.join('\n');
    return text.length > limit ? text.slice(0, limit - 1) + '…' : text;
  };
"#;

pub const SECRET_JS: &str = r#"
  const __pw = window.__roverPw || (window.__roverPw = window.__atlasPw = new WeakSet());
  const __SECRET_NAME = /passw|passwd|passphrase|passcode|pwd|(^|[^a-z])(pass|pin|otp|totp|mfa|2fa|cvv|cvc)([^a-z]|$)|one.?time|verification.?code|security.?code|auth(entication)?.?code|secret|api.?key|access.?token/i;
  const isSecret = (el) => {
    if (!el || !el.tagName) return false;
    const tag = el.tagName.toLowerCase();
    if (tag !== 'input' && tag !== 'textarea') return false;
    const type = (el.getAttribute('type') || '').toLowerCase();
    if (type === 'password') { __pw.add(el); return true; }
    if (__pw.has(el)) return true;
    if (['checkbox', 'radio', 'button', 'submit', 'reset', 'image', 'file', 'range', 'color', 'hidden'].includes(type)) return false;
    const ac = (el.getAttribute('autocomplete') || '').toLowerCase();
    if (/current-password|new-password|one-time-code/.test(ac)) return true;
    return __SECRET_NAME.test([el.name, el.id, el.getAttribute('aria-label'), el.placeholder].join(' '));
  };
"#;

pub const CHECKABLE_JS: &str = r#"
  const __CHECK_ROLES = ['checkbox', 'radio', 'switch', 'menuitemcheckbox', 'menuitemradio'];
  const isCheckable = (el) => {
    if (!el || !el.tagName) return false;
    if (el.tagName.toLowerCase() === 'input') return ['checkbox', 'radio'].includes((el.type || '').toLowerCase());
    return __CHECK_ROLES.includes((el.getAttribute('role') || '').toLowerCase());
  };
  const checkState = (el) => {
    if (!isCheckable(el)) return null;
    if (el.tagName.toLowerCase() === 'input') return el.indeterminate ? 'mixed' : !!el.checked;
    const a = (el.getAttribute('aria-checked') || '').toLowerCase();
    if (a === 'mixed') return 'mixed';
    if (a === 'true') return true;
    const inner = el.querySelector('input[type=checkbox],input[type=radio]');
    return inner ? !!inner.checked : false;
  };
  const __txt = (n) => n ? (n.innerText || n.textContent || '').replace(/\s+/g, ' ').trim() : '';
  const controlLabel = (el) => {
    const by = el.getAttribute('aria-labelledby');
    if (by) {
      const t = by.split(/\s+/).map(id => __txt(document.getElementById(id))).filter(Boolean).join(' ');
      if (t) return t;
    }
    for (const l of Array.from(el.labels || [])) { const t = __txt(l); if (t) return t; }
    const wrap = el.closest && el.closest('label');
    return wrap ? __txt(wrap) : '';
  };
  const __shown = (n) => {
    const r = n.getBoundingClientRect(), cs = getComputedStyle(n);
    return r.width >= 2 && r.height >= 2 && cs.visibility !== 'hidden' && cs.display !== 'none' && parseFloat(cs.opacity) !== 0;
  };
  const clickProxy = (el) => {
    if (!el || el.tagName.toLowerCase() !== 'input' || !isCheckable(el) || __shown(el)) return null;
    const cands = Array.from(el.labels || []);
    const wrap = el.closest('label'); if (wrap) cands.push(wrap);
    if (el.parentElement && !cands.length) cands.push(el.parentElement);
    return cands.find(__shown) || null;
  };
"#;

pub fn snapshot_script() -> String {
    format!(
        r#"(() => {{
{NORMALIZE_TEXT}
{SECRET_JS}
{CHECKABLE_JS}
  const vh = window.innerHeight, vw = window.innerWidth, MAX = 80;
  const sel = 'a[href],button,input:not([type=hidden]),select,textarea,summary,[role=button],[role=link],' +
    '[role=checkbox],[role=radio],[role=tab],[role=menuitem],[role=option],[role=switch],[role=combobox],' +
    '[role=textbox],[onclick],[contenteditable=""],[contenteditable=true],[tabindex]:not([tabindex="-1"])';
  const found = [];
  const collect = root => {{
    root.querySelectorAll(sel).forEach(e => found.push(e));
    root.querySelectorAll('*').forEach(e => {{ if (e.shadowRoot) collect(e.shadowRoot); }});
  }};
  collect(document);
  const seen = new Set(), refs = [], items = [], masks = [], proxies = [];
  const clip = (s, n) => {{ s = (s || '').replace(/\s+/g, ' ').trim(); return s.length > n ? s.slice(0, n - 1) + '…' : s; }};
  const isCard = (el) => /^cc-/.test((el.getAttribute('autocomplete') || '').toLowerCase().trim().split(/\s+/).pop() || '') ||
    /card.?num|cc.?num|cardnumber|cvc|cvv|csc|security.?code/i.test((el.name || '') + ' ' + (el.id || '')) ||
    /^[\d\s-]{{13,23}}$/.test(el.value || '') && (el.value.match(/\d/g) || []).length >= 13;
  for (const el of found) {{
    if (seen.has(el) || items.length >= MAX) continue;
    seen.add(el);
    const proxy = clickProxy(el);
    if (proxy) seen.add(proxy);
    const box = proxy || el;
    const r = box.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) continue;
    if (r.bottom < -vh * 0.2 || r.top > vh * 1.6 || r.right < 0 || r.left > vw) continue;
    const cs = getComputedStyle(box);
    if (cs.visibility === 'hidden' || cs.display === 'none' || parseFloat(cs.opacity) === 0) continue;
    const tag = el.tagName.toLowerCase();
    const type = (el.getAttribute('type') || (tag === 'input' ? 'text' : tag === 'button' ? 'button' : '')).toLowerCase();
    const formy = tag === 'input' || tag === 'select' || tag === 'textarea' || isCheckable(el);
    const value = isCheckable(el) ? '' : el.value;
    let label = el.getAttribute('aria-label') || (formy ? controlLabel(el) : '') || el.innerText || value ||
      el.placeholder || el.getAttribute('title') || el.getAttribute('alt') || el.getAttribute('name') || el.value || '';
    if (!label) {{ const img = el.querySelector('img[alt]'); if (img) label = img.alt; }}
    const secret = isSecret(el);
    if (secret) label = el.getAttribute('aria-label') || el.placeholder || el.name || 'password';
    else if ((tag === 'input' || tag === 'textarea') && isCard(el)) label = el.getAttribute('aria-label') || el.placeholder || el.name || 'card field';
    const it = {{ i: refs.length, tag: tag + (type ? ':' + type : ''), text: clip(label, 60),
                 inView: r.top >= 0 && r.bottom <= vh }};
    if (tag === 'a' && el.href) it.href = clip(el.href.replace(location.origin, ''), 50);
    const role = (el.getAttribute('role') || '').toLowerCase();
    if (role && isCheckable(el)) it.tag += '[' + role + ']';
    if (isCheckable(el)) it.checked = checkState(el);
    else if (el.hasAttribute('aria-pressed')) it.pressed = el.getAttribute('aria-pressed') === 'true';
    if (['tab', 'option'].includes(role) && el.hasAttribute('aria-selected')) it.selected = el.getAttribute('aria-selected') === 'true';
    if (tag === 'input' || tag === 'textarea') {{
      if (type === 'checkbox' || type === 'radio') {{}}
      else if (secret) it.secret = true;
      else it.value = isCard(el) ? (el.value ? '(card detail, hidden)' : '') : clip(el.value, 40);
      if (el.placeholder) it.placeholder = clip(el.placeholder, 40);
    }}
    if (tag === 'select') {{
      it.options = Array.from(el.options).slice(0, 10).map(o => clip(o.text, 24));
      it.value = clip(el.selectedOptions[0] ? el.selectedOptions[0].text : '', 30);
    }}
    if (el.disabled || el.getAttribute('aria-disabled') === 'true') it.disabled = true;
    if (secret && type !== 'password' && (el.value || '') && r.bottom > 0 && r.top < vh) masks.push([r.left, r.top, r.width, r.height]);
    refs.push(el); items.push(it); proxies.push(proxy);
  }}
  window.__roverRefs = window.__atlasRefs = refs;
  window.__roverProxies = window.__atlasProxies = proxies;
  window.__roverLabels = window.__atlasLabels = items.map(it => ({{ t: (it.text || '').toLowerCase(),
    all: ((it.text || '') + ' ' + (it.placeholder || '') + ' ' + (it.href || '')).toLowerCase() }}));
  const visual = Array.from(document.querySelectorAll('canvas,video,svg,img,object,embed')).some(el => {{
    const r = el.getBoundingClientRect(), tag = el.tagName.toLowerCase();
    const cs = getComputedStyle(el);
    if (cs.display === 'none' || cs.visibility === 'hidden' || parseFloat(cs.opacity) === 0 ||
        r.bottom < 0 || r.top > vh || r.right < 0 || r.left > vw) return false;
    return r.width >= (tag === 'canvas' || tag === 'video' ? 80 : 240) &&
           r.height >= (tag === 'canvas' || tag === 'video' ? 80 : 160);
  }});
  const de = document.documentElement;
  const mainEl = document.querySelector('main, article, [role=main]') || document.body;
  const rawText = (mainEl && mainEl.innerText && mainEl.innerText.trim().length > 120)
    ? mainEl.innerText
    : (document.body ? document.body.innerText : '');
  return JSON.stringify({{
    url: location.href, title: document.title, ready: document.readyState, scrollY: Math.round(window.scrollY),
    scrollMax: Math.max(0, Math.round(de.scrollHeight - vh)), vh: vh, vw: vw,
    items: items, visual: visual, masks: masks,
    text: cleanText(rawText, 2500)
  }});
}})()"#
    )
}

pub fn full_text_script() -> String {
    format!(
        r#"(() => {{
{NORMALIZE_TEXT}
  return JSON.stringify({{ url: location.href, title: document.title,
    text: cleanText(document.body ? document.body.innerText : '', 15000) }});
}})()"#
    )
}

/// New Rover capability: extract structured tables, lists, or CSS selector matches without 15k raw text dumps.
pub fn extract_structured_script(selector: &str, max_items: usize) -> String {
    let sel_json = serde_json::to_string(selector).unwrap_or_else(|_| "\"table, ul, ol\"".into());
    format!(
        r#"(() => {{
  const sel = {sel_json} || 'table, ul, ol, article, section';
  const nodes = Array.from(document.querySelectorAll(sel)).slice(0, {max_items});
  const results = nodes.map(n => {{
    if (n.tagName.toLowerCase() === 'table') {{
      const rows = Array.from(n.querySelectorAll('tr')).slice(0, 40).map(tr =>
        Array.from(tr.querySelectorAll('th,td')).map(td => (td.innerText || '').replace(/\s+/g, ' ').trim())
      );
      return {{ kind: 'table', rows }};
    }}
    return {{ kind: n.tagName.toLowerCase(), text: (n.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 400) }};
  }});
  return JSON.stringify({{ url: location.href, selector: sel, count: results.length, items: results }});
}})()"#
    )
}

pub const AGENT_BADGE: &str = r#"
(() => {
  if (document.getElementById('__rover_agent_glow')) return;
  const d = document.createElement('div');
  d.id = '__rover_agent_glow';
  d.setAttribute('data-atlas-glow', '1');
  d.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:2147483647;' +
    'box-shadow:inset 0 0 0 3px rgba(218,86,71,.92),inset 0 0 42px rgba(253,183,142,.48);' +
    'animation:__rover_pulse 1.8s ease-in-out infinite;';
  const s = document.createElement('style');
  s.textContent = '@keyframes __rover_pulse{0%,100%{opacity:.55}50%{opacity:1}}';
  (document.head || document.documentElement).appendChild(s);
  (document.body || document.documentElement).appendChild(d);
})()
"#;

pub const AGENT_BADGE_OFF: &str = r#"
(() => {
  ['__rover_agent_glow', '__atlas_agent_glow', '__rover_cursor', '__atlas_cursor'].forEach(id => {
    const d = document.getElementById(id); if (d) d.remove();
  });
  window.__roverCur = window.__atlasCur = null;
})()
"#;

pub const CAPTCHA_JS: &str = r#"
(function () {
  function shown(el) {
    var r = el.getBoundingClientRect(), s = getComputedStyle(el);
    return r.width > 40 && r.height > 40 && s.visibility !== 'hidden' && s.display !== 'none' && s.opacity !== '0';
  }
  var frames = document.querySelectorAll('iframe');
  for (var i = 0; i < frames.length; i++) {
    var src = frames[i].src || '', title = (frames[i].title || '').toLowerCase();
    if (!shown(frames[i])) continue;
    if (/recaptcha\/(api2|enterprise)\/(anchor|bframe)/.test(src) && src.indexOf('size=invisible') < 0) return 'reCAPTCHA';
    if (/hcaptcha\.com/.test(src) || title.indexOf('hcaptcha') >= 0) return 'hCaptcha';
    if (/challenges\.cloudflare\.com/.test(src)) return 'Cloudflare Turnstile';
    if (title.indexOf('captcha') >= 0) return 'CAPTCHA';
  }
  if (document.querySelector('#challenge-form, #challenge-running, #cf-challenge-running')) return 'Cloudflare check';
  if (/\/sorry\/index/.test(location.pathname) && document.querySelector('#captcha-form, form[action*="sorry"]'))
    return 'Google unusual-traffic check';
  return '';
})()
"#;

pub fn call_js(template: &str, args: &[Value]) -> String {
    let encoded = args
        .iter()
        .map(|a| serde_json::to_string(a).unwrap_or_else(|_| "null".into()))
        .collect::<Vec<_>>()
        .join(", ");
    template.replacen("%s", &encoded, 1)
}

/// Clicks an element by id (`window.__roverRefs[id]`), visible target label, or viewport `(x, y)`.
pub fn click_script(id: Option<usize>, target: Option<&str>, xy: Option<(f64, f64)>) -> String {
    let id_js = match id {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    };
    let target_js = serde_json::to_string(&target).unwrap_or_else(|_| "null".into());
    let xy_js = match xy {
        Some((x, y)) => format!("[{x}, {y}]"),
        None => "null".to_string(),
    };
    format!(
        r#"(() => {{
  let el = null;
  const id = {id_js};
  const target = {target_js};
  const xy = {xy_js};
  if (id !== null && window.__roverRefs && window.__roverRefs[id]) {{
    el = window.__roverRefs[id];
  }} else if (target) {{
    const want = String(target).toLowerCase().trim();
    const L = window.__roverLabels || [];
    let idx = L.findIndex(l => l.t === want);
    if (idx < 0) idx = L.findIndex(l => l.t.startsWith(want));
    if (idx < 0) idx = L.findIndex(l => l.all.includes(want));
    if (idx >= 0 && window.__roverRefs) el = window.__roverRefs[idx];
  }} else if (xy) {{
    el = document.elementFromPoint(xy[0], xy[1]);
  }}
  if (!el || !el.isConnected) return JSON.stringify({{ ok: false, err: 'Element not found on current page.' }});
  el.scrollIntoView({{ block: 'center', inline: 'center' }});
  el.click();
  const label = (el.getAttribute('aria-label') || el.innerText || el.value || el.tagName || '').replace(/\s+/g, ' ').trim().slice(0, 60);
  return JSON.stringify({{ ok: true, label }});
}})()"#
    )
}

/// Types text into an input/textarea/contenteditable field while strictly refusing password/secret/card fields.
pub fn type_script(id: Option<usize>, target: Option<&str>, text: &str, submit: bool) -> String {
    let id_js = match id {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    };
    let target_js = serde_json::to_string(&target).unwrap_or_else(|_| "null".into());
    let text_js = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"(() => {{
{SECRET_JS}
  let el = null;
  const id = {id_js};
  const target = {target_js};
  const text = {text_js};
  const submit = {submit};
  if (id !== null && window.__roverRefs && window.__roverRefs[id]) {{
    el = window.__roverRefs[id];
  }} else if (target) {{
    const want = String(target).toLowerCase().trim();
    const L = window.__roverLabels || [];
    let idx = L.findIndex(l => l.t === want);
    if (idx < 0) idx = L.findIndex(l => l.t.startsWith(want));
    if (idx < 0) idx = L.findIndex(l => l.all.includes(want));
    if (idx >= 0 && window.__roverRefs) el = window.__roverRefs[idx];
  }} else {{
    el = document.activeElement;
  }}
  if (!el || !el.isConnected) return JSON.stringify({{ ok: false, err: 'Text field not found.' }});
  if (isSecret(el)) return JSON.stringify({{ ok: false, err: 'password' }});
  el.scrollIntoView({{ block: 'center' }});
  el.focus();
  if (el.isContentEditable) {{
    document.execCommand('selectAll');
    document.execCommand('insertText', false, text);
  }} else {{
    const tag = el.tagName.toLowerCase();
    const proto = tag === 'textarea' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    const desc = Object.getOwnPropertyDescriptor(proto, 'value');
    if (desc && desc.set) desc.set.call(el, text); else el.value = text;
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }}
  if (submit) {{
    if (el.form && typeof el.form.requestSubmit === 'function') el.form.requestSubmit();
    else el.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', code: 'Enter', bubbles: true }}));
  }}
  return JSON.stringify({{ ok: true }});
}})()"#
    )
}

/// Selects an option in a `<select>` dropdown by id or label.
pub fn select_script(id: Option<usize>, target: Option<&str>, value: &str) -> String {
    let id_js = match id {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    };
    let target_js = serde_json::to_string(&target).unwrap_or_else(|_| "null".into());
    let val_js = serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"(() => {{
  let el = null;
  const id = {id_js};
  const target = {target_js};
  const want = String({val_js}).trim().toLowerCase();
  if (id !== null && window.__roverRefs && window.__roverRefs[id]) el = window.__roverRefs[id];
  else if (target) {{
    const tw = String(target).toLowerCase().trim();
    const L = window.__roverLabels || [];
    let idx = L.findIndex(l => l.t === tw || l.all.includes(tw));
    if (idx >= 0 && window.__roverRefs) el = window.__roverRefs[idx];
  }}
  if (!el || el.tagName.toLowerCase() !== 'select') return JSON.stringify({{ ok: false, err: 'Dropdown not found.' }});
  const opt = Array.from(el.options).find(o => o.text.trim().toLowerCase() === want || o.value.toLowerCase() === want)
    || Array.from(el.options).find(o => o.text.toLowerCase().includes(want));
  if (!opt) return JSON.stringify({{ ok: false, err: 'Option not found.' }});
  el.value = opt.value;
  el.dispatchEvent(new Event('input', {{ bubbles: true }}));
  el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  return JSON.stringify({{ ok: true, chosen: opt.text.trim() }});
}})()"#
    )
}

/// Scrolls the viewport vertically by `pages` viewport heights (`up` or `down`).
pub fn scroll_script(direction: &str, pages: f64) -> String {
    let sign = if direction.eq_ignore_ascii_case("up") {
        -1.0
    } else {
        1.0
    };
    format!(
        r#"(() => {{
  window.scrollBy({{ top: window.innerHeight * ({pages}) * ({sign}), behavior: 'instant' }});
  return JSON.stringify({{ y: Math.round(window.scrollY), max: Math.max(0, Math.round(document.documentElement.scrollHeight - window.innerHeight)) }});
}})()"#
    )
}
