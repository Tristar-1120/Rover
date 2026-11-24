//! Encrypted password vault (`PasswordStore`), strict origin matching (HTTP->HTTPS upgrade allowed,
//! HTTPS->HTTP downgrade blocked), two-step username flow cache, CSV importer, and secure file shredder.

use crate::secretbox::{KeyringMissing, SecretBox};
use crate::store::now_ts;
use parking_lot::Mutex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoginEntry {
    pub id: String,
    pub origin: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub note: String,
    pub created: f64,
    pub updated: f64,
    pub last_used: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PasswordVaultData {
    #[serde(default)]
    pub logins: Vec<LoginEntry>,
    #[serde(default)]
    pub never: Vec<String>,
}

pub fn normalize_origin(raw_url: &str) -> String {
    if let Ok(u) = Url::parse(raw_url.trim()) {
        let scheme = u.scheme().to_ascii_lowercase();
        if matches!(scheme.as_str(), "http" | "https") {
            if let Some(host) = u.host_str() {
                let host_low = host.to_ascii_lowercase();
                return match u.port() {
                    Some(port) => format!("{scheme}://{host_low}:{port}"),
                    None => format!("{scheme}://{host_low}"),
                };
            }
        }
    }
    String::new()
}

fn bare_host(host: &str) -> &str {
    host.strip_prefix("www.").unwrap_or(host)
}

/// Checks if a saved credential for `saved_origin` may be offered on `page_url`.
/// - Matches exact origin or `www.` twin.
/// - Allows upgrading saved `http://` credentials to `https://` on the same host/port.
/// - Strictly refuses downgrading saved `https://` credentials to `http://`.
pub fn matches_origin(saved_origin: &str, page_url: &str) -> bool {
    let saved = match Url::parse(&normalize_origin(saved_origin)) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let page = match Url::parse(&normalize_origin(page_url)) {
        Ok(u) => u,
        Err(_) => return false,
    };
    if saved.scheme() == "https" && page.scheme() != "https" {
        return false;
    }
    let sh = saved.host_str().unwrap_or("");
    let ph = page.host_str().unwrap_or("");
    if bare_host(sh) != bare_host(ph) {
        return false;
    }
    // Ports must match if explicit
    if saved.port().is_some() && page.port().is_some() && saved.port() != page.port() {
        return false;
    }
    true
}

fn new_id() -> String {
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| format!("{:x}", rng.gen_range(0..16)))
        .collect()
}

/// Overwrites a temporary password CSV file with zeroes and syncs to disk before unlinking.
pub fn shred_file(path: &Path) -> std::io::Result<()> {
    if let Ok(meta) = fs::metadata(path) {
        let len = meta.len() as usize;
        if len > 0 {
            if let Ok(mut file) = OpenOptions::new().write(true).open(path) {
                let zeroes = vec![0u8; len.min(65536)];
                let mut remaining = len;
                while remaining > 0 {
                    let chunk = remaining.min(zeroes.len());
                    let _ = file.write_all(&zeroes[..chunk]);
                    remaining -= chunk;
                }
                let _ = file.sync_all();
            }
        }
    }
    fs::remove_file(path)
}

#[derive(Clone)]
pub struct PasswordStore {
    pub path: PathBuf,
    secretbox: SecretBox,
    data: Arc<Mutex<PasswordVaultData>>,
    unreadable: Arc<Mutex<bool>>,
    error: Arc<Mutex<String>>,
    /// Tracks step-1 username submissions (`origin -> (username, timestamp)`) for 600s two-page login flows.
    username_seen: Arc<Mutex<HashMap<String, (String, f64)>>>,
}

impl PasswordStore {
    pub fn new(folder: impl AsRef<Path>, secretbox: SecretBox) -> Self {
        let path = folder.as_ref().join("passwords.dat");
        let store = Self {
            path,
            secretbox,
            data: Arc::new(Mutex::new(PasswordVaultData::default())),
            unreadable: Arc::new(Mutex::new(false)),
            error: Arc::new(Mutex::new(String::new())),
            username_seen: Arc::new(Mutex::new(HashMap::new())),
        };
        store.load();
        store
    }

