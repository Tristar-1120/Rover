//! Serverless Last-Write-Wins (LWW) CRDT Folder Sync (`FolderSync`).
//!
//! Syncs bookmarks, bookmark folders, reading list, and 90-day history across computers via a
//! cloud-synced folder (`<sync_folder>/Rover Sync/<profile>/<device_id>.json`), plus optional
//! passphrase-encrypted sync of passwords, addresses, and payment cards (`vault-salt` / `vault-check`).

use crate::autofill::{AddressEntry, AutofillStore, CardEntry};
use crate::passwords::{LoginEntry, PasswordStore};
use crate::secretbox::{derive_key, new_salt, seal, unseal, KeyringMissing};
use crate::store::{now_ts, Bookmark, HistoryEntry, ReadingItem, Store};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const HISTORY_DAYS: f64 = 90.0;
pub const TOMBSTONE_DAYS: f64 = 120.0;
pub const VAULT_CHECK_TOKEN: &[u8] = b"rover-sync";
pub const LEGACY_VAULT_CHECK_TOKEN: &[u8] = b"pelagic-sync";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRecord {
    pub t: f64,
    pub r: Option<Value>,
}

type CollectionMap = HashMap<String, SyncRecord>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DeviceSyncFile {
    pub device: String,
    pub updated: f64,
    pub open: HashMap<String, CollectionMap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
}

/// Fast 20-hex-character SHA-256 digest of a canonical JSON value.
fn record_digest(val: &Value) -> String {
    let canonical = serde_json::to_vec(val).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    let hex_str = hex::encode(hasher.finalize());
    hex_str[..20].to_string()
}

pub struct FolderSync {
    pub profile_id: String,
    pub store: Store,
    pub passwords: Option<PasswordStore>,
    pub autofill: Option<AutofillStore>,
    vault_key: Option<[u8; 64]>,
}

