//! Persistent profile storage (`Store`), bookmarks, reading list, SQLite history (WAL mode),
//! and encrypted shared API keys (`keys.dat`).

use crate::secretbox::{KeyringBackend, KeyringMissing, SecretBox};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub static SHARED_SETTINGS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "selected",
        "overrides",
        "custom",
        "multi",
        "model_cache",
        "price_overrides",
        "max_steps",
        "vision",
        "vision_every_step",
        "humanize",
        "batch",
        "token_saver",
        "delta_snapshots",
        "compact_dsl",
        "semantic_prune",
        "guard",
        "confirm_all",
        "context_limit",
        "core_instructions",
        "core_sets",
        "core_active",
        "core_enabled",
        "proxy",
    ]
    .into_iter()
    .collect()
});

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadingItem {
    pub url: String,
    pub title: String,
    pub added: f64,
    pub read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visits: i64,
    pub last: f64,
}

pub fn now_ts() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn default_data_dir() -> PathBuf {
    for var in ["ROVER_HOME", "ATLASBROWSER_HOME"] {
        if let Ok(p) = env::var(var) {
            if !p.trim().is_empty() {
                return PathBuf::from(p);
            }
        }
    }
    if let Ok(xdg) = env::var("XDG_DATA_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("Rover");
        }
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".local/share/Rover");
    }
    PathBuf::from(".rover_data")
}

#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
    pub shared: Option<Arc<Store>>,
    settings: Arc<Mutex<HashMap<String, Value>>>,
    bookmarks: Arc<Mutex<Vec<Bookmark>>>,
    folders: Arc<Mutex<Vec<String>>>,
    db: Arc<Mutex<Connection>>,
    secretbox: SecretBox,
    keys: Arc<Mutex<Option<HashMap<String, String>>>>,
    key_error: Arc<Mutex<String>>,
}

impl Store {
    pub fn new(folder: Option<PathBuf>, shared: Option<Arc<Store>>) -> Self {
        Self::with_keyring(folder, shared, None)
    }