    pub fn is_unreadable(&self) -> bool {
        *self.unreadable.lock()
    }

    pub fn error(&self) -> String {
        self.error.lock().clone()
    }

    pub fn load(&self) {
        if !self.path.exists() {
            *self.data.lock() = PasswordVaultData::default();
            *self.unreadable.lock() = false;
            return;
        }
        match fs::read_to_string(&self.path) {
            Ok(raw) => match self.secretbox.unprotect(&raw) {
                Ok(bytes) => match serde_json::from_slice::<PasswordVaultData>(&bytes) {
                    Ok(parsed) => {
                        *self.data.lock() = parsed;
                        *self.unreadable.lock() = false;
                        *self.error.lock() = String::new();
                    }
                    Err(e) => {
                        *self.unreadable.lock() = true;
                        *self.error.lock() = e.to_string();
                    }
                },
                Err(e) => {
                    *self.unreadable.lock() = true;
                    *self.error.lock() = e.to_string();
                }
            },
            Err(e) => {
                *self.unreadable.lock() = true;
                *self.error.lock() = e.to_string();
            }
        }
    }

    fn save(&self) -> Result<(), KeyringMissing> {
        if *self.unreadable.lock() {
            return Err(KeyringMissing::Unavailable(
                "Password store is locked or unreadable; refusing to overwrite".into(),
            ));
        }
        let payload = serde_json::to_vec(&*self.data.lock())
            .map_err(|e| KeyringMissing::Io(e.to_string()))?;
        let token = self.secretbox.protect(&payload)?;
        let tmp = self.path.with_extension("dat.tmp");
        fs::write(&tmp, token.as_bytes()).map_err(|e| KeyringMissing::Io(e.to_string()))?;
        fs::rename(&tmp, &self.path).map_err(|e| KeyringMissing::Io(e.to_string()))?;
        Ok(())
    }

    pub fn all(&self) -> Vec<LoginEntry> {
        self.data.lock().logins.clone()
    }

    pub fn for_url(&self, page_url: &str) -> Vec<LoginEntry> {
        self.data
            .lock()
            .logins
            .iter()
            .filter(|e| matches_origin(&e.origin, page_url))
            .cloned()
            .collect()
    }

    pub fn is_never(&self, page_url: &str) -> bool {
        let orig = normalize_origin(page_url);
        if orig.is_empty() {
            return false;
        }
        self.data
            .lock()
            .never
            .iter()
            .any(|saved| matches_origin(saved, &orig))
    }

    pub fn set_never(&self, page_url: &str, never: bool) -> Result<(), KeyringMissing> {
        let orig = normalize_origin(page_url);
        if orig.is_empty() {
            return Ok(());
        }
        {
            let mut d = self.data.lock();
            d.never.retain(|x| x != &orig);
            if never {
                d.never.push(orig);
            }
        }
        self.save()
    }

    pub fn never_sites(&self) -> Vec<String> {
        self.data.lock().never.clone()
    }

    pub fn add_or_update(
        &self,
        url: &str,
        username: &str,
        password: &str,
        note: &str,
    ) -> Result<LoginEntry, KeyringMissing> {
        let orig = normalize_origin(url);
        if orig.is_empty() || password.is_empty() {
            return Err(KeyringMissing::Io("Origin and password are required".into()));
        }
        let ts = now_ts();
        let entry = {
            let mut d = self.data.lock();
            if let Some(existing) = d
                .logins
                .iter_mut()
                .find(|e| e.origin == orig && e.username == username)
            {
                existing.password = password.to_string();
                if !note.is_empty() {
                    existing.note = note.to_string();
                }
                existing.updated = ts;
                existing.clone()
            } else {
                let item = LoginEntry {
                    id: new_id(),
                    origin: orig,
                    username: username.to_string(),
                    password: password.to_string(),
                    note: note.to_string(),
                    created: ts,
                    updated: ts,
                    last_used: 0.0,
                };
                d.logins.push(item.clone());
                item
            }
        };
        self.save()?;
        Ok(entry)
    }

