import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { SearchBar } from './components/SearchBar';
import { StartupWarningBanner } from './components/StartupWarningBanner';
import { SnippetsSettings } from './components/SnippetsSettings';
import { ConfirmModal } from './components/ConfirmModal';
import { PanelHost } from './components/PanelHost';
import { useAppStore } from './stores/app';
import type { WebSearch } from './types';

// Watson's mark — a geometric "W" whose strokes converge into a
// downward arrow at center, doubling as a cursor / pointer
// metaphor. Replaces the prior bowler-hat illustration which read
// as costumed-mascot rather than the keyboard-driven launcher
// platform Watson aims to be. The amber crossbar on the rightmost
// stroke preserves the brand's accent heritage. Strokes follow
// `currentColor` so the mark stays legible across themes.
function WatsonLogo() {
  return (
    <svg
      className="w-8 h-8 text-gray-700 dark:text-gray-200"
      viewBox="0 0 32 32"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {/* First three strokes of the W in neutral. Stops at the
          right-trough vertex so the final up-stroke can carry the
          accent color cleanly. */}
      <path d="M 4 8 L 10 24 L 16 14 L 22 24" />
      {/* Final up-stroke in amber — the brand accent carried
          forward from the prior logo's hat band, now load-bearing
          as part of the mark instead of a decorative crossing. */}
      <path d="M 22 24 L 28 8" className="stroke-amber-600" />
    </svg>
  );
}

function SettingsIcon({ onClick }: { onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className="p-1.5 rounded-lg hover:bg-[var(--selected)] transition-colors"
      title="Settings"
    >
      <svg className="w-5 h-5 text-gray-400 hover:text-gray-600" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <circle cx="12" cy="12" r="3" />
        <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
      </svg>
    </button>
  );
}

/**
 * WAT-406: header bell icon that opens the notifications drawer. A
 * small red dot appears on the bell when unread count > 0.
 */
function NotificationsBell({
  count,
  onClick,
}: {
  count: number;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={count > 0 ? `Notifications (${count} unread)` : 'Notifications'}
      title="Notifications"
      className="relative p-1.5 rounded-lg hover:bg-[var(--selected)] transition-colors"
    >
      <svg
        className="w-5 h-5 text-gray-400 hover:text-gray-600"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
      >
        <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
        <path d="M13.73 21a2 2 0 0 1-3.46 0" />
      </svg>
      {count > 0 && (
        <span
          data-testid="notifications-badge"
          aria-hidden="true"
          className="absolute top-0.5 right-0.5 min-w-[16px] h-4 px-1 text-[10px] font-semibold text-white bg-red-500 rounded-full flex items-center justify-center"
        >
          {count > 9 ? '9+' : count}
        </span>
      )}
    </button>
  );
}