    pub fn with_keyring(
        folder: Option<PathBuf>,
        shared: Option<Arc<Store>>,
        keyring: Option<Arc<dyn KeyringBackend>>,
    ) -> Self {
        let root = folder.unwrap_or_else(default_data_dir);
        let _ = fs::create_dir_all(&root);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&root, fs::Permissions::from_mode(0o700));
        }

        let secretbox = match keyring {
            Some(kr) => SecretBox::with_keyring(&root, Some(kr)),
            None => SecretBox::new(&root),
        };

        let mut defaults: HashMap<String, Value> = HashMap::new();
        defaults.insert("selected".into(), json!("gemini"));
        defaults.insert("search_engine".into(), json!("duckduckgo"));
        defaults.insert("home".into(), json!("rover:start"));
        defaults.insert("home_shortcuts".into(), json!([]));
        defaults.insert("home_shortcuts_mode".into(), json!("mine"));
        defaults.insert("home_show_footer".into(), json!(true));
        defaults.insert("bookmarks_bar".into(), json!(true));
        defaults.insert("autocomplete".into(), json!(true));
        defaults.insert("tab_sleep".into(), json!(true));
        defaults.insert("tab_sleep_minutes".into(), json!(30));
        defaults.insert("tab_preview".into(), json!(true));
        defaults.insert("vertical_tabs".into(), json!(false));
        defaults.insert("auto_group_sites".into(), json!(false));
        defaults.insert("adblock".into(), json!(true));
        defaults.insert("darkmode".into(), json!(false));
        defaults.insert("safebrowsing".into(), json!(true));
        defaults.insert("save_passwords".into(), json!(true));
        defaults.insert("autofill_enabled".into(), json!(true));
        defaults.insert("spellcheck".into(), json!(false));
        defaults.insert("spellcheck_lang".into(), json!("en-US"));
        defaults.insert("max_steps".into(), json!(30));
        defaults.insert("vision".into(), json!(false));
        defaults.insert("vision_every_step".into(), json!(false));
        defaults.insert("humanize".into(), json!(false));
        defaults.insert("batch".into(), json!(true));
        defaults.insert("token_saver".into(), json!(true));
        defaults.insert("delta_snapshots".into(), json!(true));
        defaults.insert("compact_dsl".into(), json!(true));
        defaults.insert("semantic_prune".into(), json!(true));
        defaults.insert("no_context".into(), json!(false));
        defaults.insert("guard".into(), json!(true));
        defaults.insert("confirm_all".into(), json!(false));
        defaults.insert("context_limit".into(), json!(128_000));
        defaults.insert(
            "memory_file".into(),
            json!(root.join("Rover Memory.md").to_string_lossy().to_string()),
        );

        let settings_path = root.join("settings.json");
        if let Ok(raw) = fs::read_to_string(&settings_path) {
            if let Ok(loaded) = serde_json::from_str::<HashMap<String, Value>>(&raw) {
                for (k, v) in loaded {
                    defaults.insert(k, v);
                }
            }
        }

        let bm_path = root.join("bookmarks.json");
        let bookmarks: Vec<Bookmark> = fs::read_to_string(&bm_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let folders_path = root.join("bookmark_folders.json");
        let folders: Vec<String> = fs::read_to_string(&folders_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let db_path = root.join("history.db");
        let conn = Connection::open(&db_path).expect("Failed to open SQLite history.db");
        let _ = conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS history(
                 url TEXT PRIMARY KEY,
                 title TEXT NOT NULL,
                 visits INTEGER NOT NULL,
                 last REAL NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_history_last ON history(last DESC);",
        );

        let store = Self {
            root,
            shared,
            settings: Arc::new(Mutex::new(defaults)),
            bookmarks: Arc::new(Mutex::new(bookmarks)),
            folders: Arc::new(Mutex::new(folders)),
            db: Arc::new(Mutex::new(conn)),
            secretbox,
            keys: Arc::new(Mutex::new(None)),
            key_error: Arc::new(Mutex::new(String::new())),
        };
        store.load_keys();
        store
    }

    pub fn secretbox(&self) -> &SecretBox {
        &self.secretbox
    }

    pub fn key_error(&self) -> String {
        if let Some(shared) = &self.shared {
            return shared.key_error();
        }
        self.key_error.lock().clone()
    }

    fn load_keys(&self) {
        if self.shared.is_some() {
            return;
        }
        let path = self.root.join("keys.dat");
        if !path.exists() {
            *self.keys.lock() = Some(HashMap::new());
            return;
        }
        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                *self.key_error.lock() = e.to_string();
                return;
            }
        };
        match self.secretbox.unprotect(&raw) {
            Ok(bytes) => {
                if let Ok(map) = serde_json::from_slice::<HashMap<String, String>>(&bytes) {
                    *self.keys.lock() = Some(map);
                    *self.key_error.lock() = String::new();
                } else {
                    *self.keys.lock() = None;
                    *self.key_error.lock() = "Corrupted keys.dat payload".into();
                }
            }
            Err(e) => {
                *self.keys.lock() = None;
                *self.key_error.lock() = e.to_string();
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        if let Some(shared) = &self.shared {
            if SHARED_SETTINGS.contains(key) {
                return shared.get(key);
            }
        }
        self.settings.lock().get(key).cloned()
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        self.get(key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
    }

    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
    }

    pub fn get_i64(&self, key: &str, default: i64) -> i64 {
        self.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
    }

    pub fn set(&self, key: &str, value: Value) {
        if let Some(shared) = &self.shared {
            if SHARED_SETTINGS.contains(key) {
                shared.set(key, value);
                return;
            }
        }
        self.settings.lock().insert(key.to_string(), value);
        self.save_settings();
    }

    fn save_settings(&self) {
        let map = self.settings.lock().clone();
        if let Ok(data) = serde_json::to_vec_pretty(&map) {
            Self::write_atomic(&self.root.join("settings.json"), &data);
        }
    }

    fn write_atomic(path: &Path, data: &[u8]) {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, data).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }

    // --------------------------------------------------------- API Keys
    pub fn api_key(&self, provider_id: &str, env_vars: &[&str]) -> String {
        if let Some(shared) = &self.shared {
            return shared.api_key(provider_id, env_vars);
        }
        if let Some(map) = self.keys.lock().as_ref() {
            if let Some(k) = map.get(provider_id) {
                if !k.is_empty() {
                    return k.clone();
                }
            }
        }
        for var in env_vars {
            if let Ok(val) = env::var(var) {
                if !val.trim().is_empty() {
                    return val.trim().to_string();
                }
            }
        }
        String::new()
    }

    pub fn set_api_key(&self, provider_id: &str, key: &str) -> Result<(), KeyringMissing> {
        if let Some(shared) = &self.shared {
            return shared.set_api_key(provider_id, key);
        }
        let mut guard = self.keys.lock();
        if guard.is_none() {
            let path = self.root.join("keys.dat");
            if path.exists() {
                return Err(KeyringMissing::Unavailable(self.key_error.lock().clone()));
            }
            *guard = Some(HashMap::new());
        }
        let map = guard.as_mut().unwrap();
        let trimmed = key.trim();
        if trimmed.is_empty() {
            map.remove(provider_id);
        } else {
            map.insert(provider_id.to_string(), trimmed.to_string());
        }
        let serialized =
            serde_json::to_vec(map).map_err(|e| KeyringMissing::Io(e.to_string()))?;
        let token = self.secretbox.protect(&serialized)?;
        Self::write_atomic(&self.root.join("keys.dat"), token.as_bytes());
        *self.key_error.lock() = String::new();
        Ok(())
    }

    // --------------------------------------------------------- Bookmarks & Folders
    pub fn bookmarks(&self) -> Vec<Bookmark> {
        self.bookmarks.lock().clone()
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.bookmarks.lock().iter().any(|b| b.url == url)
    }

    pub fn add_bookmark(&self, url: &str, title: &str, folder: &str) {
        let mut bms = self.bookmarks.lock();
        let clean_title = if title.trim().is_empty() {
            url.to_string()
        } else {
            title.trim().to_string()
        };
        if let Some(existing) = bms.iter_mut().find(|b| b.url == url) {
            existing.title = clean_title;
            existing.folder = folder.trim().to_string();
        } else {
            bms.push(Bookmark {
                url: url.to_string(),
                title: clean_title,
                folder: folder.trim().to_string(),
            });
        }
        drop(bms);
        if !folder.trim().is_empty() {
            self.add_folder(folder.trim());
        }
        self.save_bookmarks();
    }

    pub fn remove_bookmark(&self, url: &str) {
        self.bookmarks.lock().retain(|b| b.url != url);
        self.save_bookmarks();
    }

    pub fn set_bookmarks(&self, list: Vec<Bookmark>) {
        *self.bookmarks.lock() = list;
        self.save_bookmarks();
    }

    fn save_bookmarks(&self) {
        let list = self.bookmarks.lock().clone();
        if let Ok(data) = serde_json::to_vec_pretty(&list) {
            Self::write_atomic(&self.root.join("bookmarks.json"), &data);
        }
    }

    pub fn all_folders(&self) -> Vec<String> {
        let mut out = self.folders.lock().clone();
        for b in self.bookmarks.lock().iter() {
            if !b.folder.is_empty() && !out.contains(&b.folder) {
                out.push(b.folder.clone());
            }
        }
        out
    }

    pub fn add_folder(&self, name: &str) {
        let clean = name.trim();
        if clean.is_empty() {
            return;
        }
        let mut f = self.folders.lock();
        if !f.iter().any(|x| x == clean) {
            f.push(clean.to_string());
            if let Ok(data) = serde_json::to_vec_pretty(&*f) {
                Self::write_atomic(&self.root.join("bookmark_folders.json"), &data);
            }
        }
    }

    pub fn rename_folder(&self, old: &str, new: &str) {
        let new_clean = new.trim();
        if new_clean.is_empty() || old == new_clean {
            return;
        }
        {
            let mut f = self.folders.lock();
            for item in f.iter_mut() {
                if item == old {
                    *item = new_clean.to_string();
                }
            }
            let mut dedup = Vec::new();
            for item in f.drain(..) {
                if !dedup.contains(&item) {
                    dedup.push(item);
                }
            }
            *f = dedup;
            if let Ok(data) = serde_json::to_vec_pretty(&*f) {
                Self::write_atomic(&self.root.join("bookmark_folders.json"), &data);
            }
        }
        {
            let mut bms = self.bookmarks.lock();
            for b in bms.iter_mut() {
                if b.folder == old {
                    b.folder = new_clean.to_string();
                }
            }
        }
        self.save_bookmarks();
    }

    pub fn delete_folder(&self, name: &str, delete_bookmarks: bool) {
        {
            let mut f = self.folders.lock();
            f.retain(|x| x != name);
            if let Ok(data) = serde_json::to_vec_pretty(&*f) {
                Self::write_atomic(&self.root.join("bookmark_folders.json"), &data);
            }
        }
        {
            let mut bms = self.bookmarks.lock();
            if delete_bookmarks {
                bms.retain(|b| b.folder != name);
            } else {
                for b in bms.iter_mut() {
                    if b.folder == name {
                        b.folder.clear();
                    }
                }
            }
        }
        self.save_bookmarks();
    }

    // --------------------------------------------------------- Reading List
    pub fn reading_list(&self) -> Vec<ReadingItem> {
        self.get("reading_list")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    pub fn set_reading_list(&self, items: Vec<ReadingItem>) {
        if let Ok(v) = serde_json::to_value(items) {
            self.set("reading_list", v);
        }
    }

    pub fn add_reading(&self, url: &str, title: &str) {
        let mut items = self.reading_list();
        if let Some(existing) = items.iter_mut().find(|i| i.url == url) {
            existing.title = if title.trim().is_empty() {
                url.to_string()
            } else {
                title.to_string()
            };
        } else {
            items.insert(
                0,
                ReadingItem {
                    url: url.to_string(),
                    title: if title.trim().is_empty() {
                        url.to_string()
                    } else {
                        title.to_string()
                    },
                    added: now_ts(),
                    read: false,
                },
            );
        }
        self.set_reading_list(items);
    }

    pub fn mark_reading(&self, url: &str, read: bool) {
        let mut items = self.reading_list();
        for item in items.iter_mut() {
            if item.url == url {
                item.read = read;
            }
        }
        self.set_reading_list(items);
    }

    pub fn remove_reading(&self, url: &str) {
        let mut items = self.reading_list();
        items.retain(|i| i.url != url);
        self.set_reading_list(items);
    }

    // --------------------------------------------------------- SQLite History
    pub fn add_history(&self, url: &str, title: &str) {
        if url.is_empty()
            || url.starts_with("rover:")
            || url.starts_with("pelagic:")
            || url.starts_with("atlas:")
            || url.starts_with("about:")
            || url.starts_with("data:")
        {
            return;
        }
        let ts = now_ts();
        let conn = self.db.lock();
        let _ = conn.execute(
            "INSERT INTO history(url, title, visits, last) VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(url) DO UPDATE SET
               title = CASE WHEN excluded.title != '' THEN excluded.title ELSE history.title END,
               visits = history.visits + 1,
               last = excluded.last",
            params![url, title, ts],
        );
    }

    pub fn upsert_history_raw(&self, entry: &HistoryEntry) {
        let conn = self.db.lock();
        let _ = conn.execute(
            "INSERT INTO history(url, title, visits, last) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(url) DO UPDATE SET
               title = excluded.title,
               visits = MAX(history.visits, excluded.visits),
               last = MAX(history.last, excluded.last)",
            params![entry.url, entry.title, entry.visits, entry.last],
        );
    }

    pub fn history(&self, query: &str, limit: usize) -> Vec<HistoryEntry> {
        let conn = self.db.lock();
        let mut out = Vec::new();
        if query.trim().is_empty() {
            let mut stmt = match conn
                .prepare("SELECT url, title, visits, last FROM history ORDER BY last DESC LIMIT ?1")
            {
                Ok(s) => s,
                Err(_) => return out,
            };
            if let Ok(rows) = stmt.query_map(params![limit as i64], |r| {
                Ok(HistoryEntry {
                    url: r.get(0)?,
                    title: r.get(1)?,
                    visits: r.get(2)?,
                    last: r.get(3)?,
                })
            }) {
                for r in rows.flatten() {
                    out.push(r);
                }
            };
        } else {
            let pattern = format!("%{}%", query.trim());
            let mut stmt = match conn.prepare(
                "SELECT url, title, visits, last FROM history
                 WHERE url LIKE ?1 OR title LIKE ?1
                 ORDER BY last DESC LIMIT ?2",
            ) {
                Ok(s) => s,
                Err(_) => return out,
            };
            if let Ok(rows) = stmt.query_map(params![pattern, limit as i64], |r| {
                Ok(HistoryEntry {
                    url: r.get(0)?,
                    title: r.get(1)?,
                    visits: r.get(2)?,
                    last: r.get(3)?,
                })
            }) {
                for r in rows.flatten() {
                    out.push(r);
                }
            };
        }
        out
    }

    pub fn most_visited(&self, limit: usize) -> Vec<HistoryEntry> {
        let conn = self.db.lock();
        let mut out = Vec::new();
        let mut stmt = match conn.prepare(
            "SELECT url, title, visits, last FROM history
             ORDER BY visits DESC, last DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return out,
        };
        if let Ok(rows) = stmt.query_map(params![limit as i64], |r| {
            Ok(HistoryEntry {
                url: r.get(0)?,
                title: r.get(1)?,
                visits: r.get(2)?,
                last: r.get(3)?,
            })
        }) {
            for r in rows.flatten() {
                out.push(r);
            }
        };
        out
    }

    pub fn delete_history_url(&self, url: &str) {
        let conn = self.db.lock();
        let _ = conn.execute("DELETE FROM history WHERE url = ?1", params![url]);
    }

    pub fn clear_history(&self) {
        let conn = self.db.lock();
        let _ = conn.execute("DELETE FROM history", []);
    }
}
