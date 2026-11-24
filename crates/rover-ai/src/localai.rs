//! Rover Local (`localai`): Zero-model, zero-network, zero-token command interpreter for browser tasks
//! and background page watchers.

use crate::agent::PlannedStep;
use once_cell::sync::Lazy;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

pub const HELP_TEXT: &str = r#"**Rover Local** runs browser commands with no AI and zero tokens. Chain steps with "then".

- `open youtube` · `go to example.com` · `search for mars rovers` · `search perseverance on youtube`
- `click Sign in` · `type hello in Search` · `fill Email with me@x.com and submit` · `select Two in Size`
- `scroll down` · `scroll up 2` · `scroll to bottom` · `back` · `forward` · `reload`
- `press Enter` · `wait 3 seconds` · `new tab` · `close tab` · `bookmark this` · `zoom in` · `read this page`
- `find price on this page` · `reader mode` · `take screenshot`

**Repeat until something happens** (runs quietly in the background, wakes you only when it matches):
- `refresh every 30 seconds until "in stock" appears and notify me`
- `watch this page every 5 minutes for "sale"`
- `check https://example.com every 10 minutes when price drops below 50`
- `reload every minute until the page changes`
- `click Refresh every 20 seconds until "Confirmed" appears and open the page`
- Add `and ask ai` to have Rover AI inspect the page once, only after the match."#;

static SITES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    [
        ("youtube", "https://www.youtube.com"),
        ("google", "https://www.google.com"),
        ("gmail", "https://mail.google.com"),
        ("github", "https://github.com"),
        ("reddit", "https://www.reddit.com"),
        ("wikipedia", "https://en.wikipedia.org"),
        ("amazon", "https://www.amazon.com"),
        ("twitter", "https://x.com"),
        ("x", "https://x.com"),
        ("facebook", "https://www.facebook.com"),
        ("instagram", "https://www.instagram.com"),
        ("netflix", "https://www.netflix.com"),
        ("twitch", "https://www.twitch.tv"),
        ("linkedin", "https://www.linkedin.com"),
        ("ebay", "https://www.ebay.com"),
        ("stackoverflow", "https://stackoverflow.com"),
        ("stack overflow", "https://stackoverflow.com"),
        ("hacker news", "https://news.ycombinator.com"),
        ("hackernews", "https://news.ycombinator.com"),
        ("maps", "https://maps.google.com"),
        ("google maps", "https://maps.google.com"),
        ("chatgpt", "https://chatgpt.com"),
        ("claude", "https://claude.ai"),
        ("gemini", "https://gemini.google.com"),
        ("spotify", "https://open.spotify.com"),
        ("discord", "https://discord.com/app"),
        ("drive", "https://drive.google.com"),
        ("google drive", "https://drive.google.com"),
        ("calendar", "https://calendar.google.com"),
        ("news", "https://news.google.com"),
        ("weather", "https://weather.com"),
        ("duckduckgo", "https://duckduckgo.com"),
        ("bing", "https://www.bing.com"),
        ("nasa", "https://www.nasa.gov"),
    ]
    .into_iter()
    .collect()
});

static SEARCH_SITES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    [
        ("youtube", "https://www.youtube.com/results?search_query=%s"),
        ("amazon", "https://www.amazon.com/s?k=%s"),
        (
            "wikipedia",
            "https://en.wikipedia.org/w/index.php?search=%s",
        ),
        ("github", "https://github.com/search?q=%s"),
        ("reddit", "https://www.reddit.com/search/?q=%s"),
        ("google", "https://www.google.com/search?q=%s"),
        ("ebay", "https://www.ebay.com/sch/i.html?_nkw=%s"),
        ("maps", "https://www.google.com/maps/search/%s"),
        ("google maps", "https://www.google.com/maps/search/%s"),
        ("stackoverflow", "https://stackoverflow.com/search?q=%s"),
        ("stack overflow", "https://stackoverflow.com/search?q=%s"),
        ("bing", "https://www.bing.com/search?q=%s"),
        ("duckduckgo", "https://duckduckgo.com/?q=%s"),
        ("twitter", "https://x.com/search?q=%s"),
        ("x", "https://x.com/search?q=%s"),
    ]
    .into_iter()
    .collect()
});

