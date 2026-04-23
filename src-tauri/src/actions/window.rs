//! WAT-302: window-management geometry + OS shims.
//!
//! The geometry math lives here as a pure function with full unit tests —
//! given a monitor work-area and a (split-left / split-right / maximize /
//! center) intent, `compute_target_rect` returns the exact pixel rect the
//! foreground window should occupy. The actual "move this window to that
//! rect" step is platform-specific and lives in `actions::system` for
//! now.
//!
//! Keeping the math pure lets us ship the commands on day one with
//! confidence in the arithmetic; platform implementations can firm up
//! in follow-up tickets without risking regressions to the split ratios.
//!
//! ### Coordinate space
//!
//! All rects are in monitor-relative device-pixel coordinates where
//! (x, y) is the top-left corner. A monitor with a non-zero origin
//! (e.g. a secondary display offset by (1920, 0)) still yields correct
//! targets because the monitor rect carries that offset.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

/// The four window-placement intents registered as `>` commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    SplitLeft,
    SplitRight,
    Maximize,
    Center,
}

impl WindowAction {
    /// Parse a command id like "cmd:win-left" into the enum. Returns
    /// `None` for ids outside the WAT-302 set so the caller can fall
    /// through to other command handling.
    pub fn from_command_id(id: &str) -> Option<Self> {
        Some(match id {
            "cmd:win-left" => WindowAction::SplitLeft,
            "cmd:win-right" => WindowAction::SplitRight,
            "cmd:win-max" => WindowAction::Maximize,
            "cmd:win-center" => WindowAction::Center,
            _ => return None,
        })
    }
}

