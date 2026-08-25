//! Encrypted `.link` share manifest manager (TG Cloud 1.2.0 format).
//!
//! Binary layout (compatible with the real 305-file artifact):
//!   16-byte salt || 16-byte IV || AES-256-CBC ciphertext (PKCS#7)
//! KDF: PBKDF2-HMAC-SHA256, 10,000 iterations, 32-byte key.
//! Plaintext is JSON.

use crate::tgcloud::{
    crypto,
    models::{ShareChunkEntry, ShareFileEntry, ShareManifest, TGCloudFile},
};
use anyhow::{anyhow, Result};

/// Build a share manifest from a set of logical files.
pub fn build_manifest(files: &[TGCloudFile]) -> ShareManifest {
    let entries = files
        .iter()
        .map(|f| {
            let is_chunked = f.total_chunks > 1;
            let chunks = if is_chunked {
                f.chunks
                    .iter()
                    .map(|c| ShareChunkEntry {
                        chunk_index: c.chunk_index,
                        chunk_hash: c.chunk_hash.clone(),
                        file_id: c.telegram_file_id.clone(),
                        file_unique_id: Some(c.telegram_file_unique_id.clone()),
                        uploader: Some(format!("bot{}", c.bot_worker_id)),
                    })
                    .collect()
            } else {
                Vec::new()
            };
            ShareFileEntry {
                file_name: f.file_name.clone(),
                size_bytes: f.size_bytes,
                mime_type: f.mime_type.clone(),
                uploaded_at: f.created_at,
                file_id: if is_chunked { None } else { Some(f.file_id_tg.clone()) },
                file_unique_id: if is_chunked {
                    None
                } else {
                    Some(f.file_unique_id.clone())
                },
                uploader: Some("tgcloud-integration".into()),
                total_chunks: if is_chunked { Some(f.total_chunks) } else { None },
                chunk_size: if is_chunked {
                    Some(crate::tgcloud::CHUNK_SIZE)
                } else {
                    None
                },
                chunks,
            }
        })
        .collect();

    ShareManifest {
        version: "1.0".into(),
        kind: "batch".into(),
        created_at: chrono::Utc::now().timestamp_millis(),
        files: entries,
    }
}

/// Encrypt a manifest into the portable `.link` byte blob.
pub fn export_link(manifest: &ShareManifest, password: &str) -> Result<Vec<u8>> {
    if password.is_empty() {
        return Err(anyhow!("share password is mandatory"));
    }
    let json = serde_json::to_vec(manifest)?;
    crypto::seal_share(password, &json)
}

/// Decrypt and parse a `.link` blob.
pub fn import_link(data: &[u8], password: &str) -> Result<ShareManifest> {
    let json = crypto::open_share(password, data)?;
    let manifest: ShareManifest =
        serde_json::from_slice(&json).map_err(|e| anyhow!(".link JSON parse: {e}"))?;
    if manifest.version != "1.0" {
        return Err(anyhow!("unsupported .link version: {}", manifest.version));
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_roundtrip() {
        let m = ShareManifest {
            version: "1.0".into(),
            kind: "batch".into(),
            created_at: 0,
            files: vec![ShareFileEntry {
                file_name: "a.jpg".into(),
                size_bytes: 10,
                mime_type: Some("image/jpeg".into()),
                uploaded_at: 0,
                file_id: Some("FID".into()),
                file_unique_id: Some("UID".into()),
                uploader: None,
                total_chunks: None,
                chunk_size: None,
                chunks: vec![],
            }],
        };
        let blob = export_link(&m, "secret").unwrap();
        let back = import_link(&blob, "secret").unwrap();
        assert_eq!(back.files.len(), 1);
        assert_eq!(back.files[0].file_name, "a.jpg");
        assert!(import_link(&blob, "wrong").is_err());
    }
}
