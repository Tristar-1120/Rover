//! Built-in Website Dark Mode (`DarkMode`, `rover:darkmode`).
//!
//! Supports zero-cost Chromium `ForceDarkMode` and intelligent CSS/DOM dark mode with
//! per-site and subdomain opt-out (`darkmode_off_sites`), canvas/slide/diagram preservation
//! (`data-rover-keep`), and legacy userscript localStorage migration.

use crate::adblock::host_parents;
use rover_core::Store;
use serde_json::json;
use url::Url;

pub const SOURCE_ID: &str = "rover:darkmode";
pub const LEGACY_SOURCE_ID: &str = "pelagic:darkmode";

#[derive(Clone)]
pub struct DarkMode {
    pub store: Store,
}

impl DarkMode {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub fn is_enabled(&self) -> bool {
        self.store.get_bool("darkmode", false)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.store.set("darkmode", json!(enabled));
    }

    pub fn off_sites(&self) -> Vec<String> {
        self.store
            .get("darkmode_off_sites")
            .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
            .unwrap_or_default()
    }

    pub fn is_site_off(&self, url_or_host: &str) -> bool {
        let host = Self::extract_host(url_or_host);
        if host.is_empty() {
            return false;
        }
        let off = self.off_sites();
        host_parents(&host)
            .into_iter()
            .any(|p| off.iter().any(|s| s.eq_ignore_ascii_case(p)))
    }

    pub fn set_site_off(&self, url_or_host: &str, off: bool) {
        let host = Self::extract_host(url_or_host);
        if host.is_empty() {
            return;
        }
        let mut list = self.off_sites();
        list.retain(|s| !s.eq_ignore_ascii_case(&host));
        if off {
            list.push(host);
        }
        self.store.set("darkmode_off_sites", json!(list));
    }

    pub fn should_darken(&self, page_url: &str) -> bool {
        if !self.is_enabled() {
            return false;
        }
        if page_url.starts_with("rover:")
            || page_url.starts_with("pelagic:")
            || page_url.starts_with("atlas:")
        {
            return false;
        }
        !self.is_site_off(page_url)
    }

    fn extract_host(url_or_host: &str) -> String {
        Url::parse(url_or_host)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
            .unwrap_or_else(|| {
                url_or_host
                    .trim()
                    .trim_end_matches('.')
                    .to_ascii_lowercase()
            })
            .strip_prefix("www.")
            .unwrap_or(&Url::parse(url_or_host)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
                .unwrap_or_else(|| {
                    url_or_host
                        .trim()
                        .trim_end_matches('.')
                        .to_ascii_lowercase()
                }))
            .to_string()
    }
}
