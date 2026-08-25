//! # TGCloudProvider — the StorageProvider implementation for TG Cloud.
//!
//! Owns the local metadata DB, the bot worker pool, and the sync/share
//! subsystems. The React frontend interacts ONLY through the Tauri commands
//! in [`super::commands`]; it never touches the Bot API, crypto, chunk
//! scheduler, or SQL directly.

use crate::tgcloud::{
    backup,
    bot::BotApiClient,
    bot_pool,
    db::{self, TgDb},
    models::*,
    share,
    sync,
};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

/// Shared, cloneable handle to the TG Cloud backend.
#[derive(Clone)]
pub struct TGCloudProvider {
    app: AppHandle,
    pub db: TgDb,
    /// Directory for temp chunk assembly during downloads.
    temp_dir: PathBuf,
    /// Active cancellations keyed by upload_attempt_id / file_id.
    cancellations: Arc<tokio::sync::Mutex<HashMap<String, Arc<AtomicBool>>>>,
    /// Stable per-install device id (used in sync logs).
    device_id: String,
}

impl TGCloudProvider {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| anyhow!("app_data_dir: {e}"))?;
        std::fs::create_dir_all(&data_dir).ok();
        let db_path = data_dir.join("tgcloud.db");
        let db = db::open(&db_path)?;
        let temp_dir = data_dir.join("tgcloud-tmp");
        std::fs::create_dir_all(&temp_dir).ok();

        // Load or create a stable device id for sync.
        let device_id = std::fs::read_to_string(data_dir.join("tgcloud.device"))
            .unwrap_or_else(|_| {
                let id = uuid::Uuid::new_v4().to_string();
                let _ = std::fs::write(data_dir.join("tgcloud.device"), id.as_bytes());
                id
            });

        Ok(Self {
            app: app.clone(),
            db,
            temp_dir,
            cancellations: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            device_id,
        })
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    // ----- configuration ---------------------------------------------------

    pub async fn config(&self) -> Result<TGCloudConfig> {
        db::load_config(&self.db).await
    }

    pub async fn save_config(&self, cfg: &TGCloudConfig) -> Result<()> {
        db::save_config(&self.db, cfg).await
    }

    /// Validate a bot token via `getMe`. Does not persist anything.
    pub async fn test_bot_token(token: &str) -> Result<BotTestResult> {
        if token.trim().is_empty() {
            return Ok(BotTestResult {
                ok: false,
                username: None,
                first_name: None,
                error: Some("empty token".into()),
            });
        }
        let client = BotApiClient::new(token.trim());
        match client.get_me().await {
            Ok(u) => Ok(BotTestResult {
                ok: true,
                username: u.username,
                first_name: u.first_name,
                error: None,
            }),
            Err(e) => Ok(BotTestResult {
                ok: false,
                username: None,
                first_name: None,
                error: Some(e.to_string()),
            }),
        }
    }

    fn require_ready(cfg: &TGCloudConfig) -> Result<(i64, Vec<String>)> {
        let channel = cfg
            .storage_channel_id
            .ok_or_else(|| anyhow!("Storage channel ID not configured"))?;
        let tokens = cfg.active_tokens();
        if tokens.is_empty() {
            return Err(anyhow!("At least one enabled bot token is required"));
        }
        Ok((channel, tokens))
    }

    // ----- upload ----------------------------------------------------------

    /// Streaming, bounded-memory upload. `source_path` must be a real file
    /// the backend can open from disk — we never read the whole file.
    pub async fn upload_file(
        &self,
        source_path: &std::path::Path,
        file_name: &str,
        parent_folder_id: Option<&str>,
        mime_type: Option<&str>,
        sync_password: Option<&str>,
    ) -> Result<TGCloudFile> {
        let cfg = self.config().await?;
        let (channel, tokens) = Self::require_ready(&cfg)?;

        let metadata = std::fs::metadata(source_path)
            .map_err(|e| anyhow!("cannot stat {}: {e}", source_path.display()))?;
        let size = metadata.len();
        let total_chunks = crate::tgcloud::chunker::total_chunks_for_size(size);
        let file_id = crate::tgcloud::chunker::new_file_id();
        let now = chrono::Utc::now().timestamp();

        let queue_depth = cfg.queue_depth.max(1) as usize;
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut c = self.cancellations.lock().await;
            c.insert(file_id.clone(), cancel.clone());
        }

        let app = self.app.clone();
        let report = bot_pool::upload_chunked(
            source_path,
            file_name,
            &file_id,
            size,
            channel,
            &tokens,
            queue_depth,
            cancel,
            move |p| {
                let _ = app.emit("tgcloud-transfer", p);
            },
        )
        .await?;

        let uploader_tokens = serde_json::to_string(
            &report
                .chunks
                .iter()
                .map(|c| c.bot_worker_id)
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into());

        let file = TGCloudFile {
            file_id: file_id.clone(),
            file_name: file_name.to_string(),
            mime_type: mime_type.map(|s| s.to_string()),
            size_bytes: size,
            total_chunks,
            status: FileStatus::Complete,
            parent_folder_id: parent_folder_id.map(|s| s.to_string()),
            telegram_message_id: report.telegram_message_id,
            file_id_tg: report
                .chunks
                .last()
                .map(|c| c.telegram_file_id.clone())
                .unwrap_or_default(),
            file_unique_id: report
                .chunks
                .last()
                .map(|c| c.telegram_file_unique_id.clone())
                .unwrap_or_default(),
            checksum: report.logical_sha256.clone(),
            uploader_tokens,
            created_at: now,
            updated_at: now,
            chunks: report.chunks,
        };

        // Persist complete file BEFORE publishing sync (per spec ordering).
        db::upsert_file(&self.db, &file).await?;

        // Write a sync log + publish sync_index exactly once.
        if cfg.sync_enabled {
            if let Some(pw) = sync_password {
                let _ = self.app.emit(
                    "tgcloud-transfer",
                    TransferProgress::SyncPublishing {
                        upload_attempt_id: file_id.clone(),
                    },
                );
                let _ = self.publish_file_sync(&file, pw, &cfg).await;
            }
        }

        // Remove cancellation entry.
        self.cancellations.lock().await.remove(&file_id);
        Ok(file)
    }

    pub async fn cancel_upload(&self, file_id: &str) {
        if let Some(c) = self.cancellations.lock().await.get(file_id) {
            c.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    async fn publish_file_sync(
        &self,
        file: &TGCloudFile,
        sync_password: &str,
        cfg: &TGCloudConfig,
    ) -> Result<()> {
        let sync_channel = cfg.sync_channel_id.or(cfg.storage_channel_id).ok_or_else(
            || anyhow!("sync channel not configured"),
        )?;
        // Prefer the dedicated sync bot if set, else first storage bot.
        let bot = match &cfg.sync_bot_token {
            Some(t) if !t.trim().is_empty() => BotApiClient::new(t.trim()),
            _ => BotApiClient::new(
                cfg.active_tokens()
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow!("no bot available for sync"))?,
            ),
        };

        let data_json = db::file_to_sync_json(file);
        sync::log_mutation(
            &self.db,
            &self.device_id,
            SyncOperation::Insert,
            "cloud_files",
            &file.file_id,
            Some(&data_json),
            None,
        )
        .await?;

        let head = self
            .read_meta_i64(SYNC_CHAIN_HEAD_KEY)
            .await
            .ok()
            .flatten();
        if let Some(new_head) =
            sync::publish_pending(&self.db, &bot, sync_channel, sync_password, &self.device_id, head)
                .await?
        {
            self.write_meta_i64(SYNC_CHAIN_HEAD_KEY, new_head).await?;
        }
        Ok(())
    }

    // ----- download --------------------------------------------------------

    /// Reconstruct a file to `target_path` using bounded-memory positional
    /// writes. Chunks may download out of order; offsets guarantee a correct
    /// result regardless of Telegram arrival order.
    pub async fn download_file(
        &self,
        file_id: &str,
        target_path: &std::path::Path,
    ) -> Result<()> {
        let cfg = self.config().await?;
        let (_channel, tokens) = Self::require_ready(&cfg)?;
        let file = db::get_file(&self.db, file_id)
            .await?
            .ok_or_else(|| anyhow!("TG Cloud file not found: {file_id}"))?;

        if file.chunks.is_empty() {
            return Err(anyhow!(
                "file has no chunk records (direct files not yet supported in this path)"
            ));
        }

        let clients: Vec<(u8, BotApiClient)> = tokens
            .iter()
            .enumerate()
            .map(|(i, t)| ((i + 1) as u8, BotApiClient::new(t)))
            .collect();

        // Build download list, preferring the worker that originally uploaded.
        let file_ids: Vec<(u32, String, u8)> = file
            .chunks
            .iter()
            .map(|c| (c.chunk_index, c.telegram_file_id.clone(), c.bot_worker_id))
            .collect();

        let total = file.size_bytes;
        let app = self.app.clone();
        let fid = file_id.to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut c = self.cancellations.lock().await;
            c.insert(format!("dl:{file_id}"), cancel.clone());
        }

        bot_pool::download_chunked(
            target_path,
            file.total_chunks,
            crate::tgcloud::CHUNK_SIZE,
            file_ids,
            &clients,
            cfg.queue_depth.max(1) as usize,
            cancel,
            move |done, total_bytes| {
                let _ = app.emit(
                    "tgcloud-transfer",
                    serde_json::json!({
                        "phase": "download_progress",
                        "file_id": fid,
                        "bytes_done": done,
                        "total_bytes": total.min(total_bytes),
                    }),
                );
            },
        )
        .await
    }

    // ----- file operations -------------------------------------------------

    pub async fn list_files(&self, folder: Option<&str>) -> Result<Vec<TGCloudFile>> {
        db::list_files(&self.db, folder).await
    }

    pub async fn list_folders(&self, parent: Option<&str>) -> Result<Vec<TGCloudFolder>> {
        db::list_folders(&self.db, parent).await
    }

    pub async fn create_folder(
        &self,
        name: &str,
        parent: Option<&str>,
    ) -> Result<TGCloudFolder> {
        db::create_folder(&self.db, name, parent).await
    }

    pub async fn delete_folder(&self, folder_id: &str) -> Result<()> {
        db::delete_folder(&self.db, folder_id).await
    }

    pub async fn delete_file(&self, file_id: &str) -> Result<()> {
        // Physical chunk deletion from Telegram is best-effort; metadata is
        // always removed so the user is not stuck referencing ghosts.
        let cfg = self.config().await.ok();
        if let Some(cfg) = cfg {
            if let Ok(file) = db::get_file(&self.db, file_id).await {
                if let Some(file) = file {
                    if let (Some(channel), Some(tok)) =
                        (cfg.storage_channel_id, cfg.active_tokens().into_iter().next())
                    {
                        let client = BotApiClient::new(tok);
                        for c in &file.chunks {
                            let _ = client.delete_message(channel, c.telegram_message_id).await;
                        }
                    }
                }
            }
        }
        db::delete_file(&self.db, file_id).await
    }

    pub async fn rename_file(&self, file_id: &str, new_name: &str) -> Result<()> {
        db::rename_file(&self.db, file_id, new_name).await
    }

    pub async fn move_file(&self, file_id: &str, folder: Option<&str>) -> Result<()> {
        db::move_file(&self.db, file_id, folder).await
    }

    // ----- sharing ---------------------------------------------------------

    pub async fn create_share(
        &self,
        file_ids: &[String],
        password: &str,
    ) -> Result<Vec<u8>> {
        let mut files = Vec::with_capacity(file_ids.len());
        for id in file_ids {
            if let Some(f) = db::get_file(&self.db, id).await? {
                files.push(f);
            }
        }
        let manifest = share::build_manifest(&files);
        share::export_link(&manifest, password)
    }

    pub async fn import_share(
        &self,
        link_bytes: &[u8],
        password: &str,
    ) -> Result<ShareManifest> {
        share::import_link(link_bytes, password)
    }

    // ----- backup ----------------------------------------------------------

    pub async fn create_backup(&self, password: &str) -> Result<Vec<u8>> {
        let data_dir = self
            .app
            .path()
            .app_data_dir()
            .map_err(|e| anyhow!(e.to_string()))?;
        let db_path = data_dir.join("tgcloud.db");
        let cfg = self.config().await?;
        let cfg_json = serde_json::to_string(&cfg)?;
        backup::create_backup(&db_path, &cfg_json, password)
    }

    // ----- internal meta helpers ------------------------------------------

    async fn read_meta_i64(&self, key: &str) -> Result<Option<i64>> {
        let db = self.db.clone();
        let k = key.to_string();
        crate::tgcloud::db::with_db(&db, move |conn| {
            let mut s = conn
                .prepare("SELECT value FROM tgcloud_meta WHERE key=?1")
                .map_err(|e| anyhow!(e))?;
            s.bind((1, k.as_str())).map_err(|e| anyhow!(e))?;
            if let sqlite::State::Row = s.next().map_err(|e| anyhow!(e))? {
                let v: String = s.read("value").map_err(|e| anyhow!(e))?;
                Ok(Some(v.parse().unwrap_or(0)))
            } else {
                Ok(None)
            }
        })
        .await
    }

    async fn write_meta_i64(&self, key: &str, value: i64) -> Result<()> {
        let db = self.db.clone();
        let k = key.to_string();
        let v = value.to_string();
        crate::tgcloud::db::with_db(&db, move |conn| {
            let mut s = conn
                .prepare(
                    "INSERT INTO tgcloud_meta(key,value) VALUES(?1,?2)
                     ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                )
                .map_err(|e| anyhow!(e))?;
            s.bind((1, k.as_str())).map_err(|e| anyhow!(e))?;
            s.bind((2, v.as_str())).map_err(|e| anyhow!(e))?;
            while s.next().map_err(|e| anyhow!(e))? != sqlite::State::Done {}
            Ok(())
        })
        .await
    }
}
