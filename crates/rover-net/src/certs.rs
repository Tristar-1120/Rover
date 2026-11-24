//! Session-Scoped TLS Certificate Trust Manager (`CertTrustStore`) for local HTTPS devices.
//!
//! Ensures self-signed certificate trust decisions (`https://10.0.255.1`) are:
//! 1. Strictly scoped to `(host, sha256_fingerprint)` and expire when Rover closes.
//! 2. Isolated between normal browsing profiles and private windows.
//! 3. Immediately revoked if the device presents a different certificate fingerprint.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertErrorInfo {
    pub url: String,
    pub host: String,
    pub error_description: String,
    pub sha256_fingerprint: String,
    pub is_private: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertDecision {
    TrustedForSession,
    AskUser { fingerprint_changed: bool },
    RejectInvalid,
}

pub fn format_sha256_fingerprint(der_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(der_bytes);
    let digest = hasher.finalize();
    digest
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[derive(Clone, Default)]
pub struct CertTrustStore {
    /// `(profile_scope, host) -> sha256_fingerprint`
    trusted: Arc<RwLock<HashMap<(String, String), String>>>,
}

impl CertTrustStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn scope_key(profile_id: &str, is_private: bool) -> String {
        if is_private {
            format!("private:{profile_id}")
        } else {
            format!("normal:{profile_id}")
        }
    }

    pub fn evaluate(
        &self,
        profile_id: &str,
        is_private: bool,
        url_str: &str,
        fingerprint: &str,
    ) -> CertDecision {
        let Ok(u) = Url::parse(url_str) else {
            return CertDecision::RejectInvalid;
        };
        if u.scheme() != "https" || fingerprint.trim().is_empty() {
            return CertDecision::RejectInvalid;
        }
        let Some(host) = u.host_str().map(|h| h.to_ascii_lowercase()) else {
            return CertDecision::RejectInvalid;
        };
        let key = (Self::scope_key(profile_id, is_private), host);
        let guard = self.trusted.read();
        match guard.get(&key) {
            Some(saved_fp) if saved_fp == fingerprint => CertDecision::TrustedForSession,
            Some(_) => CertDecision::AskUser {
                fingerprint_changed: true,
            },
            None => CertDecision::AskUser {
                fingerprint_changed: false,
            },
        }
    }

    pub fn trust_for_session(
        &self,
        profile_id: &str,
        is_private: bool,
        url_or_host: &str,
        fingerprint: &str,
    ) {
        let host = Url::parse(url_or_host)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
            .unwrap_or_else(|| url_or_host.trim().to_ascii_lowercase());
        if !host.is_empty() && !fingerprint.trim().is_empty() {
            let key = (Self::scope_key(profile_id, is_private), host);
            self.trusted.write().insert(key, fingerprint.to_string());
        }
    }

    pub fn clear_private(&self, profile_id: &str) {
        let prefix = Self::scope_key(profile_id, true);
        self.trusted.write().retain(|(scope, _), _| scope != &prefix);
    }
}
