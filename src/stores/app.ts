import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type {
  SearchResult,
  Settings,
  Scratchpad,
  Note,
  StartupWarning,
  Notification,
} from '../types';

/**
 * Mutually-exclusive secondary panels rendered below the search bar
 * (above `<ResultsList />`). Exactly one panel — or none, in which
 * case `<ResultsList />` shows — is active at a time. `<PanelHost>`
 * does the rendering; setters here mutate `currentPanel` directly so
 * we never carry stale "visible" booleans across opens.
 */
export type PanelId = 'settings' | 'scratchpad' | 'noteEditor' | 'notifications';

interface AppState {
  query: string;
  results: SearchResult[];
  selectedIndex: number;
  settings: Settings | null;
  isLoading: boolean;
  /** Active secondary panel, or `null` when results are showing. */
  currentPanel: PanelId | null;
  scratchpad: string;
  currentNote: Note | null;
  startupWarnings: StartupWarning[];
  reservedPrefixes: string[];
  notifications: Notification[];
  notificationsUnread: number;
  /** WAT-404: when true, the per-result Cmd+K secondary-action menu is open
      for `results[selectedIndex]`. */
  actionMenuOpen: boolean;

  setQuery: (query: string) => void;
  setSelectedIndex: (index: number) => void;
  moveSelection: (delta: number) => void;
  loadSettings: () => Promise<void>;
  saveSettings: (settings: Settings) => Promise<void>;
  executeSelected: () => Promise<void>;
  reindexApps: () => Promise<void>;
  hideWindow: () => Promise<void>;
  setShowSettings: (show: boolean) => void;
  resizeWindow: () => Promise<void>;
  loadScratchpad: () => Promise<void>;
  saveScratchpad: (content: string) => Promise<void>;
  clearScratchpad: () => Promise<void>;
  setShowScratchpad: (show: boolean) => void;
  createNote: (title: string, content: string) => Promise<Note | null>;
  updateNote: (id: string, title: string, content: string) => Promise<Note | null>;
  deleteNote: (id: string) => Promise<void>;
  openNote: (noteId: string) => Promise<void>;
  closeNoteEditor: () => void;
  openNewNote: () => void;
  reloadNoteFromDisk: (id: string) => Promise<Note | null>;
  reindexFiles: () => Promise<number>;
  loadStartupWarnings: () => Promise<void>;
  dismissStartupWarning: (id: string) => Promise<void>;
  loadReservedPrefixes: () => Promise<void>;
  pinClipboardEntry: (id: string) => Promise<void>;
  unpinClipboardEntry: (id: string) => Promise<void>;
  loadNotifications: () => Promise<void>;
  setNotificationsOpen: (open: boolean) => void;
  dismissNotification: (id: string) => Promise<void>;
  dismissAllNotifications: () => Promise<void>;
  toggleActionMenu: () => void;
  closeActionMenu: () => void;
}

// Height constants
const HEADER_HEIGHT = 56;
const SEARCH_HEIGHT = 56;
const RESULT_HEIGHT = 64;
const SETTINGS_HEIGHT = 350;
// Height for the Quick Tips empty state. Includes p-4 padding (32),
// the "Quick Tips" header (~16), three tip lines at text-xs with
// `space-y-0.5` between them (~52), and the `space-y-1` gap between
// header and tips block (~4). Total content ≈ 104px; 115 leaves a
// small buffer for line-height variance across platforms without
// the dark band beneath that 150 produced. The earlier 110→150
// bump was diagnosing the missing resizeWindow() on app launch
// (now fixed in executeSelected) — the empty-state content itself
// was always ~104px; the window was just stuck at the prior
// results-mode height when reopened.
const EMPTY_STATE_HEIGHT = 115;
const PADDING = 28; // Extra padding for rounded corners
// On empty queries with recents present, ResultsList renders a small
// "RECENTS" label inside the same scrollable region as the rows. The
// label is `px-4 py-1.5` (~24px tall including line-height) and sits
// ABOVE the rows, so the resize calculation has to add it on top of
// `results.length * RESULT_HEIGHT` or the bottom row gets clipped.
const RECENTS_HEADER_HEIGHT = 24;
const MIN_HEIGHT = HEADER_HEIGHT + SEARCH_HEIGHT + EMPTY_STATE_HEIGHT + PADDING;
const MAX_RESULTS_HEIGHT = 320;

