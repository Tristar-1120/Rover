//! `rover-core`: High-speed native storage, profile management, capability-gated memory,
//! AEAD secretbox cryptography, password & autofill vaults, k-anonymity breach checkup,
//! outbound prompt leak guard, and serverless LWW folder sync.

pub mod autofill;
pub mod memory;
pub mod passwords;
pub mod profiles;
pub mod pwcheck;
pub mod pwguard;
pub mod secretbox;
pub mod store;
pub mod sync;

pub use autofill::{AddressEntry, AutofillStore, CardEntry};
pub use memory::{MemoryError, MemoryFile};
pub use passwords::{LoginEntry, PasswordStore};
pub use profiles::{ProfileMeta, Profiles, DEFAULT_ID};
pub use secretbox::{KeyringBackend, KeyringMissing, SecretBox};
pub use store::{Bookmark, HistoryEntry, ReadingItem, Store, SHARED_SETTINGS};
pub use sync::FolderSync;
