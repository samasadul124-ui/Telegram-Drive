//! Provider-isolated SQLite store for TG Cloud.
//!
//! Uses a separate database file (`tgcloud.db`) from the host app's
//! `shares.db`, so TG Cloud tables never collide with existing Telegram
//! Drive data. All access is serialized through a single `Mutex<Connection>`
//! consistent with the host app's [`crate::db`] pattern.

use crate::tgcloud::models::{
    ChunkRecord, FileStatus, SyncLog, SyncOperation, TGCloudConfig, TGCloudFile, TGCloudFolder,
};
use anyhow::{anyhow, Context, Result};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type TgDb = Arc<Mutex<sqlite::Connection>>;

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS tgcloud_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tgcloud_files (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id              TEXT NOT NULL UNIQUE,
    file_name            TEXT NOT NULL,
    mime_type            TEXT,
    size_bytes           INTEGER NOT NULL,
    total_chunks         INTEGER NOT NULL,
    status               TEXT NOT NULL,
    parent_folder_id     TEXT,
    telegram_message_id  INTEGER NOT NULL,
    tg_file_id           TEXT NOT NULL,
    tg_file_unique_id    TEXT NOT NULL,
    checksum             TEXT NOT NULL DEFAULT '',
    uploader_tokens      TEXT NOT NULL DEFAULT '[]',
    created_at           INTEGER NOT NULL,
    updated_at           INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tgcloud_files_folder ON tgcloud_files(parent_folder_id);
CREATE INDEX IF NOT EXISTS idx_tgcloud_files_status ON tgcloud_files(status);

CREATE TABLE IF NOT EXISTS tgcloud_chunks (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id               TEXT NOT NULL,
    chunk_index           INTEGER NOT NULL,
    chunk_size            INTEGER NOT NULL,
    chunk_hash            TEXT NOT NULL,
    telegram_message_id   INTEGER NOT NULL,
    tg_file_id            TEXT NOT NULL,
    tg_file_unique_id     TEXT NOT NULL,
    bot_worker_id         INTEGER NOT NULL,
    UNIQUE(file_id, chunk_index)
);

CREATE TABLE IF NOT EXISTS tgcloud_folders (
    folder_id        TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    parent_folder_id TEXT,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tgcloud_upload_attempts (
    upload_attempt_id TEXT PRIMARY KEY,
    file_id           TEXT NOT NULL,
    file_name         TEXT NOT NULL,
    size_bytes        INTEGER NOT NULL,
    total_chunks      INTEGER NOT NULL,
    status            TEXT NOT NULL,
    error             TEXT,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tgcloud_sync_state (
    device_id      TEXT PRIMARY KEY,
    chain_head     INTEGER,
    last_sync_at   INTEGER
);

CREATE TABLE IF NOT EXISTS tgcloud_sync_log (
    log_id              TEXT PRIMARY KEY,
    timestamp           INTEGER NOT NULL,
    device_id           TEXT NOT NULL,
    operation           TEXT NOT NULL,
    table_name          TEXT NOT NULL,
    primary_key         TEXT NOT NULL,
    data_json           TEXT,
    previous_data_json  TEXT,
    is_uploaded         INTEGER NOT NULL DEFAULT 0,
    telegram_message_id INTEGER,
    checksum            TEXT
);
CREATE INDEX IF NOT EXISTS idx_tgcloud_synclog_uploaded ON tgcloud_sync_log(is_uploaded);

CREATE TABLE IF NOT EXISTS tgcloud_bot_workers (
    slot       INTEGER PRIMARY KEY,
    token      TEXT NOT NULL DEFAULT '',
    enabled    INTEGER NOT NULL DEFAULT 0,
    username   TEXT
);

CREATE TABLE IF NOT EXISTS tgcloud_share_links (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    share_id    TEXT NOT NULL UNIQUE,
    password    TEXT NOT NULL,
    file_ids    TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER
);
"#;

pub fn open(path: &Path) -> Result<TgDb> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = sqlite::open(path)
        .with_context(|| format!("open tgcloud db at {}", path.display()))?;
    conn.execute(SCHEMA)
        .map_err(|e| anyhow!("tgcloud schema: {e}"))?;
    write_meta(&conn, "schema_version", &SCHEMA_VERSION.to_string())?;

    Ok(Arc::new(Mutex::new(conn)))
}

async fn with_db<T, F>(db: &TgDb, op: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&sqlite::Connection) -> Result<T> + Send + 'static,
{
    let db = db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db.blocking_lock();
        op(&conn)
    })
    .await
    .map_err(|e| anyhow!("db task: {e}"))?
}

