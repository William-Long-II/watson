import { useEffect, useRef } from 'react';

/**
 * WAT-405: reusable confirmation modal for destructive actions.
 *
 * Replaces native `window.confirm`, which fights Tauri window
 * transparency/focus on Windows and feels out of place against the
 * themed launcher UI. The modal is rendered as an in-app overlay with:
 *
 * - Focus trap: the cancel and confirm buttons are keyboard-tabbable
 *   but focus can't escape to elements behind the overlay.
 * - Escape → cancel.
 * - Enter → confirm (fires the action).
 * - `role="dialog"` + `aria-modal="true"` + `aria-labelledby` so
 *   screen readers announce the modal and the action title.
 * - `variant: 'danger'` tints the confirm button red for destructive
 *   actions like Delete.
 *
 * Usage:
 * ```tsx
 * const [open, setOpen] = useState(false);
 * <ConfirmModal
 *   open={open}
 *   title="Delete this note?"
 *   message="This can't be undone."
 *   confirmLabel="Delete"
 *   variant="danger"
 *   onConfirm={() => { doDelete(); setOpen(false); }}
 *   onCancel={() => setOpen(false)}
 * />
 * ```
 */
export interface ConfirmModalProps {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** `primary` uses blue; `danger` uses red. Default `primary`. */
  variant?: 'primary' | 'danger';
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmModal({
  open,
  title,
  message,
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  variant = 'primary',
  onConfirm,
  onCancel,
}: ConfirmModalProps) {
  const confirmRef = useRef<HTMLButtonElement>(null);
  const overlayRef = useRef<HTMLDivElement>(null);

  // Focus the confirm button when the modal opens — the user is
  // there deliberately and Enter should fire the action. Tab cycles
  // to cancel; Shift-Tab cycles back.
  useEffect(() => {
    if (open) {
      confirmRef.current?.focus();
    }
  }, [open]);

  if (!open) return null;

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    } else if (e.key === 'Enter') {
      // Enter always confirms regardless of which button has focus —
      // standard dialog behavior. If the user wanted cancel they'd
      // Escape.
      e.preventDefault();
      onConfirm();
    } else if (e.key === 'Tab') {
      // Focus trap: cycle between our two buttons only. Without this
      // the user's Tab could fall through into the launcher input
      // behind the overlay.
      const buttons = overlayRef.current?.querySelectorAll<HTMLButtonElement>('button');
      if (!buttons || buttons.length === 0) return;
      const first = buttons[0];
      const last = buttons[buttons.length - 1];
      const active = document.activeElement;
      if (e.shiftKey && active === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    }
  };

  const confirmColor =
    variant === 'danger'
      ? 'bg-red-500 hover:bg-red-600 text-white'
      : 'bg-blue-500 hover:bg-blue-600 text-white';

  return (
    <div
      ref={overlayRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-modal-title"
      onKeyDown={handleKeyDown}
      // Backdrop click cancels — matches every launcher convention.
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm"
    >
      <div
        className="max-w-sm mx-4 p-4 bg-[var(--background)] border border-[var(--border)] rounded-xl shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 id="confirm-modal-title" className="text-sm font-semibold mb-1">
          {title}
        </h3>
        <p className="text-xs text-gray-500 mb-3">{message}</p>
        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="px-3 py-1.5 text-sm rounded-lg bg-[var(--input-bg)] hover:bg-[var(--selected)] transition-colors"
          >
            {cancelLabel}
          </button>
          <button
            ref={confirmRef}
            type="button"
            onClick={onConfirm}
            className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${confirmColor}`}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
