# AI Workspace State — Telegram Drive (Custom Fork)

> This file is the persistent handoff record per the GitHub-Based Workspace
> Persistence and AI-Agent Handoff Protocol. Any AI agent taking over this
> project should read this file FIRST.

---

## Project Identity

| Field            | Value                                                        |
|------------------|--------------------------------------------------------------|
| Project name     | Telegram Drive (custom fork)                                 |
| Repository name  | Telegram-Drive                                               |
| Repository URL   | https://github.com/samasadul124-ui/Telegram-Drive            |
| Upstream (forked)| https://github.com/caamer20/Telegram-Drive                   |
| Current branch   | `feature/custom-changes`                                     |
| Base branch      | `main`                                                       |
| Project purpose  | Cross-platform desktop/mobile app that turns a Telegram account into a personal cloud drive. Built with Tauri + Rust + React + TypeScript. Custom changes are being applied on top of the upstream v3.5.0 codebase. |

---

## Current Development Status

### Completed

- [x] Repository shallow-cloned into temporary AI workspace (83 MB).
- [x] Git authentication configured (PAT stored in credential helper, **never committed**).
- [x] Working branch `feature/custom-changes` created from `main`.
- [x] Project structure explored and documented (see File Map below).
- [x] This `AI_WORKSPACE_STATE.md` created as the persistent handoff record.
- [x] `WORKSPACE_MANIFEST.txt` created to track temporary workspace contents.
- [x] `.env.example` template added (no real secrets committed).

### Currently Being Worked On

- **Awaiting user specification of custom changes.** The user stated they want
  to "make some changes of my own" but has not yet provided the detailed
  project specification or specific change requests. The repository has been
  cloned and prepared; implementation will begin once the user describes the
  desired changes.

### Remaining / Unfinished

- [ ] Receive and document the user's specific custom-change requirements.
- [ ] Implement changes incrementally with atomic commits.
- [ ] Test each change where applicable (frontend `npm test`, Rust `cargo check`).
- [ ] Push branch to GitHub and open a Pull Request (or merge, per user choice).
- [ ] Update this file after every milestone.
- [ ] Final validation checklist (see protocol §16).

### Known Bugs

- None introduced by custom changes yet (fresh branch from upstream).
- Refer to upstream `CHANGELOG.md` and `ARCHITECTURE_REMEDIATION.md` for
  known upstream issues.

### Known Limitations

- AI workspace is capped at ~128 MB. The full clone is 83 MB; there is
  **~45 MB of headroom**. `node_modules/` and Rust `target/` must NOT be
  installed in the workspace (they would exceed the limit). Build/test
  commands that require full dependency installation cannot run in this
  environment unless the workspace is cleaned first.
- Tauri desktop builds require platform-specific system dependencies that
  are not available in the sandbox. Code can be edited and statically
  checked, but full desktop builds must happen on the user's machine or CI.
- The supporter-service is a Cloudflare Worker; local testing requires
  Wrangler and Node dependencies.

### Current Architecture

```
Telegram-Drive/
├── app/                          # Tauri + React desktop/mobile application
│   ├── src/                      # React/TypeScript frontend
│   │   ├── components/           # UI components
│   │   ├── context/              # React context providers
│   │   ├── hooks/                # Custom React hooks
│   │   ├── services/             # Frontend service layer (Telegram API, etc.)
│   │   ├── i18n/                 # Internationalization (multi-language)
│   │   ├── theme/                # Theme system
│   │   ├── design/               # Design tokens / styles
│   │   ├── utils/                # Utility functions
│   │   ├── types/                # TypeScript type definitions
│   │   ├── config/               # App configuration
│   │   └── assets/               # Static assets
│   ├── src-tauri/                # Rust backend (Tauri)
│   │   ├── src/
│   │   │   ├── commands/         # Tauri IPC commands
│   │   │   ├── crypto/           # Encryption module
│   │   │   ├── sync_engine/      # Sync engine
│   │   │   ├── api_routes.rs     # Local REST API
│   │   │   ├── share_routes.rs   # Share-link routes
│   │   │   ├── webdav.rs         # WebDAV server
│   │   │   ├── server.rs         # Embedded HTTP server
│   │   │   ├── transfer_engine.rs# Upload/download engine
│   │   │   ├── upload_service.rs # Upload service
│   │   │   ├── crypto_commands.rs# Crypto Tauri commands
│   │   │   ├── db.rs / db_migrations.rs  # SQLite local DB
│   │   │   ├── transcode.rs / fmp4_remux.rs / mp4_utils.rs  # Media
│   │   │   ├── socks5_bridge.rs / proxy_secret.rs           # Proxy
│   │   │   ├── desktop_*.rs      # Desktop lifecycle, tray, notifications
│   │   │   └── lib.rs / main.rs  # Entry points
│   │   ├── Cargo.toml            # Rust dependencies
│   │   ├── tauri.conf.json       # Tauri configuration
│   │   └── icons/                # App icons
│   ├── package.json              # Node dependencies & scripts
│   ├── vite.config.ts            # Vite build config
│   ├── tests/                    # Frontend tests
│   └── android-overrides/        # Android-specific overrides
├── supporter-service/            # Cloudflare Worker (supporter payments)
│   ├── src/
│   │   ├── index.ts              # Worker entry point
│   │   ├── db.ts                 # D1 database layer
│   │   ├── crypto.ts             # Entitlement crypto
│   │   ├── paypal.ts             # PayPal integration
│   │   ├── terms.ts              # Terms/legal
│   │   └── types.ts              # Type definitions
│   ├── migrations/               # D1 SQL migrations
│   └── wrangler.jsonc            # Cloudflare Worker config
├── Docs/                         # Documentation & assets
├── screenshots/                  # App screenshots
├── .github/                      # GitHub workflows / templates
└── *.md                          # Top-level documentation
```

