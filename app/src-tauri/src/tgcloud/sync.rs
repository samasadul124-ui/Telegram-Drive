//! TG Cloud synchronization chain.
//!
//! Modeled on the real APK's `SyncEngine`/`SyncCrypto`/`SyncLogManager`:
//! local mutations append to `tgcloud_sync_log`; pending logs are batched
//! into an encrypted sync-node and published to the sync channel as
//! `sync_index:v1:<b64>`. Nodes chain via `prevId` (the prior node's
//! Telegram message id), forming a journal that another device with the
//! same sync password can replay.
//!
//! Sync is published EXACTLY ONCE per completed upload (or other mutation),
//! never after individual chunks.

use crate::tgcloud::{
    bot::BotApiClient,
    crypto,
    db::{self, TgDb},
    models::{SyncLog, SyncOperation},
};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

const MAX_LOGS_PER_NODE: usize = 250;

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncNode {
    pub version: String,
    pub device_id: String,
    pub timestamp: i64,
    /// Telegram message id of the previous node (chain link).
    #[serde(rename = "prevId")]
    pub prev_id: Option<i64>,
    pub entries: Vec<SyncNodeEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncNodeEntry {
    pub log_id: String,
    pub timestamp: i64,
    pub operation: String,
    #[serde(rename = "tableName")]
    pub table_name: String,
    #[serde(rename = "primaryKey")]
    pub primary_key: String,
    #[serde(rename = "dataJson")]
    pub data_json: Option<String>,
    #[serde(rename = "previousDataJson", skip_serializing_if = "Option::is_none")]
    pub previous_data_json: Option<String>,
}

/// Append a log entry locally and mark its checksum. Caller is responsible
/// for publishing (uploading) pending nodes after the logical operation is
/// fully committed.
pub async fn log_mutation(
    tgdb: &TgDb,
    device_id: &str,
    op: SyncOperation,
    table: &str,
    primary_key: &str,
    data_json: Option<&str>,
    previous_data_json: Option<&str>,
) -> Result<SyncLog> {
    let log = SyncLog {
        log_id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        device_id: device_id.to_string(),
        operation: op,
        table_name: table.to_string(),
        primary_key: primary_key.to_string(),
        data_json: data_json.map(|s| s.to_string()),
        previous_data_json: previous_data_json.map(|s| s.to_string()),
        is_uploaded: false,
        telegram_message_id: None,
        checksum: None,
    };
    db::append_sync_log(tgdb, &log).await?;
    Ok(log)
}

/// Encrypt + publish all pending sync logs as a single chained node.
/// Returns the Telegram message id. No-op if there is nothing to publish.
pub async fn publish_pending(
    tgdb: &TgDb,
    bot: &BotApiClient,
    sync_channel_id: i64,
    sync_password: &str,
    device_id: &str,
    chain_head: Option<i64>,
) -> Result<Option<i64>> {
    let pending = db::pending_sync_logs(tgdb).await?;
    if pending.is_empty() {
        return Ok(None);
    }

    let entries: Vec<SyncNodeEntry> = pending
        .iter()
        .take(MAX_LOGS_PER_NODE)
        .map(|l| SyncNodeEntry {
            log_id: l.log_id.clone(),
            timestamp: l.timestamp,
            operation: match l.operation {
                SyncOperation::Insert => "INSERT".into(),
                SyncOperation::Update => "UPDATE".into(),
                SyncOperation::Delete => "DELETE".into(),
            },
            table_name: l.table_name.clone(),
            primary_key: l.primary_key.clone(),
            data_json: l.data_json.clone(),
            previous_data_json: l.previous_data_json.clone(),
        })
        .collect();

    let node = SyncNode {
        version: "1.0".into(),
        device_id: device_id.to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        prev_id: chain_head,
        entries,
    };

    let plaintext = serde_json::to_vec(&json!(node))?;
    let envelope = crypto::seal_sync(sync_password, &plaintext)?;

    let msg = bot.send_message(sync_channel_id, &envelope).await?;

    // Mark all included logs uploaded with the node's message id + checksum.
    for e in &node.entries {
        let cs = crypto::sha256_hex(e.log_id.as_bytes());
        db::mark_sync_uploaded(tgdb, &e.log_id, msg.message_id, &cs).await?;
    }
    let _ = crypto::md5_hex(b""); // keep md5 linked for potential per-entry use
    Ok(Some(msg.message_id))
}

/// Decode a raw `sync_index:...` message into a `SyncNode`.
///
/// Remote chain traversal (walking `prevId` backwards via channel history) is
/// implemented by the provider when multi-device pull is needed; this helper
/// is the single decryption/parse entry point used by that path.
pub fn decode_envelope(sync_password: &str, text: &str) -> Result<SyncNode> {
    let plaintext = crypto::open_sync(sync_password, text)?;
    let node: SyncNode = serde_json::from_slice(&plaintext)
        .map_err(|e| anyhow!("sync node JSON parse: {e}"))?;
    Ok(node)
}
