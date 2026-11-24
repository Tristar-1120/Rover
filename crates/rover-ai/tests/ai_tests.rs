use rover_ai::{
    agent::{format_snapshot, AgentConfig, AgentState, PageItem, PageSnapshot},
    automation::{new_watcher_spec, Watcher},
    localai::{parse_local_command, LocalCommand},
    pricing::{estimate_cost, lookup_rates, normalize_rates},
    providers::{build_anthropic_payload, build_gemini_payload, preset_providers},
    token_optimizer::{SemanticPruner, SnapshotDiffer, TranslationDeduplicator},
    usage::{format_cost, Meter, UsageLog},
};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[test]
fn test_token_efficiency_decisions_extracts_and_thought_stripping() {
    let mut turns = vec![
        (
            json!({"action": "ask_user", "args": {"question": "Publish?"}}).to_string(),
            "The user replied: Save a draft only; use 2026 prices.".to_string(),
        ),
        (
            json!({"action": "click", "args": {"target": "Publish"}}).to_string(),
            "The user declined this click. Ask them how to proceed.".to_string(),
        ),
    ];
    for i in 0..100 {
        turns.push((
            json!({"thought": "working", "action": "scroll", "args": {"pages": 1}}).to_string(),
            format!("Scrolled to {i}."),
        ));
    }

    let mut state = AgentState::new(
        "Compare sources. Ask before publishing. Keep prices in USD.",
        AgentConfig::default(),
    );
    state.turns = turns;

    let obs = "CURRENT PAGE\nInteractive elements:\n[37] <button> \"Buy\"";
    let msgs = state.build_messages(obs);
    let total_text: String = msgs.iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n");

    assert!(msgs[0].content.contains("TASK: Compare sources. Ask before publishing. Keep prices in USD."));
    assert!(msgs[0].content.contains("Save a draft only; use 2026 prices."));
    assert!(msgs[0].content.contains("\"question\":\"Publish?\""));
    assert!(msgs[0].content.contains("The user declined this click."));
    assert!(msgs.last().unwrap().content.contains("[37] <button> \"Buy\""));
    assert!(total_text.len() < 5000);

    // Strip old thoughts while preserving executed arguments
    let long_thought = "This repeats planning that has already been acted on. ".repeat(20);
    let mut thought_turns = Vec::new();
    for i in 0..30 {
        thought_turns.push((
            json!({"thought": long_thought, "action": "type", "args": {"id": i, "text": "Ada"}})
                .to_string(),
            format!("Typed into [{i}]."),
        ));
    }
    state.turns = thought_turns.clone();
    let saver_msgs = state.build_messages(obs);
    let mut full_state = state.clone();
    full_state.config.token_saver = false;
    let orig_msgs = full_state.build_messages(obs);

    let saver_len: usize = saver_msgs.iter().map(|m| m.content.len()).sum();
    let orig_len: usize = orig_msgs.iter().map(|m| m.content.len()).sum();
    assert!((saver_len as f64) < (orig_len as f64) * 0.65);
    assert!(!saver_msgs.iter().any(|m| m.content.contains("planning that has already")));

    // Fresh extract deduplication
    let excerpt = "Exact article facts and the current conclusion.";
    let extract = format!("Page text:\nSource: Report (https://example.test/report)\n{excerpt}");
    state.turns = vec![(
        json!({"action": "extract", "args": {}}).to_string(),
        extract,
    )];
    let obs_dup = format!(
        "CURRENT PAGE\nURL: https://example.test/report\n[9] <button> Continue\nPage text excerpt:\n{excerpt}"
    );
    let dedup_msgs = state.build_messages(&obs_dup);
    let dedup_text = dedup_msgs
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(dedup_text.matches(excerpt).count(), 1);
}

