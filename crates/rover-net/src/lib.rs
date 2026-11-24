//! `rover-net`: High-speed Aho-Corasick + RegexSet ad & tracker blocker, Pi-hole style domain
//! blocklist, Phishing/Malware safe browsing guard, SOCKS5/HTTP/PAC proxy manager, session-scoped
//! SHA-256 TLS certificate trust store, userscript engine + 9 bundled Rover Gallery extensions,
//! and zero-cost website dark mode.

pub mod adblock;
pub mod blocklist;
pub mod certs;
pub mod darkmode;
pub mod extensions;
pub mod proxy;
pub mod safebrowsing;

pub use adblock::{Blocker, FilterEngine, ResourceKind};
pub use blocklist::{DomainBlocker, PresetList, BLOCKLIST_PRESETS};
pub use certs::{CertDecision, CertErrorInfo, CertTrustStore};
pub use darkmode::DarkMode;
pub use extensions::{ExtensionEntry, Extensions, GALLERY_SPECS};
pub use proxy::{ProxyConfig, ProxyMode, ProxyScheme};
pub use safebrowsing::{SafeBrowsingGuard, ThreatKind};
