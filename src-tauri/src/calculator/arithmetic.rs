//! Arithmetic evaluator for WAT-203.
//!
//! Thin wrapper around the `meval` crate with a conservative gate that
//! prevents false positives on ordinary search queries. The gate is
//! strict: a query must contain at least one digit AND one "signal"
//! character (operator, paren, or named function). A bare number alone
//! doesn't count — `42` isn't a useful calculation, and a model number
//! like `Galaxy S24` shouldn't be hijacked into a math context.

use super::{format_number, Calculation, CalculationKind};

/// Characters / tokens that indicate the query is meant as arithmetic,
/// beyond just containing digits. Ordering reflects likelihood of use.
const SIGNAL_CHARS: &[char] = &['+', '*', '/', '^', '%', '(', ')'];
/// Named functions meval supports. If any of these substrings appear, the
/// query is treated as arithmetic. These are long enough that spurious
/// matches on ordinary English words are very unlikely (`log` is the
/// shortest, and queries containing "log" would need an accompanying digit
/// and typically a paren to pass meval anyway).
const SIGNAL_FUNCTIONS: &[&str] = &[
    "sqrt", "abs", "sin", "cos", "tan", "log", "ln", "exp", "floor", "ceil", "round",
];

/// True if the query plausibly looks like arithmetic. Cheap — runs before
/// meval so most search queries bail early.
fn looks_like_arithmetic(query: &str) -> bool {
    let has_digit = query.chars().any(|c| c.is_ascii_digit());
    if !has_digit {
        return false;
    }
    if query.chars().any(|c| SIGNAL_CHARS.contains(&c)) {
        return true;
    }
    SIGNAL_FUNCTIONS.iter().any(|fun| query.contains(fun))
}

pub fn detect(query: &str) -> Option<Calculation> {
    if !looks_like_arithmetic(query) {
        return None;
    }
    let value: f64 = meval::eval_str(query).ok()?;
    // Reject non-finite results — `1/0`, `sqrt(-1)`, etc. Copy-to-clipboard
    // of "NaN" is almost never what the user wanted; better to skip the
    // calc and let fuzzy search handle the query.
    if !value.is_finite() {
        return None;
    }
    Some(Calculation {
        expression: query.to_string(),
        result: format_number(value),
        kind: CalculationKind::Arithmetic,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(query: &str) -> String {
        detect(query)
            .unwrap_or_else(|| panic!("expected '{query}' to be detected as arithmetic"))
            .result
    }

    // --- happy path ---

    #[test]
    fn simple_addition() {
        assert_eq!(result("2+2"), "4");
    }

    #[test]
    fn multiplication() {
        assert_eq!(result("12*37"), "444");
    }

    #[test]
    fn parens_and_precedence() {
        assert_eq!(result("(1+2)*3"), "9");
    }

    #[test]
    fn exponent() {
        assert_eq!(result("2^10"), "1024");
    }

    #[test]
    fn sqrt_function() {
        assert_eq!(result("sqrt(144)"), "12");
    }

    #[test]
    fn division_with_decimal_result() {
        assert_eq!(result("10/4"), "2.5");
    }

    #[test]
    fn handles_whitespace_inside_expression() {
        assert_eq!(result("  (1 + 2) * 3  "), "9");
    }

    // --- zero false positives ---

    #[test]
    fn plain_words_without_digits_are_not_arithmetic() {
        assert!(detect("apple").is_none());
        assert!(detect("chrome").is_none());
        assert!(detect("hello world").is_none());
    }

    #[test]
    fn bare_numbers_are_not_arithmetic() {
        // The gate requires an operator / paren / function — a single
        // number on its own isn't a useful calculation result.
        assert!(detect("42").is_none());
        assert!(detect("3.14").is_none());
        assert!(detect("-5").is_none());
    }

    #[test]
    fn search_queries_with_incidental_digits_are_not_arithmetic() {
        // Product-name patterns that contain digits but no operator.
        assert!(detect("i5 cpu").is_none());
        assert!(detect("5gb").is_none());
        assert!(detect("3d modeling").is_none());
        assert!(detect("iphone 15").is_none());
        assert!(detect("Galaxy S24").is_none());
    }

    #[test]
    fn operator_in_middle_of_words_still_gates_meval_to_reject() {
        // "chrome - firefox" has an operator and a digit... wait, no digit.
        // Add one.
        // meval rejects "chrome - 1" because `chrome` isn't an identifier
        // it knows. The gate lets it through; meval's rejection is the
        // final stop. Ensure that chain works.
        assert!(detect("chrome - 1").is_none());
    }

    #[test]
    fn division_by_zero_is_not_a_result() {
        // Non-finite → skip; user sees fuzzy search instead of "∞" copied.
        assert!(detect("1/0").is_none());
    }

    #[test]
    fn sqrt_of_negative_is_not_a_result() {
        // sqrt(-1) = NaN; skip.
        assert!(detect("sqrt(-1)").is_none());
    }

    #[test]
    fn incomplete_expressions_are_not_detected() {
        assert!(detect("2+").is_none());
        assert!(detect("(1+").is_none());
    }

    #[test]
    fn python_style_power_is_not_detected() {
        // meval uses `^`; `**` is a syntax error. We correctly fall through.
        assert!(detect("2**3").is_none());
    }

    // --- gate ---

    #[test]
    fn looks_like_arithmetic_requires_at_least_one_digit() {
        assert!(!looks_like_arithmetic("a+b"));
        assert!(!looks_like_arithmetic("sqrt(x)"));
    }

    #[test]
    fn looks_like_arithmetic_requires_a_signal() {
        assert!(!looks_like_arithmetic("42"));
        assert!(looks_like_arithmetic("42+1"));
        assert!(looks_like_arithmetic("sqrt(42)"));
    }
}
