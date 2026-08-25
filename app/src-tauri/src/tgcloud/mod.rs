//! # TG Cloud storage provider
//!
//! A second storage backend for Telegram Drive that mirrors the behavior of
//! the real TG Cloud 1.2.0 Android application (package `com.telegram.cloud`),
//! as documented by the forensic report
//! `TGCloud_1.2.0_complete_technical_forensic_report.md`.
//!
//! ## Architecture
//!
//! ```text
//! UI (Special tab)
//!   -> Tauri commands (`commands.rs`)
//!   -> TGCloudProvider (`provider.rs`)
//!        -> local SQLite metadata (`db.rs`)
//!        -> BotPool + BotApiClient (`bot_pool.rs`, `bot.rs`)
//!        -> bounded streaming chunker (`chunker.rs`)
//!        -> sync chain          (`sync.rs`)
//!        -> .link sharing       (`share.rs`)
//!        -> metadata backup     (`backup.rs`)
//! ```
//!
//! ## Non-negotiable invariants (from the project spec)
//!
//! * ONE Telegram storage channel holds every physical chunk and every sync
//!   message. Bots are workers, not storage locations.
//! * Logical chunk size is 10 MiB (`10 * 1024 * 1024`).
//! * RAM usage is independent of logical file size. A bounded producer /
//!   consumer queue applies backpressure to the file reader; we never hold an
//!   entire file in memory.
//! * Up to five bot workers claim chunks dynamically (work-stealing), chunks
//!   may arrive out of order; ordering is recovered from chunk metadata.
//! * A failed upload is NOT resumable — every attempt gets a fresh
//!   `uploadAttemptId` and stale chunks are never reused.
//! * The sync index is published exactly once, AFTER every chunk has been
//!   verified. An incomplete upload never produces a `sync_index`.
//!
//! ## Compatibility notes
//!
//! * Chunk indexing is **one-based** (matching the APK's
//!   `<name>.chunk_<N>_of_<TOTAL>` naming observed in real screenshots and
//!   the 252-chunk reference record).
//! * The per-chunk `hash:` field uses **MD5** of the physical chunk payload
//!   (the reference caption `6862e1483a5e9416` is 16 hex chars = 128 bits).
//!   The logical file `checksum` uses SHA-256. Both are documented in
//!   `models.rs`.

pub mod backup;
pub mod bot;
pub mod bot_pool;
pub mod chunker;
pub mod commands;
pub mod crypto;
pub mod db;
pub mod models;
pub mod provider;
pub mod share;
pub mod sync;

pub use models::*;
pub use provider::TGCloudProvider;

/// Logical chunk size used by TG Cloud 1.2.0: exactly 10 MiB.
pub const CHUNK_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum number of bot workers supported by the TG Cloud compatibility layer.
pub const MAX_BOT_WORKERS: usize = 5;

/// Storage-provider capability identifier used in UI routing.
pub const PROVIDER_ID: &str = "tgcloud";
