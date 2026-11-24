use rover_browser::{
    homepage::{add_shortcut, build_homepage_html, parse_home_action, HomepageConfig},
    omnibox::{inline_completion, suggest},
    playground::{
        fnv1a_rover_hash, is_playground_address, render_playground_html, verify_challenge_answer,
        CHALLENGES,
    },
    tabs::TabStrip,
    tools::{extract_reader_article, markdown_to_html, render_reader_html, unique_download_path},
};
use rover_core::Store;
use tempfile::TempDir;

#[test]
fn test_tabs_equal_width_pinning_groups_sleeping_split_and_workspaces() {
    let mut strip = TabStrip::new();
    assert_eq!(strip.tabs.len(), 1);
    assert_eq!(strip.compute_tab_width(1000), 240);

    for i in 0..8 {
        strip.open_tab(&format!("https://github.com/rover/issue/{i}"), &format!("Issue {i}"));
    }
    // 9 tabs in 1000px strip -> shrinks evenly between 140..240
    let w = strip.compute_tab_width(1000);
    assert!((140..=240).contains(&w));

    // Pinning moves tab to front and prevents sleeping
    strip.set_pinned(3, true);
    assert!(strip.tabs[0].pinned);
    assert!(!strip.can_sleep_tab(0));

    // Auto-group by site groups the unpinned GitHub tabs
    let moved = strip.group_by_site();
    assert!(moved >= 7);
    assert!(strip.groups.values().any(|g| g.name == "GitHub"));

    // Sleeping tabs: active and pinned tabs stay awake; idle background tabs sleep
    let active = strip.active_index;
    assert!(!strip.sleep_tab(active));
    assert!(strip.sleep_tab(2));
    assert!(strip.tabs[2].sleeping);
    strip.wake_tab(2);
    assert!(!strip.tabs[2].sleeping);

    // Close and Reopen Closed Tab (Ctrl+Shift+T)
    let closed_url = strip.tabs[4].url.clone();
    assert!(strip.close_index(4));
    strip.reopen_closed().unwrap();
    assert_eq!(strip.tabs.last().unwrap().url, closed_url);

    // Split View
    assert!(strip.set_split(2));
    assert!(!strip.can_sleep_tab(2), "Split view tab must not sleep");
    strip.end_split();

    // Workspaces preserve and isolate tab sets
    let main_count = strip.tabs.len();
    strip.switch_workspace("Mars Mission");
    assert_eq!(strip.tabs.len(), 1);
    strip.open_tab("https://mars.nasa.gov", "Mars");
    assert_eq!(strip.tabs.len(), 2);
    strip.switch_workspace("Main");
    assert_eq!(strip.tabs.len(), main_count);
}

#[test]
fn test_omnibox_inline_autocomplete_and_suggestions() {
    let tmp = TempDir::new().unwrap();
    let store = Store::new(Some(tmp.path().to_path_buf()), None);
    store.add_history("https://en.wikipedia.org/wiki/Mars_rover", "Mars Rover");

    assert_eq!(
        inline_completion(&store, "rov").as_deref(),
        Some("rover:playground")
    );
    assert_eq!(
        inline_completion(&store, "en.wiki").as_deref(),
        Some("en.wikipedia.org")
    );

    let rows = suggest(&store, "Mars", &["mars weather".into()]);
    assert!(rows.iter().any(|r| r.kind == "history" && r.title == "Mars Rover"));
    assert!(rows.iter().any(|r| r.kind == "search" && r.title == "mars weather"));

    // Turning off autocomplete disables inline and history suggestions
    store.set("autocomplete", serde_json::json!(false));
    assert!(inline_completion(&store, "en.wiki").is_none());
    assert_eq!(suggest(&store, "Mars", &["mars weather".into()]).len(), 1);
}