    pub fn remove(&self, id: &str) -> Result<bool, KeyringMissing> {
        let removed = {
            let mut d = self.data.lock();
            let before = d.logins.len();
            d.logins.retain(|e| e.id != id);
            d.logins.len() < before
        };
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn replace_all(&self, logins: Vec<LoginEntry>) -> Result<(), KeyringMissing> {
        self.data.lock().logins = logins;
        self.save()
    }

    /// Records a username submitted on step 1 of a multi-page login flow (valid for 600 seconds).
    pub fn record_username_step(&self, page_url: &str, username: &str) {
        let orig = normalize_origin(page_url);
        if !orig.is_empty() && !username.trim().is_empty() {
            self.username_seen
                .lock()
                .insert(orig, (username.trim().to_string(), now_ts()));
        }
    }

    /// Resolves a username for a password-only submission if step 1 happened within 600s on the same origin.
    pub fn resolve_two_step_username(&self, page_url: &str) -> Option<String> {
        let orig = normalize_origin(page_url);
        let mut map = self.username_seen.lock();
        if let Some((user, ts)) = map.get(&orig) {
            if now_ts() - *ts <= 600.0 {
                return Some(user.clone());
            }
            map.remove(&orig);
        }
        None
    }

    /// Imports logins from a Google/Chrome/Edge/Firefox CSV export string.
    /// Returns `(added_or_updated_count, skipped_count)`.
    pub fn import_csv_str(&self, csv_text: &str) -> Result<(usize, usize), KeyringMissing> {
        let rows = parse_simple_csv(csv_text);
        if rows.len() < 2 {
            return Ok((0, 0));
        }
        let header: Vec<String> = rows[0]
            .iter()
            .map(|h| h.trim().trim_start_matches('\u{feff}').to_ascii_lowercase())
            .collect();
        let url_idx = header
            .iter()
            .position(|h| matches!(h.as_str(), "url" | "origin" | "website" | "form_action"));
        let user_idx = header
            .iter()
            .position(|h| matches!(h.as_str(), "username" | "user" | "login" | "email"));
        let pw_idx = header
            .iter()
            .position(|h| matches!(h.as_str(), "password" | "pass"));
        let note_idx = header
            .iter()
            .position(|h| matches!(h.as_str(), "note" | "notes" | "comment"));

        let (Some(ui), Some(pi)) = (url_idx, pw_idx) else {
            return Ok((0, rows.len().saturating_sub(1)));
        };

        let mut imported = 0usize;
        let mut skipped = 0usize;
        for row in &rows[1..] {
            let url = row.get(ui).map(|s| s.trim()).unwrap_or("");
            let pw = row.get(pi).map(|s| s.as_str()).unwrap_or("");
            let user = user_idx
                .and_then(|i| row.get(i))
                .map(|s| s.trim())
                .unwrap_or("");
            let note = note_idx
                .and_then(|i| row.get(i))
                .map(|s| s.trim())
                .unwrap_or("");
            let orig = normalize_origin(url);
            if orig.is_empty() || pw.is_empty() {
                skipped += 1;
                continue;
            }
            self.add_or_update(&orig, user, pw, note)?;
            imported += 1;
        }
        Ok((imported, skipped))
    }

    /// Imports a CSV file and optionally shreds + deletes it immediately (e.g. Google Password Manager export).
    pub fn import_csv_file(
        &self,
        path: &Path,
        shred_after: bool,
    ) -> Result<(usize, usize), KeyringMissing> {
        let text = fs::read_to_string(path).map_err(|e| KeyringMissing::Io(e.to_string()))?;
        let res = self.import_csv_str(&text);
        if shred_after {
            let _ = shred_file(path);
        }
        res
    }
}

fn parse_simple_csv(input: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                }
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    if row.iter().any(|s| !s.is_empty()) {
                        rows.push(std::mem::take(&mut row));
                    } else {
                        row.clear();
                    }
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        if row.iter().any(|s| !s.is_empty()) {
            rows.push(row);
        }
    }
    rows
}