function WebSearchEditor({
  search,
  onSave,
  onCancel,
  onDelete,
  reservedPrefixes,
}: {
  search: WebSearch | null;
  onSave: (ws: WebSearch) => void;
  onCancel: () => void;
  onDelete?: () => void;
  reservedPrefixes: string[];
}) {
  const [name, setName] = useState(search?.name || '');
  const [keyword, setKeyword] = useState(search?.keyword || '');
  const [url, setUrl] = useState(search?.url || '');
  const [instance, setInstance] = useState(search?.instance || '');

  // Check if URL template uses {instance} placeholder
  const needsInstance = url.includes('{instance}');
  // WAT-205: don't block save on a collision — the user may be intentionally
  // testing or may have a reason to keep the row. Just surface the issue.
  const isReservedKeyword = reservedPrefixes.includes(keyword);
  const isValid = name && keyword && url && (!needsInstance || instance);

  const handleSave = () => {
    if (!isValid) return;
    onSave({
      name,
      keyword,
      url,
      requires_setup: needsInstance,
      instance: needsInstance ? instance : undefined,
    });
  };

  return (
    <div className="space-y-3 p-3 bg-[var(--input-bg)] rounded-lg">
      <div>
        <label className="text-xs text-gray-500 mb-1 block">Name</label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Google"
          className="w-full px-3 py-1.5 text-sm bg-[var(--background)] border border-[var(--border)] rounded-lg outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>
      <div>
        <label className="text-xs text-gray-500 mb-1 block">Keyword</label>
        <input
          type="text"
          value={keyword}
          onChange={(e) => setKeyword(e.target.value)}
          placeholder="g"
          className="w-full px-3 py-1.5 text-sm bg-[var(--background)] border border-[var(--border)] rounded-lg outline-none focus:ring-1 focus:ring-blue-500"
        />
        {isReservedKeyword && (
          <p
            role="alert"
            className="text-xs text-amber-600 dark:text-amber-400 mt-1"
          >
            "{keyword}" is a reserved query prefix — this web search will
            be unreachable because the prefix always triggers a built-in
            action. Pick a different keyword or save anyway if that's
            intended.
          </p>
        )}
      </div>
      <div>
        <label className="text-xs text-gray-500 mb-1 block">URL (use {'{query}'} for search term, {'{instance}'} for subdomain)</label>
        <input
          type="text"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="https://google.com/search?q={query}"
          className="w-full px-3 py-1.5 text-sm bg-[var(--background)] border border-[var(--border)] rounded-lg outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>
      {needsInstance && (
        <div>
          <label className="text-xs text-gray-500 mb-1 block">
            Instance (your subdomain, e.g., "mycompany" for mycompany.atlassian.net)
          </label>
          <input
            type="text"
            value={instance}
            onChange={(e) => setInstance(e.target.value)}
            placeholder="mycompany"
            className="w-full px-3 py-1.5 text-sm bg-[var(--background)] border border-[var(--border)] rounded-lg outline-none focus:ring-1 focus:ring-blue-500"
          />
          {!instance && (
            <p className="text-xs text-amber-500 mt-1">* Instance required for this URL template</p>
          )}
        </div>
      )}
      <div className="flex gap-2 pt-1">
        <button
          onClick={handleSave}
          disabled={!isValid}
          className="px-3 py-1.5 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          Save
        </button>
        <button
          onClick={onCancel}
          className="px-3 py-1.5 text-sm bg-[var(--selected)] rounded-lg hover:bg-[var(--border)] transition-colors"
        >
          Cancel
        </button>
        {onDelete && (
          <button
            onClick={onDelete}
            className="px-3 py-1.5 text-sm text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors ml-auto"
          >
            Delete
          </button>
        )}
      </div>
    </div>
  );
}