// ----- config -------------------------------------------------------------

const CONFIG_KEY: &str = "config_json";

pub async fn load_config(db: &TgDb) -> Result<TGCloudConfig> {
    with_db(db, |conn| {
        let cfg = read_meta(conn, CONFIG_KEY)?;
        match cfg {
            Some(json) => serde_json::from_str(&json).map_err(|e| anyhow!("config parse: {e}")),
            None => Ok(TGCloudConfig::default()),
        }
    })
    .await
}

pub async fn save_config(db: &TgDb, cfg: &TGCloudConfig) -> Result<()> {
    let json = serde_json::to_string(cfg)?;
    with_db(db, move |conn| write_meta(conn, CONFIG_KEY, &json)).await
}

fn read_meta(conn: &sqlite::Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn
        .prepare("SELECT value FROM tgcloud_meta WHERE key=?1")
        .map_err(|e| anyhow!(e))?;
    stmt.bind((1, key)).map_err(|e| anyhow!(e))?;
    if let sqlite::State::Row = stmt.next().map_err(|e| anyhow!(e))? {
        let v: String = stmt.read("value").map_err(|e| anyhow!(e))?;
        Ok(Some(v))
    } else {
        Ok(None)
    }
}

fn write_meta(conn: &sqlite::Connection, key: &str, value: &str) -> Result<()> {
    let mut stmt = conn
        .prepare(
            "INSERT INTO tgcloud_meta(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        )
        .map_err(|e| anyhow!(e))?;
    stmt.bind((1, key)).map_err(|e| anyhow!(e))?;
    stmt.bind((2, value)).map_err(|e| anyhow!(e))?;
    stmt.next().map_err(|e| anyhow!(e))?;
    Ok(())
}

// ----- files --------------------------------------------------------------

pub async fn upsert_file(db: &TgDb, f: &TGCloudFile) -> Result<()> {
    let file_id = f.file_id.clone();
    let file_name = f.file_name.clone();
    let mime = f.mime_type.clone();
    let size = f.size_bytes as i64;
    let total = f.total_chunks as i64;
    let status = f.status.as_str().to_string();
    let parent = f.parent_folder_id.clone();
    let msg_id = f.telegram_message_id;
    let tg_fid = f.file_id_tg.clone();
    let tg_uniq = f.file_unique_id.clone();
    let checksum = f.checksum.clone();
    let uploaders = f.uploader_tokens.clone();
    let now = f.updated_at;
    let created = f.created_at;
    let chunks = f.chunks.clone();

    with_db(db, move |conn| {
        let tx = conn
            .prepare(
                "INSERT INTO tgcloud_files(file_id,file_name,mime_type,size_bytes,total_chunks,
                   status,parent_folder_id,telegram_message_id,tg_file_id,tg_file_unique_id,
                   checksum,uploader_tokens,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
                 ON CONFLICT(file_id) DO UPDATE SET
                   file_name=excluded.file_name,
                   mime_type=excluded.mime_type,
                   size_bytes=excluded.size_bytes,
                   total_chunks=excluded.total_chunks,
                   status=excluded.status,
                   parent_folder_id=excluded.parent_folder_id,
                   telegram_message_id=excluded.telegram_message_id,
                   tg_file_id=excluded.tg_file_id,
                   tg_file_unique_id=excluded.tg_file_unique_id,
                   checksum=excluded.checksum,
                   uploader_tokens=excluded.uploader_tokens,
                   updated_at=excluded.updated_at",
            )
            .map_err(|e| anyhow!(e))?;
        bind_file(
            tx,
            &file_id,
            &file_name,
            mime.as_deref(),
            size,
            total,
            &status,
            parent.as_deref(),
            msg_id,
            &tg_fid,
            &tg_uniq,
            &checksum,
            &uploaders,
            created,
            now,
        )?;

        // Replace chunks.
        let mut del = conn
            .prepare("DELETE FROM tgcloud_chunks WHERE file_id=?1")
            .map_err(|e| anyhow!(e))?;
        del.bind((1, file_id.as_str())).map_err(|e| anyhow!(e))?;
        del.next().map_err(|e| anyhow!(e))?;

        for c in &chunks {
            let mut ins = conn
                .prepare(
                    "INSERT INTO tgcloud_chunks(file_id,chunk_index,chunk_size,chunk_hash,
                       telegram_message_id,tg_file_id,tg_file_unique_id,bot_worker_id)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                )
                .map_err(|e| anyhow!(e))?;
            ins.bind((1, file_id.as_str())).map_err(|e| anyhow!(e))?;
            ins.bind((2, c.chunk_index as i64)).map_err(|e| anyhow!(e))?;
            ins.bind((3, c.chunk_size as i64)).map_err(|e| anyhow!(e))?;
            ins.bind((4, c.chunk_hash.as_str())).map_err(|e| anyhow!(e))?;
            ins.bind((5, c.telegram_message_id)).map_err(|e| anyhow!(e))?;
            ins.bind((6, c.telegram_file_id.as_str()))
                .map_err(|e| anyhow!(e))?;
            ins.bind((7, c.telegram_file_unique_id.as_str()))
                .map_err(|e| anyhow!(e))?;
            ins.bind((8, c.bot_worker_id as i64))
                .map_err(|e| anyhow!(e))?;
            ins.next().map_err(|e| anyhow!(e))?;
        }
        Ok(())
    })
    .await
}