static HOST_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(https?://\S+|localhost(:\d+)?(/\S*)?|(\d{1,3}\.){3}\d{1,3}(:\d+)?(/\S*)?|[\w-]+(\.[\w-]+)+(/\S*)?)$").unwrap()
});

static EVERY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bevery\s+(?:(\d+(?:\.\d+)?)\s*)?(seconds?|secs?|s|minutes?|mins?|m|hours?|hrs?|h)\b").unwrap()
});

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LocalCommand {
    Help { text: String },
    Steps { steps: Vec<PlannedStep> },
    Watch { spec: Value },
    Unknown { msg: String },
}

pub fn site_url(name: &str) -> Option<String> {
    let n = name.trim().trim_end_matches('.').to_ascii_lowercase();
    if let Some(&u) = SITES.get(n.as_str()) {
        return Some(u.to_string());
    }
    let trimmed = name.trim();
    if HOST_RE.is_match(trimmed) {
        if trimmed.to_ascii_lowercase().starts_with("http://")
            || trimmed.to_ascii_lowercase().starts_with("https://")
        {
            return Some(trimmed.to_string());
        }
        return Some(format!("https://{trimmed}"));
    }
    None
}

fn clean_target(raw: &str) -> String {
    let s = raw
        .trim()
        .trim_matches(&['"', '\'', '“', '”', '‘', '’'][..])
        .trim();
    static SUFFIX_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)\s+(?:button|link|field|box|input|bar|tab)$").unwrap());
    SUFFIX_RE.replace(s, "").trim().to_string()
}

fn unit_seconds(u: &str) -> f64 {
    match u.to_ascii_lowercase().as_str() {
        "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3600.0,
        _ => 1.0,
    }
}

fn split_clauses(text: &str) -> Vec<String> {
    static SPLIT_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\s*(?:,\s*)?(?:\band\s+then\b|\bthen\b|;|\bafter\s+that\b)\s*").unwrap()
    });
    static AND_VERB_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\s+and\s+(click|press|type|scroll|open|go|search|fill|select|wait|reload|refresh)\b").unwrap()
    });

    let mut out = Vec::new();
    for chunk in SPLIT_RE.split(text.trim()) {
        // Handle "and <verb>" without lookahead
        let mut start = 0;
        for caps in AND_VERB_RE.captures_iter(chunk) {
            let full = caps.get(0).unwrap();
            let verb = caps.get(1).unwrap();
            let part = chunk[start..full.start()].trim_matches(&[' ', ',', '.'][..]);
            if !part.is_empty() {
                out.push(part.to_string());
            }
            start = verb.start();
        }
        let rest = chunk[start..].trim_matches(&[' ', ',', '.'][..]);
        if !rest.is_empty() {
            out.push(rest.to_string());
        }
    }
    out
}

