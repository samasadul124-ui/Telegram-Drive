// Types for the TG Cloud second storage backend ("Special" tab).
// Mirror the Rust serde types in src-tauri/src/tgcloud/models.rs.

export type TgCloudFileStatus =
  | 'complete'
  | 'uploading'
  | 'failed'
  | 'downloading';

export interface TgCloudChunkRecord {
  chunk_index: number;
  chunk_size: number;
  chunk_hash: string;
  telegram_message_id: number;
  telegram_file_id: string;
  telegram_file_unique_id: string;
  bot_worker_id: number;
}

export interface TgCloudFile {
  file_id: string;
  file_name: string;
  mime_type: string | null;
  size_bytes: number;
  total_chunks: number;
  status: TgCloudFileStatus;
  parent_folder_id: string | null;
  telegram_message_id: number;
  file_id_tg: string;
  file_unique_id: string;
  checksum: string;
  uploader_tokens: string;
  created_at: number;
  updated_at: number;
  chunks: TgCloudChunkRecord[];
}

export interface TgCloudFolder {
  folder_id: string;
  name: string;
  parent_folder_id: string | null;
  created_at: number;
  updated_at: number;
}

export interface TgCloudBotWorker {
  slot: number;
  token: string;
  enabled: boolean;
  username: string | null;
}

export interface TgCloudConfig {
  api_id: number | null;
  api_hash: string | null;
  storage_channel_id: number | null;
  bots: TgCloudBotWorker[];
  worker_count: number;
  queue_depth: number;
  sync_enabled: boolean;
  sync_channel_id: number | null;
  sync_bot_token: string | null;
  sync_password_set: boolean;
  require_share_password: boolean;
}

export interface TgCloudBotTestResult {
  ok: boolean;
  username: string | null;
  first_name: string | null;
  error: string | null;
}

export type TgCloudTransferProgress =
  | {
      phase: 'started';
      upload_attempt_id: string;
      file_id: string;
      file_name: string;
      total_bytes: number;
      total_chunks: number;
    }
  | {
      phase: 'chunk_uploaded';
      upload_attempt_id: string;
      chunk_index: number;
      bot_worker_id: number;
      bytes_sent: number;
    }
  | { phase: 'finalizing'; upload_attempt_id: string }
  | { phase: 'sync_publishing'; upload_attempt_id: string }
  | {
      phase: 'completed';
      upload_attempt_id: string;
      file_id: string;
    }
  | { phase: 'failed'; upload_attempt_id: string; error: string }
  | { phase: 'cancelled'; upload_attempt_id: string }
  | {
      phase: 'download_progress';
      file_id: string;
      bytes_done: number;
      total_bytes: number;
    };

export interface TgCloudShareChunk {
  chunk_index: number;
  chunk_hash: string;
  file_id: string;
  file_unique_id: string | null;
  uploader: string | null;
}

export interface TgCloudShareFileEntry {
  file_name: string;
  size_bytes: number;
  mime_type: string | null;
  uploaded_at: number;
  file_id: string | null;
  file_unique_id: string | null;
  uploader: string | null;
  total_chunks: number | null;
  chunk_size: number | null;
  chunks: TgCloudShareChunk[];
}

export interface TgCloudShareManifest {
  version: string;
  type: string;
  created_at: number;
  files: TgCloudShareFileEntry[];
}

export const TG_CLOUD_MAX_BOTS = 5;
export const TG_CLOUD_CHUNK_SIZE = 10 * 1024 * 1024; // 10 MiB