#[allow(clippy::too_many_arguments)]
fn bind_file(
    mut s: sqlite::Statement,
    file_id: &str,
    file_name: &str,
    mime: Option<&str>,
    size: i64,
    total: i64,
    status: &str,
    parent: Option<&str>,
    msg_id: i64,
    tg_fid: &str,
    tg_uniq: &str,
    checksum: &str,
    uploaders: &str,
    created: i64,
    updated: i64,
) -> Result<()> {
    s.bind((1, file_id)).map_err(|e| anyhow!(e))?;
    s.bind((2, file_name)).map_err(|e| anyhow!(e))?;
    match mime {
        Some(m) => s.bind((3, m)).map_err(|e| anyhow!(e))?,
        None => s.bind((3, Option::<String>::None)).map_err(|e| anyhow!(e))?,
    }
    s.bind((4, size)).map_err(|e| anyhow!(e))?;
    s.bind((5, total)).map_err(|e| anyhow!(e))?;
    s.bind((6, status)).map_err(|e| anyhow!(e))?;
    match parent {
        Some(p) => s.bind((7, p)).map_err(|e| anyhow!(e))?,
        None => s.bind((7, Option::<String>::None)).map_err(|e| anyhow!(e))?,
    }
    s.bind((8, msg_id)).map_err(|e| anyhow!(e))?;
    s.bind((9, tg_fid)).map_err(|e| anyhow!(e))?;
    s.bind((10, tg_uniq)).map_err(|e| anyhow!(e))?;
    s.bind((11, checksum)).map_err(|e| anyhow!(e))?;
    s.bind((12, uploaders)).map_err(|e| anyhow!(e))?;
    s.bind((13, created)).map_err(|e| anyhow!(e))?;
    s.bind((14, updated)).map_err(|e| anyhow!(e))?;
    s.next().map_err(|e| anyhow!(e))?;
    Ok(())
}