---

## Current Task

**Task:** Awaiting user specification of custom changes to the Telegram Drive
application. The repository has been cloned and the persistence infrastructure
is in place.

**Completed so far:**
1. Shallow clone of `samasadul124-ui/Telegram-Drive` (83 MB).
2. Git configured with user identity and PAT-based credential helper.
3. Feature branch `feature/custom-changes` created.
4. `AI_WORKSPACE_STATE.md` and `WORKSPACE_MANIFEST.txt` created.
5. `.env.example` template added.

**Remaining:**
1. User to describe the specific custom changes desired.
2. Implement changes in small, atomic commits.
3. Push to GitHub after each milestone.
4. Open PR or merge per user instruction.

**Expected next action:** Read the user's change requirements and begin
implementation, starting with the smallest impactful change.

---

## File Map

```
app/
  src/
    App.tsx                 - Root React component
    main.tsx                - React entry point
    App.css                 - Global styles
    types.ts                - Global TypeScript types
    utils.ts                - Global utility functions
    components/             - Reusable UI components (file grid, viewers, dialogs, etc.)
    context/                - React contexts (auth, transfers, theme, etc.)
    hooks/                  - Custom React hooks
    services/               - API service layer (Telegram client bridge, REST client)
    i18n/                   - Internationalization locales & config
    theme/                  - Theme definitions and custom theme editor
    design/                 - Design system tokens
    config/                 - Runtime configuration
    utils/                  - Modular utilities
    types/                  - Modular TypeScript type definitions
    assets/                 - Bundled static assets
  src-tauri/
    src/
      lib.rs                - Tauri app builder & plugin registration
      main.rs               - Binary entry point
      commands/             - Tauri command handlers (IPC from frontend)
      crypto/               - File encryption/decryption engine
      crypto_commands.rs    - Crypto IPC commands
      sync_engine/          - Background synchronization engine
      api_routes.rs         - Local REST API route definitions
      share_routes.rs       - Local share-link HTTP routes
      webdav.rs             - WebDAV server implementation
      server.rs             - Actix-web embedded server
      server_lifecycle.rs   - Server start/stop management
      transfer_engine.rs    - Upload/download transfer orchestration
      upload_service.rs     - File upload pipeline
      db.rs                 - SQLite database connection
      db_migrations.rs      - Database schema migrations
      models.rs             - Rust data models
      transcode.rs          - Video transcoding (ffmpeg)
      fmp4_remux.rs         - Fragmented MP4 remuxing for streaming
      mp4_utils.rs          - MP4 parsing utilities
      socks5_bridge.rs      - Local SOCKS5 proxy bridge
      proxy_secret.rs       - Proxy credential management
      bandwidth.rs          - Bandwidth throttling
      vpn_optimizer.rs      - VPN/network optimization
      network_keepalive.rs  - Connection keep-alive
      temp_artifacts.rs     - Temporary file management
      desktop_tray.rs       - System tray integration
      desktop_lifecycle.rs  - App lifecycle (minimize, close, startup)
      desktop_notifications.rs - Native notifications
      desktop_power.rs      - Power management
      desktop_preferences.rs- Persistent desktop preferences
      android_security.rs   - Android-specific security
      android_updates.rs    - Android update handling
      jni_cache.rs          - JNI cache for Android
      linux_startup.rs      - Linux autostart
    Cargo.toml              - Rust dependencies (grammers Telegram client, tauri, actix-web, tokio, etc.)
    tauri.conf.json         - Tauri build & window config
    capabilities/           - Tauri permission capabilities
    icons/                  - App icons (all platforms)
  package.json              - Node.js dependencies & npm scripts
  vite.config.ts            - Vite bundler config
  tsconfig.json             - TypeScript config
  vitest.config.ts          - Unit test config
  playwright.config.ts      - E2E test config
  tests/                    - Test files
  scripts/                  - Build/utility scripts
  android-overrides/        - Android build overrides
supporter-service/
  src/
    index.ts                - Cloudflare Worker request handler
    db.ts                   - D1 database operations
    crypto.ts               - Entitlement signing/verification
    paypal.ts               - PayPal order/capture webhook handling
    terms.ts                - Legal terms text
    types.ts                - TypeScript types
    *.test.ts               - Unit tests
  migrations/               - D1 SQL schema migrations
  wrangler.jsonc            - Wrangler deployment config
  package.json              - Worker dependencies
Docs/
  Telegram-Drive.html       - Full HTML documentation
  ANDROID_SIDELOAD_RELEASE.md - Android sideload guide
  assets/                   - Documentation images
.github/                    - CI workflows & GitHub templates
screenshots/                - Application screenshots
AI_WORKSPACE_STATE.md       - ← YOU ARE HERE (persistent AI handoff)
WORKSPACE_MANIFEST.txt      - Temporary workspace contents tracker
.env.example                - Environment variable template (safe placeholders)
AGENTS.md                   - Upstream agent instructions (supporter license invariants)
README.md                   - Project readme
CHANGELOG.md                - Version history
ARCHITECTURE_REMEDIATION.md - Known architecture issues & remediations
PHASE_0_BASELINE.md         - Baseline assessment
PRIVACY.md                  - Privacy policy
REST_API_Documentation.md   - Local REST API reference
SUPPORTER_*.md              - Supporter payment service documentation
SYNC_GUIDE.md               - Sync feature guide
WEBDAV_GUIDE.md             - WebDAV setup guide
```

