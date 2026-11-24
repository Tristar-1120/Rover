use rover_core::Store;
use rover_net::{
    adblock::{Blocker, FilterEngine, ResourceKind},
    blocklist::DomainBlocker,
    certs::{format_sha256_fingerprint, CertDecision, CertTrustStore},
    darkmode::DarkMode,
    extensions::{Extensions, GALLERY_SPECS},
    proxy::{ProxyConfig, ProxyMode, ProxyScheme},
    safebrowsing::{SafeBrowsingGuard, ThreatKind},
};
use std::sync::Arc;
use tempfile::TempDir;

#[test]
fn test_adblock_aho_corasick_rules_and_binary_cache() {
    let rules = r#"
! Title: Rover Test Filter List
||doubleclick.net^
||ads.example.com^$third-party
/banner_ad_$image,script
@@||ads.example.com/allowed.js$script
news.example.com##.sponsored-box
news.example.com##.promo-banner
news.example.com#@#.promo-banner
"#;

    let engine = FilterEngine::from_lists(&[rules]);

    // Pure domain rule blocks subresources but never MainFrame
    assert!(engine.should_block(
        "https://static.doubleclick.net/pixel.gif",
        "https://news.example.com/article",
        ResourceKind::Image
    ));
    assert!(!engine.should_block(
        "https://doubleclick.net/",
        "https://doubleclick.net/",
        ResourceKind::MainFrame
    ));

    // Third-party rule blocks on third-party page, allows on first-party page
    assert!(engine.should_block(
        "https://ads.example.com/track.js",
        "https://other-site.org/",
        ResourceKind::Script
    ));
    assert!(!engine.should_block(
        "https://ads.example.com/track.js",
        "https://news.example.com/",
        ResourceKind::Script
    ));

    // Exception rule @@ overrides block rule
    assert!(!engine.should_block(
        "https://ads.example.com/allowed.js",
        "https://other-site.org/",
        ResourceKind::Script
    ));

    // Token-indexed Aho-Corasick pattern rule with resource kind filter
    assert!(engine.should_block(
        "https://cdn.site.org/assets/banner_ad_728x90.png",
        "https://site.org/",
        ResourceKind::Image
    ));
    assert!(!engine.should_block(
        "https://cdn.site.org/assets/banner_ad_style.css",
        "https://site.org/",
        ResourceKind::Stylesheet
    ));

    // Cosmetic filter + #@ exception
    let css = engine.hide_css("sub.news.example.com");
    assert!(css.contains(".sponsored-box"));
    assert!(!css.contains(".promo-banner"));

    // Binary snapshot round-trip
    let bytes = engine.to_bytes();
    let restored = FilterEngine::from_bytes(&bytes).unwrap();
    assert!(restored.should_block(
        "https://static.doubleclick.net/pixel.gif",
        "https://news.example.com/",
        ResourceKind::Image
    ));

    // Per-profile Blocker pause & counters
    let tmp = TempDir::new().unwrap();
    let store = Store::new(Some(tmp.path().to_path_buf()), None);
    let blocker = Blocker::new(store, Arc::new(restored));
    assert!(blocker.check_and_record(
        "https://static.doubleclick.net/pixel.gif",
        "https://news.example.com/page",
        ResourceKind::Image
    ));
    assert_eq!(blocker.blocked_count("https://news.example.com/page"), 1);

    blocker.set_paused_for("https://news.example.com/page", true);
    assert!(!blocker.check_and_record(
        "https://static.doubleclick.net/pixel.gif",
        "https://sub.news.example.com/page",
        ResourceKind::Image
    ));
}

#[test]
fn test_domain_blocker_hosts_wildcards_and_unblock() {
    let tmp = TempDir::new().unwrap();
    let store = Store::new(Some(tmp.path().to_path_buf()), None);
    let db = DomainBlocker::new(store);

    db.load_external_list_text(
        "0.0.0.0 telemetry.bad.org\n127.0.0.1 localhost\n||tracker.evil.net^\n*.adserver.io\n",
    );
    assert_eq!(
        db.match_url_or_host("https://sub.telemetry.bad.org/ping").as_deref(),
        Some("telemetry.bad.org")
    );
    assert_eq!(
        db.match_url_or_host("https://tracker.evil.net/a.js").as_deref(),
        Some("tracker.evil.net")
    );
    assert_eq!(
        db.match_url_or_host("https://cdn.adserver.io/b.js").as_deref(),
        Some("adserver.io")
    );
    assert!(db.match_url_or_host("http://localhost:8080/").is_none());

    // Unblocking adds to allowlist and overrides external blocklist
    db.unblock_domain("telemetry.bad.org");
    assert!(db
        .match_url_or_host("https://sub.telemetry.bad.org/ping")
        .is_none());
}

