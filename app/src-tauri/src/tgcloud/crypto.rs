//! Cryptographic primitives for TG Cloud, matching the formats recovered
//! from the real APK and verified against real artifacts.
//!
//! | Format | KDF | Cipher | Notes |
//! |---|---|---|---|
//! | Sync node  | PBKDF2-HMAC-SHA256, 100_000 iters, 32-byte key | AES-256-GCM, 12-byte IV, 16-byte salt | GZIP-compressed JSON |
//! | `.link`    | PBKDF2-HMAC-SHA256, 10_000 iters, 32-byte key  | AES-256-CBC, 16-byte IV, 16-byte salt | PKCS#7, plain JSON |
//! | Backup     | SHA-256(password ‖ salt), 32-byte key          | AES-256-CBC, 16-byte IV, 16-byte salt | magic `BKP1`, PKCS#7 |

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Result};
use flate2::read::{GzDecoder, GzEncoder};
use flate2::Compression;
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::{Digest, Sha256};

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

const SALT_LEN: usize = 16;
const GCM_IV_LEN: usize = 12;
const CBC_IV_LEN: usize = 16;
const KEY_LEN: usize = 32;

// ---------------------------------------------------------------------------
// Sync: PBKDF2(100k) + AES-256-GCM + GZIP
// ---------------------------------------------------------------------------

pub const SYNC_PBKDF2_ITERATIONS: u32 = 100_000;
pub const SYNC_FORMAT_VERSION: &str = "v1";

fn derive_key_pbkdf2(password: &[u8], salt: &[u8], iterations: u32) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    pbkdf2_hmac::<Sha256>(password, salt, iterations, &mut key);
    key
}

/// Encrypt + GZIP a sync payload. Output envelope:
/// `sync_index:v1:<base64(salt || iv || ciphertext+tag)>`
pub fn seal_sync(password: &str, plaintext_json: &[u8]) -> Result<String> {
    let mut salt = [0u8; SALT_LEN];
    let mut iv = [0u8; GCM_IV_LEN];
    rand::rng().fill_bytes(&mut salt);
    rand::rng().fill_bytes(&mut iv);

    let key = derive_key_pbkdf2(password.as_bytes(), &salt, SYNC_PBKDF2_ITERATIONS);
    let cipher = Aes256Gcm::new((&key).into());

    // GZIP-compress the JSON before encryption, as the real client does.
    let mut gz = GzEncoder::new(plaintext_json, Compression::default());
    use std::io::Read;
    let mut compressed = Vec::new();
    gz.read_to_end(&mut compressed)
        .map_err(|e| anyhow!("gzip compress failed: {e}"))?;

    let ct = cipher
        .encrypt(Nonce::from_slice(&iv), compressed.as_ref())
        .map_err(|e| anyhow!("sync AES-GCM seal failed: {e}"))?;

    let mut envelope = Vec::with_capacity(SALT_LEN + GCM_IV_LEN + ct.len());
    envelope.extend_from_slice(&salt);
    envelope.extend_from_slice(&iv);
    envelope.extend_from_slice(&ct);

    use base64::Engine;
    Ok(format!(
        "sync_index:{}:{}",
        SYNC_FORMAT_VERSION,
        base64::engine::general_purpose::STANDARD.encode(envelope)
    ))
}

/// Parse `sync_index:<version>:<base64>` and decrypt. Returns the
/// decompressed JSON.
pub fn open_sync(password: &str, envelope: &str) -> Result<Vec<u8>> {
    let rest = envelope
        .strip_prefix("sync_index:")
        .ok_or_else(|| anyhow!("not a sync_index envelope"))?;
    // Allow either `v1:<b64>` or bare `<b64>` (older format).
    let b64 = match rest.split_once(':') {
        Some((_version, payload)) => payload,
        None => rest,
    };

    use base64::Engine;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| anyhow!("sync base64 decode: {e}"))?;

    if raw.len() < SALT_LEN + GCM_IV_LEN + 16 {
        return Err(anyhow!("sync payload too short"));
    }
    let salt = &raw[..SALT_LEN];
    let iv = &raw[SALT_LEN..SALT_LEN + GCM_IV_LEN];
    let ct = &raw[SALT_LEN + GCM_IV_LEN..];

    let key = derive_key_pbkdf2(password.as_bytes(), salt, SYNC_PBKDF2_ITERATIONS);
    let cipher = Aes256Gcm::new((&key).into());
    let compressed = cipher
        .decrypt(Nonce::from_slice(iv), ct)
        .map_err(|_| anyhow!("sync decryption failed (wrong password or tampered payload)"))?;

    let mut decoder = GzDecoder::new(compressed.as_slice());
    use std::io::Read;
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| anyhow!("sync gzip decompress: {e}"))?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// .link: PBKDF2(10k) + AES-256-CBC, layout salt(16) || iv(16) || ct
// ---------------------------------------------------------------------------

pub const SHARE_PBKDF2_ITERATIONS: u32 = 10_000;

