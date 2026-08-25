// Thin typed wrapper around the Tauri commands exposed by the TG Cloud
// Rust backend. The React UI must go through this module and never call
// invoke() directly for TG Cloud operations.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  TgCloudBotTestResult,
  TgCloudChunkRecord,
  TgCloudConfig,
  TgCloudFile,
  TgCloudFolder,
  TgCloudShareManifest,
  TgCloudTransferProgress,
} from '../types/tgcloud';

export const tgCloud = {
  // Config
  getConfig: () => invoke<TgCloudConfig>('tgcloud_get_config'),
  saveConfig: (config: TgCloudConfig) =>
    invoke<void>('tgcloud_save_config', { config }),
  testBotToken: (token: string) =>
    invoke<TgCloudBotTestResult>('tgcloud_test_bot_token', { token }),
  chunkSize: () => invoke<number>('tgcloud_chunk_size'),
  maxBots: () => invoke<number>('tgcloud_max_bots'),

  // Files & folders
  listFiles: (folderId: string | null) =>
    invoke<TgCloudFile[]>('tgcloud_list_files', { folderId }),
  listFolders: (parentId: string | null) =>
    invoke<TgCloudFolder[]>('tgcloud_list_folders', { parentId }),
  createFolder: (name: string, parentId: string | null) =>
    invoke<TgCloudFolder>('tgcloud_create_folder', { name, parentId }),
  deleteFolder: (folderId: string) =>
    invoke<void>('tgcloud_delete_folder', { folderId }),
  deleteFile: (fileId: string) =>
    invoke<void>('tgcloud_delete_file', { fileId }),
  renameFile: (fileId: string, newName: string) =>
    invoke<void>('tgcloud_rename_file', { fileId, newName }),
  moveFile: (fileId: string, folderId: string | null) =>
    invoke<void>('tgcloud_move_file', { fileId, folderId }),

  // Transfer
  uploadFile: (args: {
    sourcePath: string;
    fileName: string;
    parentFolderId: string | null;
    mimeType?: string | null;
    syncPassword?: string | null;
  }) =>
    invoke<TgCloudFile>('tgcloud_upload_file', {
      sourcePath: args.sourcePath,
      fileName: args.fileName,
      parentFolderId: args.parentFolderId ?? null,
      mimeType: args.mimeType ?? null,
      syncPassword: args.syncPassword ?? null,
    }),
  cancelUpload: (fileId: string) =>
    invoke<void>('tgcloud_cancel_upload', { fileId }),
  downloadFile: (fileId: string, targetPath: string) =>
    invoke<void>('tgcloud_download_file', { fileId, targetPath }),

  // Sharing & backup
  createShare: (fileIds: string[], password: string) =>
    invoke<number[]>('tgcloud_create_share', { fileIds, password }),
  importShare: (linkBytes: Uint8Array, password: string) =>
    invoke<TgCloudShareManifest>('tgcloud_import_share', {
      linkBytes: Array.from(linkBytes),
      password,
    }),
  createBackup: (password: string) =>
    invoke<number[]>('tgcloud_create_backup', { password }),
  saveFile: (path: string, bytes: Uint8Array | number[]) =>
    invoke<void>('tgcloud_save_file', {
      path,
      bytes: Array.from(bytes),
    }),
};

export function onTgCloudTransfer(
  cb: (progress: TgCloudTransferProgress) => void,
): Promise<UnlistenFn> {
  return listen<TgCloudTransferProgress>('tgcloud-transfer', (event) =>
    cb(event.payload),
  );
}

// Re-export chunk record type for callers.
export type { TgCloudChunkRecord };
