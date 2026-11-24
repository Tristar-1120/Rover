//! Dynamic Rover New Tab Homepage (`rover:start` / `start.html`).
//!
//! 1-to-1 copy of Pelagic's clean New Tab page layout (`homepage.py`), rebranded to **Rover**
//! with the exact inverted Mars color palette (`#C83F3A` / `#EF7560` / `#DA5647` / `#FDB78E` /
//! `#FED8B1` / `#47090C`) and ZERO pre-populated shortcuts by default (only `+ Add shortcut`).

use rover_core::Store;
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

pub const ROVER_ICON_PNG_BYTES: &[u8] = include_bytes!("../../../assets/rover-icon.png");

pub const ROVER_LOGO_SVG: &str = include_str!("../../../assets/rover.svg");

pub fn rover_icon_data_uri() -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = ROVER_ICON_PNG_BYTES;
    let mut b64 = String::with_capacity(((bytes.len() + 2) / 3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        b64.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        b64.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        b64.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        b64.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    format!("data:image/png;base64,{b64}")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shortcut {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomepageConfig {
    pub shortcuts_mode: String, // "mine" or "most_visited"
    pub shortcuts: Vec<Shortcut>,
    pub bg_kind: String, // "mars", "color", "image"
    pub bg_color: String,
    pub bg_image: String,
    pub bg_darken: u8,
    pub bg_blur: u8,
    pub show_logo: bool,
    pub show_tagline: bool,
    pub show_search: bool,
    pub show_shortcuts: bool,
    pub show_clock: bool,
    pub show_footer: bool,
    pub tagline: String,
}

impl Default for HomepageConfig {
    fn default() -> Self {
        Self {
            shortcuts_mode: "mine".into(),
            shortcuts: Vec::new(), // 1-to-1 with Pelagic: starts with ZERO pre-populated shortcuts
            bg_kind: "mars".into(),
            bg_color: "#2b0d0e".into(),
            bg_image: String::new(),
            bg_darken: 25,
            bg_blur: 0,
            show_logo: true,
            show_tagline: true,
            show_search: true,
            show_shortcuts: true,
            show_clock: false,
            show_footer: true,
            tagline: "Search the web — or press Ctrl+Shift+A and let AI do the browsing.".into(),
        }
    }
}

impl HomepageConfig {
    pub fn load(store: &Store) -> Self {
        let mut cfg = Self::default();
        if let Some(m) = store.get_str("home_shortcuts_mode") {
            cfg.shortcuts_mode = m;
        }
        if let Some(v) = store.get("home_shortcuts") {
            if let Ok(list) = serde_json::from_value::<Vec<Shortcut>>(v) {
                cfg.shortcuts = list;
            }
        }
        if let Some(k) = store.get_str("home_bg_kind") {
            cfg.bg_kind = k;
        }
        if let Some(c) = store.get_str("home_bg_color") {
            cfg.bg_color = c;
        }
        if let Some(img) = store.get_str("home_bg_image") {
            cfg.bg_image = img;
        }
        cfg.bg_darken = store.get_i64("home_bg_darken", 25).clamp(0, 90) as u8;
        cfg.bg_blur = store.get_i64("home_bg_blur", 0).clamp(0, 40) as u8;
        cfg.show_logo = store.get_bool("home_show_logo", true);
        cfg.show_tagline = store.get_bool("home_show_tagline", true);
        cfg.show_search = store.get_bool("home_show_search", true);
        cfg.show_shortcuts = store.get_bool("home_show_shortcuts", true);
        cfg.show_clock = store.get_bool("home_show_clock", false);
        cfg.show_footer = store.get_bool("home_show_footer", true);
        if let Some(t) = store.get_str("home_tagline") {
            cfg.tagline = t;
        }
        cfg
    }

    pub fn save(&self, store: &Store) {
        store.set("home_shortcuts_mode", json!(self.shortcuts_mode));
        store.set("home_shortcuts", json!(self.shortcuts));
        store.set("home_bg_kind", json!(self.bg_kind));
        store.set("home_bg_color", json!(self.bg_color));
        store.set("home_bg_image", json!(self.bg_image));
        store.set("home_bg_darken", json!(self.bg_darken));
        store.set("home_bg_blur", json!(self.bg_blur));
        store.set("home_show_logo", json!(self.show_logo));
        store.set("home_show_tagline", json!(self.show_tagline));
        store.set("home_show_search", json!(self.show_search));
        store.set("home_show_shortcuts", json!(self.show_shortcuts));
        store.set("home_show_clock", json!(self.show_clock));
        store.set("home_show_footer", json!(self.show_footer));
        store.set("home_tagline", json!(self.tagline));
    }
}

pub fn add_shortcut(store: &Store, title: &str, url: &str) {
    let mut cfg = HomepageConfig::load(store);
    let clean_url = if url.contains(':') {
        url.trim().to_string()
    } else {
        format!("https://{}", url.trim())
    };
    let clean_title = if title.trim().is_empty() {
        clean_url.clone()
    } else {
        title.trim().to_string()
    };
    if let Some(existing) = cfg.shortcuts.iter_mut().find(|s| s.url == clean_url) {
        existing.title = clean_title;
    } else if cfg.shortcuts.len() < 20 {
        cfg.shortcuts.push(Shortcut {
            title: clean_title,
            url: clean_url,
        });
    }
    cfg.save(store);
}

pub fn remove_shortcut(store: &Store, index: usize) {
    let mut cfg = HomepageConfig::load(store);
    if index < cfg.shortcuts.len() {
        cfg.shortcuts.remove(index);
        cfg.save(store);
    }
}

/// Parses internal homepage actions (`?rover=add`, `?rover=edit&i=0`, `?rover=customize`, `?rover=playground`,
/// plus legacy `?pelagic=...`).
pub fn parse_home_action(url_str: &str) -> Option<(String, Option<usize>)> {
    let u = Url::parse(url_str).ok()?;
    let mut action = None;
    let mut idx = None;
    for (k, v) in u.query_pairs() {
        if k == "rover" || k == "pelagic" {
            action = Some(v.into_owned());
        } else if k == "i" {
            idx = v.parse::<usize>().ok();
        }
    }
    action.map(|a| (a, idx))
}

pub fn build_homepage_html(store: &Store, engine_name: &str, search_action_url: &str) -> String {
    let cfg = HomepageConfig::load(store);
    let bg_css = match cfg.bg_kind.as_str() {
        "color" => format!("background: {};", cfg.bg_color),
        "image" if !cfg.bg_image.is_empty() => format!(
            "background: #000 url('{}') center/cover no-repeat;",
            cfg.bg_image
        ),
        // Inverted Pelagic radial gradient (#0B4A6B -> #04162B inverted into deep Mars crimson night)
        _ => "background: radial-gradient(circle at 50% 25%, #6B1E1B, #24090B 65%);".to_string(),
    };
    let dim = if cfg.bg_kind == "image" {
        (cfg.bg_darken as f32) / 100.0
    } else {
        0.0
    };
    let blur = if cfg.bg_kind == "image" {
        cfg.bg_blur
    } else {
        0
    };

    let mine = cfg.shortcuts_mode != "most_visited";
    let items: Vec<Shortcut> = if mine {
        cfg.shortcuts.clone()
    } else {
        store
            .most_visited(8)
            .into_iter()
            .map(|h| Shortcut {
                title: if h.title.is_empty() {
                    h.url.clone()
                } else {
                    h.title
                },
                url: h.url,
            })
            .collect()
    };

    let mut tiles_html = String::new();
    for (i, sc) in items.iter().enumerate() {
        let letter = sc
            .title
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        let more = if mine {
            format!(r#"<a class="more" href="?rover=edit&amp;i={i}" title="Edit shortcut">⋮</a>"#)
        } else {
            String::new()
        };
        tiles_html.push_str(&format!(
            r#"<div class="tile"><a class="go" href="{url}" title="{url}"><span class="ic"><span class="letter">{letter}</span></span><span class="name">{title}</span></a>{more}</div>"#,
            url = html_escape(&sc.url),
            letter = html_escape(&letter),
            title = html_escape(&sc.title.chars().take(24).collect::<String>()),
            more = more
        ));
    }
    if mine && items.len() < 20 {
        tiles_html.push_str(
            r#"<div class="tile"><a class="go" href="?rover=add" title="Add shortcut"><span class="ic add">+</span><span class="name">Add shortcut</span></a></div>"#,
        );
    }

    let mut parts = String::new();
    if cfg.show_clock {
        parts.push_str(r#"<div class="clock" id="clock"></div><div class="date" id="date"></div>"#);
    }
    if cfg.show_logo {
        let icon_uri = rover_icon_data_uri();
        parts.push_str(&format!(
            r#"<div class="mark"><img src="{icon_uri}" width="88" height="88" alt="Rover"></div><h1>Rover</h1>"#
        ));
    }
    if cfg.show_tagline {
        parts.push_str(&format!("<p>{}</p>", html_escape(&cfg.tagline)));
    }
    if cfg.show_search {
        parts.push_str(&format!(
            r#"<div class="search-wrap"><form id="home-form" action="{action}" method="get"><input id="home-q" name="q" placeholder="Search {engine}" autofocus autocomplete="off"></form><div class="ac-list" id="ac-list" role="listbox"></div></div>"#,
            action = html_escape(search_action_url),
            engine = html_escape(engine_name)
        ));
    }
    if cfg.show_shortcuts {
        parts.push_str(&format!(r#"<div class="tiles">{tiles_html}</div>"#));
    }
    if cfg.show_footer {
        parts.push_str(&format!(
            "<small>Searching with {}</small>",
            html_escape(engine_name)
        ));
    }

    let clock_js = if cfg.show_clock {
        r#"<script>(function(){function t(){var d=new Date();
document.getElementById('clock').textContent=d.toLocaleTimeString([],{hour:'numeric',minute:'2-digit'});
document.getElementById('date').textContent=d.toLocaleDateString([],{weekday:'long',month:'long',day:'numeric'});}
t();setInterval(t,1000);})();</script>"#
    } else {
        ""
    };
    let logo_js = if cfg.show_logo {
        r#"<script>(function(){var m=document.querySelector('.mark'),n=0,t;if(!m)return;
m.addEventListener('click',function(){n++;clearTimeout(t);m.style.transform='rotate('+(n%2?-1:1)*n*3+'deg)';
t=setTimeout(function(){n=0;m.style.transform='';},1200);if(n>=5)location.href='?rover=playground';});})();</script>"#
    } else {
        ""
    };
    let search_ac_js = if cfg.show_search {
        r#"<script>(function(){
var inp=document.getElementById('home-q'),list=document.getElementById('ac-list'),form=document.getElementById('home-form');
if(!inp||!list||!form)return;
var tok=new URLSearchParams(location.search).get('t')||'';
var items=[],sel=-1,typed='',timer=null,seq=0;
function esc(s){return String(s||'').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');}
function hide(){items=[];sel=-1;list.classList.remove('open');list.innerHTML='';}
function iconFor(k){if(k==='bookmark')return '<span class="ac-ic bm">★</span>';if(k==='history')return '<span class="ac-ic hist">↺</span>';if(k==='visit')return '<span class="ac-ic visit">🌐</span>';return '<span class="ac-ic">🔍</span>';}
function render(){
  if(!items.length){hide();return;}
  list.innerHTML=items.map(function(it,i){
    var sub=(it.kind==='bookmark'||it.kind==='history')?'<span class="ac-url">'+esc(it.url.replace(/^https?:\/\/(www\.)?/i,''))+'</span>':'';
    return '<div class="ac-row'+(i===sel?' on':'')+'" data-i="'+i+'">'+iconFor(it.kind)+'<span class="ac-title">'+esc(it.title)+'</span>'+sub+'</div>';
  }).join('');
  list.classList.add('open');
}
function goItem(it){
  if(!it||!it.url)return;
  hide();
  if(tok&&/^rover:/i.test(it.url)){
    fetch('/api/cmd?t='+encodeURIComponent(tok),{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({cmd:'navigate',url:it.url})}).catch(function(){});
  }else{
    location.href=it.url;
  }
}
function query(q,live){
  if(!tok)return;
  var id=++seq;
  fetch('/api/suggest?t='+encodeURIComponent(tok)+'&mode=search&live='+(live?1:0)+'&q='+encodeURIComponent(q))
    .then(function(r){return r.ok?r.json():null;})
    .then(function(d){
      if(!d||id!==seq||inp.value.trim()!==q)return;
      if(d.enabled===false){hide();return;}
      items=d.suggestions||[];
      if(sel>=items.length)sel=-1;
      render();
    }).catch(function(){});
}
inp.addEventListener('input',function(){
  typed=inp.value;sel=-1;
  var q=typed.trim();
  clearTimeout(timer);
  if(!q){hide();return;}
  query(q,false);
  timer=setTimeout(function(){query(q,true);},90);
});
inp.addEventListener('keydown',function(e){
  if(e.key==='ArrowDown'){
    if(!items.length)return;
    e.preventDefault();
    sel=(sel+1)>=items.length?-1:sel+1;
    inp.value=sel>=0?(items[sel].fill_text||items[sel].title):typed;
    render();
  }else if(e.key==='ArrowUp'){
    if(!items.length)return;
    e.preventDefault();
    sel=sel<=0?(sel===0?-1:items.length-1):sel-1;
    inp.value=sel>=0?(items[sel].fill_text||items[sel].title):typed;
    render();
  }else if(e.key==='Tab'&&items.length){
    e.preventDefault();
    var pick=items[sel>=0?sel:0];
    inp.value=pick.fill_text||pick.title;
    typed=inp.value;sel=-1;
  }else if(e.key==='Escape'){
    if(items.length){e.preventDefault();inp.value=typed;hide();}
  }else if(e.key==='Enter'){
    if(sel>=0&&items[sel]){e.preventDefault();goItem(items[sel]);return;}
    var v=inp.value.trim();
    if(/^https?:\/\//i.test(v)){e.preventDefault();location.href=v;}
    else if(/^rover:/i.test(v)&&tok){e.preventDefault();goItem({url:v});}
    else if(v&&!/\s/.test(v)&&v.indexOf('.')>0){e.preventDefault();location.href='https://'+v;}
    else{hide();}
  }
});
list.addEventListener('mousedown',function(e){
  var row=e.target.closest('.ac-row');
  if(!row)return;
  e.preventDefault();
  var idx=parseInt(row.getAttribute('data-i'),10);
  if(!isNaN(idx)&&items[idx])goItem(items[idx]);
});
list.addEventListener('mousemove',function(e){
  var row=e.target.closest('.ac-row');
  if(!row)return;
  var idx=parseInt(row.getAttribute('data-i'),10);
  if(!isNaN(idx)&&idx!==sel){sel=idx;render();}
});
document.addEventListener('click',function(e){if(!e.target.closest('.search-wrap'))hide();});
})();</script>"#
    } else {
        ""
    };

    format!(
        r#"<!doctype html><meta charset="utf-8"><title>New tab</title>
<style>
html,body{{margin:0;min-height:100vh}}
body{{display:flex;flex-direction:column;align-items:center;justify-content:center;color:#FEF0E4;
font-family:'Segoe UI',system-ui,sans-serif;background:#24090B}}
.bg{{position:fixed;inset:-{inset}px;{bg_css};filter:blur({blur}px);z-index:-2}}
.dim{{position:fixed;inset:0;background:rgba(0,0,0,{dim});z-index:-1}}
.mark{{margin-bottom:14px;filter:drop-shadow(0 14px 26px rgba(0,0,0,.4));transition:transform .15s;cursor:pointer}}
.mark img{{width:88px;height:88px;border-radius:50%;display:block}}
h1{{font-weight:600;font-size:46px;letter-spacing:-.5px;margin:0 0 6px;text-shadow:0 2px 12px rgba(0,0,0,.25);color:#FED8B1}}
p{{color:#D9A798;margin:0 0 26px}}
.clock{{font-size:64px;font-weight:300;letter-spacing:-1px;text-shadow:0 2px 14px rgba(0,0,0,.3);color:#FED8B1}}
.date{{color:#D9A798;margin:0 0 26px}}
.search-wrap{{position:relative;width:min(620px,86vw)}}
form{{width:100%}}input{{width:100%;box-sizing:border-box;padding:15px 20px;border-radius:30px;
border:1px solid rgba(253,183,142,.28);background:rgba(54,14,16,.84);color:#FEF0E4;font-size:17px;outline:none;
backdrop-filter:blur(8px)}}input::placeholder{{color:#C48F81}}input:focus{{border-color:#EF7560;box-shadow:0 0 0 3px #EF756044}}
.ac-list{{display:none;position:absolute;top:calc(100% + 6px);left:0;right:0;z-index:40;
background:rgba(32,10,12,.97);border:1px solid rgba(253,183,142,.32);border-radius:16px;padding:6px;
box-shadow:0 16px 40px rgba(0,0,0,.62);backdrop-filter:blur(12px);max-height:340px;overflow-y:auto}}
.ac-list.open{{display:block}}
.ac-row{{display:flex;align-items:center;gap:10px;padding:9px 12px;border-radius:10px;cursor:pointer;font-size:14.5px;color:#FEF0E4}}
.ac-row:hover,.ac-row.on{{background:rgba(239,117,96,.22);color:#fff}}
.ac-ic{{width:18px;text-align:center;flex-shrink:0;font-size:13px;opacity:.88}}
.ac-ic.bm{{color:#FDB78E}}.ac-ic.hist{{color:#C9A497}}.ac-ic.visit{{color:#FED8B1}}
.ac-title{{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1}}
.ac-url{{color:#C48F81;font-size:12px;max-width:44%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex-shrink:0}}
.tiles{{display:flex;flex-wrap:wrap;gap:8px;justify-content:center;margin-top:30px;max-width:720px}}
.tile{{position:relative;width:104px;border-radius:12px}}.tile:hover{{background:rgba(255,255,255,.08)}}
.go{{display:flex;flex-direction:column;align-items:center;gap:8px;padding:14px 6px 10px;color:inherit;text-decoration:none}}
.ic{{width:48px;height:48px;border-radius:50%;background:rgba(54,14,16,.85);display:flex;align-items:center;
justify-content:center;border:1px solid rgba(253,183,142,.22)}}
.letter{{font-weight:700;font-size:20px;color:#FDB78E}}.add{{font-size:26px;color:#FDB78E;font-weight:300}}
.name{{font-size:12.5px;max-width:96px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;text-shadow:0 1px 6px rgba(0,0,0,.35)}}
.more{{position:absolute;top:4px;right:4px;width:22px;height:22px;border-radius:50%;display:none;align-items:center;
justify-content:center;color:#FEF0E4;text-decoration:none;font-weight:700}}
.tile:hover .more{{display:flex}}.more:hover{{background:rgba(255,255,255,.15)}}
small{{margin-top:30px;color:#D9A798;opacity:.85}}
.customize{{position:fixed;right:18px;bottom:16px;padding:8px 14px;border-radius:18px;color:#FEF0E4;
background:rgba(54,14,16,.65);text-decoration:none;font-size:13px;border:1px solid rgba(253,183,142,.25)}}
.customize:hover{{border-color:#EF7560}}</style>
<div class="bg"></div><div class="dim"></div>
{parts}
<a class="customize" href="?rover=customize" title="Change the background, shortcuts and what shows here">✎ Customize</a>
{clock_js}{logo_js}{search_ac_js}"#,
        inset = blur * 2
    )
}

pub fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
