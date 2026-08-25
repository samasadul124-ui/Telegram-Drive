//! Data models for the TG Cloud provider.
//!
//! Field names and semantics mirror the Room entities recovered from the
//! real APK (`cloud_files`, `upload_tasks`, `download_tasks`, `sync_logs`,
//! `sync_metadata`) but the Rust layer uses strongly-typed structs and a
//! provider-prefixed SQLite schema (`tgcloud_*`) so it never collides with
//! the existing Telegram Drive tables.

use serde::{Deserialize, Serialize};

/// Status of a logical cloud file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    /// All chunks uploaded and verified, sync index published.
    Complete,
    /// Upload in progress (or interrupted — see attempt isolation).
    Uploading,
    /// Upload attempt failed/cancelled. The logical record is NOT presented
    /// as a usable file and must not be resumed.
    Failed,
    /// Download in progress.
    Downloading,
}

impl FileStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileStatus::Complete => "complete",
            FileStatus::Uploading => "uploading",
            FileStatus::Failed => "failed",
            FileStatus::Downloading => "downloading",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "complete" => FileStatus::Complete,
            "uploading" => FileStatus::Uploading,
            "downloading" => FileStatus::Downloading,
            _ => FileStatus::Failed,
        }
    }
}

/// A single physical 10 MiB chunk of a logical file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRecord {
    /// One-based chunk index (`chunk:<N>`).
    pub chunk_index: u32,
    /// Size in bytes of this chunk (the last may be smaller).
    pub chunk_size: u64,
    /// MD5 hex digest of the physical chunk payload.
    pub chunk_hash: String,
    /// Telegram message id holding the document in the storage channel.
    pub telegram_message_id: i64,
    /// Telegram `file_id` (Bot API) for the document.
    pub telegram_file_id: String,
    /// Telegram `file_unique_id` for the document.
    pub telegram_file_unique_id: String,
    /// Which bot worker uploaded this chunk (1..=5).
    pub bot_worker_id: u8,
}

/// A logical TG Cloud file — what the user sees in the Special tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TGCloudFile {
    /// Stable UUID v4 assigned at upload start.
    pub file_id: String,
    /// Original filename.
    pub file_name: String,
    pub mime_type: Option<String>,
    pub size_bytes: u64,
    pub total_chunks: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub status: FileStatus,
    /// Logical parent folder (`None` = root).
    pub parent_folder_id: Option<String>,
    /// Telegram message id of the *representative* document / direct file.
    /// For chunked files this is the last chunk's message id and is also the
    /// record's `telegram_message_id` column.
    pub telegram_message_id: i64,
    /// Representative Telegram file_id (direct files; chunked files use
    /// per-chunk records).
    pub file_id_tg: String,
    pub file_unique_id: String,
    /// SHA-256 hex digest of the complete logical file (computed streaming).
    pub checksum: String,
    /// JSON array of uploader bot tokens/identities, mirroring the APK's
    /// `cloud_files.uploader_tokens`. Stored as a JSON string.
    pub uploader_tokens: String,
    /// Physical chunks (empty for direct / single-message files).
    pub chunks: Vec<ChunkRecord>,
}

/// A logical folder. Physical storage is always one channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TGCloudFolder {
    pub folder_id: String,
    pub name: String,
    pub parent_folder_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A configured bot worker slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotWorkerConfig {
    /// 1-based slot index (1..=5).
    pub slot: u8,
    /// Bot token from BotFather. Empty means slot unused.
    pub token: String,
    pub enabled: bool,
    /// Resolved bot username after `getMe` validation (display only).
    pub username: Option<String>,
}

impl BotWorkerConfig {
    /// Masked token for safe display: first 8 chars + `…` + last 4.
    pub fn masked_token(&self) -> String {
        if self.token.len() <= 16 {
            return "••••••••".to_string();
        }
        format!("{}…{}", &self.token[..8], &self.token[self.token.len() - 4..])
    }
}

