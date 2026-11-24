//! Capability-gated local Markdown memory file (`Rover Memory.md`).
//!
//! Ensures the AI agent or tools can never read or append to the user's persistent notes
//! unless an explicit user capability flag (`authorized = true`) is passed.

use parking_lot::Mutex;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

pub const DEFAULT_MAX_CHARS: usize = 100_000;
pub const INITIAL_CONTENT: &str = "# Rover Memory\n\nNotes saved from Rover AI conversations.\n";

static PATH_LOCKS: Lazy<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn lock_for(path: &Path) -> Arc<Mutex<()>> {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut map = PATH_LOCKS.lock();
    map.entry(canonical)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Explicit user authorization is required to access Rover Memory")]
    PermissionDenied,
    #[error("Memory file exceeds maximum allowed length ({0} characters)")]
    TooLarge(usize),
    #[error("Memory note is empty")]
    EmptyEntry,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug)]
pub struct MemoryFile {
    pub path: PathBuf,
    pub max_chars: usize,
}

impl MemoryFile {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            max_chars: DEFAULT_MAX_CHARS,
        }
    }

    pub fn with_max_chars(path: impl AsRef<Path>, max_chars: usize) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            max_chars: max_chars.max(1),
        }
    }

    fn authorize(authorized: bool) -> Result<(), MemoryError> {
        if !authorized {
            return Err(MemoryError::PermissionDenied);
        }
        Ok(())
    }

    pub fn initialize(&self, authorized: bool) -> Result<String, MemoryError> {
        Self::authorize(authorized)?;
        let guard_lock = lock_for(&self.path);
        let _guard = guard_lock.lock();
        if self.path.exists() {
            return self.read_unlocked();
        }
        self.write_atomic(INITIAL_CONTENT)?;
        Ok(INITIAL_CONTENT.to_string())
    }

    pub fn read(&self, authorized: bool) -> Result<String, MemoryError> {
        Self::authorize(authorized)?;
        let guard_lock = lock_for(&self.path);
        let _guard = guard_lock.lock();
        self.read_unlocked()
    }

    fn read_unlocked(&self) -> Result<String, MemoryError> {
        if !self.path.exists() {
            return Ok(String::new());
        }
        let file = File::open(&self.path)?;
        let max_bytes = (self.max_chars * 4 + 1) as u64;
        let mut buf = Vec::new();
        file.take(max_bytes).read_to_end(&mut buf)?;
        let text = String::from_utf8_lossy(&buf).into_owned();
        if text.chars().count() > self.max_chars {
            return Err(MemoryError::TooLarge(self.max_chars));
        }
        Ok(text)
    }

    pub fn append(&self, entry: &str, authorized: bool) -> Result<String, MemoryError> {
        Self::authorize(authorized)?;
        let clean = entry.trim();
        if clean.is_empty() {
            return Err(MemoryError::EmptyEntry);
        }
        let guard_lock = lock_for(&self.path);
        let _guard = guard_lock.lock();
        let existing = if self.path.exists() {
            self.read_unlocked()?
        } else {
            INITIAL_CONTENT.to_string()
        };
        let sep = if existing.ends_with("\n\n") || existing.is_empty() {
            ""
        } else if existing.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        let updated = format!("{existing}{sep}{clean}\n");
        if updated.chars().count() > self.max_chars {
            return Err(MemoryError::TooLarge(self.max_chars));
        }
        self.write_atomic(&updated)?;
        Ok(updated)
    }

    fn write_atomic(&self, content: &str) -> Result<(), MemoryError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("md.tmp");
        {
            let mut file = File::create(&tmp)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
        }
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}
