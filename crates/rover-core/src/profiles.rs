//! Multi-profile management (`Profiles`) with isolated storage and shared AI/proxy settings.

use crate::store::Store;
use parking_lot::Mutex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const DEFAULT_ID: &str = "default";

pub const COLORS: &[&str] = &[
    "#DA5647", // Inverted Mars Crimson (default)
    "#EF7560", // Martian Coral
    "#C83F3A", // Olympus Red
    "#FDB78E", // Dusty Peach
    "#FED8B1", // Apricot Sand
    "#B83328", // Canyon Rust
    "#F59E0B", // Solar Flare
    "#47090C", // Deep Maroon
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileMeta {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ProfilesFile {
    pub profiles: Vec<ProfileMeta>,
    pub last: String,
    #[serde(default)]
    pub delete: Vec<String>,
}

#[derive(Clone)]
pub struct Profiles {
    pub root: PathBuf,
    state: Arc<Mutex<ProfilesFile>>,
    stores: Arc<Mutex<HashMap<String, Arc<Store>>>>,
    root_store: Arc<Store>,
}

impl Profiles {
    pub fn new(root_store: Arc<Store>) -> Self {
        let root = root_store.root.clone();
        let file_path = root.join("profiles.json");
        let mut loaded: ProfilesFile = fs::read_to_string(&file_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        if !loaded.profiles.iter().any(|p| p.id == DEFAULT_ID) {
            loaded.profiles.insert(
                0,
                ProfileMeta {
                    id: DEFAULT_ID.to_string(),
                    name: "Personal".to_string(),
                    color: COLORS[0].to_string(),
                },
            );
        }
        if loaded.last.is_empty() || !loaded.profiles.iter().any(|p| p.id == loaded.last) {
            loaded.last = DEFAULT_ID.to_string();
        }

        let mut stores = HashMap::new();
        stores.insert(DEFAULT_ID.to_string(), root_store.clone());

        let mgr = Self {
            root,
            state: Arc::new(Mutex::new(loaded)),
            stores: Arc::new(Mutex::new(stores)),
            root_store,
        };
        mgr.purge_pending();
        mgr.save();
        mgr
    }

    pub fn list(&self) -> Vec<ProfileMeta> {
        self.state.lock().profiles.clone()
    }

    pub fn get(&self, pid: &str) -> Option<ProfileMeta> {
        self.state
            .lock()
            .profiles
            .iter()
            .find(|p| p.id == pid)
            .cloned()
    }

    pub fn last_id(&self) -> String {
        self.state.lock().last.clone()
    }

    pub fn set_last(&self, pid: &str) {
        let mut st = self.state.lock();
        if st.profiles.iter().any(|p| p.id == pid) {
            st.last = pid.to_string();
            drop(st);
            self.save();
        }
    }

    pub fn folder(&self, pid: &str) -> PathBuf {
        if pid == DEFAULT_ID {
            self.root.clone()
        } else {
            self.root.join("profiles").join(pid)
        }
    }

    pub fn store_for(&self, pid: &str) -> Arc<Store> {
        if pid == DEFAULT_ID {
            return self.root_store.clone();
        }
        let mut map = self.stores.lock();
        if let Some(s) = map.get(pid) {
            return s.clone();
        }
        let folder = self.folder(pid);
        let store = Arc::new(Store::with_keyring(
            Some(folder),
            Some(self.root_store.clone()),
            None,
        ));
        map.insert(pid.to_string(), store.clone());
        store
    }

    pub fn add(&self, name: &str, color: Option<&str>) -> ProfileMeta {
        let mut rng = rand::thread_rng();
        let id: String = (0..10)
            .map(|_| format!("{:x}", rng.gen_range(0..16)))
            .collect();
        let mut st = self.state.lock();
        let chosen_color = color
            .filter(|c| !c.trim().is_empty())
            .map(|c| c.to_string())
            .unwrap_or_else(|| COLORS[st.profiles.len() % COLORS.len()].to_string());
        let clean_name = if name.trim().is_empty() {
            format!("Profile {}", st.profiles.len() + 1)
        } else {
            name.trim().to_string()
        };
        let meta = ProfileMeta {
            id: id.clone(),
            name: clean_name,
            color: chosen_color,
        };
        st.profiles.push(meta.clone());
        drop(st);
        let _ = fs::create_dir_all(self.folder(&id));
        self.save();
        meta
    }

    pub fn update(&self, pid: &str, name: &str, color: Option<&str>) -> bool {
        let mut st = self.state.lock();
        if let Some(p) = st.profiles.iter_mut().find(|p| p.id == pid) {
            if !name.trim().is_empty() {
                p.name = name.trim().to_string();
            }
            if let Some(c) = color {
                if !c.trim().is_empty() {
                    p.color = c.trim().to_string();
                }
            }
            drop(st);
            self.save();
            return true;
        }
        false
    }

    pub fn remove(&self, pid: &str) -> bool {
        if pid == DEFAULT_ID {
            return false;
        }
        let mut st = self.state.lock();
        let before = st.profiles.len();
        st.profiles.retain(|p| p.id != pid);
        if st.profiles.len() == before {
            return false;
        }
        if st.last == pid {
            st.last = DEFAULT_ID.to_string();
        }
        if !st.delete.contains(&pid.to_string()) {
            st.delete.push(pid.to_string());
        }
        drop(st);
        self.stores.lock().remove(pid);
        self.save();
        true
    }

    pub fn purge_pending(&self) {
        let mut st = self.state.lock();
        let mut remaining = Vec::new();
        for pid in st.delete.drain(..) {
            if pid == DEFAULT_ID {
                continue;
            }
            let dir = self.folder(&pid);
            if dir.exists() && fs::remove_dir_all(&dir).is_err() {
                remaining.push(pid);
            }
        }
        st.delete = remaining;
    }

    fn save(&self) {
        let st = self.state.lock().clone();
        if let Ok(data) = serde_json::to_vec_pretty(&st) {
            let path = self.root.join("profiles.json");
            let tmp = path.with_extension("tmp");
            if fs::write(&tmp, data).is_ok() {
                let _ = fs::rename(&tmp, &path);
            }
        }
    }
}

pub fn sanitize_profile_dir(root: &Path, pid: &str) -> PathBuf {
    if pid == DEFAULT_ID {
        root.to_path_buf()
    } else {
        root.join("profiles").join(pid)
    }
}
