#![recursion_limit = "256"]

//! `rover-browser`: Pure Chromium (CEF) browser engine, tab management (equal-width tabs,
//! groups, sleeping tabs, split view, vertical tabs, workspaces), omnibox autocomplete,
//! Rover homepage (`rover:start`), reader mode, Markdown/code file viewer, downloads & yt-dlp
//! integration, and the 21-challenge Rover Playground (`rover:playground`).

pub mod chromium_ui;
#[cfg(target_os = "windows")]
pub mod chromium_win;
pub mod homepage;
pub mod omnibox;
pub mod playground;
pub mod tabs;
pub mod tools;
pub mod webui;

pub use homepage::{build_homepage_html, parse_home_action, HomepageConfig, Shortcut};
pub use omnibox::{inline_completion, search_url, suggest, OmniboxSuggestion};
pub use playground::{fnv1a_rover_hash, is_playground_address, render_playground_html, CHALLENGES};
pub use tabs::{GroupColor, Tab, TabGroup, TabStrip, Workspace, GROUP_COLORS};
pub use tools::{
    extract_reader_article, markdown_to_html, render_fileview_html, render_reader_html,
    unique_download_path, YtdlJobSpec,
};
