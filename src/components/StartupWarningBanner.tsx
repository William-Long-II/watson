import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../stores/app';
import type { StartupWarning } from '../types';

interface Props {
  /**
   * Injected action for "Change hotkey" to keep the banner decoupled from
   * the Settings panel's open/close state owner. App passes the real
   * `setShowSettings(true)` wrapper.
   */
  onOpenSettings: () => void;
}

/**
 * WAT-105: renders a banner for each startup warning surfaced by the Rust
 * backend. Invisible when there are no warnings. Each banner has a dismiss
 * button; hotkey-related warnings additionally offer "Change hotkey" which
 * opens the Settings panel.
 */
export function StartupWarningBanner({ onOpenSettings }: Props) {
  const { startupWarnings, loadStartupWarnings, dismissStartupWarning } = useAppStore();

  useEffect(() => {
    loadStartupWarnings();
  }, [loadStartupWarnings]);

  if (startupWarnings.length === 0) return null;

  return (
    <ul
      role="alert"
      aria-live="polite"
      className="border-b border-[var(--border)] bg-amber-50 dark:bg-amber-900/20"
    >
      {startupWarnings.map((warning) => (
        <WarningRow
          key={warning.id}
          warning={warning}
          onDismiss={() => dismissStartupWarning(warning.id)}
          onOpenSettings={onOpenSettings}
        />
      ))}
    </ul>
  );
}

function WarningRow({
  warning,
  onDismiss,
  onOpenSettings,
}: {
  warning: StartupWarning;
  onDismiss: () => void;
  onOpenSettings: () => void;
}) {
  return (
    <li className="flex items-start gap-2 px-4 py-2 text-xs text-amber-900 dark:text-amber-100">
      <svg
        className="w-4 h-4 mt-0.5 shrink-0"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        aria-hidden="true"
      >
        <path d="M12 9v4m0 4h.01" />
        <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0Z" />
      </svg>

      <p className="flex-1 leading-snug">{warning.message}</p>

      {warning.kind === 'shortcut_unavailable' && (
        <button
          type="button"
          onClick={onOpenSettings}
          className="text-blue-600 dark:text-blue-400 hover:underline whitespace-nowrap"
        >
          Change hotkey
        </button>
      )}

      {warning.kind === 'settings_from_newer_version' && (
        <button
          type="button"
          onClick={() => {
            // WAT-206: open the config dir in the OS file browser. Best-
            // effort — surface errors to the console so users aren't
            // left wondering why nothing happened. WAT-406 will move this
            // to the notifications drawer.
            invoke('open_config_folder').catch((err) =>
              console.error('Failed to open config folder:', err),
            );
          }}
          className="text-blue-600 dark:text-blue-400 hover:underline whitespace-nowrap"
        >
          Open config folder
        </button>
      )}

      {warning.kind === 'accessibility_permission_denied' && (
        <AccessibilityPermissionActions />
      )}

      <button
        type="button"
        onClick={onDismiss}
        aria-label={`Dismiss warning: ${warning.message}`}
        className="shrink-0 text-amber-900/60 hover:text-amber-900 dark:text-amber-100/60 dark:hover:text-amber-100"
      >
        <svg
          className="w-4 h-4"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          aria-hidden="true"
        >
          <path d="M18 6 6 18M6 6l12 12" />
        </svg>
      </button>
    </li>
  );
}

/**
 * macOS-only AX permission actions: opens the Accessibility pane in
 * System Settings, then re-probes on the user's "Re-check" click. The
 * re-probe clears its own banner on success via
 * `clear_accessibility_permission_denied` in the backend, so we just
 * call `loadStartupWarnings` to refresh the list.
 */
function AccessibilityPermissionActions() {
  const { loadStartupWarnings } = useAppStore();
  return (
    <>
      <button
        type="button"
        onClick={() =>
          invoke('open_accessibility_settings').catch((err) =>
            console.error('Failed to open Accessibility settings:', err),
          )
        }
        className="text-blue-600 dark:text-blue-400 hover:underline whitespace-nowrap"
      >
        Grant access
      </button>
      <button
        type="button"
        onClick={async () => {
          try {
            await invoke<boolean>('recheck_accessibility_permission');
          } catch (err) {
            console.error('Failed to re-check Accessibility permission:', err);
          } finally {
            // Refresh either way — backend clears the warning when
            // trusted, re-records when still denied, so the banner
            // list always reflects current state after this call.
            loadStartupWarnings();
          }
        }}
        className="text-blue-600 dark:text-blue-400 hover:underline whitespace-nowrap"
      >
        Re-check
      </button>
    </>
  );
}
