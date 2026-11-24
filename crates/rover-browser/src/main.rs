#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

//! `rover` binary entry point:
//! - Windows default (`rover.exe [url]`): Launches Rover as its OWN standalone native desktop
//!   pure Chromium (CEF 156, `libcef.dll`) browser window — zero Microsoft Edge / WebView2.
//! - `rover --private [url]`: Launches a Private Window with in-memory Chromium session storage
//! - `rover --bench`: Runs live speed & token-compression benchmarks across all Rover crates
//! - `rover --local "<command>"`: Parses and executes a zero-token Rover Local command
//! - `rover --serve [--port 8420]`: Starts the web preview HTTP server (useful on headless Linux)
//! - `rover --version`: Prints Rover 2.0 version info

use rover_ai::{
    agent::{format_snapshot, AgentConfig, AgentState, PageItem, PageSnapshot},
    localai::parse_local_command,
    token_optimizer::{SemanticPruner, SnapshotDiffer, TranslationDeduplicator},
};
use rover_browser::{
    homepage::{build_homepage_html, ROVER_ICON_PNG_BYTES},
    playground::render_playground_html,
    webui::render_browser_shell_html,
};
use rover_core::{
    secretbox::{derive_key, seal, unseal},
    Store,
};
use rover_net::{
    adblock::{FilterEngine, ResourceKind},
    blocklist::parse_domain_list,
};
use std::env;
use std::time::Instant;
use tiny_http::{Header, Response, Server};

#[cfg(target_os = "windows")]
fn attach_parent_console_if_present() {
    extern "system" {
        fn AttachConsole(dwProcessId: u32) -> i32;
    }
    unsafe {
        AttachConsole(0xFFFF_FFFF);
    }
}

fn main() {
    // CRITICAL FOR CHROMIUM CEF: Handle Chromium renderer/GPU/network helper subprocesses first!
    #[cfg(target_os = "windows")]
    if let Some(code) = rover_browser::chromium_win::try_run_cef_subprocess() {
        std::process::exit(code);
    }

    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        #[cfg(target_os = "windows")]
        attach_parent_console_if_present();
        println!("Rover 2.0.0 (Native Rust + Pure Chromium 156 CEF — Inverted Mars Edition)");
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        #[cfg(target_os = "windows")]
        attach_parent_console_if_present();
        println!(
            "Rover 2.0.0 — Pure Chromium (CEF) Rust browser & ultra-low-token autonomous AI agent\n\n\
             Usage:\n  \
               rover [URL]                    Launch native Rover Chromium Browser\n  \
               rover --private [URL]          Launch Private Window\n  \
               rover --bench                  Run speed & token efficiency benchmarks\n  \
               rover --local \"<command>\"      Parse a zero-token Rover Local command\n  \
               rover --serve [--port <PORT>]  Launch headless HTTP WebUI server (default: 8420)"
        );
        return;
    }

    if let Some(pos) = args.iter().position(|a| a == "--local") {
        #[cfg(target_os = "windows")]
        attach_parent_console_if_present();
        let cmd_text = args.get(pos + 1).map(|s| s.as_str()).unwrap_or("help");
        let parsed = parse_local_command(cmd_text, "https://rover.app");
        println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        return;
    }

    if args.iter().any(|a| a == "--bench") {
        #[cfg(target_os = "windows")]
        attach_parent_console_if_present();
        run_benchmarks();
        return;
    }

    let explicit_serve = args.iter().any(|a| a == "--serve");
    let private_mode = args.iter().any(|a| a == "--private");
    #[cfg(target_os = "windows")]
    if !explicit_serve {
        let initial_url = args
            .iter()
            .skip(1)
            .find(|a| !a.starts_with('-'))
            .cloned();
        rover_browser::chromium_win::run_chromium_browser(initial_url, private_mode);
        return;
    }
    let _ = private_mode;

    let port: u16 = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse().ok())
        .unwrap_or(8420);

    let _ = explicit_serve;
    serve_ui(port);
}

