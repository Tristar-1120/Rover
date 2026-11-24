//! Proxy & VPN Connection Manager (`ProxyConfig`).
//!
//! Translates connection settings (`system`, `direct`, `manual` HTTP/HTTPS/SOCKS5/SOCKS4, `pac`)
//! into Chromium command-line flags (including WebRTC UDP leak protection and SOCKS5 remote DNS
//! resolution via `--host-resolver-rules`) and tests proxy connectivity with raw SOCKS5/HTTP handshakes.

use rover_core::Store;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub const PROXY_PASSWORD_KEY: &str = "rover-proxy-password";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    System,
    Direct,
    Manual,
    Pac,
}

impl Default for ProxyMode {
    fn default() -> Self {
        Self::System
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyScheme {
    Http,
    Https,
    Socks5,
    Socks4,
}

impl Default for ProxyScheme {
    fn default() -> Self {
        Self::Socks5
    }
}

impl ProxyScheme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Socks5 => "socks5",
            Self::Socks4 => "socks4",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyConfig {
    #[serde(default)]
    pub mode: ProxyMode,
    #[serde(default)]
    pub scheme: ProxyScheme,
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub user: String,
    #[serde(default = "default_bypass")]
    pub bypass: String,
    #[serde(default)]
    pub pac: String,
    #[serde(default = "default_true")]
    pub webrtc_private: bool,
}

fn default_port() -> u16 {
    9050
}
fn default_bypass() -> String {
    "localhost, 127.0.0.1, ::1, <local>".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            mode: ProxyMode::System,
            scheme: ProxyScheme::Socks5,
            host: String::new(),
            port: 9050,
            user: String::new(),
            bypass: default_bypass(),
            pac: String::new(),
            webrtc_private: true,
        }
    }
}

impl ProxyConfig {
    pub fn load(store: &Store) -> Self {
        store
            .get("proxy")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, store: &Store, password: Option<&str>) {
        if let Ok(v) = serde_json::to_value(self) {
            store.set("proxy", v);
        }
        if let Some(pw) = password {
            let _ = store.set_api_key(PROXY_PASSWORD_KEY, pw);
        }
    }

    pub fn chromium_flags(&self) -> Vec<String> {
        let mut flags = Vec::new();
        match self.mode {
            ProxyMode::System => {}
            ProxyMode::Direct => {
                flags.push("--no-proxy-server".to_string());
            }
            ProxyMode::Pac => {
                if !self.pac.trim().is_empty() {
                    flags.push(format!("--proxy-pac-url={}", self.pac.trim()));
                }
                if self.webrtc_private {
                    flags.push(
                        "--force-webrtc-ip-handling-policy=disable_non_proxied_udp".to_string(),
                    );
                }
            }
            ProxyMode::Manual => {
                let h = self.host.trim();
                if !h.is_empty() && self.port > 0 {
                    flags.push(format!(
                        "--proxy-server={}://{}:{}",
                        self.scheme.as_str(),
                        h,
                        self.port
                    ));
                    if !self.bypass.trim().is_empty() {
                        let bypass_clean = self
                            .bypass
                            .split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(";");
                        flags.push(format!("--proxy-bypass-list={bypass_clean}"));
                    }
                    if self.scheme == ProxyScheme::Socks5 {
                        flags.push(
                            "--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE 127.0.0.1, EXCLUDE ::1, EXCLUDE localhost"
                                .to_string(),
                        );
                    }
                    if self.webrtc_private {
                        flags.push(
                            "--force-webrtc-ip-handling-policy=disable_non_proxied_udp".to_string(),
                        );
                    }
                }
            }
        }
        flags
    }

    /// Tests whether the configured proxy server accepts connections (and speaks SOCKS5 if configured).
    pub fn test_connection(&self, timeout_ms: u64) -> Result<String, String> {
        let h = self.host.trim();
        if h.is_empty() || self.port == 0 {
            return Err("Host and port are required".into());
        }
        let addr_str = format!("{h}:{}", self.port);
        let addr = addr_str
            .to_socket_addrs()
            .map_err(|e| format!("Could not resolve {addr_str}: {e}"))?
            .next()
            .ok_or_else(|| format!("No address found for {addr_str}"))?;

        let timeout = Duration::from_millis(timeout_ms.max(200));
        let mut stream = TcpStream::connect_timeout(&addr, timeout)
            .map_err(|e| format!("Connection failed: {e}"))?;
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));

        if self.scheme == ProxyScheme::Socks5 {
            stream
                .write_all(&[0x05, 0x02, 0x00, 0x02])
                .map_err(|e| format!("SOCKS5 handshake write failed: {e}"))?;
            let mut reply = [0u8; 2];
            stream
                .read_exact(&mut reply)
                .map_err(|e| format!("SOCKS5 handshake read failed: {e}"))?;
            if reply[0] != 0x05 {
                return Err(format!("Server did not reply with SOCKS5 (got 0x{:02x})", reply[0]));
            }
        }
        Ok(format!(
            "Connected to {}://{}:{}",
            self.scheme.as_str(),
            h,
            self.port
        ))
    }
}
