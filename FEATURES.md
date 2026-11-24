# Rover Features

Rover is a high-speed Rust browser engine and ultra-low-token autonomous AI browsing agent for Linux, Windows, and macOS.

## AI Assistant, Agent & Token Savers
- **Rover AI Panel** (`Ctrl+Shift+A`): Describe a task or ask a question; the agent navigates, clicks, types, selects, and extracts while glowing amber-cyan on the page.
- **DOM Delta Snapshots**: Sends only changed/new interactive elements and text diffs between steps on the same URL (saving 58–74% of multi-step input tokens).
- **Compact Element DSL & Semantic Pruner**: Prioritizes `<main>` and `<article>` content over nav/footer noise, collapses duplicate lines (`[repeated xN]`), and strips old model `thought` prose while retaining executed action arguments and user decisions.
- **Structured DOM Extraction (`extract_structured`)**: Extracts tables and lists via CSS selectors into compact structured JSON without 15,000-character raw text dumps.
- **Adaptive Vision**: Only captures screenshots on new URLs, scrolling, tab switches, `<canvas>`/`<video>` visual pages, or action retries.
- **No Context Mode**: Sends only your immediate message, always-on instruction set, and current page.
- **26+ Built-in Providers**: Gemini (with `thinkingLevel: "low"`), Anthropic (with ephemeral `cache_control`), OpenAI, OpenRouter, Groq, Mistral, DeepSeek, xAI, Together, Fireworks, Cerebras, SambaNova, NVIDIA NIM, Perplexity, Cohere, Hugging Face, GitHub Models, Hyperbolic, Nebius, SiliconFlow, Novita, Moonshot, Zhipu, Qwen, Ollama, LM Studio, plus custom endpoints.
- **Rover Local**: Zero-AI, zero-network, zero-token command parser (`open`, `search`, `click`, `type`, `fill ... and submit`, `select`, `scroll`, `wait`, `reader mode`, `take screenshot`, and `every N seconds until ...` watchers).
- **Safety Built In**: Never types passwords or sees card fields, masks secret bounding boxes in screenshots, asks before irreversible actions (`buy`, `send`, `delete`, `post`, `publish`), treats page text as data not instructions, hands CAPTCHAs back to the user, and blocks accidental password leaks in chat (`pwguard`).

## Two-Tier Background Automations (Zero AI Tokens)
- **Tier-1 Fast Native HTTP Poll (~48 KB RAM)**: Watchers without interactive JS click steps poll in native Rust using 3,000× less memory than a background Chromium tab.
- **Tier-2 Headless Browser Tab**: Watchers with pre-check steps (`click`, `type`, `select`) run in an invisible background tab.
- **Rising-Edge AI Wakeup**: Conditions (`text`, `gone`, `number` with `near` anchor, `changed` SHA-1 digest, `element`, `url`, `title`, `all`/`any`) are evaluated locally; Rover AI is invoked at most once after a match.

## Speed & Privacy Engine
- **Aho-Corasick Ad & Tracker Blocker**: Multi-pattern automaton + domain hash tables matching 100k+ EasyList/EasyPrivacy rules in ~2 µs/request with `<2 ms` binary cache startup (`bincode`).
- **Pi-hole Style Blocked Domains**: Blocks whole domains and subdomains from `/etc/hosts`, Adblock `||domain^`, wildcards `*.domain`, and external blocklists (StevenBlack, OISD, HaGeZi).
- **Safe Browsing Threat Guard**: Local Phishing Army, OpenPhish, and URLhaus verification with shared-host protection (`github.io`, `vercel.app`, etc.).
- **ChaCha20-Poly1305 AEAD Vault & LWW Folder Sync**: Hardware-accelerated encryption (>2,000 MB/s) for passwords, Luhn-validated payment cards, addresses, API keys, and serverless cloud-folder sync (`Rover Sync`).

## Tabs, Tools & Rover Playground
- **Tabs & Workspaces**: Equal-width tabs, pinned tabs, 9-color collapsible tab groups, auto-grouping by site, memory-saving sleeping tabs, horizontal split view, vertical tabs, and named workspaces.
- **Page Tools**: Fast Rust Reader Mode (Dark, Sepia, Light), string-deduplicated Page Translator, Markdown & code File Viewer, Chrome SQLite History importer, and YouTube video/audio downloader (`yt-dlp`).
- **Rover Playground (`rover:playground`)**: 21 hands-on planetary expedition challenges testing every feature of Rover.
