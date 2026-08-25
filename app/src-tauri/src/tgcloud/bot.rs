//! Minimal Telegram Bot API client used by the TG Cloud worker pool.
//!
//! Only the handful of methods TG Cloud needs are implemented:
//! `getMe`, `sendDocument` (multipart), `sendMessage`, `getFile`, and
//! file download. The client is intentionally thin — retries, rate limiting
//! and worker scheduling live in [`super::bot_pool`].

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::time::Duration;

const API_BASE: &str = "https://api.telegram.org";

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramUser {
    pub id: i64,
    pub username: Option<String>,
    pub first_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramMessage {
    pub message_id: i64,
    #[serde(default)]
    pub document: Option<TelegramDocument>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramDocument {
    pub file_id: String,
    pub file_unique_id: String,
    #[serde(default)]
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramFile {
    pub file_id: String,
    pub file_unique_id: String,
    pub file_path: Option<String>,
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
    error_code: Option<i32>,
}

/// A stateless Bot API handle. Cheap to clone.
#[derive(Clone)]
pub struct BotApiClient {
    token: String,
    http: reqwest::Client,
}

impl BotApiClient {
    pub fn new(token: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(300)) // 10 MiB on a slow link can take a while
            .build()
            .expect("failed to build reqwest client");
        Self {
            token: token.into(),
            http,
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    fn endpoint(&self, method: &str) -> String {
        format!("{API_BASE}/bot{}/{}", self.token, method)
    }

    fn file_endpoint(&self, path: &str) -> String {
        format!("{API_BASE}/file/bot{}/{path}", self.token)
    }

    /// `getMe` — validate token and return bot identity.
    pub async fn get_me(&self) -> Result<TelegramUser> {
        let resp: ApiResponse<TelegramUser> = self
            .http
            .get(self.endpoint("getMe"))
            .send()
            .await
            .context("getMe network error")?
            .json()
            .await
            .context("getMe: invalid JSON")?;
        resp.result
            .ok_or_else(|| anyhow!("getMe failed: {:?}", resp.description))
    }

    /// `sendDocument` with an in-memory chunk and caption.
    ///
    /// The chunk is held in a single 10 MiB buffer owned by the caller; we do
    /// not copy the whole file here. `part` is the filename Telegram will show
    /// (e.g. `movie.mp4.chunk_7_of_252`).
    pub async fn send_document(
        &self,
        chat_id: i64,
        data: Vec<u8>,
        filename: &str,
        caption: &str,
    ) -> Result<TelegramMessage> {
        let part = reqwest::multipart::Part::bytes(data)
            .file_name(filename.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| anyhow!("invalid mime: {e}"))?;

        let form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .text("caption", caption.to_string())
            .part("document", part);

        let resp: ApiResponse<TelegramMessage> = self
            .http
            .post(self.endpoint("sendDocument"))
            .multipart(form)
            .send()
            .await
            .context("sendDocument network error")?
            .json()
            .await
            .context("sendDocument: invalid JSON")?;

        match resp.result {
            Some(m) if m.document.is_some() => Ok(m),
            _ => Err(anyhow!(
                "sendDocument failed: {:?} (code {:?})",
                resp.description,
                resp.error_code
            )),
        }
    }

    /// Send a text message (used for sync_index publication).
    pub async fn send_message(&self, chat_id: i64, text: &str) -> Result<TelegramMessage> {
        let resp: ApiResponse<TelegramMessage> = self
            .http
            .post(self.endpoint("sendMessage"))
            .form(&[
                ("chat_id", chat_id.to_string()),
                ("text", text.to_string()),
                ("disable_web_page_preview", "true".to_string()),
            ])
            .send()
            .await
            .context("sendMessage network error")?
            .json()
            .await
            .context("sendMessage: invalid JSON")?;
        resp.result
            .ok_or_else(|| anyhow!("sendMessage failed: {:?}", resp.description))
    }

    /// `getFile` — resolve a `file_path` for a file_id.
    pub async fn get_file(&self, file_id: &str) -> Result<TelegramFile> {
        let resp: ApiResponse<TelegramFile> = self
            .http
            .get(self.endpoint("getFile"))
            .query(&[("file_id", file_id)])
            .send()
            .await
            .context("getFile network error")?
            .json()
            .await
            .context("getFile: invalid JSON")?;
        resp.result
            .ok_or_else(|| anyhow!("getFile failed: {:?}", resp.description))
    }

    /// Download a document's bytes. Used for chunk downloads where each chunk
    /// is at most 10 MiB (acceptable to buffer individually), and for direct
    /// files within Telegram's normal size envelope.
    pub async fn download_file(&self, file_path: &str) -> Result<Vec<u8>> {
        let bytes = self
            .http
            .get(self.file_endpoint(file_path))
            .send()
            .await
            .context("download network error")?
            .error_for_status()?
            .bytes()
            .await
            .context("download body error")?;
        Ok(bytes.to_vec())
    }

    /// `deleteMessage` — orphan cleanup on failed uploads.
    pub async fn delete_message(&self, chat_id: i64, message_id: i64) -> Result<()> {
        let _: ApiResponse<bool> = self
            .http
            .post(self.endpoint("deleteMessage"))
            .form(&[
                ("chat_id", chat_id.to_string()),
                ("message_id", message_id.to_string()),
            ])
            .send()
            .await?
            .json()
            .await?;
        Ok(())
    }
}
