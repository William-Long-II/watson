import '@testing-library/jest-dom/vitest';
import { vi, afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';

// Global mock for the Tauri invoke bridge. Individual tests that care about
// invoke behavior override via vi.mocked(invoke).mockImplementation(...).
// Without a stub here, any store action that calls invoke() during a test
// rejects with "window.__TAURI_INTERNALS__ is undefined".
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => undefined),
}));

// Also stub window controls so store.resizeWindow() and friends don't throw.
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn(() => ({
    startDragging: vi.fn(async () => {}),
    show: vi.fn(async () => {}),
    hide: vi.fn(async () => {}),
    setFocus: vi.fn(async () => {}),
  })),
}));

afterEach(() => {
  cleanup();
});