fn parse_clause(c: &str) -> Option<PlannedStep> {
    let trimmed = c.trim();
    let low = trimmed.to_ascii_lowercase();

    if matches!(low.as_str(), "back" | "go back" | "please go back") {
        return Some(PlannedStep::new("back", json!({})));
    }
    if matches!(low.as_str(), "forward" | "go forward") {
        return Some(PlannedStep::new("forward", json!({})));
    }
    if matches!(
        low.as_str(),
        "reload" | "refresh" | "reload page" | "refresh page" | "reload the page" | "refresh the page"
    ) {
        return Some(PlannedStep::new("reload", json!({})));
    }
    if matches!(low.as_str(), "new tab") {
        return Some(PlannedStep::new("new_tab", json!({})));
    }
    if matches!(low.as_str(), "close tab" | "close this tab") {
        return Some(PlannedStep::new("close_tab", json!({})));
    }
    if matches!(
        low.as_str(),
        "bookmark" | "bookmark this" | "bookmark this page" | "bookmark page"
    ) {
        return Some(PlannedStep::new("bookmark", json!({})));
    }
    if matches!(low.as_str(), "reader" | "reader mode" | "toggle reader") {
        return Some(PlannedStep::new("reader", json!({})));
    }
    if matches!(
        low.as_str(),
        "screenshot" | "take screenshot" | "capture screenshot"
    ) {
        return Some(PlannedStep::new("screenshot", json!({})));
    }
    if matches!(
        low.as_str(),
        "read"
            | "read page"
            | "read this page"
            | "extract"
            | "get the text of this page"
            | "show me the text of this page"
    ) {
        return Some(PlannedStep::new("extract", json!({})));
    }

    // scroll to top/bottom
    static SCROLL_END_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)^scroll\s+(?:all\s+the\s+way\s+)?(?:to\s+(?:the\s+)?)?(top|bottom)$")
            .unwrap()
    });
    if let Some(caps) = SCROLL_END_RE.captures(&low) {
        let dir = if &caps[1] == "top" { "up" } else { "down" };
        return Some(PlannedStep::new(
            "scroll",
            json!({ "direction": dir, "pages": 60.0 }),
        ));
    }

    // scroll up/down [N]
    static SCROLL_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)^scroll\s+(down|up)(?:\s+(?:by\s+)?(\d+(?:\.\d+)?)(?:\s*(?:pages?|screens?))?)?$")
            .unwrap()
    });
    if let Some(caps) = SCROLL_RE.captures(&low) {
        let dir = caps[1].to_ascii_lowercase();
        let pages = caps
            .get(2)
            .and_then(|m| m.as_str().parse::<f64>().ok())
            .unwrap_or(1.0);
        return Some(PlannedStep::new(
            "scroll",
            json!({ "direction": dir, "pages": pages }),
        ));
    }

    // wait N seconds
    static WAIT_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)^(?:wait|pause|sleep)\s+(?:for\s+)?(\d+(?:\.\d+)?)\s*(\w+)?$").unwrap()
    });
    if let Some(caps) = WAIT_RE.captures(&low) {
        let n = caps[1].parse::<f64>().unwrap_or(1.0);
        let u = caps.get(2).map(|m| m.as_str()).unwrap_or("s");
        return Some(PlannedStep::new(
            "wait",
            json!({ "seconds": n * unit_seconds(u) }),
        ));
    }

    // zoom in/out/reset
    static ZOOM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^zoom\s+(in|out|reset)$").unwrap());
    if let Some(caps) = ZOOM_RE.captures(&low) {
        return Some(PlannedStep::new(
            "zoom",
            json!({ "direction": caps[1].to_ascii_lowercase() }),
        ));
    }

    // press key
    static PRESS_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)^press\s+(?:the\s+)?(?:key\s+)?([\w +]+)$").unwrap());
    if let Some(caps) = PRESS_RE.captures(&low) {
        let k = caps[1].trim();
        let mapped = match k {
            "enter" | "return" => Some("Enter".to_string()),
            "tab" => Some("Tab".to_string()),
            "escape" | "esc" => Some("Escape".to_string()),
            "space" => Some("Space".to_string()),
            "backspace" => Some("Backspace".to_string()),
            "delete" => Some("Delete".to_string()),
            "home" => Some("Home".to_string()),
            "end" => Some("End".to_string()),
            "page down" | "pagedown" => Some("PageDown".to_string()),
            "page up" | "pageup" => Some("PageUp".to_string()),
            "arrow down" | "down" => Some("ArrowDown".to_string()),
            "arrow up" | "up" => Some("ArrowUp".to_string()),
            "arrow left" | "left" => Some("ArrowLeft".to_string()),
            "arrow right" | "right" => Some("ArrowRight".to_string()),
            _ => None,
        };
        if let Some(key_name) = mapped {
            return Some(PlannedStep::new("press_key", json!({ "key": key_name })));
        }
    }

    // find X on this page
    static FIND_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?i)^(?:find|search\s+for|look\s+for)\s+["“”'‘’]?(.+?)["“”'‘’]?\s+(?:on|in)\s+(?:this\s+|the\s+)?page$"#).unwrap()
    });
    if let Some(caps) = FIND_RE.captures(trimmed) {
        return Some(PlannedStep::new("find", json!({ "text": caps[1].trim() })));
    }

    // type / fill (with optional "and submit")
    static SUBMIT_TAIL_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)^(.*?)\s+(?:and\s+)?(?:then\s+)?(?:submit|press\s+enter|hit\s+enter)$")
            .unwrap()
    });
    let (body, submit) = match SUBMIT_TAIL_RE.captures(trimmed) {
        Some(caps) => (caps.get(1).unwrap().as_str(), true),
        None => (trimmed, false),
    };

    static TYPE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?i)^(?:type|enter|write|input)\s+["“”'‘’]?(.+?)["“”'‘’]?\s+(?:in|into|on)\s+(?:the\s+)?["“”'‘’]?(.+?)["“”'‘’]?$"#).unwrap()
    });
    if let Some(caps) = TYPE_RE.captures(body) {
        let text = caps[1].trim_matches(&['"', '\'', '“', '”', '‘', '’'][..]);
        let target = clean_target(&caps[2]);
        return Some(PlannedStep::new(
            "type",
            json!({ "target": target, "text": text, "submit": submit }),
        ));
    }

    static FILL_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?i)^fill(?:\s+in)?\s+(?:the\s+)?["“”'‘’]?(.+?)["“”'‘’]?\s+with\s+["“”'‘’]?(.+?)["“”'‘’]?$"#).unwrap()
    });
    if let Some(caps) = FILL_RE.captures(body) {
        let target = clean_target(&caps[1]);
        let text = caps[2].trim_matches(&['"', '\'', '“', '”', '‘', '’'][..]);
        return Some(PlannedStep::new(
            "type",
            json!({ "target": target, "text": text, "submit": submit }),
        ));
    }

    static SELECT_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?i)^(?:select|choose|pick)\s+["“”'‘’]?(.+?)["“”'‘’]?\s+(?:in|from)\s+(?:the\s+)?["“”'‘’]?(.+?)["“”'‘’]?$"#).unwrap()
    });
    if let Some(caps) = SELECT_RE.captures(trimmed) {
        let val = caps[1].trim_matches(&['"', '\'', '“', '”', '‘', '’'][..]);
        let target = clean_target(&caps[2]);
        return Some(PlannedStep::new(
            "select",
            json!({ "target": target, "value": val }),
        ));
    }

    static CLICK_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?i)^(?:click|tap|hit|press)\s+(?:on\s+)?(?:the\s+)?(?:button\s+|link\s+)?["“”'‘’]?(.+?)["“”'‘’]?$"#).unwrap()
    });
    if let Some(caps) = CLICK_RE.captures(trimmed) {
        let target = clean_target(&caps[1]);
        return Some(PlannedStep::new("click", json!({ "target": target })));
    }

    // search (with optional site)
    static SEARCH_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?i)^(?:search|look\s+up|google|find)\s+(?:for\s+)?["“”'‘’]?(.+?)["“”'‘’]?(?:\s+(?:on|in|at)\s+(.+))?$"#).unwrap()
    });
    if let Some(caps) = SEARCH_RE.captures(trimmed) {
        let q = caps[1].trim();
        let site = caps
            .get(2)
            .map(|m| m.as_str().trim().to_ascii_lowercase())
            .unwrap_or_default();
        if !site.is_empty() {
            if let Some(&tmpl) = SEARCH_SITES.get(site.as_str()) {
                let encoded = utf8_percent_encode(q, NON_ALPHANUMERIC).to_string();
                return Some(PlannedStep::new(
                    "navigate",
                    json!({ "url": tmpl.replace("%s", &encoded) }),
                ));
            }
            return Some(PlannedStep::new(
                "search",
                json!({ "query": format!("{q} {site}") }),
            ));
        }
        return Some(PlannedStep::new("search", json!({ "query": q })));
    }

    // open / go to
    static OPEN_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?i)^(?:open|go\s+to|goto|visit|navigate\s+to|launch|load|take\s+me\s+to)\s+(?:the\s+)?(.+)$",
        )
        .unwrap()
    });
    if let Some(caps) = OPEN_RE.captures(trimmed) {
        let what = caps[1]
            .trim()
            .trim_matches(&['"', '\'', '“', '”', '‘', '’'][..]);
        if let Some(u) = site_url(what) {
            return Some(PlannedStep::new("navigate", json!({ "url": u })));
        }
        return Some(PlannedStep::new("search", json!({ "query": what })));
    }

    if let Some(u) = site_url(trimmed) {
        return Some(PlannedStep::new("navigate", json!({ "url": u })));
    }
    None
}

