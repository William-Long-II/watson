//! WAT-203: inline calculator + unit/currency conversion.
//!
//! The launcher's highest-frequency "I wish Watson did this" feature. Runs
//! as the very first step in the search pipeline: if [`detect`] returns
//! `Some(Calculation)`, the caller short-circuits and shows that single
//! result — typing `12*37` should not return an app named "1237".
//!
//! ## Routing
//!
//! [`detect`] tries, in order:
//! 1. [`units::detect`] — `30 C to F`, `5 miles to km`, etc.
//! 2. [`currency::detect`] — `100 USD to EUR`.
//! 3. [`arithmetic::detect`] — `12 * 37`, `sqrt(144)`, `(1+2)^3`.
//!
//! Unit/currency conversion is tried before arithmetic because a query
//! like `5 m to km` would otherwise partially parse as arithmetic (`5`
//! followed by noise, which meval rejects anyway — but avoiding the
//! wasted call is cleaner).
//!
//! ## False-positive discipline
//!
//! The hard rule, documented by tests in each submodule: queries like
//! `apple`, `chrome`, `1.0`, `i5 cpu`, `5gb` must return `None`. A single
//! number alone (`42`) is not a useful result, so arithmetic requires at
//! least one operator or function call to fire.

pub mod arithmetic;
pub mod currency;
pub mod units;

use serde::{Deserialize, Serialize};

/// A successful calculation result, ready to render as a search result.
/// The caller turns this into a `SearchResult` with action `CopyClipboard`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Calculation {
    /// Echoes the user's input, normalized. Rendered as the result's name.
    pub expression: String,
    /// Formatted result to display and to copy to clipboard on Enter.
    pub result: String,
    /// Which detector produced this result — useful for icon selection and
    /// tests.
    pub kind: CalculationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalculationKind {
    Arithmetic,
    UnitConversion,
    CurrencyConversion,
}

/// Try to interpret `query` as a calculation. Returns `None` if the query
/// doesn't look like one — the caller continues with normal search.
pub fn detect(query: &str) -> Option<Calculation> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Order matters: unit/currency parsers are more specific (they require
    // a `to|in` token) and would otherwise be missed because meval would
    // reject the surrounding unit names.
    if let Some(calc) = units::detect(trimmed) {
        return Some(calc);
    }
    if let Some(calc) = currency::detect(trimmed) {
        return Some(calc);
    }
    arithmetic::detect(trimmed)
}

/// Format a floating-point result for display. Trims insignificant
/// trailing zeros ("12.500000" → "12.5", "12.000000" → "12") while
/// preserving enough precision for scientific work. Shared across the
/// arithmetic and conversion evaluators so all output formats identically.
pub(crate) fn format_number(n: f64) -> String {
    if !n.is_finite() {
        return if n.is_nan() { "NaN".to_string() } else { "∞".to_string() };
    }
    // Use up to 10 significant digits then strip trailing zeros. Avoids
    // scientific notation for everyday numbers (12.5, 3.14, -273.15) while
    // keeping very small/large numbers readable.
    let formatted = format!("{n:.10}");
    if formatted.contains('.') {
        let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    } else {
        formatted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_is_not_a_calculation() {
        assert!(detect("").is_none());
        assert!(detect("   ").is_none());
    }

    #[test]
    fn plain_words_are_not_calculations() {
        // Zero-false-positive discipline: common search queries must
        // never look like calculations.
        for non_calc in ["apple", "chrome", "firefox", "hello world", "i5 cpu", "5gb", "3d modeling", "1.0"] {
            assert!(
                detect(non_calc).is_none(),
                "'{non_calc}' should not be detected as a calculation"
            );
        }
    }

    #[test]
    fn arithmetic_query_routes_to_arithmetic_kind() {
        let calc = detect("2+2").unwrap();
        assert_eq!(calc.kind, CalculationKind::Arithmetic);
    }

    #[test]
    fn unit_query_routes_to_unit_conversion_kind() {
        let calc = detect("30 C to F").unwrap();
        assert_eq!(calc.kind, CalculationKind::UnitConversion);
    }

    #[test]
    fn currency_query_routes_to_currency_conversion_kind() {
        let calc = detect("100 USD to EUR").unwrap();
        assert_eq!(calc.kind, CalculationKind::CurrencyConversion);
    }

    // --- format_number ---

    #[test]
    fn format_number_trims_trailing_zeros() {
        assert_eq!(format_number(12.5), "12.5");
        assert_eq!(format_number(12.0), "12");
        assert_eq!(format_number(12.500), "12.5");
    }

    #[test]
    fn format_number_preserves_precision_without_noise() {
        assert_eq!(format_number(3.14), "3.14");
        assert_eq!(format_number(-273.15), "-273.15");
    }

    #[test]
    fn format_number_handles_infinity_and_nan() {
        assert_eq!(format_number(f64::NAN), "NaN");
        assert_eq!(format_number(f64::INFINITY), "∞");
        assert_eq!(format_number(f64::NEG_INFINITY), "∞");
    }
}