impl FolderSync {
    pub fn new(
        profile_id: impl Into<String>,
        store: Store,
        passwords: Option<PasswordStore>,
        autofill: Option<AutofillStore>,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            store,
            passwords,
            autofill,
            vault_key: None,
        }
    }

    pub fn device_id(&self) -> String {
        if let Some(id) = self.store.get_str("sync_device_id") {
            if !id.is_empty() {
                return id;
            }
        }
        let mut rng = rand::thread_rng();
        let id: String = (0..12)
            .map(|_| format!("{:x}", rng.gen_range(0..16)))
            .collect();
        self.store.set("sync_device_id", json!(id));
        id
    }

    pub fn sync_dir(&self, base_folder: &Path) -> PathBuf {
        base_folder.join("Rover Sync").join(&self.profile_id)
    }

    /// Configures or verifies the passphrase-derived vault key in `<sync_folder>/Rover Sync/<profile>/`.
    pub fn unlock_vault(
        &mut self,
        base_folder: &Path,
        passphrase: &str,
    ) -> Result<(), KeyringMissing> {
        let dir = self.sync_dir(base_folder);
        fs::create_dir_all(&dir).map_err(|e| KeyringMissing::Io(e.to_string()))?;
        let salt_path = dir.join("vault-salt");
        let check_path = dir.join("vault-check");

        if salt_path.exists() && check_path.exists() {
            let salt_b64 =
                fs::read_to_string(&salt_path).map_err(|e| KeyringMissing::Io(e.to_string()))?;
            let salt = B64
                .decode(salt_b64.trim())
                .map_err(|_| KeyringMissing::DecryptionFailed)?;
            let key = derive_key(passphrase, &salt)?;
            let check_token =
                fs::read_to_string(&check_path).map_err(|e| KeyringMissing::Io(e.to_string()))?;
            let plain = unseal(&check_token, &key)?;
            if plain != VAULT_CHECK_TOKEN && plain != LEGACY_VAULT_CHECK_TOKEN {
                return Err(KeyringMissing::DecryptionFailed);
            }
            self.vault_key = Some(key);
            Ok(())
        } else {
            let salt_b64 = new_salt();
            let salt = B64
                .decode(&salt_b64)
                .map_err(|_| KeyringMissing::DecryptionFailed)?;
            let key = derive_key(passphrase, &salt)?;
            let check_token = seal(VAULT_CHECK_TOKEN, &key);
            fs::write(&salt_path, salt_b64.as_bytes())
                .map_err(|e| KeyringMissing::Io(e.to_string()))?;
            fs::write(&check_path, check_token.as_bytes())
                .map_err(|e| KeyringMissing::Io(e.to_string()))?;
            self.vault_key = Some(key);
            Ok(())
        }
    }

    /// Executes a bidirectional LWW merge across local state and all peer `<device>.json` files.
    pub fn sync_now(&self, base_folder: &Path) -> Result<usize, KeyringMissing> {
        let dir = self.sync_dir(base_folder);
        fs::create_dir_all(&dir).map_err(|e| KeyringMissing::Io(e.to_string()))?;
        let now = now_ts();
        let dev_id = self.device_id();
        let state_path = self.store.root.join("sync_state.json");

        // sync_state.json: { collection: { key: (Option<digest>, timestamp) } }
        let mut prev_state: HashMap<String, HashMap<String, (Option<String>, f64)>> =
            fs::read_to_string(&state_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();

        // 1. Collect local snapshots
        let local_open = self.collect_local_open(now);
        let local_vault = if self.vault_key.is_some() {
            Some(self.collect_local_vault())
        } else {
            None
        };

        // 2. Build local LWW records with tombstones
        let mut my_open = self.diff_collections(&local_open, &mut prev_state, now);
        let mut my_vault = local_vault
            .as_ref()
            .map(|v| self.diff_collections(v, &mut prev_state, now));

        // 3. Read peer device files and merge LWW
        let mut peers_read = 0usize;
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let Ok(raw) = fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(peer) = serde_json::from_str::<DeviceSyncFile>(&raw) else {
                    continue;
                };
                peers_read += 1;
                Self::merge_into(&mut my_open, &peer.open, now);
                if let (Some(ref mut mv), Some(ref key), Some(ref peer_token)) =
                    (&mut my_vault, &self.vault_key, &peer.vault)
                {
                    let bytes = unseal(peer_token, key)?;
                    if let Ok(peer_vault) =
                        serde_json::from_slice::<HashMap<String, CollectionMap>>(&bytes)
                    {
                        Self::merge_into(mv, &peer_vault, now);
                    }
                }
            }
        }

        // 4. Apply merged collections back to local storage and update sync_state
        self.apply_open(&my_open, &mut prev_state);
        if let Some(ref mv) = my_vault {
            self.apply_vault(mv, &mut prev_state)?;
        }

        // 5. Write updated sync_state.json and this device's sync file
        if let Ok(state_bytes) = serde_json::to_vec_pretty(&prev_state) {
            let _ = fs::write(&state_path, state_bytes);
        }

        let vault_ciphertext = match (&my_vault, &self.vault_key) {
            (Some(mv), Some(key)) => {
                let bytes =
                    serde_json::to_vec(mv).map_err(|e| KeyringMissing::Io(e.to_string()))?;
                Some(seal(&bytes, key))
            }
            _ => None,
        };

        let dev_file = DeviceSyncFile {
            device: dev_id.clone(),
            updated: now,
            open: my_open,
            vault: vault_ciphertext,
        };
        let out_path = dir.join(format!("{dev_id}.json"));
        let tmp_path = out_path.with_extension("json.tmp");
        let bytes =
            serde_json::to_vec_pretty(&dev_file).map_err(|e| KeyringMissing::Io(e.to_string()))?;
        fs::write(&tmp_path, bytes).map_err(|e| KeyringMissing::Io(e.to_string()))?;
        fs::rename(&tmp_path, &out_path).map_err(|e| KeyringMissing::Io(e.to_string()))?;

        Ok(peers_read)
    }

    fn collect_local_open(&self, now: f64) -> HashMap<String, HashMap<String, Value>> {
        let mut map = HashMap::new();

        // bookmarks
        let mut bm_map = HashMap::new();
        for b in self.store.bookmarks() {
            bm_map.insert(
                b.url.clone(),
                json!({ "title": b.title, "folder": b.folder }),
            );
        }
        map.insert("bookmarks".into(), bm_map);

        // folders
        let mut f_map = HashMap::new();
        for f in self.store.all_folders() {
            f_map.insert(f, json!({}));
        }
        map.insert("folders".into(), f_map);

        // reading
        let mut r_map = HashMap::new();
        for r in self.store.reading_list() {
            r_map.insert(
                r.url.clone(),
                json!({ "title": r.title, "added": r.added, "read": r.read }),
            );
        }
        map.insert("reading".into(), r_map);

        // history (last 90 days)
        let cutoff = now - HISTORY_DAYS * 86400.0;
        let mut h_map = HashMap::new();
        for h in self.store.history("", 5000) {
            if h.last >= cutoff {
                h_map.insert(
                    h.url.clone(),
                    json!({ "title": h.title, "visits": h.visits, "last": h.last }),
                );
            }
        }
        map.insert("history".into(), h_map);
        map
    }

    fn collect_local_vault(&self) -> HashMap<String, HashMap<String, Value>> {
        let mut map = HashMap::new();
        let mut logins_map = HashMap::new();
        if let Some(ref pw_store) = self.passwords {
            for l in pw_store.all() {
                let key = format!("{}\0{}", l.origin, l.username);
                logins_map.insert(key, serde_json::to_value(&l).unwrap_or(json!({})));
            }
        }
        map.insert("logins".into(), logins_map);

        let mut addr_map = HashMap::new();
        let mut card_map = HashMap::new();
        if let Some(ref af_store) = self.autofill {
            for a in af_store.addresses() {
                addr_map.insert(a.id.clone(), serde_json::to_value(&a).unwrap_or(json!({})));
            }
            for c in af_store.cards() {
                card_map.insert(
                    c.cc_number.clone(),
                    serde_json::to_value(&c).unwrap_or(json!({})),
                );
            }
        }
        map.insert("addresses".into(), addr_map);
        map.insert("cards".into(), card_map);
        map
    }

    fn diff_collections(
        &self,
        current: &HashMap<String, HashMap<String, Value>>,
        prev_state: &mut HashMap<String, HashMap<String, (Option<String>, f64)>>,
        now: f64,
    ) -> HashMap<String, CollectionMap> {
        let mut result = HashMap::new();
        let tombstone_cutoff = now - TOMBSTONE_DAYS * 86400.0;

        for (col_name, items) in current {
            let prev_col = prev_state.entry(col_name.clone()).or_default();
            let mut col_out = CollectionMap::new();

            for (k, val) in items {
                let dig = record_digest(val);
                let ts = match prev_col.get(k) {
                    Some((Some(old_dig), old_ts)) if old_dig == &dig => *old_ts,
                    _ => now,
                };
                col_out.insert(
                    k.clone(),
                    SyncRecord {
                        t: ts,
                        r: Some(val.clone()),
                    },
                );
            }

            // Emit tombstones for keys that existed in prev_col but are now missing locally
            for (k, (old_dig, old_ts)) in prev_col.iter() {
                if !items.contains_key(k) {
                    let ts = if old_dig.is_some() { now } else { *old_ts };
                    if ts >= tombstone_cutoff {
                        col_out.insert(k.clone(), SyncRecord { t: ts, r: None });
                    }
                }
            }
            result.insert(col_name.clone(), col_out);
        }
        result
    }

    fn merge_into(
        target: &mut HashMap<String, CollectionMap>,
        peer: &HashMap<String, CollectionMap>,
        now: f64,
    ) {
        let tombstone_cutoff = now - TOMBSTONE_DAYS * 86400.0;
        for (col, peer_items) in peer {
            let target_col = target.entry(col.clone()).or_default();
            for (k, peer_rec) in peer_items {
                if peer_rec.r.is_none() && peer_rec.t < tombstone_cutoff {
                    continue;
                }
                match target_col.get(k) {
                    Some(local_rec) if local_rec.t >= peer_rec.t => {}
                    _ => {
                        target_col.insert(k.clone(), peer_rec.clone());
                    }
                }
            }
        }
    }

    fn apply_open(
        &self,
        merged: &HashMap<String, CollectionMap>,
        prev_state: &mut HashMap<String, HashMap<String, (Option<String>, f64)>>,
    ) {
        // folders
        if let Some(folders_col) = merged.get("folders") {
            let state_col = prev_state.entry("folders".into()).or_default();
            for (name, rec) in folders_col {
                if let Some(ref val) = rec.r {
                    self.store.add_folder(name);
                    state_col.insert(name.clone(), (Some(record_digest(val)), rec.t));
                } else {
                    self.store.delete_folder(name, false);
                    state_col.insert(name.clone(), (None, rec.t));
                }
            }
        }

        // bookmarks
        if let Some(bm_col) = merged.get("bookmarks") {
            let state_col = prev_state.entry("bookmarks".into()).or_default();
            let mut new_bms = Vec::new();
            for (url, rec) in bm_col {
                if let Some(ref val) = rec.r {
                    let title = val
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or(url)
                        .to_string();
                    let folder = val
                        .get("folder")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    new_bms.push(Bookmark {
                        url: url.clone(),
                        title,
                        folder,
                    });
                    state_col.insert(url.clone(), (Some(record_digest(val)), rec.t));
                } else {
                    state_col.insert(url.clone(), (None, rec.t));
                }
            }
            new_bms.sort_by(|a, b| a.url.cmp(&b.url));
            self.store.set_bookmarks(new_bms);
        }

        // reading
        if let Some(read_col) = merged.get("reading") {
            let state_col = prev_state.entry("reading".into()).or_default();
            let mut new_reading = Vec::new();
            for (url, rec) in read_col {
                if let Some(ref val) = rec.r {
                    let title = val
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or(url)
                        .to_string();
                    let added = val.get("added").and_then(|v| v.as_f64()).unwrap_or(rec.t);
                    let read = val.get("read").and_then(|v| v.as_bool()).unwrap_or(false);
                    new_reading.push(ReadingItem {
                        url: url.clone(),
                        title,
                        added,
                        read,
                    });
                    state_col.insert(url.clone(), (Some(record_digest(val)), rec.t));
                } else {
                    state_col.insert(url.clone(), (None, rec.t));
                }
            }
            new_reading.sort_by(|a, b| {
                b.added
                    .partial_cmp(&a.added)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.store.set_reading_list(new_reading);
        }

        // history
        if let Some(hist_col) = merged.get("history") {
            let state_col = prev_state.entry("history".into()).or_default();
            for (url, rec) in hist_col {
                if let Some(ref val) = rec.r {
                    let entry = HistoryEntry {
                        url: url.clone(),
                        title: val
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        visits: val.get("visits").and_then(|v| v.as_i64()).unwrap_or(1),
                        last: val.get("last").and_then(|v| v.as_f64()).unwrap_or(rec.t),
                    };
                    self.store.upsert_history_raw(&entry);
                    state_col.insert(url.clone(), (Some(record_digest(val)), rec.t));
                } else {
                    self.store.delete_history_url(url);
                    state_col.insert(url.clone(), (None, rec.t));
                }
            }
        }
    }

    fn apply_vault(
        &self,
        merged: &HashMap<String, CollectionMap>,
        prev_state: &mut HashMap<String, HashMap<String, (Option<String>, f64)>>,
    ) -> Result<(), KeyringMissing> {
        if let (Some(ref pw_store), Some(logins_col)) = (&self.passwords, merged.get("logins")) {
            let state_col = prev_state.entry("logins".into()).or_default();
            let mut list = Vec::new();
            for (k, rec) in logins_col {
                if let Some(ref val) = rec.r {
                    if let Ok(entry) = serde_json::from_value::<LoginEntry>(val.clone()) {
                        list.push(entry);
                        state_col.insert(k.clone(), (Some(record_digest(val)), rec.t));
                    }
                } else {
                    state_col.insert(k.clone(), (None, rec.t));
                }
            }
            pw_store.replace_all(list)?;
        }

        if let Some(ref af_store) = self.autofill {
            let mut addrs = af_store.addresses();
            let mut cards = af_store.cards();
            let mut changed = false;

            if let Some(addr_col) = merged.get("addresses") {
                let state_col = prev_state.entry("addresses".into()).or_default();
                let mut new_addrs = Vec::new();
                for (k, rec) in addr_col {
                    if let Some(ref val) = rec.r {
                        if let Ok(a) = serde_json::from_value::<AddressEntry>(val.clone()) {
                            new_addrs.push(a);
                            state_col.insert(k.clone(), (Some(record_digest(val)), rec.t));
                        }
                    } else {
                        state_col.insert(k.clone(), (None, rec.t));
                    }
                }
                addrs = new_addrs;
                changed = true;
            }

            if let Some(card_col) = merged.get("cards") {
                let state_col = prev_state.entry("cards".into()).or_default();
                let mut new_cards = Vec::new();
                for (k, rec) in card_col {
                    if let Some(ref val) = rec.r {
                        if let Ok(c) = serde_json::from_value::<CardEntry>(val.clone()) {
                            new_cards.push(c);
                            state_col.insert(k.clone(), (Some(record_digest(val)), rec.t));
                        }
                    } else {
                        state_col.insert(k.clone(), (None, rec.t));
                    }
                }
                cards = new_cards;
                changed = true;
            }

            if changed {
                af_store.replace_all(addrs, cards)?;
            }
        }
        Ok(())
    }
}
