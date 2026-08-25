import { useEffect, useState } from 'react';
import {
  Bot,
  Check,
  Cloud,
  KeyRound,
  Loader2,
  Save,
  X,
  AlertTriangle,
} from 'lucide-react';
import { tgCloud } from '../../services/tgcloud';
import type {
  TgCloudBotTestResult,
  TgCloudBotWorker,
  TgCloudConfig,
} from '../../types/tgcloud';
import { TG_CLOUD_MAX_BOTS } from '../../types/tgcloud';

interface Props {
  config: TgCloudConfig | null;
  onClose: () => void;
  onSaved: (cfg: TgCloudConfig) => void;
}

function emptyBots(): TgCloudBotWorker[] {
  return Array.from({ length: TG_CLOUD_MAX_BOTS }, (_, i) => ({
    slot: i + 1,
    token: '',
    enabled: false,
    username: null,
  }));
}

export function TGCloudSettingsPanel({ config, onClose, onSaved }: Props) {
  const [apiId, setApiId] = useState<string>('');
  const [apiHash, setApiHash] = useState<string>('');
  const [channelId, setChannelId] = useState<string>('');
  const [bots, setBots] = useState<TgCloudBotWorker[]>(emptyBots());
  const [workerCount, setWorkerCount] = useState(2);
  const [queueDepth, setQueueDepth] = useState(4);
  const [syncEnabled, setSyncEnabled] = useState(false);
  const [syncChannelId, setSyncChannelId] = useState<string>('');
  const [syncBotToken, setSyncBotToken] = useState<string>('');
  const [syncPassword, setSyncPassword] = useState<string>('');
  const [requireSharePassword, setRequireSharePassword] = useState(true);

  const [testingSlot, setTestingSlot] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!config) return;
    setApiId(config.api_id?.toString() ?? '');
    setApiHash(config.api_hash ?? '');
    setChannelId(config.storage_channel_id?.toString() ?? '');
    setWorkerCount(config.worker_count || 2);
    setQueueDepth(config.queue_depth || 4);
    setSyncEnabled(config.sync_enabled);
    setSyncChannelId(config.sync_channel_id?.toString() ?? '');
    setSyncBotToken(config.sync_bot_token ?? '');
    setRequireSharePassword(config.require_share_password);
    // Preserve usernames on loaded bots; ensure 5 slots exist.
    const loaded = [...(config.bots ?? [])];
    setBots(
      emptyBots().map((blank) => {
        const existing = loaded.find((b) => b.slot === blank.slot);
        return existing ?? blank;
      }),
    );
  }, [config]);

  const updateBot = (slot: number, patch: Partial<TgCloudBotWorker>) => {
    setBots((prev) =>
      prev.map((b) => (b.slot === slot ? { ...b, ...patch } : b)),
    );
  };

  const testBot = async (slot: number) => {
    const bot = bots.find((b) => b.slot === slot);
    if (!bot || !bot.token.trim()) return;
    setTestingSlot(slot);
    setError(null);
    try {
      const result: TgCloudBotTestResult = await tgCloud.testBotToken(bot.token.trim());
      updateBot(slot, {
        enabled: result.ok,
        username: result.username,
      });
      if (!result.ok) setError(`Bot ${slot}: ${result.error ?? 'validation failed'}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setTestingSlot(null);
    }
  };

  const mask = (token: string) => {
    if (!token) return '';
    if (token.length <= 16) return '••••••••';
    return `${token.slice(0, 8)}…${token.slice(-4)}`;
  };

  const validBotCount = bots.filter((b) => b.enabled && b.token.trim()).length;

  const save = async () => {
    setError(null);
    if (!channelId.trim()) {
      setError('Storage Channel ID is required.');
      return;
    }
    if (validBotCount < 1) {
      setError('At least one valid, enabled bot token is required.');
      return;
    }
    setSaving(true);
    try {
      const toSave: TgCloudConfig = {
        api_id: apiId.trim() ? Number(apiId.trim()) : null,
        api_hash: apiHash.trim() || null,
        storage_channel_id: Number(channelId.trim()),
        bots: bots.map((b) => ({
          slot: b.slot,
          token: b.token.trim(),
          enabled: b.enabled && !!b.token.trim(),
          username: b.username,
        })),
        worker_count: Math.min(Math.max(1, workerCount), 5),
        queue_depth: Math.min(Math.max(1, queueDepth), 16),
        sync_enabled: syncEnabled,
        sync_channel_id: syncChannelId.trim() ? Number(syncChannelId.trim()) : null,
        sync_bot_token: syncBotToken.trim() || null,
        sync_password_set: syncPassword.length > 0 || (config?.sync_password_set ?? false),
        require_share_password: requireSharePassword,
      };
      await tgCloud.saveConfig(toSave);
      onSaved(toSave);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-[400] flex items-center justify-center bg-black/50 p-4">
      <div className="quiet-surface flex max-h-[90vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-app-border bg-app-surface shadow-2xl">
        {/* Header */}
        <div className="flex items-center gap-2 border-b border-app-border px-5 py-3">
          <Cloud className="h-5 w-5 text-app-accent" />
          <h2 className="text-sm font-semibold">TG Cloud Settings</h2>
          <button
            onClick={onClose}
            className="ml-auto rounded p-1 hover:bg-app-hover"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="flex-1 space-y-6 overflow-y-auto p-5">
          {error && (
            <div className="flex items-start gap-2 rounded border border-red-500/30 bg-red-500/10 p-3 text-xs text-red-300">
              <AlertTriangle className="mt-0.5 h-4 w-4 flex-shrink-0" />
              <span>{error}</span>
            </div>
          )}

          {/* Telegram API */}
          <section>
            <h3 className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase text-app-text-tertiary">
              <KeyRound className="h-3.5 w-3.5" /> Telegram API
            </h3>
            <div className="grid grid-cols-2 gap-3">
              <label className="text-xs">
                API ID
                <input
                  type="text"
                  value={apiId}
                  onChange={(e) => setApiId(e.target.value.replace(/\D/g, ''))}
                  className="quiet-surface mt-1 w-full rounded border border-app-border px-2 py-1.5"
                  placeholder="123456"
                />
              </label>
              <label className="text-xs">
                API Hash
                <input
                  type="password"
                  value={apiHash}
                  onChange={(e) => setApiHash(e.target.value)}
                  className="quiet-surface mt-1 w-full rounded border border-app-border px-2 py-1.5"
                  placeholder="0123456789abcdef…"
                />
              </label>
            </div>
            <p className="mt-1 text-[11px] text-app-text-tertiary">
              Separate from your normal Telegram Drive account. Get credentials at my.telegram.org.
            </p>
          </section>

          {/* Storage */}
          <section>
            <h3 className="mb-2 text-xs font-semibold uppercase text-app-text-tertiary">
              Storage
            </h3>
            <label className="text-xs">
              Storage Channel ID
              <input
                type="text"
                value={channelId}
                onChange={(e) => setChannelId(e.target.value.replace(/[^\d-]/g, ''))}
                className="quiet-surface mt-1 w-full rounded border border-app-border px-2 py-1.5"
                placeholder="-1001234567890"
              />
            </label>
            <p className="mt-1 text-[11px] text-app-text-tertiary">
              ONE channel for all physical chunks and sync messages. Bots must be administrators.
            </p>
          </section>

          {/* Bot Pool */}
          <section>
            <h3 className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase text-app-text-tertiary">
              <Bot className="h-3.5 w-3.5" /> Bot Pool ({validBotCount}/{TG_CLOUD_MAX_BOTS} active)
            </h3>
            <div className="space-y-2">
              {bots.map((bot) => (
                <div key={bot.slot} className="flex items-center gap-2">
                  <span className="w-16 text-xs text-app-text-tertiary">Bot Token {bot.slot}</span>
                  <input
                    type="password"
                    value={bot.token}
                    onChange={(e) =>
                      updateBot(bot.slot, { token: e.target.value, enabled: false, username: null })
                    }
                    className="quiet-surface flex-1 rounded border border-app-border px-2 py-1.5 font-mono text-xs"
                    placeholder={bot.username ? mask(bot.token) : '123456:ABC-DEF…'}
                  />
                  {bot.username && (
                    <span className="w-32 truncate text-[11px] text-emerald-400" title={`@${bot.username}`}>
                      @{bot.username}
                    </span>
                  )}
                  <button
                    onClick={() => testBot(bot.slot)}
                    disabled={testingSlot === bot.slot || !bot.token.trim()}
                    className="quiet-control flex h-8 items-center gap-1 rounded border border-app-border px-2 text-xs hover:bg-app-hover disabled:opacity-40"
                  >
                    {testingSlot === bot.slot ? (
                      <Loader2 className="h-3 w-3 animate-spin" />
                    ) : bot.enabled ? (
                      <Check className="h-3 w-3 text-emerald-400" />
                    ) : (
                      'Test'
                    )}
                  </button>
                  {bot.token && (
                    <button
                      onClick={() =>
                        updateBot(bot.slot, { token: '', enabled: false, username: null })
                      }
                      className="quiet-control rounded p-1.5 hover:bg-app-hover"
                      title="Remove token"
                    >
                      <X className="h-3.5 w-3.5" />
                    </button>
                  )}
                </div>
              ))}
            </div>
          </section>

          {/* Transfer */}
          <section>
            <h3 className="mb-2 text-xs font-semibold uppercase text-app-text-tertiary">
              Transfer
            </h3>
            <div className="grid grid-cols-3 gap-3 text-xs">
              <div>
                <label>Chunk Size</label>
                <input
                  disabled
                  value="10 MiB"
                  className="quiet-surface mt-1 w-full rounded border border-app-border px-2 py-1.5 opacity-60"
                />
              </div>
              <label>
                Worker Count
                <input
                  type="number"
                  min={1}
                  max={5}
                  value={workerCount}
                  onChange={(e) => setWorkerCount(Number(e.target.value))}
                  className="quiet-surface mt-1 w-full rounded border border-app-border px-2 py-1.5"
                />
              </label>
              <label>
                Queue Depth
                <input
                  type="number"
                  min={1}
                  max={16}
                  value={queueDepth}
                  onChange={(e) => setQueueDepth(Number(e.target.value))}
                  className="quiet-surface mt-1 w-full rounded border border-app-border px-2 py-1.5"
                />
              </label>
            </div>
            <p className="mt-1 text-[11px] text-app-text-tertiary">
              Transfer RAM ≈ queue_depth × 10 MiB. Default 4 → ~40 MiB chunk buffers (plus per-request overhead).
            </p>
          </section>

          {/* Sync */}
          <section>
            <h3 className="mb-2 text-xs font-semibold uppercase text-app-text-tertiary">
              Sync
            </h3>
            <label className="flex items-center gap-2 text-xs">
              <input
                type="checkbox"
                checked={syncEnabled}
                onChange={(e) => setSyncEnabled(e.target.checked)}
              />
              Enable encrypted multi-device sync
            </label>
            {syncEnabled && (
              <div className="mt-2 space-y-2">
                <input
                  type="text"
                  value={syncChannelId}
                  onChange={(e) => setSyncChannelId(e.target.value.replace(/[^\d-]/g, ''))}
                  className="quiet-surface w-full rounded border border-app-border px-2 py-1.5 text-xs"
                  placeholder="Sync Channel ID (defaults to storage channel)"
                />
                <input
                  type="password"
                  value={syncBotToken}
                  onChange={(e) => setSyncBotToken(e.target.value)}
                  className="quiet-surface w-full rounded border border-app-border px-2 py-1.5 font-mono text-xs"
                  placeholder="Sync bot token (optional, defaults to first bot)"
                />
                <input
                  type="password"
                  value={syncPassword}
                  onChange={(e) => setSyncPassword(e.target.value)}
                  className="quiet-surface w-full rounded border border-app-border px-2 py-1.5 text-xs"
                  placeholder={config?.sync_password_set ? '•••••••• (set — type to change)' : 'Sync password'}
                />
              </div>
            )}
          </section>

          {/* Sharing */}
          <section>
            <h3 className="mb-2 text-xs font-semibold uppercase text-app-text-tertiary">
              Sharing
            </h3>
            <label className="flex items-center gap-2 text-xs">
              <input
                type="checkbox"
                checked={requireSharePassword}
                onChange={(e) => setRequireSharePassword(e.target.checked)}
              />
              Require password for .link shares
            </label>
          </section>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2 border-t border-app-border px-5 py-3">
          <button
            onClick={onClose}
            className="quiet-control rounded border border-app-border px-4 py-1.5 text-xs hover:bg-app-hover"
          >
            Cancel
          </button>
          <button
            onClick={save}
            disabled={saving}
            className="quiet-control flex items-center gap-1.5 rounded bg-app-accent px-4 py-1.5 text-xs font-medium text-white hover:opacity-90 disabled:opacity-50"
          >
            {saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Save className="h-3.5 w-3.5" />}
            Save Configuration
          </button>
        </div>
      </div>
    </div>
  );
}