function SettingsPanel({ onClose }: { onClose: () => void }) {
  const { settings, saveSettings, reindexApps, reindexFiles, reservedPrefixes } = useAppStore();
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [isAddingNew, setIsAddingNew] = useState(false);
  const [version, setVersion] = useState('');
  const [updateStatus, setUpdateStatus] = useState<'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'none' | 'error'>('idle');
  const [updateError, setUpdateError] = useState('');
  // WAT-405: confirm-modal state for Clear Clipboard History.
  const [showClearClipboardConfirm, setShowClearClipboardConfirm] = useState(false);
  // WAT-402: focus the panel container on mount so the Esc handler
  // catches even before the user clicks into a field. The user's prior
  // focus (the SettingsIcon button) wouldn't bubble Esc into our region.
  const panelRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    panelRef.current?.focus();
  }, []);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion('unknown'));
  }, []);

  const checkForUpdates = async () => {
    setUpdateStatus('checking');
    setUpdateError('');
    try {
      const update = await check();
      if (update) {
        setUpdateStatus('available');
        // Auto-download
        setUpdateStatus('downloading');
        await update.downloadAndInstall();
        setUpdateStatus('ready');
      } else {
        setUpdateStatus('none');
      }
    } catch (err) {
      setUpdateStatus('error');
      setUpdateError(err instanceof Error ? err.message : 'Update check failed');
    }
  };

  const handleRelaunch = async () => {
    await relaunch();
  };

  if (!settings) return null;

  const handleThemeChange = (mode: 'light' | 'dark' | 'system') => {
    saveSettings({ ...settings, theme: { ...settings.theme, mode } });
  };

  const handleSaveWebSearch = (ws: WebSearch, index?: number) => {
    const newWebSearches = [...settings.web_searches];
    if (index !== undefined) {
      newWebSearches[index] = ws;
    } else {
      newWebSearches.push(ws);
    }
    saveSettings({ ...settings, web_searches: newWebSearches });
    setEditingIndex(null);
    setIsAddingNew(false);
  };

  const handleDeleteWebSearch = (index: number) => {
    const newWebSearches = settings.web_searches.filter((_, i) => i !== index);
    saveSettings({ ...settings, web_searches: newWebSearches });
    setEditingIndex(null);
  };

  return (
    // WAT-402: panel-level Esc closes Settings — matches the behavior
    // we already have on NoteEditor / Scratchpad / dialogs. tabIndex=-1
    // lets the panel receive keyboard events without being part of the
    // tab order.
    <div
      ref={panelRef}
      role="region"
      aria-label="Settings"
      tabIndex={-1}
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          onClose();
        }
      }}
      className="p-4 border-t border-[var(--border)] max-h-[350px] overflow-y-auto outline-none"
    >
      <div className="flex justify-between items-center mb-4">
        <h3 className="font-semibold">Settings</h3>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close settings"
          className="text-gray-400 hover:text-gray-600 focus:outline-none focus:ring-2 focus:ring-blue-500/50 rounded"
        >
          <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
            <path d="M18 6L6 18M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div className="space-y-4">
        {/* Theme */}
        <div>
          <label className="text-sm text-gray-500 mb-2 block">Theme</label>
          <div className="flex gap-2">
            {(['light', 'dark', 'system'] as const).map((mode) => (
              <button
                key={mode}
                onClick={() => handleThemeChange(mode)}
                className={`px-3 py-1.5 rounded-lg text-sm capitalize transition-colors ${
                  settings.theme.mode === mode
                    ? 'bg-blue-500 text-white'
                    : 'bg-[var(--input-bg)] hover:bg-[var(--selected)]'
                }`}
              >
                {mode}
              </button>
            ))}
          </div>
        </div>

        {/* Web Searches */}
        <div>
          <div className="flex justify-between items-center mb-2">
            <label className="text-sm text-gray-500">Web Searches</label>
            {!isAddingNew && editingIndex === null && (
              <button
                onClick={() => setIsAddingNew(true)}
                className="text-xs text-blue-500 hover:text-blue-600"
              >
                + Add New
              </button>
            )}
          </div>

          <div className="space-y-2">
            {isAddingNew && (
              <WebSearchEditor
                search={null}
                onSave={(ws) => handleSaveWebSearch(ws)}
                onCancel={() => setIsAddingNew(false)}
                reservedPrefixes={reservedPrefixes}
              />
            )}

            {settings.web_searches.map((ws, index) => (
              <div key={`${ws.keyword}-${index}`}>
                {editingIndex === index ? (
                  <WebSearchEditor
                    search={ws}
                    onSave={(updated) => handleSaveWebSearch(updated, index)}
                    onCancel={() => setEditingIndex(null)}
                    onDelete={() => handleDeleteWebSearch(index)}
                    reservedPrefixes={reservedPrefixes}
                  />
                ) : (
                  <div
                    onClick={() => !isAddingNew && setEditingIndex(index)}
                    className="flex items-center justify-between p-2 bg-[var(--input-bg)] rounded-lg cursor-pointer hover:bg-[var(--selected)] transition-colors"
                  >
                    <div className="flex items-center gap-2">
                      <div>
                        <span className="font-medium text-sm">{ws.name}</span>
                        <span className="text-xs text-gray-400 ml-2">({ws.keyword})</span>
                      </div>
                      {ws.url.includes('{instance}') && !ws.instance && (
                        <span className="text-[10px] px-1.5 py-0.5 bg-amber-500/20 text-amber-600 dark:text-amber-400 rounded font-medium">
                          Setup needed
                        </span>
                      )}
                      {reservedPrefixes.includes(ws.keyword) && (
                        <span
                          className="text-[10px] px-1.5 py-0.5 bg-amber-500/20 text-amber-600 dark:text-amber-400 rounded font-medium"
                          title={`"${ws.keyword}" is a reserved query prefix — this search is unreachable.`}
                        >
                          Shadowed
                        </span>
                      )}
                    </div>
                    <svg className="w-4 h-4 text-gray-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path d="M9 18l6-6-6-6" />
                    </svg>
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* Search ranking */}
        <div>
          <label className="text-sm text-gray-500 mb-2 block">Search Ranking</label>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={settings.search.use_frequency_ranking}
              onChange={(e) => {
                saveSettings({
                  ...settings,
                  search: { ...settings.search, use_frequency_ranking: e.target.checked },
                });
              }}
              className="w-4 h-4"
            />
            <span className="text-sm">Prefer frequently-used apps on ties</span>
          </label>
          <p className="text-xs text-gray-400 mt-1 ml-6">
            When two results match your query equally well, show the one you use more often first.
          </p>
        </div>

        {/* File Search */}
        <div>
          <div className="flex justify-between items-center mb-2">
            <label className="text-sm text-gray-500">File Search</label>
            <button
              onClick={() => {
                saveSettings({
                  ...settings,
                  file_search: { ...settings.file_search, enabled: !settings.file_search.enabled }
                });
              }}
              className={`px-2 py-0.5 text-xs rounded transition-colors ${
                settings.file_search.enabled
                  ? 'bg-green-500 text-white'
                  : 'bg-gray-300 text-gray-600'
              }`}
            >
              {settings.file_search.enabled ? 'Enabled' : 'Disabled'}
            </button>
          </div>
          {settings.file_search.enabled && (
            <div className="space-y-2">
              <div className="text-xs text-gray-400">
                Indexed paths: {settings.file_search.indexed_paths.join(', ')}
              </div>
              <button
                onClick={async () => {
                  const count = await reindexFiles();
                  console.log(`Indexed ${count} files`);
                }}
                className="px-3 py-1.5 rounded-lg text-sm bg-[var(--input-bg)] hover:bg-[var(--selected)] transition-colors"
              >
                Re-index Files
              </button>
            </div>
          )}
        </div>

        {/* WAT-301: snippets CRUD lives in its own component to keep
            this file readable. */}
        <SnippetsSettings />

        {/* WAT-303: clipboard privacy filter */}
        <div>
          <label className="text-sm text-gray-500 mb-2 block">Clipboard Ignore Patterns</label>
          <p className="text-xs text-gray-400 mb-2">
            One regex per line. Clipboard content matching any pattern is silently dropped before being recorded
            &mdash; useful for filtering password-manager output, API tokens, or anything else you don&rsquo;t want
            in history. Invalid patterns are ignored; valid ones still apply.
          </p>
          <textarea
            value={(settings.clipboard?.ignore_patterns ?? []).join('\n')}
            onChange={(e) => {
              const lines = e.target.value.split('\n');
              saveSettings({
                ...settings,
                clipboard: { ...settings.clipboard, ignore_patterns: lines },
              });
            }}
            placeholder={`Examples:\n^eyJ[A-Za-z0-9_-]*\\.   (JWTs)\n^[0-9]{13,19}$         (credit-card numbers)`}
            className="w-full h-24 px-3 py-2 text-xs font-mono bg-[var(--input-bg)] border border-[var(--border)] rounded-lg resize-y outline-none focus:ring-1 focus:ring-blue-500"
          />
        </div>

        {/* Actions */}
        <div>
          <label className="text-sm text-gray-500 mb-2 block">Actions</label>
          <div className="flex flex-wrap gap-2">
            <button
              onClick={() => reindexApps()}
              className="px-3 py-1.5 rounded-lg text-sm bg-[var(--input-bg)] hover:bg-[var(--selected)] transition-colors"
            >
              Re-index Applications
            </button>
            <button
              onClick={() => setShowClearClipboardConfirm(true)}
              className="px-3 py-1.5 rounded-lg text-sm bg-[var(--input-bg)] hover:bg-[var(--selected)] transition-colors"
            >
              Clear Clipboard History
            </button>
          </div>
        </div>

        {/* Updates */}
        <div>
          <label className="text-sm text-gray-500 mb-2 block">Updates</label>
          <div className="flex flex-wrap items-center gap-2">
            {updateStatus === 'ready' ? (
              <button
                onClick={handleRelaunch}
                className="px-3 py-1.5 rounded-lg text-sm bg-green-500 text-white hover:bg-green-600 transition-colors"
              >
                Restart to Update
              </button>
            ) : (
              <button
                onClick={checkForUpdates}
                disabled={updateStatus === 'checking' || updateStatus === 'downloading'}
                className="px-3 py-1.5 rounded-lg text-sm bg-[var(--input-bg)] hover:bg-[var(--selected)] transition-colors disabled:opacity-50"
              >
                {updateStatus === 'checking' ? 'Checking...' :
                 updateStatus === 'downloading' ? 'Downloading...' :
                 'Check for Updates'}
              </button>
            )}
            {updateStatus === 'none' && (
              <span className="text-xs text-green-500">Up to date</span>
            )}
            {updateStatus === 'error' && (
              <span className="text-xs text-red-500">{updateError || 'Update failed'}</span>
            )}
          </div>
        </div>

        {/* Help & About */}
        <div className="text-xs text-gray-400 pt-2 border-t border-[var(--border)]">
          <p>Hotkey: Alt+Space</p>
          <p>Quick keys: <span className="text-blue-400 font-mono">n</span> new note • <span className="text-blue-400 font-mono">f</span> files • <span className="text-blue-400 font-mono">s</span> scratchpad</p>
          <p className="mt-2 text-gray-500">Watson v{version}</p>
        </div>
      </div>

      <ConfirmModal
        open={showClearClipboardConfirm}
        title="Clear clipboard history?"
        message="This removes all unpinned clipboard entries. Pinned entries are kept."
        confirmLabel="Clear"
        variant="danger"
        onConfirm={async () => {
          setShowClearClipboardConfirm(false);
          try {
            await invoke('clear_clipboard_history');
          } catch (e) {
            console.error('Failed to clear clipboard:', e);
          }
        }}
        onCancel={() => setShowClearClipboardConfirm(false)}
      />
    </div>
  );
}

function App() {
  const {
    loadSettings,
    reindexApps,
    settings,
    currentPanel,
    setShowSettings,
    resizeWindow,
    loadReservedPrefixes,
    setQuery,
    loadNotifications,
    notificationsUnread,
    setNotificationsOpen,
  } = useAppStore();
  const showSettings = currentPanel === 'settings';
  const notificationsOpen = currentPanel === 'notifications';

  useEffect(() => {
    loadSettings();
    reindexApps();
    loadReservedPrefixes();
    // WAT-406: load notifications + unread count on mount so the
    // header badge reflects reality before the user clicks the bell.
    loadNotifications();
    // WAT-304: populate the empty-query recents carousel on first paint
    // so the user doesn't have to type anything to see their recent apps
    // and files. Fires the same code path as clearing an existing query.
    setQuery('');
    resizeWindow(); // Set initial window size

    // Disable default context menu
    const handleContextMenu = (e: MouseEvent) => {
      e.preventDefault();
    };
    document.addEventListener('contextmenu', handleContextMenu);
    return () => document.removeEventListener('contextmenu', handleContextMenu);
  }, []);

  // Apply theme. When mode === 'system', also subscribe to OS-level
  // theme changes so flipping macOS / Windows light/dark while Watson
  // is open propagates immediately — previously the effect only ran
  // on mount and on settings change, so a runtime OS theme flip was
  // ignored until the next launch.
  useEffect(() => {
    if (!settings) return;

    const { mode } = settings.theme;
    const root = document.documentElement;
    const mql = window.matchMedia('(prefers-color-scheme: dark)');

    const applyMode = () => {
      if (mode === 'system') {
        root.classList.toggle('dark', mql.matches);
      } else {
        root.classList.toggle('dark', mode === 'dark');
      }
    };

    applyMode();

    if (mode !== 'system') {
      // Explicit light/dark choice — no need to listen to the OS.
      return;
    }
    // Track OS preference changes only while in 'system' mode. Older
    // Safari uses addListener; addEventListener is the modern path
    // and supported by all Tauri-target browsers (Chromium / WebKit).
    mql.addEventListener('change', applyMode);
    return () => mql.removeEventListener('change', applyMode);
  }, [settings?.theme.mode]);

  return (
    // Aero-glass + Hexclad-mesh exploration. The `glass-surface-dark`
    // class layers four backgrounds (inner top highlight, amber corner
    // glow, hex texture, base vertical gradient) for depth without OS
    // transparency. `glass-rim-dark` adds the 1px light-catching top
    // edge + dark bottom shadow via a pseudo-element. Falls back to
    // the existing `--background` token in light mode (light-glass
    // treatment is a follow-up if this direction lands).
    <div className="glass-surface-dark glass-rim-dark text-[var(--foreground)] rounded-xl overflow-hidden border border-[var(--border)] shadow-2xl">
      {/* Header - draggable */}
      <div
        data-tauri-drag-region
        onMouseDown={async (e) => {
          // Only start drag if clicking on the header itself, not buttons
          if ((e.target as HTMLElement).closest('button')) return;
          e.preventDefault();
          try {
            await getCurrentWindow().startDragging();
          } catch (err) {
            console.error('Failed to start dragging:', err);
          }
        }}
        className="flex items-center justify-between px-4 py-3 border-b border-[var(--border)] cursor-move select-none"
      >
        <div className="flex items-center gap-2 pointer-events-none">
          <WatsonLogo />
          <span className="text-lg font-semibold">Watson</span>
        </div>
        <div className="flex items-center gap-1">
          <NotificationsBell
            count={notificationsUnread}
            onClick={() => setNotificationsOpen(!notificationsOpen)}
          />
          <SettingsIcon onClick={() => setShowSettings(!showSettings)} />
        </div>
      </div>

      <StartupWarningBanner onOpenSettings={() => setShowSettings(true)} />

      <SearchBar />

      <PanelHost
        panel={currentPanel}
        settingsPanel={<SettingsPanel onClose={() => setShowSettings(false)} />}
      />

    </div>
  );
}

export default App;