#[test]
fn test_homepage_customization_and_reader_mode() {
    let tmp = TempDir::new().unwrap();
    let store = Store::new(Some(tmp.path().to_path_buf()), None);
    assert!(store.bookmarks().is_empty(), "Fresh profile must have zero pre-populated bookmarks");
    assert!(HomepageConfig::load(&store).shortcuts.is_empty(), "Fresh profile must have zero pre-populated shortcuts");

    add_shortcut(&store, "NASA Mars", "mars.nasa.gov");
    let html = build_homepage_html(&store, "DuckDuckGo", "https://duckduckgo.com/");
    assert!(html.contains("<h1>Rover</h1>"));
    assert!(html.contains("https://mars.nasa.gov"));
    assert!(html.contains("?rover=playground"));
    assert_eq!(
        parse_home_action("https://rover.local/start.html?rover=edit&i=2"),
        Some(("edit".into(), Some(2)))
    );

    let mut cfg = HomepageConfig::load(&store);
    cfg.show_logo = false;
    cfg.save(&store);
    let html_no_logo = build_homepage_html(&store, "DuckDuckGo", "https://duckduckgo.com/");
    assert!(!html_no_logo.contains("<h1>Rover</h1>"));

    // Reader mode extraction drops nav/sidebar/footer and keeps article paragraphs
    let para = "Mars is the fourth planet from the Sun and the second-smallest planet in the Solar System. Scientists study its geology and atmosphere. ";
    let raw_page = format!(
        "<!doctype html><title>Mars News</title><body><nav>Home · Shop</nav><aside>Buy mugs!</aside><article><h1>Red Planet</h1>{}{}{}</article><footer>Copyright 2026</footer></body>",
        format!("<p>{para} Part 1.</p>"),
        format!("<p>{para} Part 2.</p>"),
        format!("<p>{para} Part 3.</p>")
    );
    let article = extract_reader_article(&raw_page).unwrap();
    assert_eq!(article.title, "Red Planet");
    assert_eq!(article.paragraphs.len(), 3);
    let reader_html = render_reader_html(&article, "https://mars.example/news", "sepia", 20);
    assert!(reader_html.contains("rgb(244, 236, 216)"));
    assert!(!reader_html.contains("Buy mugs!"));
    assert!(!reader_html.contains("Copyright 2026"));

    // Markdown to HTML & unique download path
    let md_html = markdown_to_html("# Rover Mission\n\n- **Speed**: 100x\n- `zero-copy`");
    assert!(md_html.contains("<h1>Rover Mission</h1>"));
    assert!(md_html.contains("<strong>Speed</strong>"));
    assert!(md_html.contains("<code>zero-copy</code>"));

    let f1 = tmp.path().join("report.pdf");
    std::fs::write(&f1, b"pdf").unwrap();
    let f2 = unique_download_path(tmp.path(), "report.pdf");
    assert_eq!(f2.file_name().unwrap().to_str().unwrap(), "report (1).pdf");
}

#[test]
fn test_rover_playground_21_challenges_and_fnv1a_hashes() {
    assert!(is_playground_address("rover:playground"));
    assert!(is_playground_address("pelagic:playground"));
    assert_eq!(CHALLENGES.len(), 21);

    for ch in CHALLENGES {
        assert!(
            verify_challenge_answer(ch.id, ch.expected_answer),
            "Challenge {} failed verification",
            ch.id
        );
        assert_eq!(fnv1a_rover_hash(ch.expected_answer).len(), 8);
    }
    let html = render_playground_html();
    assert!(html.contains("Rover Playground"));
    assert!(html.contains("OLYMPUS"));
}

#[test]
fn test_chromium_ui_pelagic_1to1_and_no_edge_branding() {
    let chrome = rover_browser::chromium_ui::render_chrome_bar_html("test-tok");
    assert!(chrome.contains("Search or enter address"));
    assert!(chrome.contains("New tab (Ctrl+T)"));
    assert!(chrome.contains("Rover AI (Ctrl+Shift+A)"));
    assert!(!chrome.contains("builtins"));
    assert!(!chrome.to_ascii_lowercase().contains("webview2"));

    let panel = rover_browser::chromium_ui::render_ai_panel_html("test-tok");
    assert!(panel.contains("Provider"));
    assert!(panel.contains("Model"));
    assert!(panel.contains("Always-on"));
    assert!(panel.contains("Readable"));
    assert!(panel.contains("Raw"));
    assert!(panel.contains("Options and usage"));
    assert!(panel.contains("Clear history"));
    assert!(panel.contains("Send"));
    assert!(!panel.to_ascii_lowercase().contains("pelagic"));

    let about = rover_browser::chromium_ui::render_dialog_html("about", "test-tok", None);
    assert!(about.contains("Chromium 156"));
}
