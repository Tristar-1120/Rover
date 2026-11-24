# Rover 2.0 (Rust Rebuild)

**[Features](FEATURES.md)** · **[Changelog](CHANGELOG.md)** · **[Website](website/index.html)**

**Rover** is a ground-up Rust rebuild and complete rebrand of Pelagic — engineered for **100× faster execution**, **58–74% lower AI token usage**, and **expanded autonomous capabilities**.

---

## Architecture & Workspace Layout

Rover is structured as a modular 4-crate Rust workspace:

- **`crates/rover-core`**:
  - **Storage & Profiles (`store`, `profiles`)**: SQLite history in WAL mode (`rusqlite`), bookmarks & folders, reading list, shared vs. per-profile settings, and deferred profile deletion.
  - **AEAD Secretbox (`secretbox`)**: Hardware-accelerated `ChaCha20-Poly1305` AEAD (`RV2`) + `scrypt` key derivation (>2,000 MB/s throughput, ~280× faster than Python's HMAC integer-XOR loop) with 100% backward compatibility for legacy `PS1` ciphertexts, OS keyring integration, and `ROVER_PASSWORD_STORE=basic` (`0o600` local key file).
  - **Password & Autofill Vaults (`passwords`, `autofill`, `pwcheck`, `pwguard`)**: Origin upgrade/downgrade protection (allows HTTP→HTTPS upgrade, blocks HTTPS→HTTP downgrade), two-step username flow cache (600s), Google/Chrome/Firefox CSV import with zero-fill file shredding, Luhn payment card validation (CVC never saved), K-anonymity Have I Been Pwned SHA-1 range checkup, and outbound prompt password leak guard.
  - **Serverless LWW Folder Sync (`sync`)**: Synchronizes bookmarks, folders, reading list, 90-day history, and passphrase-encrypted vault items across devices via `<sync_folder>/Rover Sync/<profile>/<device_id>.json` with 120-day tombstones.
  - **Capability-Gated Memory (`memory`)**: `Rover Memory.md` with strict user-capability authorization (`PermissionDenied` on unauthorized access) and atomic temp-file + `fsync` writes.

- **`crates/rover-net`**:
  - **Aho-Corasick + Domain Suffix Adblock Engine (`adblock`)**: Replaces per-rule Python regex loops with an `AhoCorasick` multi-pattern token automaton + O(1) domain hash tables and `bincode` binary snapshot caching (<2 ms load time, ~2 µs per request).
  - **Pi-hole Domain Blocklist (`blocklist`)**: High-speed (>1M lines/sec) parser for `/etc/hosts`, Adblock `||domain^`, wildcards `*.domain`, and plain domain lists (StevenBlack, OISD, HaGeZi) with per-profile allowlists and `rover:blocked` warning pages.
  - **Safe Browsing Threat Guard (`safebrowsing`)**: Local Phishing Army, OpenPhish, and URLhaus checking with shared-host protection (`github.io`, `vercel.app`, `pages.dev`, etc.).
  - **Proxy & Session TLS Trust (`proxy`, `certs`)**: HTTP/HTTPS/SOCKS5/SOCKS4/PAC proxy configuration, raw SOCKS5 handshake verifier, WebRTC UDP leak protection flags, and session-scoped SHA-256 self-signed certificate pinning isolated between normal and private profiles.
  - **Userscripts, 9 Bundled Gallery Extensions & Dark Mode (`extensions`, `darkmode`)**: `==UserScript==` engine with `GM_*` compatibility shim (`__rover_gm_`), 9 first-party Rover Gallery extensions (`rover-gallery:*`), and website Dark Mode (`rover:darkmode`).

- **`crates/rover-ai`**:
  - **Next-Gen Token Optimization Suite (`token_optimizer`)**:
    1. **DOM Delta Snapshots (`SnapshotDiffer`)**: Tracks canonical hashes of interactive elements and page text across consecutive steps on the same URL, sending only added/modified elements and omitting unchanged DOM text — reducing multi-step agent input tokens by **58–74%**.
    2. **Compact Element DSL (`CompactDsl`)**: Eliminates repeated JSON keys and redundant `placeholder`/`value` strings that duplicate the element's label.
    3. **Semantic Main-Content Prioritization & Repetition Collapsing (`SemanticPruner`)**: Extracts `<main>`/`<article>` content ahead of navigation/footer boilerplate and collapses consecutive duplicate lines into `line [repeated xN]`.
    4. **Translation String-Table Deduplicator (`TranslationDeduplicator`)**: Deduplicates repeated UI strings before batching page translation calls, saving **35–45% tokens**.
    5. **Thought Stripping & Adaptive Vision (`compact_action`, `needs_screenshot`)**: Strips old model `thought` prose from history while preserving executed arguments, pins user decisions, bounds historical extracts with honest omission markers, and skips screenshots on textual/form steps.
  - **26+ AI Providers & Hierarchical USD Meter (`providers`, `pricing`, `usage`)**: Automatic Anthropic ephemeral prompt caching (`cache_control`), Gemini low-thinking budget (`thinkingLevel: "low"`), `<think>` block stripping, exact context-tiered/date-tiered USD rate tables, thread-safe hierarchical `Meter`, and append-only `usage.jsonl` v2 log.
  - **Rover Local & Two-Tier Watchers (`localai`, `automation`)**: Zero-token command interpreter and Two-Tier background watchers (Tier-1 Native HTTP polling in ~48 KB RAM without a Chromium tab; Tier-2 headless browser tab for JS steps).

- **`crates/rover-browser`**:
  - **Pure Chromium 156 (CEF) Native Windows Desktop Browser (`chromium_win`, `chromium_ui`)**: Standalone native Win32 dark window (`1360×860`, embedded circular Inverted Mars Rover icon, DWM dark titlebar) powered by pure open-source **Chromium 156 (`libcef.dll`)** with zero Microsoft Edge / WebView2 dependency. Matches Pelagic 1-to-1 with zero pre-populated bookmarks or shortcuts, real Chromium network-level Aho-Corasick adblocking, bottom-docked Chromium DevTools, native Win32 popup menus, owned dark dialog windows, and the live autonomous Rover AI panel.
  - **Tabs, Groups, Sleeping Tabs, Split View & Workspaces (`tabs`)**: Equal-width tabs, pinned tabs, 9 group colors, auto-group by site, idle tab sleeper, horizontal split view, and named workspaces.
  - **Omnibox, Homepage, Tools & 21-Challenge Rover Playground (`omnibox`, `homepage`, `tools`, `playground`, `webui`)**: Inline autocomplete, `rover:start` Martian Horizon homepage (empty shortcuts by default), Reader Mode, Markdown/code file viewer, Chrome SQLite history importer, `yt-dlp` command builder, and `rover:playground` (21 interactive challenges salted with `'rover:'` FNV-1a hashes).

---

## Visual Identity — Inverted Mars Palette

Rover's icon ([`assets/rover-icon.png`](assets/rover-icon.png), [`assets/rover.ico`](assets/rover.ico), [`assets/rover.svg`](assets/rover.svg)) is a direct parody of Pelagic's ocean-whale badge, replacing the whale with a 6-wheeled Mars Rover and mathematically inverting the color palette (`255 - RGB`):

- **Top Mars Crimson**: `#C83F3A` (inverse of `#37C0C5`)
- **Upper Coral Terracotta**: `#EF7560` (inverse of `#108A9F`)
- **Wavy Crater Horizon**: `#DA5647` (inverse of `#25A9B8`)
- **Dusty Martian Peach**: `#FDB78E` (inverse of `#024871`)
- **Apricot Sand**: `#FED8B1` (inverse of `#01274E`)
- **Deep Maroon Rover Silhouette**: `#47090C` (inverse of `#B8F6F3`)

---

## Build, Test, Run & Windows `.exe` Installer

```bash
# Run the full test suite across all 4 crates
cargo test

# Run the speed & token-compression benchmark suite
cargo run --release --bin rover -- --bench

# Execute a zero-token Rover Local command
cargo run --release --bin rover -- --local 'open github then fill Search with rover and submit'

# Launch the interactive Rover Browser & AI Agent UI server (default port 8420)
cargo run --release --bin rover -- --serve --port 8420

# Cross-compile Windows 64-bit rover.exe + build native NSIS installer dist/RoverSetup-2.0.0.exe
./packaging/windows/build_installer.sh
```
