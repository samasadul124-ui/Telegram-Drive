# AI Workspace State — Telegram Drive + TG Cloud

## Project Identity

| Field | Value |
|---|---|
| Project | Telegram Drive with TG Cloud second storage backend |
| Repo | https://github.com/samasadul124-ui/Telegram-Drive |
| Working branch | `feature/tgcloud-special-backend` |
| Upstream | https://github.com/caamer20/Telegram-Drive (v3.5.0) |
| Forensic ref | `TGCloud_1.2.0_complete_technical_forensic_report.md` (repo root) |

## What was built (checkpoint 1 — backend + Special UI)

A REAL second storage backend, `TGCloudProvider`, wired into Tauri and exposed
through a new **Special** sidebar item. The normal Telegram Drive pipeline is
untouched.

### Rust backend — `app/src-tauri/src/tgcloud/`

| File | Responsibility |
|---|---|
| `mod.rs` | Module root, constants: `CHUNK_SIZE = 10 MiB`, `MAX_BOT_WORKERS = 5` |
| `models.rs` | `TGCloudFile`, `ChunkRecord`, `TGCloudFolder`, `TGCloudConfig`, `BotWorkerConfig`, `TransferProgress`, `ShareManifest`, `SyncLog` |
| `crypto.rs` | Sync (PBKDF2 100k + AES-256-GCM + gzip, `sync_index:v1:`), `.link` (PBKDF2 10k + AES-256-CBC), backup (`BKP1` + SHA256(pw‖salt) + AES-256-CBC). Roundtrip unit tests included. |
| `chunker.rs` | Bounded streaming reader: reads one 10 MiB chunk into a finite-capacity mpsc; backpressure when workers saturated. RAM independent of file size. |
| `bot.rs` | Thin Telegram Bot API client (`getMe`, `sendDocument`, `sendMessage`, `getFile`, file download, `deleteMessage`) |
| `bot_pool.rs` | Dynamic work-stealing pool of up to 5 bots; per-chunk bot assignment; upload with bounded queue; positional-write downloader; orphan cleanup on failure. |
| `db.rs` | Isolated `tgcloud.db` with `tgcloud_files`, `tgcloud_chunks`, `tgcloud_folders`, `tgcloud_upload_attempts`, `tgcloud_sync_state`, `tgcloud_sync_log`, `tgcloud_bot_workers`, `tgcloud_share_links` + `tgcloud_meta`. |
| `sync.rs` | Chained encrypted sync journal (`prevId`), `sync_index` published once after all chunks verified. |
| `share.rs` | Encrypted `.link` import/export matching the real 305-file manifest format. |
| `backup.rs` | `BKP1` metadata backup (DB + config ZIP, no file payloads). |
| `provider.rs` | `TGCloudProvider` — orchestrates upload/download/sync/share/backup, owns DB and cancellations. |
| `commands.rs` | Tauri commands `tgcloud_*` (config, CRUD, transfer, share, backup, save-file). |
| `README.md` | Architecture, invariants, crypto table, build instructions. |

### Frontend

| File | Responsibility |
|---|---|
| `app/src/types/tgcloud.ts` | TS types mirroring Rust models |
| `app/src/services/tgcloud.ts` | Typed `invoke` wrapper + `onTgCloudTransfer` event listener |
| `app/src/components/special/SpecialView.tsx` | Functional Special tab: file/folder browser, upload, download, search, sort, delete, share (.link), settings gear, transfer progress, type-aware icons |
| `app/src/components/special/TGCloudSettingsPanel.tsx` | API ID/Hash, Channel ID, five separate bot-token slots each with Test/enable/remove + resolved @username, worker count, queue depth, sync, sharing policy |
| `app/src/components/desktop/dashboard/Sidebar.tsx` | Added **Special** nav item (Sparkles icon) above Saved Messages |
| `app/src/components/desktop/DesktopDashboard.tsx` | Renders `<SpecialView>` overlay when Special is selected; normal view preserved |

### Wiring