fn run_benchmarks() {
    println!("=========================================================================");
    println!("  ROVER 2.0 (RUST) — SPEED & TOKEN EFFICIENCY BENCHMARK SUITE");
    println!("=========================================================================\n");

    // 1. Adblock Aho-Corasick + Domain Suffix Filter Engine
    let mut filter_lines = String::new();
    for i in 0..10_000 {
        filter_lines.push_str(&format!("||adserver-{i}.example.net^\n"));
        filter_lines.push_str(&format!("/tracker_pixel_{i}_$image,script,third-party\n"));
    }
    let t0 = Instant::now();
    let engine = FilterEngine::from_lists(&[&filter_lines]);
    let compile_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let bytes = engine.to_bytes();
    let t_cache = Instant::now();
    let restored = FilterEngine::from_bytes(&bytes).unwrap();
    let cache_ms = t_cache.elapsed().as_secs_f64() * 1000.0;

    let t_match = Instant::now();
    let mut blocked = 0usize;
    for i in 0..10_000 {
        let url = if i % 2 == 0 {
            format!(
                "https://cdn.site.org/assets/tracker_pixel_{}_banner.png",
                i % 5000
            )
        } else {
            format!("https://cdn.site.org/assets/clean_article_{i}.js")
        };
        if restored.should_block(&url, "https://news.example.com/", ResourceKind::Image) {
            blocked += 1;
        }
    }
    let avg_us = (t_match.elapsed().as_secs_f64() * 1_000_000.0) / 10_000.0;
    println!(
        "[1] Aho-Corasick Adblock Engine (20,000 rules):\n    \
         • Cold compile:      {compile_ms:.2} ms\n    \
         • Binary cache load: {cache_ms:.2} ms ({:.1} KB snapshot)\n    \
         • Match latency:     {avg_us:.2} µs/request ({blocked}/10,000 blocked)\n",
        bytes.len() as f64 / 1024.0
    );

    // 2. Pi-hole Domain Blocklist Parser
    let mut hosts_blob = String::new();
    for i in 0..100_000 {
        hosts_blob.push_str(&format!("0.0.0.0 telemetry-{i}.tracker.org\n"));
    }
    let t_hosts = Instant::now();
    let parsed_domains = parse_domain_list(&hosts_blob);
    let hosts_ms = t_hosts.elapsed().as_secs_f64() * 1000.0;
    println!(
        "[2] Pi-hole Domain Blocklist Parser (100,000 hosts lines):\n    \
         • Parse time:        {hosts_ms:.2} ms ({:.1}M lines/sec, {} unique domains)\n",
        100.0 / hosts_ms,
        parsed_domains.len()
    );

    // 3. ChaCha20-Poly1305 AEAD Vault Cryptography
    let key = derive_key("benchmark-key", b"1234567890123456").unwrap();
    let payload = vec![0x42u8; 1024 * 1024]; // 1 MB vault
    let t_enc = Instant::now();
    let token = seal(&payload, &key);
    let enc_ms = t_enc.elapsed().as_secs_f64() * 1000.0;
    let t_dec = Instant::now();
    let plain = unseal(&token, &key).unwrap();
    let dec_ms = t_dec.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(plain.len(), payload.len());
    println!(
        "[3] Secretbox ChaCha20-Poly1305 AEAD (1 MB Sync Vault):\n    \
         • Seal throughput:   {:.0} MB/s ({enc_ms:.2} ms)\n    \
         • Unseal throughput: {:.0} MB/s ({dec_ms:.2} ms)\n",
        1000.0 / enc_ms,
        1000.0 / dec_ms
    );

    // 4. Token Optimization Benchmark (20-step agent run + DOM Delta + Translation Dedup)
    let mut items = Vec::new();
    for i in 0..60 {
        items.push(PageItem {
            i,
            tag: "button:button".into(),
            text: format!("Control Action {i}"),
            placeholder: Some(format!("Control Action {i}")),
            value: None,
            checked: None,
            href: None,
            options: None,
            disabled: None,
            secret: None,
            in_view: true,
        });
    }
    let base_snap = PageSnapshot {
        url: "https://mission.rover.app/console".into(),
        title: "Mission Console".into(),
        ready: "complete".into(),
        scroll_y: 0,
        scroll_max: 1600,
        vw: 1280,
        vh: 800,
        items,
        visual: false,
        text: SemanticPruner::clean_text(
            &("Telemetry nominal. ".repeat(30) + "\nSensor OK\nSensor OK\nSensor OK\n"),
            2500,
        ),
    };

    let mut naive_chars = 0usize;
    let mut rover_chars = 0usize;
    let mut differ = SnapshotDiffer::new();
    let mut state = AgentState::new(
        "Monitor console and update controls",
        AgentConfig::default(),
    );

    for step in 0..20 {
        let mut snap = base_snap.clone();
        snap.items[step % 60].value = Some(format!("Updated-{step}"));
        let full_obs = format_snapshot(Some(&snap), &[], "", false);
        naive_chars += full_obs.len() + step * 350;

        let (delta_obs, used_delta) = differ.format_delta(&snap, true);
        let obs_to_send = if used_delta {
            delta_obs
        } else {
            format_snapshot(Some(&snap), &[], "", true)
        };
        state.turns.push((
            format!(
                "{{\"thought\":\"Updating control {step} based on telemetry plan.\",\"action\":\"type\",\"args\":{{\"id\":{},\"text\":\"Updated-{step}\"}}}}",
                step % 60
            ),
            format!("Typed into [{}].", step % 60),
        ));
        let msgs = state.build_messages(&obs_to_send);
        rover_chars += msgs.iter().map(|m| m.content.len()).sum::<usize>();
    }

    let token_savings_pct = (1.0 - (rover_chars as f64 / naive_chars as f64)) * 100.0;
    let sample_strings: Vec<String> = (0..100)
        .map(|i| format!("Button Label {}", i % 18))
        .collect();
    let (uniq, _) = TranslationDeduplicator::deduplicate(&sample_strings);
    let tr_savings_pct = (1.0 - (uniq.len() as f64 / sample_strings.len() as f64)) * 100.0;

    println!(
        "[4] Rover Next-Gen Token Optimizer (20-step Agent Run):\n    \
         • Unoptimized prompt volume: {:>8} chars (~{} tokens)\n    \
         • Rover optimized volume:    {:>8} chars (~{} tokens)\n    \
         • Total Agent Token Savings: {:.1}% reduction\n    \
         • Translation Dedup Savings: {:.1}% reduction ({} -> {} unique strings)\n",
        naive_chars,
        naive_chars / 4,
        rover_chars,
        rover_chars / 4,
        token_savings_pct,
        tr_savings_pct,
        sample_strings.len(),
        uniq.len()
    );
}