pub async fn list_files(db: &TgDb, folder: Option<&str>) -> Result<Vec<TGCloudFile>> {
    let folder = folder.map(|s| s.to_string());
    with_db(db, move |conn| {
        let sql = match &folder {
            Some(_) => "SELECT * FROM tgcloud_files WHERE parent_folder_id=?1 AND status!='failed' ORDER BY file_name",
            None => "SELECT * FROM tgcloud_files WHERE parent_folder_id IS NULL AND status!='failed' ORDER BY file_name",
        };
        let mut stmt = conn.prepare(sql).map_err(|e| anyhow!(e))?;
        if let Some(f) = &folder {
            stmt.bind((1, f.as_str())).map_err(|e| anyhow!(e))?;
        }
        let mut out = Vec::new();
        while let sqlite::State::Row = stmt.next().map_err(|e| anyhow!(e))? {
            out.push(read_file_row(&stmt)?);
        }
        // Attach chunks.
        for f in &mut out {
            f.chunks = read_chunks(conn, &f.file_id)?;
        }
        Ok(out)
    })
    .await
}

pub async fn get_file(db: &TgDb, file_id: &str) -> Result<Option<TGCloudFile>> {
    let fid = file_id.to_string();
    with_db(db, move |conn| {
        let mut stmt = conn
            .prepare("SELECT * FROM tgcloud_files WHERE file_id=?1")
            .map_err(|e| anyhow!(e))?;
        stmt.bind((1, fid.as_str())).map_err(|e| anyhow!(e))?;
        if let sqlite::State::Row = stmt.next().map_err(|e| anyhow!(e))? {
            let mut f = read_file_row(&stmt)?;
            f.chunks = read_chunks(conn, &fid)?;
            Ok(Some(f))
        } else {
            Ok(None)
        }
    })
    .await
}

pub async fn delete_file(db: &TgDb, file_id: &str) -> Result<()> {
    let fid = file_id.to_string();
    with_db(db, move |conn| {
        for sql in [
            "DELETE FROM tgcloud_chunks WHERE file_id=?1",
            "DELETE FROM tgcloud_files WHERE file_id=?1",
        ] {
            let mut s = conn.prepare(sql).map_err(|e| anyhow!(e))?;
            s.bind((1, fid.as_str())).map_err(|e| anyhow!(e))?;
            s.next().map_err(|e| anyhow!(e))?;
        }
        Ok(())
    })
    .await
}

pub async fn rename_file(db: &TgDb, file_id: &str, new_name: &str) -> Result<()> {
    let fid = file_id.to_string();
    let name = new_name.to_string();
    with_db(db, move |conn| {
        let mut s = conn
            .prepare("UPDATE tgcloud_files SET file_name=?1, updated_at=?2 WHERE file_id=?3")
            .map_err(|e| anyhow!(e))?;
        s.bind((1, name.as_str())).map_err(|e| anyhow!(e))?;
        s.bind((2, chrono::Utc::now().timestamp()))
            .map_err(|e| anyhow!(e))?;
        s.bind((3, fid.as_str())).map_err(|e| anyhow!(e))?;
        s.next().map_err(|e| anyhow!(e))?;
        Ok(())
    })
    .await
}

pub async fn move_file(db: &TgDb, file_id: &str, folder: Option<&str>) -> Result<()> {
    let fid = file_id.to_string();
    let f = folder.map(|s| s.to_string());
    with_db(db, move |conn| {
        let mut s = conn
            .prepare("UPDATE tgcloud_files SET parent_folder_id=?1, updated_at=?2 WHERE file_id=?3")
            .map_err(|e| anyhow!(e))?;
        match &f {
            Some(v) => s.bind((1, v.as_str())).map_err(|e| anyhow!(e))?,
            None => s.bind((1, Option::<String>::None)).map_err(|e| anyhow!(e))?,
        }
        s.bind((2, chrono::Utc::now().timestamp()))
            .map_err(|e| anyhow!(e))?;
        s.bind((3, fid.as_str())).map_err(|e| anyhow!(e))?;
        s.next().map_err(|e| anyhow!(e))?;
        Ok(())
    })
    .await
}