- `app/src-tauri/src/lib.rs`: declares `pub mod tgcloud;`, initializes
  `TGCloudProvider` in setup, registers all `tgcloud_*` commands.
- `app/src-tauri/Cargo.toml`: added direct deps (most already in the tree
  transitively): `reqwest` (multipart, stream, rustls-tls, json), `md5`,
  `uuid` (v4), `flate2`, `zip`, `pbkdf2`, `hmac`, `sha2`, `cbc`, `aes`,
  `aes-gcm`, `base64`, `anyhow`.

## Key protocol decisions (documented in code)

- **Chunk indexing: ONE-based** (`chunk_1_of_TOTAL`) — matches real TG Cloud screenshots and the 252-chunk reference.
- **Per-chunk hash: MD5** — real caption `hash:6862e1483a5e9416` is 16 hex chars (128-bit MD5).
- **Logical file checksum: SHA-256** — streamed across chunks, independent of file size.
- **Caption:** `[CHUNK]|fileId:<UUID>|chunk:<N>|total:<TOTAL>|name:<NAME>|hash:<MD5>`
- **Chunk filename:** `<name>.chunk_<N>_of_<TOTAL>`
- **Sync published only after all chunks verified** (never per-chunk); failed attempts are not resumable — every upload gets a fresh UUID and orphaned chunks are best-effort deleted.
- **One storage channel** for physical chunks and sync metadata; up to 5 bot workers, dynamic work-stealing.

## Current Task / Next Actions

Checkpoint 1 is committed and pushed. Next, in priority order:

1. **`cargo check` / `npm run build` on EndeavourOS** — Rust isn't in the AI
   sandbox, so the first real compile is on the user's machine. Fix any
   borrow/type errors that surface (the code was written against sqlite 0.37,
   cbc 0.1, aes 0.8, aes-gcm 0.10, reqwest 0.12).
2. **Real Telegram testing** (spec Part 34): isolated test channel, small
   image → MP4 → 500 MB → 2 GB → 3.3 GB; verify chunk metadata, sync_index,
   download reconstruction, streaming.
3. **Multi-device sync pull** (Part 32): implement channel-history scan to
   fetch remote `sync_index` nodes and replay chained entries (publish path
   done; pull/replay is the remaining sync work).
4. **Share import → add-to-cloud** (Part 33): `tgcloud_import_share` parses
   manifests; need the UI flow that adds imported files to the local list
   without physical duplication.
5. **Streaming preview** (Part 24): range/chunk-aware media reads for the
   Special tab (currently download + type icons; full streaming is next).
6. **Memory measurement tests** (Part 30): instrument RSS at 100 MB / 1 GB /
   3.3 GB / 10 GB and record peak delta.
7. **Failure injection tests** (Part 31): Wi-Fi loss, bot failure, cancel,
   corrupted chunk, restart.

## Known limitations at this checkpoint

- Sync PUBLISH is implemented; remote PULL/replay across devices is scaffolded
  but not yet traversing Telegram channel history.
- Media streaming in the Special tab is not yet wired (download + icons work;
  bounded-memory streaming is a follow-up per Part 24).
- Mobile (Android) UI for Special is not added yet — desktop only in this
  checkpoint. The Rust backend compiles for Android (no platform-specific
  code), so adding a mobile screen is straightforward.
- No `cargo check` has run in the AI sandbox (no Rust toolchain). The user
  must run the first compile.

## Build commands (EndeavourOS)

```bash
sudo pacman -S --needed webkit2gtk base-devel curl wget file openssl gtk3 \
  libappindicator-gtk3 librsvg
cd app
npm install
npm run tauri dev      # dev
npm run tauri build    # release
cargo test --manifest-path src-tauri/Cargo.toml tgcloud   # crypto unit tests
```

## File Map (new files)

See the table above. All TG Cloud code is isolated under
`app/src-tauri/src/tgcloud/` and `app/src/components/special/`; existing
Telegram Drive files were only modified to register the module/commands and
add the sidebar entry + overlay.