/// Compute the target rect for a window action.
///
/// - `monitor` is the work-area rect of the monitor that currently owns
///   the window (NOT the full monitor rect — the work area excludes the
///   taskbar / menu bar). A non-zero origin carries over: splitting on
///   the second monitor produces a rect within that monitor's origin,
///   not the primary's.
/// - `window` is the current (width, height) of the foreground window.
///   Only `Center` uses it; the other three ignore it and take a fixed
///   fraction of the monitor.
pub fn compute_target_rect(action: WindowAction, monitor: Rect, window: Size) -> Rect {
    match action {
        WindowAction::SplitLeft => Rect {
            x: monitor.x,
            y: monitor.y,
            // Half, rounding down. If monitor.w is odd, left half gets
            // the smaller slice and right half picks up the extra pixel
            // via `monitor.w - half` — ensures full coverage with no
            // 1-pixel gap.
            w: monitor.w / 2,
            h: monitor.h,
        },
        WindowAction::SplitRight => {
            let half = monitor.w / 2;
            Rect {
                x: monitor.x + half,
                y: monitor.y,
                w: monitor.w - half,
                h: monitor.h,
            }
        }
        WindowAction::Maximize => monitor,
        WindowAction::Center => {
            // Clamp the window size to the monitor so an over-sized
            // window doesn't spill into negative coordinates.
            let w = window.w.min(monitor.w).max(1);
            let h = window.h.min(monitor.h).max(1);
            Rect {
                x: monitor.x + (monitor.w - w) / 2,
                y: monitor.y + (monitor.h - h) / 2,
                w,
                h,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD: Rect = Rect { x: 0, y: 0, w: 1920, h: 1080 };
    const FOURK: Rect = Rect { x: 0, y: 0, w: 3840, h: 2160 };
    const SECOND_HD: Rect = Rect { x: 1920, y: 0, w: 1920, h: 1080 };
    const WINDOW_1200_800: Size = Size { w: 1200, h: 800 };

    // --- from_command_id ---

    #[test]
    fn from_command_id_recognizes_each_action() {
        assert_eq!(WindowAction::from_command_id("cmd:win-left"), Some(WindowAction::SplitLeft));
        assert_eq!(WindowAction::from_command_id("cmd:win-right"), Some(WindowAction::SplitRight));
        assert_eq!(WindowAction::from_command_id("cmd:win-max"), Some(WindowAction::Maximize));
        assert_eq!(WindowAction::from_command_id("cmd:win-center"), Some(WindowAction::Center));
    }

    #[test]
    fn from_command_id_returns_none_for_unrelated_ids() {
        assert_eq!(WindowAction::from_command_id("cmd:lock"), None);
        assert_eq!(WindowAction::from_command_id(""), None);
        assert_eq!(WindowAction::from_command_id("cmd:win"), None);
    }

    // --- compute_target_rect: split ---

    #[test]
    fn split_left_occupies_left_half_of_primary_monitor() {
        let r = compute_target_rect(WindowAction::SplitLeft, HD, WINDOW_1200_800);
        assert_eq!(r, Rect { x: 0, y: 0, w: 960, h: 1080 });
    }

    #[test]
    fn split_right_occupies_right_half_of_primary_monitor() {
        let r = compute_target_rect(WindowAction::SplitRight, HD, WINDOW_1200_800);
        assert_eq!(r, Rect { x: 960, y: 0, w: 960, h: 1080 });
    }

    #[test]
    fn split_halves_tile_with_no_gap_on_odd_widths() {
        // Odd-width monitor (e.g., 1921) — left half gets 960, right
        // half gets 961, combined = 1921. No overlap, no gap.
        let odd = Rect { x: 0, y: 0, w: 1921, h: 1080 };
        let l = compute_target_rect(WindowAction::SplitLeft, odd, WINDOW_1200_800);
        let r = compute_target_rect(WindowAction::SplitRight, odd, WINDOW_1200_800);
        assert_eq!(l.w + r.w, odd.w, "halves must sum to full width with no gap");
        assert_eq!(l.x + l.w, r.x, "right half must start exactly where left ends");
    }

    #[test]
    fn split_on_secondary_monitor_carries_origin_offset() {
        // Secondary display at (1920, 0): splitting left should yield
        // a rect at x=1920, NOT x=0. Multi-monitor regression guard.
        let l = compute_target_rect(WindowAction::SplitLeft, SECOND_HD, WINDOW_1200_800);
        assert_eq!(l, Rect { x: 1920, y: 0, w: 960, h: 1080 });
        let r = compute_target_rect(WindowAction::SplitRight, SECOND_HD, WINDOW_1200_800);
        assert_eq!(r.x, 1920 + 960);
    }

    // --- compute_target_rect: maximize ---

    #[test]
    fn maximize_fills_the_monitor_work_area() {
        let r = compute_target_rect(WindowAction::Maximize, HD, WINDOW_1200_800);
        assert_eq!(r, HD);
    }

    #[test]
    fn maximize_on_4k_monitor_returns_4k_rect() {
        let r = compute_target_rect(WindowAction::Maximize, FOURK, WINDOW_1200_800);
        assert_eq!(r, FOURK);
    }

    // --- compute_target_rect: center ---

    #[test]
    fn center_places_window_at_monitor_midpoint() {
        // 1200x800 centered on 1920x1080:
        //   x = 0 + (1920 - 1200) / 2 = 360
        //   y = 0 + (1080 - 800) / 2 = 140
        let r = compute_target_rect(WindowAction::Center, HD, WINDOW_1200_800);
        assert_eq!(r, Rect { x: 360, y: 140, w: 1200, h: 800 });
    }

    #[test]
    fn center_preserves_window_size_when_it_fits() {
        let tiny = Size { w: 100, h: 100 };
        let r = compute_target_rect(WindowAction::Center, HD, tiny);
        assert_eq!(r.w, 100);
        assert_eq!(r.h, 100);
    }

    #[test]
    fn center_clamps_oversized_window_to_monitor() {
        // Window bigger than the monitor — clamp to monitor so we don't
        // place into negative coordinates.
        let huge = Size { w: 5000, h: 5000 };
        let r = compute_target_rect(WindowAction::Center, HD, huge);
        assert!(r.x >= 0);
        assert!(r.y >= 0);
        assert_eq!(r.w, HD.w);
        assert_eq!(r.h, HD.h);
    }

    #[test]
    fn center_on_secondary_monitor_offsets_correctly() {
        let r = compute_target_rect(WindowAction::Center, SECOND_HD, WINDOW_1200_800);
        // Should center within the second monitor — x starts at 1920.
        assert_eq!(r.x, 1920 + 360);
        assert_eq!(r.y, 140);
    }
}