pub fn parse_steps(text: &str) -> Result<Vec<PlannedStep>, String> {
    let mut steps = Vec::new();
    for part in split_clauses(text) {
        match parse_clause(&part) {
            Some(s) => steps.push(s),
            None => return Err(part),
        }
    }
    if steps.is_empty() {
        return Err(text.to_string());
    }
    Ok(steps)
}

fn parse_condition(after: &str) -> Option<Value> {
    static NUM_BELOW_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)(?:(\w+)\s+)?(?:drops?\s+|goes?\s+|falls?\s+|gets?\s+|is\s+|becomes?\s+)?(?:below|under|less\s+than|lower\s+than|<)\s*\$?(\d+(?:[.,]\d+)?)").unwrap()
    });
    if let Some(caps) = NUM_BELOW_RE.captures(after) {
        let num = caps[2].replace(',', "").parse::<f64>().ok()?;
        let near = caps
            .get(1)
            .map(|m| m.as_str())
            .filter(|w| matches!(w.to_ascii_lowercase().as_str(), "price" | "cost" | "score" | "count"));
        let mut obj = json!({ "type": "number", "op": "<", "num": num });
        if let Some(n) = near {
            obj["near"] = json!(n);
        }
        return Some(obj);
    }

    static NUM_ABOVE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)(?:(\w+)\s+)?(?:rises?\s+|goes?\s+|gets?\s+|is\s+|becomes?\s+)?(?:above|over|more\s+than|greater\s+than|higher\s+than|>)\s*\$?(\d+(?:[.,]\d+)?)").unwrap()
    });
    if let Some(caps) = NUM_ABOVE_RE.captures(after) {
        let num = caps[2].replace(',', "").parse::<f64>().ok()?;
        let near = caps
            .get(1)
            .map(|m| m.as_str())
            .filter(|w| matches!(w.to_ascii_lowercase().as_str(), "price" | "cost" | "score" | "count"));
        let mut obj = json!({ "type": "number", "op": ">", "num": num });
        if let Some(n) = near {
            obj["near"] = json!(n);
        }
        return Some(obj);
    }

    static GONE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?i)(?:until|when|if|for|once)\s+(?:the\s+page\s+(?:shows|says|contains)\s+)?["“”'‘’]?(.+?)["“”'‘’]?\s+(?:disappears?|is\s+gone|goes\s+away|vanishes)"#).unwrap()
    });
    if let Some(caps) = GONE_RE.captures(after) {
        return Some(json!({ "type": "gone", "value": caps[1].trim() }));
    }

    static CHANGED_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)(?:until|when|if|once)\s+(?:the\s+)?(?:page|site|it|something|anything)\s+(?:changes?|updates?|is\s+different)|\bany\s+change\b|\bchanges?\b").unwrap()
    });
    if CHANGED_RE.is_match(after) {
        return Some(json!({ "type": "changed" }));
    }

    static TEXT_APPEARS_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?i)(?:until|when|if|once)\s+(?:the\s+page\s+(?:shows|says|contains)\s+)?["“”'‘’]?(.+?)["“”'‘’]?\s+(?:appears?|shows?(?:\s+up)?|is\s+(?:there|available|visible|in\s+stock|back)|contains?|exists?|comes?\s+up|loads?)"#).unwrap()
    });
    if let Some(caps) = TEXT_APPEARS_RE.captures(after) {
        return Some(json!({ "type": "text", "value": caps[1].trim() }));
    }

    static QUOTED_FOR_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?i)(?:for|until|when|says)\s+["“”'‘’]([^"“”'‘’]+?)["“”'‘’]"#).unwrap()
    });
    if let Some(caps) = QUOTED_FOR_RE.captures(after) {
        return Some(json!({ "type": "text", "value": caps[1].trim() }));
    }
    None
}

