import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Cloud,
  CloudOff,
  Download,
  FileIcon,
  Folder,
  FolderPlus,
  Image as ImageIcon,
  Loader2,
  Film,
  FileText,
  Archive,
  Music,
  RefreshCw,
  Settings as SettingsIcon,
  Share2,
  Trash2,
  Upload,
  ArrowLeft,
} from 'lucide-react';
import { save } from '@tauri-apps/plugin-dialog';
import { open as openPath } from '@tauri-apps/plugin-dialog';
import { tgCloud, onTgCloudTransfer } from '../../services/tgcloud';
import type {
  TgCloudConfig,
  TgCloudFile,
  TgCloudFolder,
  TgCloudTransferProgress,
} from '../../types/tgcloud';
import { TGCloudSettingsPanel } from './TGCloudSettingsPanel';
import { formatBytes } from '../../utils/files';

function iconForFile(file: TgCloudFile) {
  const mime = (file.mime_type ?? '').toLowerCase();
  const name = file.file_name.toLowerCase();
  if (mime.startsWith('image/')) return ImageIcon;
  if (mime.startsWith('video/')) return Film;
  if (mime.startsWith('audio/')) return Music;
  if (mime === 'application/pdf' || name.endsWith('.pdf')) return FileText;
  if (/\.(zip|rar|7z|tar|gz|bz2|xz)$/.test(name)) return Archive;
  return FileIcon;
}

interface SpecialViewProps {
  onClose?: () => void;
}

