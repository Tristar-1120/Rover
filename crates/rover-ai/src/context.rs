//! Always-on user instruction sets (`core_sets`, `core_active`, `core_enabled`) and
//! transparent context window token estimation (`estimate_context`).

use rover_core::Store;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstructionSet {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

pub fn instruction_sets(store: &Store) -> Vec<InstructionSet> {
    let mut sets: Vec<InstructionSet> = store
        .get("core_sets")
        .and_then(|v| serde_json::from_value::<Vec<InstructionSet>>(v).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|s| !s.name.trim().is_empty())
        .collect();

    if sets.is_empty() {
        if let Some(text) = store.get_str("core_instructions") {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                sets.push(InstructionSet {
                    name: "My instructions".to_string(),
                    text: trimmed.to_string(),
                });
            }
        }
    }
    sets
}

pub fn active_set(store: &Store) -> Option<InstructionSet> {
    let sets = instruction_sets(store);
    if sets.is_empty() {
        return None;
    }
    if let Some(active_name) = store.get_str("core_active") {
        if let Some(found) = sets.iter().find(|s| s.name == active_name) {
            return Some(found.clone());
        }
    }
    sets.into_iter().next()
}

pub fn core_enabled(store: &Store) -> bool {
    store.get_bool("core_enabled", true)
}

pub fn save_sets(
    store: &Store,
    sets: &[InstructionSet],
    active: Option<&str>,
    enabled: Option<bool>,
) {
    store.set("core_sets", json!(sets));
    let active_name = active
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| sets.first().map(|s| s.name.clone()))
        .unwrap_or_default();
    store.set("core_active", json!(active_name));
    if let Some(en) = enabled {
        store.set("core_enabled", json!(en));
    }
    let current_text = active_set(store)
        .map(|s| s.text.trim().to_string())
        .unwrap_or_default();
    store.set("core_instructions", json!(current_text));
}

pub fn core_system(store: &Store) -> String {
    if !core_enabled(store) {
        return String::new();
    }
    let Some(cur) = active_set(store) else {
        return String::new();
    };
    let text = cur.text.trim();
    if text.is_empty() {
        return String::new();
    }
    format!(
        "\n\nPersistent core instructions written by the user (keep these throughout the task; the current user request takes precedence):\n<core_instructions>\n{text}\n</core_instructions>"
    )
}

pub fn estimate_context(messages: &[ChatMessage], system: &str, image_count: usize) -> usize {
    let chars: usize =
        system.len() + messages.iter().map(|m| m.content.len()).sum::<usize>();
    (chars + 3) / 4 + 8 * messages.len() + 1000 * image_count
}