fn parse_then(after: &str) -> Vec<String> {
    let low = after.to_ascii_lowercase();
    let mut then = Vec::new();
    if ["notify", "tell", "alert", "ping", "let me know", "message", "remind"]
        .iter()
        .any(|w| low.contains(w))
    {
        then.push("notify".to_string());
    }
    if ["beep", "sound", "ring"].iter().any(|w| low.contains(w)) {
        then.push("beep".to_string());
    }
    if ["open it", "open the page", "open that", "open the site", "take me there", "show me"]
        .iter()
        .any(|w| low.contains(w))
    {
        then.push("open".to_string());
    }
    if ["ask ai", "ask the ai", "summarize", "summarise", "explain"]
        .iter()
        .any(|w| low.contains(w))
    {
        then.push("ask_ai".to_string());
    }
    then
}

pub fn parse_local_command(text: &str, current_url: &str) -> LocalCommand {
    let t = text.trim();
    if t.is_empty()
        || matches!(
            t.to_ascii_lowercase().as_str(),
            "help" | "?" | "commands" | "what can you do" | "what can you do?"
        )
    {
        return LocalCommand::Help {
            text: HELP_TEXT.to_string(),
        };
    }

    if let Some(caps) = EVERY_RE.captures(t) {
        let full = caps.get(0).unwrap();
        let before = t[..full.start()].trim_matches(&[' ', ',', '.', '-'][..]);
        let after = t[full.end()..].trim_matches(&[' ', ',', '.'][..]);
        let n = caps
            .get(1)
            .and_then(|m| m.as_str().parse::<f64>().ok())
            .unwrap_or(1.0);
        let sec = n * unit_seconds(&caps[2]);

        let cond = parse_condition(after)
            .or_else(|| parse_condition(before))
            .unwrap_or_else(|| json!({ "type": "changed" }));
        let mut then = parse_then(after);
        if then.is_empty() {
            then = parse_then(before);
        }
        if then.is_empty() {
            then.push("notify".to_string());
        }

        static PRE_CLEAN_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?i)^(?:please\s+)?(?:watch|monitor|keep\s+checking|check|keep\s+an\s+eye\s+on|poll)?\s*(?:this\s+page|the\s+page|this\s+site|it)?\s*").unwrap()
        });
        let mut pre = PRE_CLEAN_RE.replace(before, "").trim().to_string();
        let mut url = String::new();
        if let Some(first_tok) = pre.split_whitespace().next() {
            if let Some(u) = site_url(first_tok) {
                url = u;
                pre = pre[first_tok.len()..]
                    .trim_matches(&[' ', ','][..])
                    .to_string();
            }
        }
        if let Some(idx) = pre.to_ascii_lowercase().find("until ") {
            pre.truncate(idx);
        }
        if let Some(idx) = pre.to_ascii_lowercase().find("when ") {
            pre.truncate(idx);
        }
        let pre = pre.trim_matches(&[' ', ','][..]);

        let mut steps = if pre.is_empty() {
            Vec::new()
        } else {
            match parse_steps(pre) {
                Ok(s) => s,
                Err(bad) => {
                    return LocalCommand::Unknown {
                        msg: format!(
                            "I don't understand “{bad}”. Say `help` to see what I can do."
                        ),
                    }
                }
            }
        };

        if !steps.is_empty() && steps[0].action == "navigate" && url.is_empty() {
            url = steps[0]
                .args
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            steps.remove(0);
        }
        steps.retain(|s| s.action != "reload");
        let reload = steps.is_empty();
        if url.is_empty() {
            url = current_url.to_string();
        }
        if !url.to_ascii_lowercase().starts_with("http://")
            && !url.to_ascii_lowercase().starts_with("https://")
        {
            return LocalCommand::Unknown {
                msg: "Open the page you want to watch first, or include its address.".to_string(),
            };
        }
        let short_host = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("www.");
        let short_name: String = short_host.chars().take(30).collect();

        return LocalCommand::Watch {
            spec: json!({
                "name": format!("Watch {short_name}"),
                "url": url,
                "every": sec,
                "condition": cond,
                "steps": steps,
                "then": then,
                "repeat": t.to_ascii_lowercase().contains("every time") || t.to_ascii_lowercase().contains("repeatedly"),
                "reload": reload,
            }),
        };
    }

    match parse_steps(t) {
        Ok(steps) => LocalCommand::Steps { steps },
        Err(bad) => LocalCommand::Unknown {
            msg: format!(
                "I don't understand “{bad}”. Say `help` to see what I can do — or use Rover Agent mode for anything open-ended."
            ),
        },
    }
}
