//! `rover-ai`: Ultra-low-token AI provider engine, exact USD token accounting,
//! next-gen token optimization (DOM delta snapshots, compact DSL, semantic pruning,
//! translation string-table deduplication), zero-token Rover Local command parser,
//! two-tier background watchers, and autonomous browsing agent.

pub mod agent;
pub mod automation;
pub mod context;
pub mod localai;
pub mod pagejs;
pub mod pricing;
pub mod providers;
pub mod token_optimizer;
pub mod usage;

pub use agent::{
     format_snapshot, needs_screenshot, parse_action, parse_plan, system_prompt, AgentConfig,
    AgentState, PageItem, PageSnapshot, PlannedStep, TabSummary,
};
pub use automation::{
    check_condition, new_watcher_spec, normalize_condition, Condition, Watcher, WatcherSpec,
};
pub use context::{
    active_set, core_enabled, core_system, estimate_context, instruction_sets, save_sets,
    ChatMessage, InstructionSet,
};
pub use localai::{parse_local_command, LocalCommand};
pub use pricing::{estimate_cost, lookup_rates, normalize_rates, RateTable, PRICING_CHECKED};
pub use providers::{chat_sync, pick_model, preset_providers, strip_think, Provider, ProviderKind};
pub use token_optimizer::{
    compact_action, head_tail, CompactDsl, SemanticPruner, SnapshotDiffer, TranslationDeduplicator,
};
pub use usage::{format_cost, Meter, MeterSnapshot, UsageLog};
