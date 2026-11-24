//! Password checkup (`pwcheck`): k-anonymity Have I Been Pwned range lookups, reused password detection,
//! and weak password entropy/class evaluation.

use crate::passwords::LoginEntry;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};

pub const RANGE_URL_PREFIX: &str = "https://api.pwnedpasswords.com/range/";

static COMMON_PASSWORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "password",
        "123456",
        "12345678",
        "123456789",
        "qwerty",
        "abc123",
        "111111",
        "letmein",
        "iloveyou",
        "admin",
        "welcome",
        "monkey",
        "dragon",
        "football",
        "baseball",
        "sunshine",
        "princess",
        "password1",
        "qwerty123",
        "1q2w3e4r",
        "000000",
        "trustno1",
        "passw0rd",
        "starwars",
        "whatever",
    ]
    .into_iter()
    .collect()
});

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordIssue {
    pub leaked: u64,
    pub reused: usize,
    pub weak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckupResult {
    pub issues: HashMap<String, PasswordIssue>,
    pub checked: usize,
    pub error: String,
}

pub fn sha1_hex(pw: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(pw.as_bytes());
    hex::encode_upper(hasher.finalize())
}

pub fn leak_count<F>(
    pw: &str,
    ranges: &mut HashMap<String, String>,
    mut fetch: F,
) -> Result<u64, String>
where
    F: FnMut(&str) -> Result<String, String>,
{
    let h = sha1_hex(pw);
    let (prefix, rest) = h.split_at(5);
    if !ranges.contains_key(prefix) {
        let body = fetch(prefix)?;
        ranges.insert(prefix.to_string(), body);
    }
    let body = ranges.get(prefix).unwrap();
    for line in body.lines() {
        if let Some((suffix, count_str)) = line.trim().split_once(':') {
            if suffix.eq_ignore_ascii_case(rest) {
                return Ok(count_str.trim().parse::<u64>().unwrap_or(0));
            }
        }
    }
    Ok(0)
}

pub fn is_weak(pw: &str) -> bool {
    let chars: Vec<char> = pw.chars().collect();
    if chars.len() < 8 || COMMON_PASSWORDS.contains(pw.to_ascii_lowercase().as_str()) {
        return true;
    }
    let unique: HashSet<char> = chars.iter().copied().collect();
    if unique.len() <= 3 {
        return true;
    }
    let has_lower = chars.iter().any(|c| c.is_ascii_lowercase());
    let has_upper = chars.iter().any(|c| c.is_ascii_uppercase());
    let has_digit = chars.iter().any(|c| c.is_ascii_digit());
    let has_symbol = chars.iter().any(|c| !c.is_alphanumeric());
    let kinds = (has_lower as u8) + (has_upper as u8) + (has_digit as u8) + (has_symbol as u8);
    kinds < 2 && chars.len() < 14
}

pub fn checkup<F>(logins: &[LoginEntry], mut fetch: F) -> CheckupResult
where
    F: FnMut(&str) -> Result<String, String>,
{
    let mut by_pw: HashMap<&str, Vec<&str>> = HashMap::new();
    for login in logins {
        if !login.password.is_empty() {
            by_pw
                .entry(login.password.as_str())
                .or_default()
                .push(login.id.as_str());
        }
    }
    let mut ranges = HashMap::new();
    let mut issues = HashMap::new();
    let mut error = String::new();
    let mut checked = 0usize;

    for (pw, ids) in &by_pw {
        checked += ids.len();
        let mut leaked = 0u64;
        if error.is_empty() {
            match leak_count(pw, &mut ranges, &mut fetch) {
                Ok(n) => leaked = n,
                Err(e) => error = format!("couldn't reach the breach database ({e})"),
            }
        }
        let weak = is_weak(pw);
        let reused = ids.len().saturating_sub(1);
        if leaked > 0 || weak || reused > 0 {
            for id in ids {
                issues.insert(
                    (*id).to_string(),
                    PasswordIssue {
                        leaked,
                        reused,
                        weak,
                    },
                );
            }
        }
    }
    CheckupResult {
        issues,
        checked,
        error,
    }
}