/// Persisted TG Cloud configuration. Secrets are stored via the host app's
/// secure storage; this struct is serialized to the app DB.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TGCloudConfig {
    /// Telegram API ID (my.telegram.org) — separate from the normal
    /// Telegram Drive user session.
    pub api_id: Option<i32>,
    pub api_hash: Option<String>,
    /// The ONE channel used for both physical chunks and sync messages.
    pub storage_channel_id: Option<i64>,
    /// Up to five bot token slots.
    pub bots: Vec<BotWorkerConfig>,

    // --- Transfer ---
    /// Worker count actually active (1..=5). Defaults to number of valid bots.
    pub worker_count: u8,
    /// Bounded chunk-queue capacity (number of 10 MiB buffers in flight).
    /// Directly bounds transfer RAM.
    pub queue_depth: u8,

    // --- Sync ---
    pub sync_enabled: bool,
    pub sync_channel_id: Option<i64>,
    pub sync_bot_token: Option<String>,
    /// Sync password is NEVER stored in plaintext; we store only a verifier
    /// salt/hash and derive keys at runtime. `sync_password_set` indicates
    /// whether the user has configured one.
    pub sync_password_set: bool,

    // --- Sharing ---
    pub require_share_password: bool,
}

impl TGCloudConfig {
    /// Number of enabled, non-empty bots.
    pub fn valid_bot_count(&self) -> usize {
        self.bots
            .iter()
            .filter(|b| b.enabled && !b.token.trim().is_empty())
            .count()
    }

    /// Tokens of enabled bots, in slot order.
    pub fn active_tokens(&self) -> Vec<String> {
        self.bots
            .iter()
            .filter(|b| b.enabled && !b.token.trim().is_empty())
            .map(|b| b.token.clone())
            .collect()
    }

    pub fn is_configured(&self) -> bool {
        self.storage_channel_id.is_some() && self.valid_bot_count() >= 1
    }
}

/// Result of testing a bot token (`getMe`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotTestResult {
    pub ok: bool,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub error: Option<String>,
}

/// Live progress event emitted to the UI during transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum TransferProgress {
    Started {
        upload_attempt_id: String,
        file_id: String,
        file_name: String,
        total_bytes: u64,
        total_chunks: u32,
    },
    ChunkUploaded {
        upload_attempt_id: String,
        chunk_index: u32,
        bot_worker_id: u8,
        bytes_sent: u64,
    },
    /// Final logical hash verification pass.
    Finalizing {
        upload_attempt_id: String,
    },
    SyncPublishing {
        upload_attempt_id: String,
    },
    Completed {
        upload_attempt_id: String,
        file_id: String,
    },
    Failed {
        upload_attempt_id: String,
        error: String,
    },
    Cancelled {
        upload_attempt_id: String,
    },
}

/// A chunk job handed from the bounded reader to a bot worker.
#[derive(Debug)]
pub(crate) struct ChunkJob {
    pub file_id: String,
    pub file_name: String,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub data: Vec<u8>,
}

/// Result returned by a worker after uploading a chunk.
#[derive(Debug, Clone)]
pub(crate) struct ChunkUploadOutcome {
    pub chunk_index: u32,
    pub chunk_size: u64,
    pub chunk_hash: String,
    pub telegram_message_id: i64,
    pub telegram_file_id: String,
    pub telegram_file_unique_id: String,
    pub bot_worker_id: u8,
}

/// Direction of a sync operation (mirrors `SyncOperation` in the APK).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SyncOperation {
    Insert,
    Update,
    Delete,
}

/// One sync log record, mirroring `sync_logs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncLog {
    pub log_id: String,
    pub timestamp: i64,
    pub device_id: String,
    pub operation: SyncOperation,
    pub table_name: String,
    pub primary_key: String,
    pub data_json: Option<String>,
    pub previous_data_json: Option<String>,
    pub is_uploaded: bool,
    pub telegram_message_id: Option<i64>,
    pub checksum: Option<String>,
}

/// Portable `.link` manifest (decrypted form).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareManifest {
    pub version: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub created_at: i64,
    pub files: Vec<ShareFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareFileEntry {
    pub file_name: String,
    pub size_bytes: u64,
    pub mime_type: Option<String>,
    pub uploaded_at: i64,
    /// Telegram file_id for direct files; for chunked files see `chunks`.
    pub file_id: Option<String>,
    pub file_unique_id: Option<String>,
    /// Uploader identity (token) — present in real TG Cloud .link files.
    pub uploader: Option<String>,
    pub total_chunks: Option<u32>,
    pub chunk_size: Option<u64>,
    #[serde(default)]
    pub chunks: Vec<ShareChunkEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareChunkEntry {
    pub chunk_index: u32,
    pub chunk_hash: String,
    pub file_id: String,
    pub file_unique_id: Option<String>,
    pub uploader: Option<String>,
}
