import { invoke } from '@tauri-apps/api/core';
import type { SearchResult } from '../types';

/**
 * WAT-404: secondary actions per result type.
 *
 * The primary action is whatever Enter does (launch app, open file,
 * paste snippet, etc.). Secondary actions are the "what else might I
 * want to do with this result" set, surfaced via the Cmd+K menu.
 *
 * Returning `[]` means the menu is hidden for that result type — keeps
 * the UX clean for results where there's nothing useful beyond Enter
 * (system commands, calculations).
 */
export interface SecondaryAction {
  /** Display label in the menu. */
  label: string;
  /** Run the action; resolves when the IPC completes. */
  run: () => Promise<void>;
}

export function getSecondaryActions(result: SearchResult): SecondaryAction[] {
  const a = result.action;
  switch (a.type) {
    case 'launch_app':
      return [
        {
          label: 'Reveal in folder',
          run: async () => {
            await invoke('reveal_file', { path: a.path });
          },
        },
        {
          label: 'Copy path',
          run: async () => {
            await invoke('copy_to_clipboard', { content: a.path });
          },
        },
      ];

    case 'open_file':
      return [
        {
          label: 'Reveal in folder',
          run: async () => {
            await invoke('reveal_file', { path: a.path });
          },
        },
        {
          label: 'Copy path',
          run: async () => {
            await invoke('copy_to_clipboard', { content: a.path });
          },
        },
      ];

    case 'open_url':
      return [
        {
          label: 'Copy URL',
          run: async () => {
            await invoke('copy_to_clipboard', { content: a.url });
          },
        },
      ];

    case 'paste_snippet':
      return [
        {
          label: 'Copy expansion (no paste)',
          run: async () => {
            await invoke('copy_to_clipboard', { content: a.expansion });
          },
        },
      ];

    // Primary action is already "copy" for these — no useful secondary.
    case 'copy_clipboard':
    case 'run_command':
    case 'open_note':
    default:
      return [];
  }
}