export const useAppStore = create<AppState>((set, get) => ({
  query: '',
  results: [],
  selectedIndex: 0,
  settings: null,
  isLoading: false,
  currentPanel: null,
  scratchpad: '',
  currentNote: null,
  startupWarnings: [],
  reservedPrefixes: [],
  notifications: [],
  notificationsUnread: 0,
  actionMenuOpen: false,

  setQuery: async (query: string) => {
    // Frontend-only shortcut: 's ' (s + trailing space) opens the
    // scratchpad. Single-letter `s` no longer intercepts because it
    // collided with searches starting with that letter (a tab named
    // "Slack" was unreachable). The `s ` form is also rare enough in
    // real text that the shortcut still feels deliberate. Bare
    // backtick remains a one-key trigger; handled in SearchBar so it
    // never enters the query at all.
    if (query === 's ') {
      set({ query: '', selectedIndex: 0, results: [], actionMenuOpen: false });
      get().setShowScratchpad(true);
      return;
    }

    // WAT-404: changing the query invalidates whatever menu was open
    // for the prior selection.
    set({ query, selectedIndex: 0, actionMenuOpen: false });

    // WAT-304: empty queries are now meaningful — the backend returns a
    // "Recents" view (recently-launched apps + recently-opened files).
    // Drop the pre-WAT-304 short-circuit so the query='' path calls
    // through. On a fresh install the backend returns []; the UI then
    // renders the quick-tips placeholder as before.
    try {
      const results = await invoke<SearchResult[]>('search', { query });
      set({ results });
      get().resizeWindow();
    } catch (error) {
      console.error('Search error:', error);
      set({ results: [] });
      get().resizeWindow();
    }
  },

  setSelectedIndex: (index: number) => {
    const { results } = get();
    if (index >= 0 && index < results.length) {
      set({ selectedIndex: index });
    }
  },

  moveSelection: (delta: number) => {
    const { selectedIndex, results } = get();
    const newIndex = Math.max(0, Math.min(results.length - 1, selectedIndex + delta));
    // WAT-404: close any open action menu when the user navigates to a
    // different row — otherwise the menu would attach to a row the user
    // isn't pointing at anymore.
    set({ selectedIndex: newIndex, actionMenuOpen: false });
  },

  loadSettings: async () => {
    try {
      const settings = await invoke<Settings>('get_settings');
      set({ settings });
    } catch (error) {
      console.error('Failed to load settings:', error);
    }
  },

  saveSettings: async (settings: Settings) => {
    try {
      await invoke('save_settings_cmd', { settings });
      set({ settings });
    } catch (error) {
      console.error('Failed to save settings:', error);
    }
  },

  executeSelected: async () => {
    const { results, selectedIndex, hideWindow, openNote } = get();
    const selected = results[selectedIndex];

    if (!selected) return;

    try {
      // Handle note actions specially - open in editor instead of invoking backend
      if (selected.action.type === 'open_note') {
        set({ query: '', results: [], selectedIndex: 0 });
        await openNote(selected.action.note_id);
        return;
      }

      // The "Create new note" pseudo-result in the notes route fires this.
      // Frontend-only action: clear the query and open the editor with a
      // blank note.
      if (selected.action.type === 'create_new_note') {
        set({ query: '', results: [], selectedIndex: 0 });
        get().openNewNote();
        return;
      }

      // The "Re-index files now" pseudo-result. Run the backend
      // re-index, then re-fire the current notes/files query so the
      // user immediately sees whatever just got indexed (or, if
      // nothing did, the same affordance is still there to retry).
      if (selected.action.type === 'reindex_files') {
        const currentQuery = get().query;
        try {
          await invoke('execute_action', { action: selected.action });
        } catch (e) {
          console.error('Re-index files failed:', e);
        }
        // Refresh — fire the same query through the pipeline so
        // freshly-indexed files surface without the user retyping.
        await get().setQuery(currentQuery);
        return;
      }

      // Hide window first, then execute action (except for focus_window which needs foreground)
      if (selected.action.type === 'focus_window') {
        await invoke('execute_action', { action: selected.action });
        set({ query: '', results: [], selectedIndex: 0 });
        await get().resizeWindow();
        await hideWindow();
      } else {
        set({ query: '', results: [], selectedIndex: 0 });
        // Resize BEFORE hiding so the next show comes back at the
        // empty-state height instead of the stale results-mode height
        // (otherwise reopening via Alt+Space shows Quick Tips with a
        // huge dark band beneath, sized to the prior result list).
        await get().resizeWindow();
        await hideWindow();
        await invoke('execute_action', { action: selected.action });
      }
    } catch (error) {
      console.error('Failed to execute action:', error);
    }
  },

  reindexApps: async () => {
    set({ isLoading: true });
    try {
      await invoke('reindex_apps');
    } catch (error) {
      console.error('Failed to reindex:', error);
    }
    set({ isLoading: false });
  },

  hideWindow: async () => {
    try {
      await invoke('hide_window');
    } catch (error) {
      console.error('Failed to hide window:', error);
    }
  },

  setShowSettings: (show: boolean) => {
    set({ currentPanel: show ? 'settings' : null });
    get().resizeWindow();
  },

  resizeWindow: async () => {
    const { results, currentPanel, query } = get();

    let height: number;

    if (currentPanel === 'noteEditor') {
      height = HEADER_HEIGHT + SEARCH_HEIGHT + 350 + PADDING; // Note editor height
    } else if (currentPanel === 'scratchpad') {
      height = HEADER_HEIGHT + SEARCH_HEIGHT + 280 + PADDING; // Scratchpad height
    } else if (currentPanel === 'settings') {
      height = HEADER_HEIGHT + SEARCH_HEIGHT + SETTINGS_HEIGHT + PADDING;
    } else if (results.length > 0) {
      const resultsHeight = Math.min(results.length * RESULT_HEIGHT, MAX_RESULTS_HEIGHT);
      // The empty-query path ("Recents") renders a small label ABOVE
      // the result rows inside the same scrollable region. Add its
      // height so the bottom row doesn't get clipped at the window
      // edge — that was the cutoff bug visible in early Phase 1B
      // captures.
      const recentsHeader = !query.trim() ? RECENTS_HEADER_HEIGHT : 0;
      height = HEADER_HEIGHT + SEARCH_HEIGHT + recentsHeader + resultsHeight + PADDING;
    } else {
      height = MIN_HEIGHT;
    }

    try {
      await invoke('resize_window', { height });
    } catch (error) {
      console.error('Failed to resize window:', error);
    }
  },

  loadScratchpad: async () => {
    try {
      const pad = await invoke<Scratchpad>('get_scratchpad');
      set({ scratchpad: pad.content });
    } catch (e) {
      console.error('Failed to load scratchpad:', e);
    }
  },

  saveScratchpad: async (content: string) => {
    try {
      await invoke('set_scratchpad', { content });
      set({ scratchpad: content });
    } catch (e) {
      console.error('Failed to save scratchpad:', e);
    }
  },

  clearScratchpad: async () => {
    try {
      await invoke('clear_scratchpad');
      set({ scratchpad: '' });
    } catch (e) {
      console.error('Failed to clear scratchpad:', e);
    }
  },

  setShowScratchpad: (show: boolean) => {
    set({ currentPanel: show ? 'scratchpad' : null });
    get().resizeWindow();
  },

  createNote: async (title: string, content: string) => {
    try {
      const note = await invoke<Note>('create_note', { title, content });
      return note;
    } catch (e) {
      console.error('Failed to create note:', e);
      return null;
    }
  },

  updateNote: async (id: string, title: string, content: string) => {
    try {
      const note = await invoke<Note>('update_note', { id, title, content });
      set({ currentNote: note });
      return note;
    } catch (e) {
      console.error('Failed to update note:', e);
      return null;
    }
  },

  deleteNote: async (id: string) => {
    try {
      await invoke('delete_note', { id });
      set({ currentNote: null, currentPanel: null });
      get().resizeWindow();
    } catch (e) {
      console.error('Failed to delete note:', e);
    }
  },

  openNote: async (noteId: string) => {
    try {
      const note = await invoke<Note | null>('get_note', { id: noteId });
      if (note) {
        set({ currentNote: note, currentPanel: 'noteEditor' });
        get().resizeWindow();
      }
    } catch (e) {
      console.error('Failed to open note:', e);
    }
  },

  closeNoteEditor: () => {
    set({ currentPanel: null, currentNote: null });
    get().resizeWindow();
  },

  openNewNote: () => {
    set({ currentNote: null, currentPanel: 'noteEditor' });
    get().resizeWindow();
  },

  reloadNoteFromDisk: async (id: string) => {
    try {
      const note = await invoke<Note>('reload_note_from_disk', { id });
      // Update the in-editor state so the banner disappears and the
      // editor's content reflects the disk version immediately.
      set({ currentNote: note });
      return note;
    } catch (e) {
      console.error('Failed to reload note from disk:', e);
      return null;
    }
  },

  reindexFiles: async () => {
    try {
      const count = await invoke<number>('reindex_files');
      return count;
    } catch (e) {
      console.error('Failed to reindex files:', e);
      return 0;
    }
  },

  loadStartupWarnings: async () => {
    try {
      const warnings = await invoke<StartupWarning[]>('get_startup_warnings');
      set({ startupWarnings: warnings });
    } catch (e) {
      console.error('Failed to load startup warnings:', e);
    }
  },

  dismissStartupWarning: async (id: string) => {
    // Optimistic update: remove locally first so the banner doesn't linger
    // while we await the backend confirmation.
    set({ startupWarnings: get().startupWarnings.filter((w) => w.id !== id) });
    try {
      await invoke('dismiss_startup_warning', { id });
    } catch (e) {
      console.error('Failed to dismiss startup warning:', e);
    }
  },

  loadReservedPrefixes: async () => {
    try {
      const prefixes = await invoke<string[]>('get_reserved_prefixes');
      set({ reservedPrefixes: prefixes });
    } catch (e) {
      console.error('Failed to load reserved prefixes:', e);
    }
  },

  // WAT-303: pin / unpin flips the flag locally (optimistic) then
  // fires the IPC. If the IPC fails we log and leave local state as-is;
  // the next search refreshes from the authoritative backend.
  pinClipboardEntry: async (id: string) => {
    set({
      results: get().results.map((r) => (r.id === id ? { ...r, pinned: true } : r)),
    });
    try {
      await invoke('pin_clipboard_entry', { id });
      // Refresh current view so the ordering reflects the new pin.
      await get().setQuery(get().query);
    } catch (e) {
      console.error('Failed to pin clipboard entry:', e);
    }
  },

  unpinClipboardEntry: async (id: string) => {
    set({
      results: get().results.map((r) => (r.id === id ? { ...r, pinned: false } : r)),
    });
    try {
      await invoke('unpin_clipboard_entry', { id });
      await get().setQuery(get().query);
    } catch (e) {
      console.error('Failed to unpin clipboard entry:', e);
    }
  },

  // WAT-406: notifications drawer. Keep the unread count in the store
  // so the header badge updates without an extra IPC on every render.
  loadNotifications: async () => {
    try {
      const [list, unread] = await Promise.all([
        invoke<Notification[]>('list_notifications'),
        invoke<number>('notifications_unread_count'),
      ]);
      set({ notifications: list, notificationsUnread: unread });
    } catch (e) {
      console.error('Failed to load notifications:', e);
    }
  },

  setNotificationsOpen: (open: boolean) => {
    set({ currentPanel: open ? 'notifications' : null });
    if (open) {
      // Re-fetch when the drawer opens so late-arriving notifications
      // (e.g. an update check that just failed) appear without a
      // separate refresh affordance.
      void get().loadNotifications();
    }
  },

  dismissNotification: async (id: string) => {
    // Optimistic: mark dismissed locally, decrement unread.
    const current = get().notifications;
    const next = current.map((n) =>
      n.id === id ? { ...n, dismissed: true } : n,
    );
    const unread = next.filter((n) => !n.dismissed).length;
    set({ notifications: next, notificationsUnread: unread });
    try {
      await invoke('dismiss_notification', { id });
    } catch (e) {
      console.error('Failed to dismiss notification:', e);
    }
  },

  dismissAllNotifications: async () => {
    const next = get().notifications.map((n) => ({ ...n, dismissed: true }));
    set({ notifications: next, notificationsUnread: 0 });
    try {
      await invoke('dismiss_all_notifications');
    } catch (e) {
      console.error('Failed to dismiss all notifications:', e);
    }
  },

  // WAT-404: action menu only meaningful when there is something to act
  // on. Toggle is a no-op for the empty-results case.
  toggleActionMenu: () => {
    const { results, selectedIndex } = get();
    if (results.length === 0 || results[selectedIndex] === undefined) {
      set({ actionMenuOpen: false });
      return;
    }
    set({ actionMenuOpen: !get().actionMenuOpen });
  },

  closeActionMenu: () => set({ actionMenuOpen: false }),
}));
