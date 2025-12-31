import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { SearchBar } from './components/SearchBar';
import { ResultsList } from './components/ResultsList';
import { Scratchpad } from './components/Scratchpad';
import { useAppStore } from './stores/app';
import type { WebSearch } from './types';

// Watson's iconic bowler hat
function WatsonLogo() {
  return (
    <svg className="w-8 h-8" viewBox="0 0 32 32" fill="none">
      {/* Bowler hat */}
      <ellipse cx="16" cy="22" rx="14" ry="3" className="fill-gray-700" />
      <path d="M6 22c0-8 4-14 10-14s10 6 10 14" className="fill-gray-800" />
      <ellipse cx="16" cy="8" rx="6" ry="2" className="fill-gray-700" />
      {/* Hat band */}
      <rect x="8" y="18" width="16" height="2" rx="0.5" className="fill-amber-600" />
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

function WebSearchEditor({
  search,
  onSave,
  onCancel,
  onDelete
}: {
  search: WebSearch | null;
  onSave: (ws: WebSearch) => void;
  onCancel: () => void;
  onDelete?: () => void;
}) {
  const [name, setName] = useState(search?.name || '');
  const [keyword, setKeyword] = useState(search?.keyword || '');
  const [url, setUrl] = useState(search?.url || '');
  const [instance, setInstance] = useState(search?.instance || '');

  // Check if URL template uses {instance} placeholder
  const needsInstance = url.includes('{instance}');
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
  const { settings, saveSettings, reindexApps } = useAppStore();
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [isAddingNew, setIsAddingNew] = useState(false);
  const [version, setVersion] = useState('');
  const [updateStatus, setUpdateStatus] = useState<'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'none' | 'error'>('idle');
  const [updateError, setUpdateError] = useState('');

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
    <div className="p-4 border-t border-[var(--border)] max-h-[350px] overflow-y-auto">
      <div className="flex justify-between items-center mb-4">
        <h3 className="font-semibold">Settings</h3>
        <button onClick={onClose} className="text-gray-400 hover:text-gray-600">
          <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
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
              onClick={async () => {
                try {
                  await invoke('clear_clipboard_history');
                } catch (e) {
                  console.error('Failed to clear clipboard:', e);
                }
              }}
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
          <p>Type <span className="text-blue-400 font-mono">cb</span> for clipboard history</p>
          <p className="mt-2 text-gray-500">Watson v{version}</p>
        </div>
      </div>
    </div>
  );
}

function App() {
  const { loadSettings, reindexApps, settings, showSettings, setShowSettings, resizeWindow, scratchpadVisible } = useAppStore();

  useEffect(() => {
    loadSettings();
    reindexApps();
    resizeWindow(); // Set initial window size

    // Disable default context menu
    const handleContextMenu = (e: MouseEvent) => {
      e.preventDefault();
    };
    document.addEventListener('contextmenu', handleContextMenu);
    return () => document.removeEventListener('contextmenu', handleContextMenu);
  }, []);

  // Apply theme
  useEffect(() => {
    if (!settings) return;

    const { mode } = settings.theme;
    const root = document.documentElement;

    if (mode === 'system') {
      const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      root.classList.toggle('dark', isDark);
    } else {
      root.classList.toggle('dark', mode === 'dark');
    }
  }, [settings?.theme.mode]);

  return (
    <div className="bg-[var(--background)] text-[var(--foreground)] rounded-xl overflow-hidden border border-[var(--border)] shadow-2xl">
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
        <SettingsIcon onClick={() => setShowSettings(!showSettings)} />
      </div>

      <SearchBar />

      {scratchpadVisible ? (
        <Scratchpad />
      ) : showSettings ? (
        <SettingsPanel onClose={() => setShowSettings(false)} />
      ) : (
        <ResultsList />
      )}
    </div>
  );
}

export default App;
