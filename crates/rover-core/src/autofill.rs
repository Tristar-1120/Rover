//! Encrypted Address & Payment Card autofill vault (`AutofillStore`), Luhn check,
//! card brand classification, masked display (`Visa •••• 4242`), and CVC exclusion.

use crate::secretbox::{KeyringMissing, SecretBox};
use crate::store::now_ts;
use parking_lot::Mutex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AddressEntry {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub organization: String,
    #[serde(default)]
    pub street: String,
    #[serde(default)]
    pub street2: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub postal: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub tel: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub created: f64,
    #[serde(default)]
    pub updated: f64,
    #[serde(default)]
    pub last_used: f64,
}

impl AddressEntry {
    pub fn given_name(&self) -> String {
        self.name
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    }

    pub fn family_name(&self) -> String {
        let parts: Vec<&str> = self.name.split_whitespace().collect();
        if parts.len() > 1 {
            parts[1..].join(" ")
        } else {
            String::new()
        }
    }

    pub fn summary(&self) -> String {
        let mut bits = Vec::new();
        for f in [&self.name, &self.street, &self.city, &self.email] {
            if !f.trim().is_empty() {
                bits.push(f.trim());
            }
        }
        bits.join(" · ")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CardEntry {
    pub id: String,
    #[serde(default)]
    pub cc_name: String,
    pub cc_number: String,
    #[serde(default)]
    pub cc_month: String,
    #[serde(default)]
    pub cc_year: String,
    #[serde(default)]
    pub nickname: String,
    #[serde(default)]
    pub created: f64,
    #[serde(default)]
    pub updated: f64,
    #[serde(default)]
    pub last_used: f64,
}

impl CardEntry {
    pub fn masked_label(&self) -> String {
        let digits = digits_only(&self.cc_number);
        let tail = if digits.len() >= 4 {
            &digits[digits.len() - 4..]
        } else {
            digits.as_str()
        };
        let brand = card_brand(&digits);
        let head = if !self.nickname.trim().is_empty() {
            self.nickname.trim()
        } else if !brand.is_empty() {
            brand
        } else {
            "Card"
        };
        format!("{head} •••• {tail}")
    }
}

pub fn digits_only(raw: &str) -> String {
    raw.chars().filter(|c| c.is_ascii_digit()).collect()
}

pub fn luhn_ok(raw: &str) -> bool {
    let digits: Vec<u32> = raw.chars().filter_map(|c| c.to_digit(10)).collect();
    if !(12..=19).contains(&digits.len()) {
        return false;
    }
    let mut sum = 0u32;
    for (idx, &d) in digits.iter().rev().enumerate() {
        if idx % 2 == 1 {
            let doubled = d * 2;
            sum += if doubled > 9 { doubled - 9 } else { doubled };
        } else {
            sum += d;
        }
    }
    sum % 10 == 0
}

pub fn card_brand(raw: &str) -> &'static str {
    let d = digits_only(raw);
    if d.starts_with('4') {
        "Visa"
    } else if d.starts_with("34") || d.starts_with("37") {
        "Amex"
    } else if d.starts_with("6011") || d.starts_with("65") {
        "Discover"
    } else if d.len() >= 2 && matches!(d[..2].parse::<u32>(), Ok(51..=55)) {
        "Mastercard"
    } else if d.len() >= 4 && matches!(d[..4].parse::<u32>(), Ok(2221..=2720)) {
        "Mastercard"
    } else {
        "Card"
    }
}

fn new_id() -> String {
    let mut rng = rand::thread_rng();
    (0..24)
        .map(|_| format!("{:x}", rng.gen_range(0..16)))
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AutofillData {
    #[serde(default)]
    pub addresses: Vec<AddressEntry>,
    #[serde(default)]
    pub cards: Vec<CardEntry>,
}

#[derive(Clone)]
pub struct AutofillStore {
    pub path: PathBuf,
    secretbox: SecretBox,
    data: Arc<Mutex<AutofillData>>,
    unreadable: Arc<Mutex<bool>>,
}

impl AutofillStore {
    pub fn new(folder: impl AsRef<Path>, secretbox: SecretBox) -> Self {
        let path = folder.as_ref().join("autofill.dat");
        let store = Self {
            path,
            secretbox,
            data: Arc::new(Mutex::new(AutofillData::default())),
            unreadable: Arc::new(Mutex::new(false)),
        };
        store.load();
        store
    }

    pub fn load(&self) {
        if !self.path.exists() {
            *self.data.lock() = AutofillData::default();
            *self.unreadable.lock() = false;
            return;
        }
        if let Ok(raw) = fs::read_to_string(&self.path) {
            if let Ok(bytes) = self.secretbox.unprotect(&raw) {
                if let Ok(parsed) = serde_json::from_slice::<AutofillData>(&bytes) {
                    *self.data.lock() = parsed;
                    *self.unreadable.lock() = false;
                    return;
                }
            }
        }
        *self.unreadable.lock() = true;
    }

    fn save(&self) -> Result<(), KeyringMissing> {
        if *self.unreadable.lock() {
            return Err(KeyringMissing::Unavailable(
                "Autofill store is locked; refusing to overwrite".into(),
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

    pub fn addresses(&self) -> Vec<AddressEntry> {
        self.data.lock().addresses.clone()
    }

    pub fn cards(&self) -> Vec<CardEntry> {
        self.data.lock().cards.clone()
    }

    pub fn save_address(&self, mut addr: AddressEntry) -> Result<AddressEntry, KeyringMissing> {
        let ts = now_ts();
        if addr.id.is_empty() {
            addr.id = new_id();
            addr.created = ts;
        }
        addr.updated = ts;
        {
            let mut d = self.data.lock();
            if let Some(existing) = d.addresses.iter_mut().find(|a| a.id == addr.id) {
                *existing = addr.clone();
            } else {
                d.addresses.push(addr.clone());
            }
        }
        self.save()?;
        Ok(addr)
    }

    pub fn remove_address(&self, id: &str) -> Result<bool, KeyringMissing> {
        let removed = {
            let mut d = self.data.lock();
            let before = d.addresses.len();
            d.addresses.retain(|a| a.id != id);
            d.addresses.len() < before
        };
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn save_card(&self, mut card: CardEntry) -> Result<CardEntry, KeyringMissing> {
        let digits = digits_only(&card.cc_number);
        if !luhn_ok(&digits) {
            return Err(KeyringMissing::Io("Invalid payment card number".into()));
        }
        card.cc_number = digits;
        let ts = now_ts();
        if card.id.is_empty() {
            card.id = new_id();
            card.created = ts;
        }
        card.updated = ts;
        {
            let mut d = self.data.lock();
            if let Some(existing) = d
                .cards
                .iter_mut()
                .find(|c| c.id == card.id || c.cc_number == card.cc_number)
            {
                card.id = existing.id.clone();
                *existing = card.clone();
            } else {
                d.cards.push(card.clone());
            }
        }
        self.save()?;
        Ok(card)
    }

    pub fn remove_card(&self, id: &str) -> Result<bool, KeyringMissing> {
        let removed = {
            let mut d = self.data.lock();
            let before = d.cards.len();
            d.cards.retain(|c| c.id != id);
            d.cards.len() < before
        };
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn replace_all(
        &self,
        addresses: Vec<AddressEntry>,
        cards: Vec<CardEntry>,
    ) -> Result<(), KeyringMissing> {
        {
            let mut d = self.data.lock();
            d.addresses = addresses;
            d.cards = cards;
        }
        self.save()
    }
}