fn read_file_row(s: &sqlite::Statement) -> Result<TGCloudFile> {
    let read_str = |col: &str| -> Result<String> { s.read::<String, _>(col).map_err(|e| anyhow!(e)) };
    let read_opt = |col: &str| -> Option<String> { s.read::<Option<String>, _>(col).ok().flatten() };
    let read_i64 = |col: &str| -> Result<i64> { s.read::<i64, _>(col).map_err(|e| anyhow!(e)) };

    Ok(TGCloudFile {
        file_id: read_str("file_id")?,
        file_name: read_str("file_name")?,
        mime_type: read_opt("mime_type"),
        size_bytes: read_i64("size_bytes")? as u64,
        total_chunks: read_i64("total_chunks")? as u32,
        status: FileStatus::from_str(&read_str("status")?),
        parent_folder_id: read_opt("parent_folder_id"),
        telegram_message_id: read_i64("telegram_message_id")?,
        file_id_tg: read_str("tg_file_id")?,
        file_unique_id: read_str("tg_file_unique_id")?,
        checksum: read_str("checksum")?,
        uploader_tokens: read_str("uploader_tokens")?,
        created_at: read_i64("created_at")?,
        updated_at: read_i64("updated_at")?,
        chunks: Vec::new(),
    })
}

fn read_chunks(conn: &sqlite::Connection, file_id: &str) -> Result<Vec<ChunkRecord>> {
    let mut s = conn
        .prepare(
            "SELECT chunk_index,chunk_size,chunk_hash,telegram_message_id,tg_file_id,
                    tg_file_unique_id,bot_worker_id
             FROM tgcloud_chunks WHERE file_id=?1 ORDER BY chunk_index",
        )
        .map_err(|e| anyhow!(e))?;
    s.bind((1, file_id)).map_err(|e| anyhow!(e))?;
    let mut out = Vec::new();
    while let sqlite::State::Row = s.next().map_err(|e| anyhow!(e))? {
        out.push(ChunkRecord {
            chunk_index: s.read::<i64, _>("chunk_index").map_err(|e| anyhow!(e))? as u32,
            chunk_size: s.read::<i64, _>("chunk_size").map_err(|e| anyhow!(e))? as u64,
            chunk_hash: s.read::<String, _>("chunk_hash").map_err(|e| anyhow!(e))?,
            telegram_message_id: s
                .read::<i64, _>("telegram_message_id")
                .map_err(|e| anyhow!(e))?,
            telegram_file_id: s.read::<String, _>("tg_file_id").map_err(|e| anyhow!(e))?,
            telegram_file_unique_id: s
                .read::<String, _>("tg_file_unique_id")
                .map_err(|e| anyhow!(e))?,
            bot_worker_id: s.read::<i64, _>("bot_worker_id").map_err(|e| anyhow!(e))? as u8,
        });
    }
    Ok(out)
}

// ----- folders ------------------------------------------------------------

pub async fn list_folders(db: &TgDb, parent: Option<&str>) -> Result<Vec<TGCloudFolder>> {
    let p = parent.map(|s| s.to_string());
    with_db(db, move |conn| {
        let sql = match &p {
            Some(_) => "SELECT * FROM tgcloud_folders WHERE parent_folder_id=?1 ORDER BY name",
            None => "SELECT * FROM tgcloud_folders WHERE parent_folder_id IS NULL ORDER BY name",
        };
        let mut s = conn.prepare(sql).map_err(|e| anyhow!(e))?;
        if let Some(v) = &p {
            s.bind((1, v.as_str())).map_err(|e| anyhow!(e))?;
        }
        let mut out = Vec::new();
        while let sqlite::State::Row = s.next().map_err(|e| anyhow!(e))? {
            out.push(TGCloudFolder {
                folder_id: s.read::<String, _>("folder_id").map_err(|e| anyhow!(e))?,
                name: s.read::<String, _>("name").map_err(|e| anyhow!(e))?,
                parent_folder_id: s
                    .read::<Option<String>, _>("parent_folder_id")
                    .ok()
                    .flatten(),
                created_at: s.read::<i64, _>("created_at").map_err(|e| anyhow!(e))?,
                updated_at: s.read::<i64, _>("updated_at").map_err(|e| anyhow!(e))?,
            });
        }
        Ok(out)
    })
    .await
}

