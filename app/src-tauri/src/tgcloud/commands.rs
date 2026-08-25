//! Tauri IPC commands exposed to the React frontend (Special tab).
//!
//! All commands are prefixed `tgcloud_` to avoid collision with the existing
//! Telegram Drive commands. The frontend calls them via `invoke()`.

use crate::tgcloud::{
    models::*,
    provider::TGCloudProvider,
    CHUNK_SIZE, MAX_BOT_WORKERS,
};
use tauri::State;

/// Shared state type registered with Tauri in `lib.rs`.
pub type TgCloudState = TGCloudProvider;

// ----- configuration -------------------------------------------------------

#[tauri::command]
pub async fn tgcloud_get_config(state: State<'_, TgCloudState>) -> Result<TGCloudConfig, String> {
    state.config().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tgcloud_save_config(
    state: State<'_, TgCloudState>,
    config: TGCloudConfig,
) -> Result<(), String> {
    state.save_config(&config).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tgcloud_test_bot_token(token: String) -> Result<BotTestResult, String> {
    TGCloudProvider::test_bot_token(&token)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tgcloud_chunk_size() -> u64 {
    CHUNK_SIZE
}

#[tauri::command]
pub async fn tgcloud_max_bots() -> usize {
    MAX_BOT_WORKERS
}

// ----- files / folders -----------------------------------------------------

#[tauri::command]
pub async fn tgcloud_list_files(
    state: State<'_, TgCloudState>,
    folder_id: Option<String>,
) -> Result<Vec<TGCloudFile>, String> {
    state
        .list_files(folder_id.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tgcloud_list_folders(
    state: State<'_, TgCloudState>,
    parent_id: Option<String>,
) -> Result<Vec<TGCloudFolder>, String> {
    state
        .list_folders(parent_id.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tgcloud_create_folder(
    state: State<'_, TgCloudState>,
    name: String,
    parent_id: Option<String>,
) -> Result<TGCloudFolder, String> {
    state
        .create_folder(&name, parent_id.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tgcloud_delete_folder(
    state: State<'_, TgCloudState>,
    folder_id: String,
) -> Result<(), String> {
    state.delete_folder(&folder_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tgcloud_delete_file(
    state: State<'_, TgCloudState>,
    file_id: String,
) -> Result<(), String> {
    state.delete_file(&file_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tgcloud_rename_file(
    state: State<'_, TgCloudState>,
    file_id: String,
    new_name: String,
) -> Result<(), String> {
    state
        .rename_file(&file_id, &new_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tgcloud_move_file(
    state: State<'_, TgCloudState>,
    file_id: String,
    folder_id: Option<String>,
) -> Result<(), String> {
    state
        .move_file(&file_id, folder_id.as_deref())
        .await
        .map_err(|e| e.to_string())
}

// ----- upload / download ---------------------------------------------------

#[tauri::command]
pub async fn tgcloud_upload_file(
    state: State<'_, TgCloudState>,
    source_path: String,
    file_name: String,
    parent_folder_id: Option<String>,
    mime_type: Option<String>,
    sync_password: Option<String>,
) -> Result<TGCloudFile, String> {
    state
        .upload_file(
            std::path::Path::new(&source_path),
            &file_name,
            parent_folder_id.as_deref(),
            mime_type.as_deref(),
            sync_password.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tgcloud_cancel_upload(
    state: State<'_, TgCloudState>,
    file_id: String,
) -> Result<(), String> {
    state.cancel_upload(&file_id).await;
    Ok(())
}

#[tauri::command]
pub async fn tgcloud_download_file(
    state: State<'_, TgCloudState>,
    file_id: String,
    target_path: String,
) -> Result<(), String> {
    state
        .download_file(&file_id, std::path::Path::new(&target_path))
        .await
        .map_err(|e| e.to_string())
}

// ----- sharing -------------------------------------------------------------

#[tauri::command]
pub async fn tgcloud_create_share(
    state: State<'_, TgCloudState>,
    file_ids: Vec<String>,
    password: String,
) -> Result<Vec<u8>, String> {
    state
        .create_share(&file_ids, &password)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tgcloud_import_share(
    state: State<'_, TgCloudState>,
    link_bytes: Vec<u8>,
    password: String,
) -> Result<ShareManifest, String> {
    state
        .import_share(&link_bytes, &password)
        .await
        .map_err(|e| e.to_string())
}

/// Save an arbitrary byte blob to a user-chosen path (used for `.link` export
/// and backup downloads). Implemented on the Rust side so the Tauri fs scope
/// is not required for arbitrary paths.
#[tauri::command]
pub async fn tgcloud_save_file(path: String, bytes: Vec<u8>) -> Result<(), String> {
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| format!("write {path}: {e}"))
}

// ----- backup --------------------------------------------------------------

#[tauri::command]
pub async fn tgcloud_create_backup(
    state: State<'_, TgCloudState>,
    password: String,
) -> Result<Vec<u8>, String> {
    state
        .create_backup(&password)
        .await
        .map_err(|e| e.to_string())
}