---

## Important Technical Decisions

1. **Framework:** Tauri v2 (Rust backend) + React 19 + TypeScript (frontend).
2. **Telegram Client:** `grammers` (Rust MTProto library) — pinned to a specific
   git revision (`d07f96f`). The client connects directly to Telegram; no
   intermediate server is used for file storage.
3. **Storage Backend:** Telegram itself (Saved Messages + channels serve as
   "folders"). A local SQLite database stores metadata, preferences, and
   transfer queue state.
4. **File Size Limit:** ~2,000,000,000 bytes per Telegram object. Encrypted
   files have slightly lower plaintext capacity due to envelope overhead.
5. **Encryption:** Optional client-side encryption (AES-based) in the Rust
   `crypto/` module. Keys never leave the device.
6. **Local Server:** Embedded Actix-web server provides a REST API and WebDAV
   access, both loopback-only (127.0.0.1) and off by default, with API key
   authentication.
7. **Media Streaming:** fMP4 remuxing and optional transcoding via ffmpeg for
   in-browser video playback. HLS.js on the frontend.
8. **Supporter System:** A separate Cloudflare Worker (`supporter-service/`)
   handles a one-time $5.00 USD lifetime ad-free entitlement via PayPal. D1
   database. **This is a protected compatibility contract — see AGENTS.md and
   SUPPORTER_LICENSE_INVARIANTS.md before modifying anything related to
   payments, entitlements, or ads.**
9. **Platforms:** Windows, macOS (Intel + Apple Silicon), Linux, and Android.
10. **Build System:** Vite for frontend, Cargo for Rust, Tauri CLI for
    packaging. npm scripts orchestrate everything.
11. **Testing:** Vitest (frontend unit), Playwright (E2E/visual), Rust tests
    via Cargo. Cloudflare Worker tests via Vitest.
12. **i18n:** i18next with multiple locales. Validation scripts enforce
    key completeness.
13. **Concurrency:** Tokio async runtime in Rust. React Query + virtualized
    lists on the frontend for large folders.