#[test]
fn test_rover_dom_delta_snapshots_and_semantic_pruner() {
    // 1. SemanticPruner collapses repeated lines into [repeated xN]
    let raw = "  Repeated   line \nRepeated line\nRepeated line\nDistinct final fact.";
    let cleaned = SemanticPruner::clean_text(raw, 2500);
    assert!(cleaned.contains("Repeated line [repeated x3]"));
    assert!(cleaned.contains("Distinct final fact."));

    // 2. SnapshotDiffer emits compact delta when on the same URL
    let mut items = Vec::new();
    for i in 0..40 {
        items.push(PageItem {
            i,
            tag: "button".into(),
            text: format!("Button {i}"),
            placeholder: None,
            value: None,
            checked: None,
            href: None,
            options: None,
            disabled: None,
            secret: None,
            in_view: true,
        });
    }
    let snap1 = PageSnapshot {
        url: "https://rover.app/mission".into(),
        title: "Mission Control".into(),
        ready: "complete".into(),
        scroll_y: 0,
        scroll_max: 800,
        vw: 1280,
        vh: 800,
        items: items.clone(),
        visual: false,
        text: "Telemetry stream nominal. ".repeat(40),
    };

    let mut differ = SnapshotDiffer::new();
    let (_d1, used_delta1) = differ.format_delta(&snap1, true);
    assert!(!used_delta1, "First snapshot on URL must be full");

    // Step 2 on same URL: only 1 input field changed
    let mut snap2 = snap1.clone();
    snap2.items[5].value = Some("Curiosity".into());
    let (delta_text, used_delta2) = differ.format_delta(&snap2, true);
    assert!(used_delta2, "Second snapshot on same URL must use DOM delta");
    let full_text = format_snapshot(Some(&snap2), &[], "", true);
    assert!(
        delta_text.len() * 3 < full_text.len(),
        "DOM delta must cut snapshot size by >66% (delta={} vs full={})",
        delta_text.len(),
        full_text.len()
    );

    // 3. TranslationDeduplicator saves tokens on repeated UI strings
    let dom_nodes = vec![
        "Add to cart".to_string(),
        "In stock".to_string(),
        "Add to cart".to_string(),
        "In stock".to_string(),
        "Add to cart".to_string(),
    ];
    let (unique, map) = TranslationDeduplicator::deduplicate(&dom_nodes);
    assert_eq!(unique.len(), 2);
    let translated = vec!["Añadir al carrito".to_string(), "En stock".to_string()];
    let expanded = TranslationDeduplicator::expand(&translated, &map);
    assert_eq!(expanded[0], "Añadir al carrito");
    assert_eq!(expanded[2], "Añadir al carrito");
    assert_eq!(expanded[3], "En stock");
}

#[test]
fn test_adaptive_vision_and_no_context_mode() {
    let snap = PageSnapshot {
        url: "https://example.test/form".into(),
        title: "Form".into(),
        ready: "complete".into(),
        scroll_y: 0,
        scroll_max: 0,
        vw: 1280,
        vh: 800,
        items: vec![PageItem {
            i: 1,
            tag: "input:text".into(),
            text: "Email".into(),
            placeholder: Some("Email".into()),
            value: None,
            checked: None,
            href: None,
            options: None,
            disabled: None,
            secret: None,
            in_view: true,
        }],
        visual: false,
        text: "A readable form".into(),
    };

    let mut state = AgentState::new("Fill form", AgentConfig::default());
    assert!(state.needs_screenshot(Some(&snap), 1));
    state.last_vision_state = Some(AgentState::vision_state(1, &snap));
    // Textual progress on same tab/url/scrollY skips screenshot
    assert!(!state.needs_screenshot(Some(&snap), 1));
    // Visual canvas, new URL, scroll, or tab switch forces screenshot
    let mut canvas_snap = snap.clone();
    canvas_snap.visual = true;
    assert!(state.needs_screenshot(Some(&canvas_snap), 1));
    assert!(state.needs_screenshot(Some(&snap), 2));

    // No-context mode keeps only task, user decisions, last step, and current page
    state.config.no_context = true;
    state.turns = (1..=8)
        .map(|n| {
            (
                format!("{{\"action\":\"click\",\"args\":{{\"id\":{n}}}}}"),
                format!("Clicked [{n}] \"Next\"."),
            )
        })
        .collect();
    state.turns.insert(
        2,
        (
            "{\"action\":\"ask_user\",\"args\":{}}".into(),
            "The user replied: use the blue form".into(),
        ),
    );
    let min_msgs = state.build_messages("CURRENT PAGE: question 9");
    let joined = min_msgs
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(min_msgs.len(), 4);
    assert!(min_msgs[0].content.contains("use the blue form"));
    assert!(joined.contains("[8]"));
    assert!(!joined.contains("[7]"));
}

