import { useAppStore } from '../stores/app';
import type { Notification, NotificationSeverity } from '../types';

/**
 * WAT-406: drawer panel listing non-fatal notifications.
 *
 * Rendered below the search bar when `notificationsOpen` is true (the
 * store toggle the header bell icon drives). Complementary to the
 * WAT-105 startup-warning banner — startup warnings push HERE too so
 * dismissing the loud banner doesn't erase the record.
 */
export function NotificationsDrawer() {
  const { notifications, notificationsOpen, dismissNotification, dismissAllNotifications, setNotificationsOpen } =
    useAppStore();

  if (!notificationsOpen) return null;

  const active = notifications.filter((n) => !n.dismissed);
  const dismissed = notifications.filter((n) => n.dismissed);

  return (
    <div
      role="region"
      aria-label="Notifications"
      className="border-t border-[var(--border)] max-h-[320px] overflow-y-auto p-3"
    >
      <div className="flex justify-between items-center mb-2">
        <h3 className="text-sm font-semibold">Notifications</h3>
        <div className="flex gap-2 items-center">
          {active.length > 0 && (
            <button
              type="button"
              onClick={() => void dismissAllNotifications()}
              className="text-xs text-gray-500 hover:text-gray-700"
            >
              Dismiss all
            </button>
          )}
          <button
            type="button"
            aria-label="Close notifications"
            onClick={() => setNotificationsOpen(false)}
            className="text-gray-400 hover:text-gray-600"
          >
            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>

      {notifications.length === 0 && (
        <p className="text-xs text-gray-400 italic">No notifications.</p>
      )}

      {active.map((n) => (
        <NotificationRow key={n.id} notification={n} onDismiss={() => void dismissNotification(n.id)} />
      ))}

      {dismissed.length > 0 && (
        <>
          <p className="text-[10px] uppercase tracking-wider text-gray-400 mt-3 mb-1">Dismissed</p>
          {dismissed.map((n) => (
            <NotificationRow key={n.id} notification={n} onDismiss={null} />
          ))}
        </>
      )}
    </div>
  );
}

function NotificationRow({
  notification,
  onDismiss,
}: {
  notification: Notification;
  onDismiss: (() => void) | null;
}) {
  const tone = severityTone(notification.severity);
  return (
    <div
      className={`flex items-start gap-2 p-2 mb-1 rounded-lg ${tone.bg} ${
        notification.dismissed ? 'opacity-60' : ''
      }`}
    >
      <span className={`mt-0.5 text-xs font-medium ${tone.fg}`} aria-hidden="true">
        {tone.icon}
      </span>
      <div className="flex-1 min-w-0">
        <p className={`text-xs font-medium truncate ${tone.fg}`}>{notification.title}</p>
        <p className="text-xs text-gray-600 dark:text-gray-300 mt-0.5">{notification.message}</p>
        <p className="text-[10px] text-gray-400 mt-1">
          {new Date(notification.created_at * 1000).toLocaleString()}
        </p>
      </div>
      {onDismiss && (
        <button
          type="button"
          aria-label={`Dismiss: ${notification.title}`}
          onClick={onDismiss}
          className="shrink-0 text-gray-400 hover:text-gray-600"
        >
          <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M18 6L6 18M6 6l12 12" />
          </svg>
        </button>
      )}
    </div>
  );
}

function severityTone(s: NotificationSeverity): { bg: string; fg: string; icon: string } {
  switch (s) {
    case 'error':
      return { bg: 'bg-red-50 dark:bg-red-900/20', fg: 'text-red-700 dark:text-red-300', icon: '!' };
    case 'warning':
      return { bg: 'bg-amber-50 dark:bg-amber-900/20', fg: 'text-amber-700 dark:text-amber-300', icon: '⚠' };
    case 'info':
    default:
      return { bg: 'bg-blue-50 dark:bg-blue-900/20', fg: 'text-blue-700 dark:text-blue-300', icon: 'i' };
  }
}