export function SpecialView({ onClose }: SpecialViewProps) {
  const [config, setConfig] = useState<TgCloudConfig | null>(null);
  const [configLoading, setConfigLoading] = useState(true);
  const [showSettings, setShowSettings] = useState(false);

  const [folders, setFolders] = useState<TgCloudFolder[]>([]);
  const [files, setFiles] = useState<TgCloudFile[]>([]);
  const [parentStack, setParentStack] = useState<(TgCloudFolder | null)[]>([null]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [search, setSearch] = useState('');

  const [progress, setProgress] = useState<Record<string, TgCloudTransferProgress>>({});

  const currentFolder = parentStack[parentStack.length - 1] ?? null;

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [f, fl] = await Promise.all([
        tgCloud.listFolders(currentFolder?.folder_id ?? null),
        tgCloud.listFiles(currentFolder?.folder_id ?? null),
      ]);
      setFolders(f);
      setFiles(fl);
      setSelected(new Set());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [currentFolder?.folder_id]);

  const loadConfig = useCallback(async () => {
    setConfigLoading(true);
    try {
      const cfg = await tgCloud.getConfig();
      setConfig(cfg);
      if (!cfg.is_configured) setShowSettings(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setConfigLoading(false);
    }
  }, []);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  useEffect(() => {
    if (config) refresh();
  }, [config, refresh]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onTgCloudTransfer((p) => {
      setProgress((prev) => {
        const key =
          'file_id' in p ? p.file_id : (p as { upload_attempt_id?: string }).upload_attempt_id ?? '';
        if (!key) return prev;
        if (p.phase === 'completed' || p.phase === 'failed' || p.phase === 'cancelled') {
          const { [key]: _removed, ...rest } = prev;
          return rest;
        }
        return { ...prev, [key]: p };
      });
      if (p.phase === 'completed') {
        refresh();
      }
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, [refresh]);

  const visibleFiles = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return files;
    return files.filter((f) => f.file_name.toLowerCase().includes(q));
  }, [files, search]);

  const visibleFolders = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return folders;
    return folders.filter((f) => f.name.toLowerCase().includes(q));
  }, [folders, search]);

  const openFolder = (folder: TgCloudFolder) => {
    setParentStack((s) => [...s, folder]);
  };

  const navigateUp = () => {
    if (parentStack.length > 1) {
      setParentStack((s) => s.slice(0, -1));
    }
  };

  const handleUpload = async () => {
    try {
      const selectedFiles = await openPath({
        multiple: true,
        directory: false,
      });
      if (!selectedFiles) return;
      const paths = Array.isArray(selectedFiles) ? selectedFiles : [selectedFiles];
      for (const path of paths) {
        const name = path.split(/[\\/]/).pop() ?? path;
        try {
          await tgCloud.uploadFile({
            sourcePath: path,
            fileName: name,
            parentFolderId: currentFolder?.folder_id ?? null,
            mimeType: null,
            syncPassword: null,
          });
        } catch (e) {
          setError(`Upload failed for ${name}: ${String(e)}`);
        }
      }
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDownload = async (file: TgCloudFile) => {
    const target = await save({
      defaultPath: file.file_name,
      title: `Download ${file.file_name}`,
    });
    if (!target) return;
    try {
      await tgCloud.downloadFile(file.file_id, target);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (file: TgCloudFile) => {
    if (!confirm(`Delete "${file.file_name}" from TG Cloud? Physical chunks will be removed from Telegram.`)) return;
    try {
      await tgCloud.deleteFile(file.file_id);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleCreateFolder = async () => {
    const name = prompt('Folder name');
    if (!name) return;
    try {
      await tgCloud.createFolder(name, currentFolder?.folder_id ?? null);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleShare = async (file: TgCloudFile) => {
    const pw = prompt('Set a password for this .link share (mandatory)');
    if (!pw) return;
    try {
      const bytes = await tgCloud.createShare([file.file_id], pw);
      const target = await save({
        defaultPath: `${file.file_name}.link`,
        title: 'Save TG Cloud share link',
      });
      if (target) {
        const data = new Uint8Array(bytes as unknown as number[]);
        await tgCloud.saveFile(target, data);
      }
    } catch (e) {
      setError(String(e));
    }
  };

  if (configLoading) {
    return (
      <div className="flex h-full items-center justify-center text-app-text-secondary">
        <Loader2 className="mr-2 h-5 w-5 animate-spin" /> Loading TG Cloud…
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col bg-app-base text-app-text">
      {/* Top bar */}
      <div className="flex items-center gap-2 border-b border-app-border px-4 py-2">
        {onClose && (
          <button
            onClick={onClose}
            className="quiet-control rounded p-1 hover:bg-app-hover"
            title="Back to Telegram Drive"
          >
            <ArrowLeft className="h-4 w-4" />
          </button>
        )}
        <Cloud className="h-5 w-5 text-app-accent" />
        <h2 className="text-sm font-semibold">Special — TG Cloud</h2>
        <div className="mx-2 h-4 w-px bg-app-border" />
        <button
          onClick={navigateUp}
          disabled={parentStack.length <= 1}
          className="quiet-control rounded px-2 py-1 text-xs text-app-text-secondary hover:text-app-text disabled:opacity-40"
        >
          ↑ Up
        </button>
        <span className="truncate text-xs text-app-text-tertiary">
          / {parentStack.slice(1).map((f) => f?.name ?? 'Root').join(' / ')}
        </span>
        <div className="ml-auto flex items-center gap-1">
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search…"
            className="quiet-surface h-7 w-44 rounded border border-app-border px-2 text-xs"
          />
          <button onClick={refresh} className="quiet-control rounded p-1.5 hover:bg-app-hover" title="Refresh">
            <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
          </button>
          <button
            onClick={handleCreateFolder}
            className="quiet-control flex items-center gap-1 rounded px-2 py-1 text-xs hover:bg-app-hover"
          >
            <FolderPlus className="h-4 w-4" /> Folder
          </button>
          <button
            onClick={handleUpload}
            disabled={!config?.is_configured}
            className="quiet-control flex items-center gap-1 rounded bg-app-accent px-2 py-1 text-xs font-medium text-white hover:opacity-90 disabled:opacity-40"
          >
            <Upload className="h-4 w-4" /> Upload
          </button>
          <button
            onClick={() => setShowSettings(true)}
            className="quiet-control rounded p-1.5 hover:bg-app-hover"
            title="TG Cloud Settings"
          >
            <SettingsIcon className="h-4 w-4" />
          </button>
        </div>
      </div>

      {error && (
        <div className="flex items-center gap-2 border-b border-red-500/30 bg-red-500/10 px-4 py-2 text-xs text-red-300">
          <CloudOff className="h-4 w-4" />
          <span className="flex-1">{error}</span>
          <button onClick={() => setError(null)} className="underline">
            Dismiss
          </button>
        </div>
      )}

      {!config?.is_configured ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
          <Cloud className="h-12 w-12 text-app-text-tertiary" />
          <h3 className="text-base font-medium">Configure TG Cloud</h3>
          <p className="max-w-md text-sm text-app-text-secondary">
            Add a storage channel ID and at least one bot token to activate the Special storage backend.
          </p>
          <button
            onClick={() => setShowSettings(true)}
            className="quiet-control rounded bg-app-accent px-4 py-2 text-sm font-medium text-white"
          >
            Open TG Cloud Settings
          </button>
        </div>
      ) : (
        <div className="flex-1 overflow-auto p-3">
          {/* Folders */}
          {visibleFolders.length > 0 && (
            <div className="mb-4 grid grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-2">
              {visibleFolders.map((folder) => (
                <button
                  key={folder.folder_id}
                  onDoubleClick={() => openFolder(folder)}
                  onClick={() => openFolder(folder)}
                  className="quiet-control flex items-center gap-2 rounded border border-app-border bg-app-surface p-3 text-left hover:border-app-accent/40"
                >
                  <Folder className="h-8 w-8 text-amber-400" />
                  <div className="min-w-0">
                    <div className="truncate text-sm font-medium">{folder.name}</div>
                    <div className="text-xs text-app-text-tertiary">Folder</div>
                  </div>
                </button>
              ))}
            </div>
          )}

          {/* Files */}
          {loading ? (
            <div className="flex items-center justify-center p-8 text-app-text-secondary">
              <Loader2 className="mr-2 h-5 w-5 animate-spin" /> Loading…
            </div>
          ) : visibleFiles.length === 0 ? (
            <div className="flex flex-col items-center justify-center p-12 text-center text-app-text-tertiary">
              <FileIcon className="mb-3 h-10 w-10 opacity-50" />
              <p className="text-sm">No files in this folder</p>
              <p className="mt-1 text-xs">Upload a file or create a folder to get started.</p>
            </div>
          ) : (
            <div className="overflow-hidden rounded border border-app-border">
              <table className="w-full text-sm">
                <thead className="bg-app-surface text-xs uppercase text-app-text-tertiary">
                  <tr>
                    <th className="w-8 p-2"></th>
                    <th className="p-2 text-left">Name</th>
                    <th className="w-24 p-2 text-left">Size</th>
                    <th className="w-24 p-2 text-left">Chunks</th>
                    <th className="w-48 p-2 text-left">Progress</th>
                    <th className="w-32 p-2 text-right">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {visibleFiles.map((file) => {
                    const Icon = iconForFile(file);
                    const isSelected = selected.has(file.file_id);
                    const p = progress[file.file_id];
                    let progressText = '';
                    if (p) {
                      if (p.phase === 'started') progressText = 'Starting…';
                      else if (p.phase === 'chunk_uploaded')
                        progressText = `Uploading chunk ${p.chunk_index}…`;
                      else if (p.phase === 'finalizing') progressText = 'Finalizing…';
                      else if (p.phase === 'sync_publishing') progressText = 'Publishing sync…';
                      else if (p.phase === 'download_progress')
                        progressText = `Downloading ${formatBytes(p.bytes_done)} / ${formatBytes(p.total_bytes)}`;
                    }
                    return (
                      <tr
                        key={file.file_id}
                        onClick={() =>
                          setSelected((s) => {
                            const n = new Set(s);
                            if (n.has(file.file_id)) n.delete(file.file_id);
                            else n.add(file.file_id);
                            return n;
                          })
                        }
                        className={`cursor-pointer border-t border-app-border-subtle hover:bg-app-hover ${
                          isSelected ? 'bg-app-selected' : ''
                        }`}
                      >
                        <td className="p-2">
                          <Icon className="h-4 w-4 text-app-text-secondary" />
                        </td>
                        <td className="truncate p-2 font-medium" title={file.file_name}>
                          {file.file_name}
                        </td>
                        <td className="p-2 text-app-text-secondary">{formatBytes(file.size_bytes)}</td>
                        <td className="p-2 text-app-text-secondary">
                          {file.total_chunks > 1 ? `${file.total_chunks} × 10 MiB` : '—'}
                        </td>
                        <td className="p-2 text-xs text-app-text-secondary">{progressText}</td>
                        <td className="p-2">
                          <div className="flex items-center justify-end gap-1">
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                handleDownload(file);
                              }}
                              className="quiet-control rounded p-1 hover:bg-app-hover"
                              title="Download"
                            >
                              <Download className="h-3.5 w-3.5" />
                            </button>
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                handleShare(file);
                              }}
                              className="quiet-control rounded p-1 hover:bg-app-hover"
                              title="Share (.link)"
                            >
                              <Share2 className="h-3.5 w-3.5" />
                            </button>
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                handleDelete(file);
                              }}
                              className="quiet-control rounded p-1 text-app-danger hover:bg-app-danger/10"
                              title="Delete"
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </button>
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {showSettings && (
        <TGCloudSettingsPanel
          config={config}
          onClose={() => {
            setShowSettings(false);
            loadConfig();
          }}
          onSaved={(cfg) => {
            setConfig(cfg);
          }}
        />
      )}
    </div>
  );
}