#[test]
fn test_safebrowsing_shared_hosts_and_session_bypass() {
    let tmp = TempDir::new().unwrap();
    let store = Store::new(Some(tmp.path().to_path_buf()), None);
    let guard = SafeBrowsingGuard::new(store);

    guard.add_host_list("phish-login.example\ngithub.io\n", ThreatKind::Phishing);
    guard.add_url_list(
        "https://evil-user.github.io/fake-bank/login.html\n",
        ThreatKind::Phishing,
    );

    // Whole-host phishing domain blocked
    assert_eq!(
        guard.check("https://www.phish-login.example/index"),
        Some(ThreatKind::Phishing)
    );
    // Shared host github.io is NOT blanket-blocked, only the specific malicious path is blocked
    assert_eq!(guard.check("https://good-user.github.io/docs/"), None);
    assert_eq!(
        guard.check("https://evil-user.github.io/fake-bank/login.html?user=1"),
        Some(ThreatKind::Phishing)
    );

    // Session bypass allows continuing
    guard.allow_for_session("https://phish-login.example");
    assert_eq!(guard.check("https://phish-login.example/index"), None);
}

#[test]
fn test_proxy_flags_and_certificate_session_trust() {
    let cfg = ProxyConfig {
        mode: ProxyMode::Manual,
        scheme: ProxyScheme::Socks5,
        host: "127.0.0.1".into(),
        port: 9050,
        user: String::new(),
        bypass: "localhost, 127.0.0.1".into(),
        pac: String::new(),
        webrtc_private: true,
    };
    let flags = cfg.chromium_flags().join(" ");
    assert!(flags.contains("--proxy-server=socks5://127.0.0.1:9050"));
    assert!(flags.contains("--host-resolver-rules=MAP * ~NOTFOUND"));
    assert!(flags.contains("disable_non_proxied_udp"));

    // Certificate trust isolation & changed fingerprint detection
    let certs = CertTrustStore::new();
    let fp1 = format_sha256_fingerprint(b"cert-v1");
    let fp2 = format_sha256_fingerprint(b"cert-v2");

    assert_eq!(
        certs.evaluate("default", false, "https://10.0.255.1/admin", &fp1),
        CertDecision::AskUser {
            fingerprint_changed: false
        }
    );
    certs.trust_for_session("default", false, "https://10.0.255.1/admin", &fp1);
    assert_eq!(
        certs.evaluate("default", false, "https://10.0.255.1/status", &fp1),
        CertDecision::TrustedForSession
    );
    // Private profile does NOT inherit normal profile's self-signed cert trust
    assert_eq!(
        certs.evaluate("default", true, "https://10.0.255.1/status", &fp1),
        CertDecision::AskUser {
            fingerprint_changed: false
        }
    );
    // Changed certificate on same host must prompt again with fingerprint_changed = true
    assert_eq!(
        certs.evaluate("default", false, "https://10.0.255.1/status", &fp2),
        CertDecision::AskUser {
            fingerprint_changed: true
        }
    );
}

#[test]
fn test_extensions_gallery_darkmode_and_private_window_rules() {
    let tmp = TempDir::new().unwrap();
    let store = Store::new(Some(tmp.path().to_path_buf()), None);
    let dm = DarkMode::new(store);
    assert!(!dm.is_enabled());
    dm.set_enabled(true);
    assert!(dm.should_darken("https://en.wikipedia.org/wiki/Mars"));
    assert!(!dm.should_darken("rover:playground"));
    dm.set_site_off("https://wikipedia.org", true);
    assert!(!dm.should_darken("https://en.wikipedia.org/wiki/Mars"));

    let exts = Extensions::new(tmp.path());
    assert_eq!(GALLERY_SPECS.len(), 9);
    let yt = exts.install_from_gallery("youtube-enhancer").unwrap();
    assert!(yt.enabled);

    // Matches youtube.com in normal windows, never runs in private windows
    assert_eq!(
        exts.scripts_for_url("https://www.youtube.com/watch?v=123", false)
            .len(),
        1
    );
    assert_eq!(
        exts.scripts_for_url("https://www.youtube.com/watch?v=123", true)
            .len(),
        0
    );
    assert_eq!(
        exts.scripts_for_url("https://example.com/", false).len(),
        0
    );
}
