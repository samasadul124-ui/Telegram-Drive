//! TG Cloud metadata backup / restore.
//!
//! Matches the real APK's backup envelope:
//!   magic "BKP1" || 16-byte salt || 16-byte IV || AES-256-CBC(PKCS#7)
//!   key = SHA-256(UTF-8(password) || salt)
//!
//! The plaintext is a ZIP containing the metadata DB and config JSON. It
//! does NOT contain the multi-gigabyte Telegram payloads (those live in
//! Telegram itself and are re-fetched by file id).

use crate::tgcloud::crypto;
use anyhow::{anyhow, Context, Result};
use std::io::{Read, Write};
use std::path::Path;

const BACKUP_DB_NAME: &str = "tgcloud.db";
const BACKUP_CONFIG_NAME: &str = "tgcloud-config.json";
const BACKUP_MANIFEST_NAME: &str = "backup_manifest.json";

/// Create a backup blob from the on-disk tgcloud DB and config JSON.
pub fn create_backup(db_path: &Path, config_json: &str, password: &str) -> Result<Vec<u8>> {
    let db_bytes = std::fs::read(db_path)
        .with_context(|| format!("read db {}", db_path.display()))?;

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file(BACKUP_DB_NAME, opts)?;
    zip.write_all(&db_bytes)?;

    zip.start_file(BACKUP_CONFIG_NAME, opts)?;
    zip.write_all(config_json.as_bytes())?;

    zip.start_file(BACKUP_MANIFEST_NAME, opts)?;
    let manifest = serde_json::json!({
        "format": "tgcloud-backup",
        "version": "1.0",
        "created_at": chrono::Utc::now().timestamp_millis(),
        "contains": [BACKUP_DB_NAME, BACKUP_CONFIG_NAME],
    });
    zip.write_all(serde_json::to_vec_pretty(&manifest)?.as_slice())?;

    let cursor = zip.finish()?;
    let plaintext = cursor.into_inner();

    crypto::seal_backup(password, &plaintext)
}

/// Restore a backup: decrypts, unzips, returns the DB bytes and config JSON.
/// The caller is responsible for swapping the live DB.
pub fn restore_backup(blob: &[u8], password: &str) -> Result<(Vec<u8>, String)> {
    let plaintext = crypto::open_backup(password, blob)?;
    let cursor = std::io::Cursor::new(plaintext);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| anyhow!("zip: {e}"))?;

    let mut db_bytes = None;
    let mut config = String::new();

    for i in 0..archive.len() {
        let mut f = archive.by_index(i).map_err(|e| anyhow!("zip entry: {e}"))?;
        let name = f.name().to_string();
        if name == BACKUP_DB_NAME {
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            db_bytes = Some(buf);
        } else if name == BACKUP_CONFIG_NAME {
            f.read_to_string(&mut config)?;
        }
    }

    let db_bytes = db_bytes.ok_or_else(|| anyhow!("backup missing {BACKUP_DB_NAME}"))?;
    Ok((db_bytes, config))
}
