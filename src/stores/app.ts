import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { SearchResult, Settings, Scratchpad, Note } from '../types';

interface AppState {
  query: string;
  results: SearchResult[];
  selectedIndex: number;
  settings: Settings | null;
  isLoading: boolean;
  showSettings: boolean;
  scratchpad: string;
  scratchpadVisible: boolean;
  currentNote: Note | null;
  noteEditorVisible: boolean;

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
  reindexFiles: () => Promise<number>;
}

// Height constants
const HEADER_HEIGHT = 56;
const SEARCH_HEIGHT = 56;
const RESULT_HEIGHT = 64;
const SETTINGS_HEIGHT = 350;
const EMPTY_STATE_HEIGHT = 110; // Height for quick tips
const PADDING = 28; // Extra padding for rounded corners
const MIN_HEIGHT = HEADER_HEIGHT + SEARCH_HEIGHT + EMPTY_STATE_HEIGHT + PADDING;
const MAX_RESULTS_HEIGHT = 320;

export const useAppStore = create<AppState>((set, get) => ({
  query: '',
  results: [],
  selectedIndex: 0,
  settings: null,
  isLoading: false,
  showSettings: false,
  scratchpad: '',
  scratchpadVisible: false,
  currentNote: null,
  noteEditorVisible: false,

  setQuery: async (query: string) => {
    set({ query, selectedIndex: 0 });

    if (!query.trim()) {
      set({ results: [] });
      get().resizeWindow();
      return;
    }

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
    set({ selectedIndex: newIndex });
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
    const { results, selectedIndex, hideWindow } = get();
    const selected = results[selectedIndex];

    if (!selected) return;

    try {
      // Hide window first, then execute action
      set({ query: '', results: [], selectedIndex: 0 });
      await hideWindow();
      await invoke('execute_action', { action: selected.action });
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
    set({ showSettings: show });
    get().resizeWindow();
  },

  resizeWindow: async () => {
    const { results, showSettings, scratchpadVisible, noteEditorVisible } = get();

    let height: number;

    if (noteEditorVisible) {
      height = HEADER_HEIGHT + SEARCH_HEIGHT + 350 + PADDING; // Note editor height
    } else if (scratchpadVisible) {
      height = HEADER_HEIGHT + SEARCH_HEIGHT + 280 + PADDING; // Scratchpad height
    } else if (showSettings) {
      height = HEADER_HEIGHT + SEARCH_HEIGHT + SETTINGS_HEIGHT + PADDING;
    } else if (results.length > 0) {
      const resultsHeight = Math.min(results.length * RESULT_HEIGHT, MAX_RESULTS_HEIGHT);
      height = HEADER_HEIGHT + SEARCH_HEIGHT + resultsHeight + PADDING;
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
    set({ scratchpadVisible: show, showSettings: false });
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
      set({ currentNote: null, noteEditorVisible: false });
      get().resizeWindow();
    } catch (e) {
      console.error('Failed to delete note:', e);
    }
  },

  openNote: async (noteId: string) => {
    try {
      const note = await invoke<Note | null>('get_note', { id: noteId });
      if (note) {
        set({ currentNote: note, noteEditorVisible: true, showSettings: false, scratchpadVisible: false });
        get().resizeWindow();
      }
    } catch (e) {
      console.error('Failed to open note:', e);
    }
  },

  closeNoteEditor: () => {
    set({ noteEditorVisible: false, currentNote: null });
    get().resizeWindow();
  },

  openNewNote: () => {
    set({
      currentNote: null,
      noteEditorVisible: true,
      showSettings: false,
      scratchpadVisible: false
    });
    get().resizeWindow();
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
}));