pub fn seal_share(password: &str, plaintext_json: &[u8]) -> Result<Vec<u8>> {
    let mut salt = [0u8; SALT_LEN];
    let mut iv = [0u8; CBC_IV_LEN];
    rand::rng().fill_bytes(&mut salt);
    rand::rng().fill_bytes(&mut iv);

    let key = derive_key_pbkdf2(password.as_bytes(), &salt, SHARE_PBKDF2_ITERATIONS);
    let ct = cbc_encrypt_pkcs7(&key, &iv, plaintext_json)?;

    let mut out = Vec::with_capacity(SALT_LEN + CBC_IV_LEN + ct.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn open_share(password: &str, data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < SALT_LEN + CBC_IV_LEN + 16 {
        return Err(anyhow!(".link payload too short"));
    }
    let salt = &data[..SALT_LEN];
    let iv = &data[SALT_LEN..SALT_LEN + CBC_IV_LEN];
    let ct = &data[SALT_LEN + CBC_IV_LEN..];
    let key = derive_key_pbkdf2(password.as_bytes(), salt, SHARE_PBKDF2_ITERATIONS);
    cbc_decrypt_pkcs7(&key, iv, ct)
}

// ---------------------------------------------------------------------------
// Backup: magic "BKP1" || salt(16) || iv(16) || AES-256-CBC, key=SHA256(pw||salt)
// ---------------------------------------------------------------------------

pub const BACKUP_MAGIC: &[u8; 4] = b"BKP1";

pub fn seal_backup(password: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut salt = [0u8; SALT_LEN];
    let mut iv = [0u8; CBC_IV_LEN];
    rand::rng().fill_bytes(&mut salt);
    rand::rng().fill_bytes(&mut iv);

    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(&salt);
    let key: [u8; KEY_LEN] = hasher.finalize().into();

    let ct = cbc_encrypt_pkcs7(&key, &iv, plaintext)?;

    let mut out = Vec::with_capacity(4 + SALT_LEN + CBC_IV_LEN + ct.len());
    out.extend_from_slice(BACKUP_MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn open_backup(password: &str, data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 4 + SALT_LEN + CBC_IV_LEN + 16 || &data[..4] != BACKUP_MAGIC {
        return Err(anyhow!("not a BKP1 backup"));
    }
    let salt = &data[4..4 + SALT_LEN];
    let iv = &data[4 + SALT_LEN..4 + SALT_LEN + CBC_IV_LEN];
    let ct = &data[4 + SALT_LEN + CBC_IV_LEN..];

    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(salt);
    let key: [u8; KEY_LEN] = hasher.finalize().into();

    cbc_decrypt_pkcs7(&key, iv, ct)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn cbc_encrypt_pkcs7(key: &[u8; KEY_LEN], iv: &[u8; CBC_IV_LEN], plaintext: &[u8]) -> Result<Vec<u8>> {
    let enc = Aes256CbcEnc::new(key.into(), iv.into());
    // cbc crate requires a mutable Vec with spare capacity.
    let mut buf = vec![0u8; plaintext.len() + 16];
    buf[..plaintext.len()].copy_from_slice(plaintext);
    let ct_len = enc
        .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
        .map_err(|e| anyhow!("CBC encrypt: {e:?}"))?
        .len();
    buf.truncate(ct_len);
    Ok(buf)
}

fn cbc_decrypt_pkcs7(key: &[u8; KEY_LEN], iv: &[u8], ct: &[u8]) -> Result<Vec<u8>> {
    if iv.len() != CBC_IV_LEN {
        return Err(anyhow!("bad IV length"));
    }
    let mut iv_arr = [0u8; CBC_IV_LEN];
    iv_arr.copy_from_slice(iv);
    let dec = Aes256CbcDec::new(key.into(), (&iv_arr).into());
    let mut buf = ct.to_vec();
    let pt = dec
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| anyhow!("CBC decrypt failed (wrong password or tampered payload)"))?;
    Ok(pt.to_vec())
}

/// MD5 hex of a chunk payload (TG Cloud 1.2.0 per-chunk hash).
pub fn md5_hex(data: &[u8]) -> String {
    let digest = md5::compute(data);
    format!("{:x}", digest)
}

/// SHA-256 hex used as the logical file checksum.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_roundtrip() {
        let msg = br#"{"hello":"sync","n":42}"#;
        let env = seal_sync("correct horse", msg).unwrap();
        assert!(env.starts_with("sync_index:v1:"));
        let out = open_sync("correct horse", &env).unwrap();
        assert_eq!(out, msg);
        assert!(open_sync("wrong", &env).is_err());
    }

    #[test]
    fn share_roundtrip() {
        let msg = br#"{"version":"1.0","type":"batch"}"#;
        let blob = seal_share("pw", msg).unwrap();
        // layout: 16 salt + 16 iv + >=16 ct
        assert!(blob.len() >= 48);
        assert_eq!(open_share("pw", &blob).unwrap(), msg);
        assert!(open_share("bad", &blob).is_err());
    }

    #[test]
    fn backup_roundtrip() {
        let msg = b"backup payload";
        let blob = seal_backup("hunter2", msg).unwrap();
        assert_eq!(&blob[..4], BACKUP_MAGIC);
        assert_eq!(open_backup("hunter2", &blob).unwrap(), msg);
        assert!(open_backup("wrong", &blob).is_err());
    }

    #[test]
    fn md5_known() {
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }
}
