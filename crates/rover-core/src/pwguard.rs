//! Outbound prompt leak guard (`pwguard`).
//!
//! Scans user messages locally in microseconds before sending to any AI provider, warning if
//! the text appears to contain a password, PIN, 2FA/OTP code, API key, or credentialed URL.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

static PHRASE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?i)\b(?P<what>pass(?:word|wd|phrase|code)?|pwd|pin(?:\s*(?:code|number))?|(?:2fa|mfa|otp|one[\s-]?time|verification|security|login|auth(?:entication)?)\s*code|otp|api[\s_-]?key|secret(?:\s*key)?|access\s*token|(?:auth|bearer)\s*token|recovery\s*(?:code|key|phrase)|seed\s*phrase|login\s*details|credentials)\b(?:\s+(?:for|on|to)\s+[\w.@-]+)?\s*(?P<sep>(?:is|was|would\s+be|will\s+be|should\s+be)\s*[:=]?|=|:|-)\s*["'`“‘]?(?P<value>[^\s"'`”’]{3,})"#,
    )
    .expect("valid PHRASE_RE")
});

static KEYS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?:sk-(?:ant-)?[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{30,}|xox[abpr]-[A-Za-z0-9-]{10,}|glpat-[A-Za-z0-9_-]{16,})",
    )
    .expect("valid KEYS_RE")
});

static URL_LOGIN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b[a-z][a-z0-9+.-]*://[^\s/:@]+:[^\s/@]+@").expect("valid URL_LOGIN_RE")
});

static NOT_VALUES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "a", "an", "the", "my", "your", "not", "too", "very", "so", "now", "still", "also", "just",
        "being", "been", "no", "wrong", "incorrect", "correct", "right", "invalid", "expired",
        "weak", "strong", "secure", "safe", "unsafe", "short", "long", "same", "different",
        "required", "needed", "missing", "forgotten", "lost", "changed", "reset", "saved",
        "stored", "hidden", "shown", "visible", "empty", "blank", "set", "ok", "okay", "fine",
        "good", "bad", "case", "sensitive", "there", "here", "what", "which", "how", "why", "when",
        "where", "that", "this", "it", "stolen", "leaked", "compromised", "breached", "working",
        "accepted", "rejected", "locked", "manager", "generator", "field", "box", "page", "screen",
        "prompt", "and", "or", "but", "if", "for", "on", "in", "of", "to", "with", "you", "me",
        "i", "we", "they", "should", "would", "could", "can", "will", "must", "need",
    ]
    .into_iter()
    .collect()
});

pub fn mask_secret(value: &str) -> String {
    let mut chars = value.chars();
    let first = chars.next().map(|c| c.to_string()).unwrap_or_default();
    let rest_len = chars.count();
    let dots = rest_len.clamp(3, 8);
    format!("{first}{}", "•".repeat(dots))
}

/// Checks if `text` looks like it contains a password, PIN, OTP, API key, or login URL.
/// Returns `Some(reason)` with the sensitive value masked, or `None` if safe.
pub fn check(text: &str) -> Option<String> {
    for caps in PHRASE_RE.captures_iter(text) {
        let raw_val = caps.name("value").map(|m| m.as_str()).unwrap_or("");
        let value = raw_val.trim_end_matches(&['.', ',', ';', '!', '?', ')'][..]);
        let sep = caps.name("sep").map(|m| m.as_str()).unwrap_or("");
        let explicit_punct = sep.ends_with(':') || sep.ends_with('=');
        if value.chars().count() < 3
            || (!explicit_punct && NOT_VALUES.contains(value.to_ascii_lowercase().as_str()))
        {
            continue;
        }
        let what_raw = caps
            .name("what")
            .map(|m| m.as_str())
            .unwrap_or("password")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();
        let what = match what_raw.as_str() {
            "pass" | "passwd" | "pwd" => "password",
            "pin" | "pin code" | "pin number" => "PIN",
            "otp" => "one-time code",
            other => other,
        };
        return Some(format!(
            "it looks like it contains a {what} ({what}: {})",
            mask_secret(value)
        ));
    }
    if let Some(m) = KEYS_RE.find(text) {
        return Some(format!(
            "it looks like it contains an API key or token ({})",
            mask_secret(m.as_str())
        ));
    }
    if URL_LOGIN_RE.is_match(text) {
        return Some("it has a link with a username and password in it".to_string());
    }
    None
}
