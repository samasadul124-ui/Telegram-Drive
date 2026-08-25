# TG Cloud Provider (`src/tgcloud/`)

A real second storage backend for Telegram Drive that implements the TG Cloud
1.2.0 protocol reconstructed from the forensic report
(`TGCloud_1.2.0_complete_technical_forensic_report.md` at the repo root).

It is exposed in the UI as the **Special** tab and is fully independent of the
normal Telegram Drive pipeline — files never cross between backends.

## Architecture

```
SpecialView.tsx  ──invoke──►  tgcloud::commands  ──►  TGCloudProvider
                                                          │
                          db.rs (tgcloud.db, isolated) ◄──┼──► bot_pool.rs ──► BotApiClient ──► Telegram
                                                          │         (work-stealing, up to 5 bots)
                                                          ├──► chunker.rs (10 MiB streaming reader, bounded queue)
                                                          ├──► sync.rs    (PBKDF2 100k + AES-256-GCM + gzip, chained journal)
                                                          ├──► share.rs   (PBKDF2 10k + AES-256-CBC, .link manifests)
                                                          └──► backup.rs  ("BKP1" + SHA-256(pw‖salt) + AES-256-CBC)
```

## Key invariants (do not violate)

| Rule | Where |
|---|---|
| ONE Telegram channel for all chunks + sync | `models::TGCloudConfig.storage_channel_id` |
| Logical chunk size = 10 MiB | `CHUNK_SIZE` in `mod.rs` |
| Bounded-memory streaming; RAM ≈ `queue_depth × 10 MiB` | `chunker.rs` + `bot_pool.rs` |
| Up to 5 bot workers, dynamic work-stealing | `bot_pool::upload_chunked` |
| One-based chunk indexing (TG Cloud 1.2.0 compat) | `chunker.rs`, `bot.rs` caption |
| Per-chunk hash = **MD5** of payload (matches real captions) | `crypto::md5_hex` |
| Logical file checksum = SHA-256 | `bot_pool::UploadState` |
| Chunk metadata caption format | `bot_pool::worker_loop` |
| Failed uploads are NOT resumable (fresh `uploadAttemptId` per attempt) | `provider::upload_file` (UUID per call) |
| `sync_index` published only after all chunks verified | `provider::publish_file_sync` |
| Sync envelope `sync_index:v1:<base64(salt‖iv‖ct+tag)>` | `crypto::seal_sync` |

## Caption / metadata format

Every physical chunk document in Telegram carries:

```
[CHUNK]|fileId:<UUID>|chunk:<N>|total:<TOTAL>|name:<NAME>|hash:<MD5>
```

`<N>` is 1-based. The chunk filename is `<name>.chunk_<N>_of_<TOTAL>`, matching
real TG Cloud screenshots.

## Cryptographic formats

| Artifact | KDF | Cipher | Layout |
|---|---|---|---|
| Sync node | PBKDF2-HMAC-SHA256, 100 000 iters, 32 B | AES-256-GCM (12 B IV, 16 B salt), gzip JSON | `sync_index:v1:<b64(salt‖iv‖ct+tag)>` |
| `.link` share | PBKDF2-HMAC-SHA256, 10 000 iters, 32 B | AES-256-CBC, PKCS#7 (16 B salt, 16 B IV) | `salt‖iv‖ct` |
| Backup | `SHA-256(password‖salt)` | AES-256-CBC, PKCS#7 (16 B salt, 16 B IV) | `BKP1‖salt‖iv‖ct` |

These match the values recovered from the real APK and verified against real
artifacts (Sections 12, 15, 19 of the forensic report).

## Building / testing on EndeavourOS

```bash
# System deps for Tauri on Arch/EndeavourOS:
sudo pacman -S --needed webkit2gtk base-devel curl wget file openssl gtk3 libappindicator-gtk3 librsvg

cd app
npm install
npm run tauri dev      # or: npm run tauri build
```

Rust unit tests for the crypto round-trips:

```bash
cd app/src-tauri
cargo test tgcloud
```

## Configuration

Set values from **Settings → TG Cloud** in the running app (or the gear icon in
the Special tab):

- **API ID / API Hash** — my.telegram.org credentials for this backend (separate
  from the normal Telegram Drive login).
- **Storage Channel ID** — the single channel (e.g. `-1001234567890`) where
  chunks and sync messages are posted.
- **Bot Tokens 1–5** — paste, click **Test** to validate via `getMe`, enable.
  At least one enabled token is required. Tokens are stored in the local
  `tgcloud.db`; never log them and never display them after save (only a
  masked prefix/suffix).
- **Worker Count / Queue Depth** — queue depth bounds transfer RAM
  (`depth × 10 MiB`). The default of 4 keeps chunk buffers around ~40 MiB.
- **Sync** — enable for multi-device sync; set a strong password (the sync
  namespace is keyed by this password, so different passwords on the same
  channel produce independent logical clouds).