#[test]
fn test_pricing_meter_and_usage_log_regressions() {
    // OpenAI cache discount
    let (cost, est) = estimate_cost("openai", "gpt-4o-mini", 10_000, 1_000, 4_000, 0, 0, None, None, None);
    assert!((cost.unwrap() - 0.0018).abs() < 1e-9 && est);

    // Anthropic separate cache write durations
    let (cost2, _) = estimate_cost(
        "anthropic",
        "claude-sonnet-5-5",
        6_000,
        1_000,
        2_000,
        3_000,
        1_000,
        None,
        None,
        None,
    );
    assert!((cost2.unwrap() - 0.0214).abs() < 1e-9);

    // Gemini long-context boundary (> 200,000 tokens) and promotional date cutoff
    assert!(
        (estimate_cost("gemini", "gemini-2.5-pro", 200_000, 1_000, 0, 0, 0, None, None, None)
            .0
            .unwrap()
            - 0.26)
            .abs()
            < 1e-9
    );
    assert!(
        (estimate_cost(
            "gemini",
            "models/gemini-2.5-pro",
            200_001,
            1_000,
            100_001,
            0,
            0,
            None,
            None,
            None
        )
        .0
        .unwrap()
            - 0.29000025)
            .abs()
            < 1e-9
    );
    assert_eq!(
        lookup_rates("gemini", "gemini-3.8-flash", 1000, Some("2026-12-31"))
            .unwrap()
            .input,
        0.75
    );
    assert_eq!(
        lookup_rates("gemini", "gemini-3.8-flash", 1000, Some("2027-01-01"))
            .unwrap()
            .input,
        1.50
    );

    // Unknown and gateway models are never guessed
    assert!(lookup_rates("openrouter", "openai/gpt-4o-mini", 1000, None).is_none());
    assert!(normalize_rates(&json!({"input": -1, "output": 2})).is_none());

    // Hierarchical session + task Meter
    let session = Arc::new(Meter::new(None));
    let task = Meter::new(Some(session.clone()));
    task.add(
        "anthropic",
        6_000,
        1_000,
        false,
        "claude-sonnet-5-5",
        2_000,
        3_000,
        1_000,
        None,
        None,
    );
    task.add(
        "openrouter",
        10_000,
        1_000,
        false,
        "custom",
        0,
        0,
        0,
        Some(&json!(0.125)),
        None,
    );
    assert_eq!(task.snapshot(), session.snapshot());
    assert!((session.cost_usd().unwrap() - 0.1464).abs() < 1e-9);
    assert!(format_cost(Some(1e-12), 0, false, false).contains("<$0.00000001"));

    // UsageLog persistence
    let tmp = TempDir::new().unwrap();
    let log = UsageLog::new(tmp.path().join("usage.log"));
    log.record("agent", "anthropic", "claude-sonnet-5-5", "Mars task", &task, "done");
    let totals = log.totals();
    assert_eq!(totals["tasks"], 1);
    assert!((totals["cost_usd"].as_f64().unwrap() - 0.1464).abs() < 1e-9);
}

#[test]
fn test_localai_watchers_and_provider_payloads() {
    // Rover Local multi-step command
    let cmd = parse_local_command(
        "open github then fill Search with rover-rust and submit",
        "https://rover.app",
    );
    if let LocalCommand::Steps { steps } = cmd {
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].action, "navigate");
        assert_eq!(steps[0].args["url"], "https://github.com");
        assert_eq!(steps[1].action, "type");
        assert_eq!(steps[1].args["submit"], true);
    } else {
        panic!("Expected Steps");
    }

    // Rover Local watcher creation
    let wcmd = parse_local_command(
        "check https://example.com every 10 minutes when price drops below 50 and notify me",
        "",
    );
    if let LocalCommand::Watch { spec } = wcmd {
        assert_eq!(spec["every"], 600.0);
        assert_eq!(spec["condition"]["type"], "number");
        assert_eq!(spec["condition"]["num"], 50.0);
    } else {
        panic!("Expected Watch");
    }

    // Watcher rising-edge evaluation & Two-Tier HTTP check
    let spec = new_watcher_spec(
        "Price Watch",
        "https://shop.example/rover",
        15.0,
        &json!({"type": "number", "near": "Price", "op": "<", "num": 50}),
        vec![],
        vec!["notify".into(), "ask_ai".into()],
        "Summarize deal",
        "",
        false,
        true,
        "ai",
    )
    .unwrap();
    assert!(spec.can_use_fast_http_tier());

    let mut watcher = Watcher::new(spec);
    let mut snap = PageSnapshot {
        url: "https://shop.example/rover".into(),
        title: "Shop".into(),
        text: "Rover model kit. Price: $65.00".into(),
        ..Default::default()
    };
    let (fired1, _) = watcher.evaluate_snapshot(&snap, 1_700_000_010.0);
    assert!(!fired1, "Must not fire when price is $65");

    snap.text = "Rover model kit. Price: $42.50".into();
    let (fired2, _) = watcher.evaluate_snapshot(&snap, 1_700_000_030.0);
    assert!(fired2, "Must fire once when price drops to $42.50");
    assert!(!watcher.enabled, "Non-repeating watcher stops after firing");

    // Provider payloads (Anthropic cache_control + Gemini thinkingLevel low)
    assert_eq!(preset_providers().len(), 26);
    let anth = preset_providers()
        .into_iter()
        .find(|p| p.id == "anthropic")
        .unwrap();
    let anth_payload = build_anthropic_payload(&anth, &[], "System prompt", &[], 4096);
    assert_eq!(
        anth_payload["system"][0]["cache_control"]["type"],
        "ephemeral"
    );

    let gem = preset_providers()
        .into_iter()
        .find(|p| p.id == "gemini")
        .unwrap();
    let gem_payload = build_gemini_payload(&gem, &[], "System", &[], 4096);
    assert_eq!(
        gem_payload["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "low"
    );
}
