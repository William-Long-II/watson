//! WAT-407: window layout math — breakpoints for width-scaling on
//! ultrawide monitors, and horizontal centering on a monitor.
//!
//! Separated from `lib.rs::resize_window` / `show_window` so the
//! breakpoint math is testable in isolation — the Tauri-side calls
//! that read monitor dimensions and push `set_size` / `set_position`
//! stay thin adapters around these pure functions.

/// Width Watson uses on ordinary (sub-1600px) displays — the pre-WAT-407
/// baseline. Exposed as a constant so tests and consumers can reference
/// the default without magic numbers.
pub const DEFAULT_WIDTH: u32 = 600;

/// Viewport threshold above which Watson widens from 600 → 700. Using
/// `>` (strict) rather than `>=` so "exactly 1600px" monitors keep the
/// default width — conservative bias toward "don't surprise existing
/// users."
pub const MID_BREAKPOINT: f64 = 1600.0;

/// Viewport threshold above which Watson widens from 700 → 800.
pub const LARGE_BREAKPOINT: f64 = 2400.0;

/// Pick the window width for a monitor with the given logical width.
///
/// ```text
/// monitor_logical_width ≤ 1600 → 600  (default)
/// 1600 < width ≤ 2400           → 700  (mid)
/// width > 2400                  → 800  (ultrawide / 4K)
/// ```
pub fn compute_window_width(monitor_logical_width: f64) -> u32 {
    if monitor_logical_width > LARGE_BREAKPOINT {
        800
    } else if monitor_logical_width > MID_BREAKPOINT {
        700
    } else {
        DEFAULT_WIDTH
    }
}

/// Compute the x-coordinate that horizontally centers a window of
/// `window_width` on a monitor whose left edge is at `monitor_x` and
/// whose width is `monitor_width`. All values in logical pixels.
///
/// Secondary-monitor safe: a non-zero `monitor_x` carries through, so
/// centering on a display at (1920, 0) with width 1920 and a 600px
/// window yields `1920 + (1920 - 600) / 2 = 2580`, not `660`.
pub fn center_x_on_monitor(monitor_x: f64, monitor_width: f64, window_width: u32) -> f64 {
    monitor_x + (monitor_width - window_width as f64) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- compute_window_width ---

    #[test]
    fn default_width_on_sub_mid_monitor() {
        assert_eq!(compute_window_width(1024.0), 600);
        assert_eq!(compute_window_width(1366.0), 600);
        assert_eq!(compute_window_width(1440.0), 600);
    }

    #[test]
    fn exactly_mid_breakpoint_stays_default() {
        // `>` comparison — exactly 1600 keeps the baseline. Pin the
        // boundary behavior so a `>=` regression would be caught.
        assert_eq!(compute_window_width(1600.0), 600);
    }

    #[test]
    fn mid_width_for_viewports_above_mid_breakpoint() {
        assert_eq!(compute_window_width(1601.0), 700);
        assert_eq!(compute_window_width(1920.0), 700);
        assert_eq!(compute_window_width(2048.0), 700);
        assert_eq!(compute_window_width(2400.0), 700);
    }

    #[test]
    fn large_width_for_viewports_above_large_breakpoint() {
        assert_eq!(compute_window_width(2401.0), 800);
        assert_eq!(compute_window_width(3440.0), 800); // Common ultrawide
        assert_eq!(compute_window_width(3840.0), 800); // 4K
        assert_eq!(compute_window_width(5120.0), 800); // 5K
    }

    #[test]
    fn edge_case_zero_width_is_default() {
        // Defensive: a zero-width monitor is nonsense but shouldn't
        // return a weird value. Falls into the "≤ mid" branch.
        assert_eq!(compute_window_width(0.0), DEFAULT_WIDTH);
    }

    // --- center_x_on_monitor ---

    #[test]
    fn centers_on_primary_monitor() {
        // 600px window on a 1920-wide primary monitor:
        //   x = 0 + (1920 - 600) / 2 = 660
        assert_eq!(center_x_on_monitor(0.0, 1920.0, 600), 660.0);
    }

    #[test]
    fn centers_on_secondary_monitor_with_offset() {
        // Secondary at x=1920, width=2560, window=800:
        //   x = 1920 + (2560 - 800) / 2 = 2800
        assert_eq!(center_x_on_monitor(1920.0, 2560.0, 800), 2800.0);
    }

    #[test]
    fn centers_ultrawide_window_on_ultrawide_monitor() {
        // Window of 800 on a 3440-wide ultrawide:
        //   x = 0 + (3440 - 800) / 2 = 1320
        assert_eq!(center_x_on_monitor(0.0, 3440.0, 800), 1320.0);
    }

    #[test]
    fn handles_fractional_monitor_widths() {
        // Some HiDPI displays report fractional logical widths (e.g.
        // 1792.5 on a 16" MBP @ 2x). Centering still returns a sensible
        // float.
        let x = center_x_on_monitor(0.0, 1792.5, 700);
        // (1792.5 - 700) / 2 = 546.25
        assert!((x - 546.25).abs() < 1e-9);
    }
}