pub async fn create_folder(
    db: &TgDb,
    name: &str,
    parent: Option<&str>,
) -> Result<TGCloudFolder> {
    let folder = TGCloudFolder {
        folder_id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        parent_folder_id: parent.map(|s| s.to_string()),
        created_at: chrono::Utc::now().timestamp(),
        updated_at: chrono::Utc::now().timestamp(),
    };
    let f = folder.clone();
    with_db(db, move |conn| {
        let mut s = conn
            .prepare(
                "INSERT INTO tgcloud_folders(folder_id,name,parent_folder_id,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5)",
            )
            .map_err(|e| anyhow!(e))?;
        s.bind((1, f.folder_id.as_str())).map_err(|e| anyhow!(e))?;
        s.bind((2, f.name.as_str())).map_err(|e| anyhow!(e))?;
        match &f.parent_folder_id {
            Some(p) => s.bind((3, p.as_str())).map_err(|e| anyhow!(e))?,
            None => s.bind((3, Option::<String>::None)).map_err(|e| anyhow!(e))?,
        }
        s.bind((4, f.created_at)).map_err(|e| anyhow!(e))?;
        s.bind((5, f.updated_at)).map_err(|e| anyhow!(e))?;
        s.next().map_err(|e| anyhow!(e))?;
        Ok(())
    })
    .await?;
    Ok(folder)
}

pub async fn delete_folder(db: &TgDb, folder_id: &str) -> Result<()> {
    let fid = folder_id.to_string();
    with_db(db, move |conn| {
        let mut s = conn
            .prepare("DELETE FROM tgcloud_folders WHERE folder_id=?1")
            .map_err(|e| anyhow!(e))?;
        s.bind((1, fid.as_str())).map_err(|e| anyhow!(e))?;
        s.next().map_err(|e| anyhow!(e))?;
        Ok(())
    })
    .await
}

// ----- sync logs ----------------------------------------------------------

pub async fn append_sync_log(db: &TgDb, log: &SyncLog) -> Result<()> {
    let l = log.clone();
    with_db(db, move |conn| {
        let mut s = conn
            .prepare(
                "INSERT INTO tgcloud_sync_log(log_id,timestamp,device_id,operation,table_name,
                   primary_key,data_json,previous_data_json,is_uploaded,telegram_message_id,checksum)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            )
            .map_err(|e| anyhow!(e))?;
        s.bind((1, l.log_id.as_str())).map_err(|e| anyhow!(e))?;
        s.bind((2, l.timestamp)).map_err(|e| anyhow!(e))?;
        s.bind((3, l.device_id.as_str())).map_err(|e| anyhow!(e))?;
        let op = match l.operation {
            SyncOperation::Insert => "INSERT",
            SyncOperation::Update => "UPDATE",
            SyncOperation::Delete => "DELETE",
        };
        s.bind((4, op)).map_err(|e| anyhow!(e))?;
        s.bind((5, l.table_name.as_str())).map_err(|e| anyhow!(e))?;
        s.bind((6, l.primary_key.as_str())).map_err(|e| anyhow!(e))?;
        match &l.data_json {
            Some(v) => s.bind((7, v.as_str())).map_err(|e| anyhow!(e))?,
            None => s.bind((7, Option::<String>::None)).map_err(|e| anyhow!(e))?,
        }
        match &l.previous_data_json {
            Some(v) => s.bind((8, v.as_str())).map_err(|e| anyhow!(e))?,
            None => s.bind((8, Option::<String>::None)).map_err(|e| anyhow!(e))?,
        }
        s.bind((9, l.is_uploaded as i64)).map_err(|e| anyhow!(e))?;
        match l.telegram_message_id {
            Some(v) => s.bind((10, v)).map_err(|e| anyhow!(e))?,
            None => s.bind((10, Option::<i64>::None)).map_err(|e| anyhow!(e))?,
        }
        match &l.checksum {
            Some(v) => s.bind((11, v.as_str())).map_err(|e| anyhow!(e))?,
            None => s.bind((11, Option::<String>::None)).map_err(|e| anyhow!(e))?,
        }
        s.next().map_err(|e| anyhow!(e))?;
        Ok(())
    })
    .await
}