---

## Dependencies

### Frontend (app/package.json)
- React 19, React DOM 19
- Tauri Apps API v2 + plugins (clipboard, deep-link, dialog, opener, os,
  process, shell, store, updater)
- @tanstack/react-query v5, @tanstack/react-virtual v3
- @dnd-kit (drag and drop)
- framer-motion v12 (animations)
- i18next v26, react-i18next v17
- lucide-react (icons)
- hls.js (video streaming), mp4box (MP4 parsing)
- pdfjs-dist (PDF viewing)
- qrcode.react (QR codes)
- sonner (toast notifications)
- Dev: Vite 7, TypeScript 5.8, Tailwind CSS 4, Vitest 4, Playwright 1.62

### Backend (app/src-tauri/Cargo.toml)
- tauri v2
- grammers-client / grammers-session / grammers-mtsender / grammers-tl-types
  (pinned to git rev d07f96f)
- tokio (full features)
- actix-web v4, actix-cors, actix-files, actix-multipart
- serde, serde_json
- image (bmp, gif, jpeg, png, webp)
- base64, chrono, log, env_logger, futures, bytes, async-stream
- tauri plugins: store, window-state, shell, dialog, fs, updater,
  opener, os, process

### Supporter Service (supporter-service/package.json)
- Cloudflare Workers runtime
- D1 database
- PayPal API integration
- Dev: Wrangler, Vitest

---

## Commands

> ⚠️ **Workspace constraint:** Installing `node_modules/` (~200+ MB) or
> Rust `target/` (~1+ GB) will exceed the 128 MB workspace limit. These
> commands should be run on the user's local machine or in CI, NOT in the
> temporary AI workspace unless the workspace has been expanded/cleaned.

### Frontend (run from `app/`)
```bash
npm install              # Install dependencies (DO NOT run in 128MB workspace)
npm run dev              # Start Vite dev server
npm run build            # Type-check + production build (tsc && vite build)
npm test                 # Run Vitest unit tests
npm run i18n:check       # Validate i18n keys
npm run visual:test      # Run Playwright E2E/visual tests
```

### Rust Backend (run from `app/`)
```bash
cargo check              # Type-check Rust code (requires Rust toolchain)
cargo test               # Run Rust tests
cargo build              # Debug build
cargo build --release    # Release build
```

### Tauri Desktop (run from `app/`)
```bash
npm run tauri dev        # Launch desktop app in dev mode
npm run tauri build      # Build desktop installers
```

### Supporter Service (run from `supporter-service/`)
```bash
npm install              # Install dependencies
npm run dev              # Wrangler dev server
npm test                 # Run Worker unit tests
npm run deploy           # Deploy to Cloudflare (requires authorization)
```

---

## Known Problems

1. **Workspace size limit:** The shallow clone is 83 MB. Installing npm or
   Cargo dependencies will blow past the 128 MB limit. Code editing and
   static analysis are fine; full builds/tests require the user's machine
   or CI.
2. **No Tauri system deps in sandbox:** The sandbox lacks WebKit2GTK, libayatana-appindicator, and other Tauri Linux dependencies, so `tauri dev`/`tauri build` cannot run here.
3. **Supporter service requires Cloudflare credentials:** Deploying or
   integration-testing the Worker requires `CLOUDFLARE_API_TOKEN`,
   `PAYPAL_*` secrets, and D1 database binding — none of which are (or
   should be) committed to the repo.
4. **Upstream is actively maintained:** This fork is 4 hours behind the
   latest upstream commit at clone time. Merge conflicts may arise when
   pulling upstream changes.

---

## Git Workflow for This Project

- Work on branch `feature/custom-changes`.
- Make small, atomic commits with meaningful messages.
- Push after every meaningful milestone.
- Never commit secrets, `.env`, `node_modules/`, or `target/`.
- When the feature is complete, rebase/squash and open a PR to `main`,
  or merge directly per user instruction.

---

## Recovery Procedure

If the workspace is lost:

1. Clone the repo: `git clone --depth 1 -b feature/custom-changes https://github.com/samasadul124-ui/Telegram-Drive.git`
2. Read this file (`AI_WORKSPACE_STATE.md`).
3. Check `git log --oneline -20` for recent commits.
4. Read `WORKSPACE_MANIFEST.txt` if present.
5. Continue from the "Expected next action" in the Current Task section.

---

*Last updated: 2026-08-26 — Initial setup. Awaiting user change specification.*