fn serve_ui(preferred_port: u16) {
    let mut chosen_port = preferred_port;
    let mut server_opt = None;
    for p in preferred_port..preferred_port.saturating_add(20) {
        if let Ok(srv) = Server::http(format!("0.0.0.0:{p}")) {
            chosen_port = p;
            server_opt = Some(srv);
            break;
        }
    }
    let server = server_opt.expect("Failed to bind Rover UI HTTP server");
    let local_url = format!("http://localhost:{chosen_port}");
    println!("Rover Browser 2.0 listening on {local_url}");

    let shell_html = render_browser_shell_html();
    let playground_html = render_playground_html();
    let store = Store::new(Some(std::env::temp_dir().join("rover_live_server_v2")), None);
    let home_html = build_homepage_html(&store, "DuckDuckGo", "https://duckduckgo.com/");

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        if url == "/rover-icon.png" || url == "/favicon.ico" {
            let header = Header::from_bytes(&b"Content-Type"[..], &b"image/png"[..]).unwrap();
            let response = Response::from_data(ROVER_ICON_PNG_BYTES.to_vec()).with_header(header);
            let _ = request.respond(response);
            continue;
        }

        let (content_type, body) = if url.starts_with("/playground") {
            ("text/html; charset=utf-8", playground_html.clone())
        } else if url.starts_with("/home") {
            ("text/html; charset=utf-8", home_html.clone())
        } else if url.starts_with("/api/status") {
            (
                "application/json",
                serde_json::json!({
                    "app": "Rover",
                    "version": "2.0.0",
                    "theme": "Inverted Mars",
                    "engine": "Rust",
                    "providers": 26,
                    "challenges": 21
                })
                .to_string(),
            )
        } else {
            ("text/html; charset=utf-8", shell_html.clone())
        };

        let header = Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap();
        let response = Response::from_string(body).with_header(header);
        let _ = request.respond(response);
    }
}
