use rover_core::{
    autofill::{card_brand, luhn_ok, AddressEntry, AutofillStore, CardEntry},
    memory::{MemoryError, MemoryFile},
    passwords::{matches_origin, PasswordStore},
    profiles::{Profiles, DEFAULT_ID},
    pwcheck::{checkup, is_weak, sha1_hex},
    pwguard,
    secretbox::{derive_key, seal, seal_legacy_ps1, unseal, MemoryKeyring, SecretBox},
    store::Store,
    sync::FolderSync,
};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[test]
fn test_secretbox_aead_and_legacy_ps1_compatibility() {
    let key = derive_key("mars-rover-passphrase", b"1234567890123456").unwrap();
    let msg = b"top secret mission telemetry";

    // Fast RV2 ChaCha20-Poly1305 AEAD
    let rv2_token = seal(msg, &key);
    assert_eq!(unseal(&rv2_token, &key).unwrap(), msg);

    // Legacy PS1 HMAC-SHA256 CTR backward compatibility
    let ps1_token = seal_legacy_ps1(msg, &key);
    assert_eq!(unseal(&ps1_token, &key).unwrap(), msg);

    // Wrong key fails authentication
    let wrong_key = derive_key("wrong-passphrase", b"1234567890123456").unwrap();
    assert!(unseal(&rv2_token, &wrong_key).is_err());
    assert!(unseal(&ps1_token, &wrong_key).is_err());
}

#[test]
fn test_secretbox_keyring_and_refusal_when_missing() {
    let tmp = TempDir::new().unwrap();
    let kr = Arc::new(MemoryKeyring::new());
    let sb = SecretBox::with_keyring(tmp.path(), Some(kr.clone()));
    let ct = sb.protect(b"api-key-123").unwrap();
    assert_eq!(sb.unprotect(&ct).unwrap(), b"api-key-123");

    // Simulate keyring failure/lock on reopen: must refuse rather than generating a new key
    kr.set_failing(Some("Keyring daemon locked".into()));
    let sb_locked = SecretBox::with_keyring(tmp.path(), Some(kr.clone()));
    assert!(sb_locked.protect(b"new-secret").is_err());
    assert!(sb_locked.unprotect(&ct).is_err());
}

#[test]
fn test_memory_capability_gating() {
    let tmp = TempDir::new().unwrap();
    let mem = MemoryFile::new(tmp.path().join("Rover Memory.md"));

    // Unauthorized access must always return PermissionDenied
    assert!(matches!(
        mem.initialize(false),
        Err(MemoryError::PermissionDenied)
    ));
    assert!(matches!(
        mem.read(false),
        Err(MemoryError::PermissionDenied)
    ));
    assert!(matches!(
        mem.append("secret note", false),
        Err(MemoryError::PermissionDenied)
    ));

    // Authorized access works atomically
    mem.initialize(true).unwrap();
    let updated = mem.append("- First crater observation", true).unwrap();
    assert!(updated.contains("# Rover Memory"));
    assert!(updated.contains("- First crater observation"));
    assert_eq!(mem.read(true).unwrap(), updated);
}

#[test]
fn test_profiles_shared_settings_and_isolation() {
    let tmp = TempDir::new().unwrap();
    let root_store = Arc::new(Store::new(Some(tmp.path().to_path_buf()), None));
    let profiles = Profiles::new(root_store.clone());

    let work = profiles.add("Work", Some("#38BDF8"));
    assert_ne!(work.id, DEFAULT_ID);
    let work_store = profiles.store_for(&work.id);

    // Bookmarks and history are isolated per profile
    root_store.add_bookmark("https://personal.example", "Personal Site", "Home");
    work_store.add_bookmark("https://work.example", "Work Portal", "Office");
    assert!(root_store.is_bookmarked("https://personal.example"));
    assert!(!root_store.is_bookmarked("https://work.example"));
    assert!(work_store.is_bookmarked("https://work.example"));
    assert!(!work_store.is_bookmarked("https://personal.example"));

    // AI settings and API keys are shared across all profiles
    work_store.set("max_steps", json!(45));
    assert_eq!(root_store.get_i64("max_steps", 30), 45);
    root_store.set_api_key("openai", "sk-shared-key-999").unwrap();
    assert_eq!(work_store.api_key("openai", &[]), "sk-shared-key-999");

    // Deferred deletion removes profile directory (drop open SQLite store first for Windows)
    drop(work_store);
    assert!(profiles.remove(&work.id));
    profiles.purge_pending();
    assert!(!profiles.folder(&work.id).exists());
}

#[test]
fn test_passwords_origin_matching_and_csv_import() {
    assert!(matches_origin("https://example.com", "https://www.example.com/login"));
    assert!(matches_origin("http://example.com", "https://example.com/secure"));
    // HTTPS -> HTTP downgrade must be strictly blocked
    assert!(!matches_origin("https://example.com", "http://example.com/insecure"));

    let tmp = TempDir::new().unwrap();
    let sb = SecretBox::new(tmp.path());
    let pw_store = PasswordStore::new(tmp.path(), sb);

    pw_store
        .add_or_update("https://rover.app/login", "commander", "correct-horse-battery", "")
        .unwrap();
    assert_eq!(pw_store.for_url("https://www.rover.app/dashboard").len(), 1);
    assert_eq!(pw_store.for_url("http://rover.app/dashboard").len(), 0);

    // Two-step sign-in username resolution
    pw_store.record_username_step("https://accounts.google.com/v3/signin", "astro@rover.app");
    assert_eq!(
        pw_store
            .resolve_two_step_username("https://accounts.google.com/v3/challenge")
            .as_deref(),
        Some("astro@rover.app")
    );

    // Google CSV import + shred
    let csv_path = tmp.path().join("Google Passwords.csv");
    std::fs::write(
        &csv_path,
        "name,url,username,password,note\nGitHub,https://github.com/login,dev,gh-pass-123,Work token\n",
    )
    .unwrap();
    let (imported, skipped) = pw_store.import_csv_file(&csv_path, true).unwrap();
    assert_eq!((imported, skipped), (1, 0));
    assert!(!csv_path.exists(), "shredded CSV must be deleted");
    assert_eq!(pw_store.for_url("https://github.com")[0].username, "dev");
}