pub async fn pending_sync_logs(db: &TgDb) -> Result<Vec<SyncLog>> {
    with_db(db, |conn| {
        let mut s = conn
            .prepare("SELECT * FROM tgcloud_sync_log WHERE is_uploaded=0 ORDER BY timestamp")
            .map_err(|e| anyhow!(e))?;
        let mut out = Vec::new();
        while let sqlite::State::Row = s.next().map_err(|e| anyhow!(e))? {
            out.push(SyncLog {
                log_id: s.read::<String, _>("log_id").map_err(|e| anyhow!(e))?,
                timestamp: s.read::<i64, _>("timestamp").map_err(|e| anyhow!(e))?,
                device_id: s.read::<String, _>("device_id").map_err(|e| anyhow!(e))?,
                operation: match s.read::<String, _>("operation").map_err(|e| anyhow!(e))?.as_str() {
                    "DELETE" => SyncOperation::Delete,
                    "UPDATE" => SyncOperation::Update,
                    _ => SyncOperation::Insert,
                },
                table_name: s.read::<String, _>("table_name").map_err(|e| anyhow!(e))?,
                primary_key: s.read::<String, _>("primary_key").map_err(|e| anyhow!(e))?,
                data_json: s.read::<Option<String>, _>("data_json").ok().flatten(),
                previous_data_json: s
                    .read::<Option<String>, _>("previous_data_json")
                    .ok()
                    .flatten(),
                is_uploaded: s.read::<i64, _>("is_uploaded").map_err(|e| anyhow!(e))? != 0,
                telegram_message_id: s
                    .read::<Option<i64>, _>("telegram_message_id")
                    .ok()
                    .flatten(),
                checksum: s.read::<Option<String>, _>("checksum").ok().flatten(),
            });
        }
        Ok(out)
    })
    .await
}

pub async fn mark_sync_uploaded(
    db: &TgDb,
    log_id: &str,
    telegram_message_id: i64,
    checksum: &str,
) -> Result<()> {
    let lid = log_id.to_string();
    let cs = checksum.to_string();
    with_db(db, move |conn| {
        let mut s = conn
            .prepare(
                "UPDATE tgcloud_sync_log SET is_uploaded=1, telegram_message_id=?1, checksum=?2
                 WHERE log_id=?3",
            )
            .map_err(|e| anyhow!(e))?;
        s.bind((1, telegram_message_id)).map_err(|e| anyhow!(e))?;
        s.bind((2, cs.as_str())).map_err(|e| anyhow!(e))?;
        s.bind((3, lid.as_str())).map_err(|e| anyhow!(e))?;
        s.next().map_err(|e| anyhow!(e))?;
        Ok(())
    })
    .await
}

/// Serialize a TGCloudFile to the JSON form used inside sync logs.
pub fn file_to_sync_json(f: &TGCloudFile) -> String {
    json!({
        "id": f.file_id,
        "telegram_message_id": f.telegram_message_id,
        "file_id": f.file_id_tg,
        "file_unique_id": f.file_unique_id,
        "file_name": f.file_name,
        "mime_type": f.mime_type,
        "size_bytes": f.size_bytes,
        "uploaded_at": f.created_at,
        "checksum": f.checksum,
        "uploader_tokens": f.uploader_tokens,
        "total_chunks": f.total_chunks,
        "chunks": f.chunks.iter().map(|c| json!({
            "chunk_index": c.chunk_index,
            "chunk_size": c.chunk_size,
            "chunk_hash": c.chunk_hash,
            "telegram_message_id": c.telegram_message_id,
            "file_id": c.telegram_file_id,
            "file_unique_id": c.telegram_file_unique_id,
            "bot_worker_id": c.bot_worker_id,
        })).collect::<Vec<_>>(),
    })
    .to_string()
}