#[test]
fn test_pwguard_and_pwcheck() {
    // Conversational question must NOT trigger password guard
    assert!(pwguard::check("My password is wrong and expired").is_none());
    assert!(pwguard::check("How do I reset my password?").is_none());

    // Actual leaked password or key in prompt MUST trigger password guard
    let hit1 = pwguard::check("my password is SuperSecret99!").unwrap();
    assert!(hit1.contains("password") && hit1.contains("S••••"));
    assert!(!hit1.contains("SuperSecret99!"));

    let hit2 = pwguard::check("Here is my key sk-ant-api03-abcdef1234567890abcdef").unwrap();
    assert!(hit2.contains("API key"));

    let hit3 = pwguard::check("Connect to https://admin:hunter2@internal.example.com").unwrap();
    assert!(hit3.contains("username and password"));

    // Password checkup (HIBP k-anonymity + weak + reused)
    assert!(is_weak("password"));
    assert!(is_weak("12345678"));
    assert!(!is_weak("Mars#Rover2026!"));

    let tmp = TempDir::new().unwrap();
    let sb = SecretBox::new(tmp.path());
    let store = PasswordStore::new(tmp.path(), sb);
    let l1 = store
        .add_or_update("https://a.example", "u1", "password", "")
        .unwrap();
    let l2 = store
        .add_or_update("https://b.example", "u2", "password", "")
        .unwrap();
    let h = sha1_hex("password");
    let suffix = &h[5..];
    let res = checkup(&store.all(), |_prefix| Ok(format!("{suffix}:9500000\n")));
    assert_eq!(res.checked, 2);
    assert_eq!(res.issues[&l1.id].leaked, 9_500_000);
    assert_eq!(res.issues[&l1.id].reused, 1);
    assert!(res.issues[&l2.id].weak);
}

#[test]
fn test_autofill_luhn_and_folder_sync() {
    assert!(luhn_ok("4111 1111 1111 1111"));
    assert!(!luhn_ok("4111 1111 1111 1112"));
    assert_eq!(card_brand("4111111111111111"), "Visa");

    // Two-computer folder sync test (bookmarks, history, deletions, and encrypted vault)
    let sync_root = TempDir::new().unwrap();
    let pc1_dir = TempDir::new().unwrap();
    let pc2_dir = TempDir::new().unwrap();

    let s1 = Store::new(Some(pc1_dir.path().to_path_buf()), None);
    let pw1 = PasswordStore::new(pc1_dir.path(), s1.secretbox().clone());
    let af1 = AutofillStore::new(pc1_dir.path(), s1.secretbox().clone());

    let s2 = Store::new(Some(pc2_dir.path().to_path_buf()), None);
    let pw2 = PasswordStore::new(pc2_dir.path(), s2.secretbox().clone());
    let af2 = AutofillStore::new(pc2_dir.path(), s2.secretbox().clone());

    s1.add_bookmark("https://mars.nasa.gov", "Mars Mission", "Space");
    pw1.add_or_update("https://mars.nasa.gov", "Curiosity", "Jezero-Crater-1", "")
        .unwrap();
    af1.save_card(CardEntry {
        cc_name: "Commander".into(),
        cc_number: "4111111111111111".into(),
        cc_month: "12".into(),
        cc_year: "2029".into(),
        ..Default::default()
    })
    .unwrap();
    af1.save_address(AddressEntry {
        name: "Mara Okafor".into(),
        city: "Houston".into(),
        ..Default::default()
    })
    .unwrap();

    let mut sync1 = FolderSync::new("default", s1.clone(), Some(pw1.clone()), Some(af1.clone()));
    sync1.unlock_vault(sync_root.path(), "shared-vault-pass").unwrap();
    sync1.sync_now(sync_root.path()).unwrap();

    let mut sync2 = FolderSync::new("default", s2.clone(), Some(pw2.clone()), Some(af2.clone()));
    sync2.unlock_vault(sync_root.path(), "shared-vault-pass").unwrap();
    sync2.sync_now(sync_root.path()).unwrap();

    assert!(s2.is_bookmarked("https://mars.nasa.gov"));
    assert_eq!(pw2.for_url("https://mars.nasa.gov")[0].username, "Curiosity");
    assert_eq!(af2.cards()[0].masked_label(), "Visa •••• 1111");
    assert_eq!(af2.addresses()[0].given_name(), "Mara");

    // Delete bookmark on PC2 and verify tombstone propagates to PC1
    s2.remove_bookmark("https://mars.nasa.gov");
    sync2.sync_now(sync_root.path()).unwrap();
    sync1.sync_now(sync_root.path()).unwrap();
    assert!(!s1.is_bookmarked("https://mars.nasa.gov"));
}
