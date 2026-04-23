//! Currency conversion for WAT-203.
//!
//! Uses an offline exchange-rate table. Rates are snapshotted at build time
//! and will drift — the result banner surfaces the snapshot date so the
//! user knows not to trust the output for high-stakes work. A follow-up
//! ticket can replace this with an online fetcher (cached locally, refreshed
//! weekly) without changing the caller's contract.
//!
//! Pattern: `<amount> <FROM> (to|in) <TO>` where FROM and TO are 3-letter
//! ISO codes we recognize. Case-insensitive.

use super::{format_number, Calculation, CalculationKind};

/// Date the `RATES` table was last snapshotted. Shown in the result so
/// users know the age of the data. Update this when refreshing rates.
const RATES_SNAPSHOT_DATE: &str = "2026-04-01";

/// Exchange rate to 1 USD. USD is the reference; conversions are
/// FROM → USD → TO. Rates are approximate and for display only.
struct Rate {
    code: &'static str,
    per_usd: f64,
}

const RATES: &[Rate] = &[
    Rate { code: "USD", per_usd: 1.0 },
    Rate { code: "EUR", per_usd: 0.92 },
    Rate { code: "GBP", per_usd: 0.79 },
    Rate { code: "JPY", per_usd: 151.0 },
    Rate { code: "CAD", per_usd: 1.37 },
    Rate { code: "AUD", per_usd: 1.52 },
    Rate { code: "CHF", per_usd: 0.91 },
];

fn lookup_rate(code: &str) -> Option<&'static Rate> {
    let upper = code.to_ascii_uppercase();
    RATES.iter().find(|r| r.code == upper)
}

pub fn detect(query: &str) -> Option<Calculation> {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.len() != 4 {
        return None;
    }
    let amount: f64 = tokens[0].parse().ok()?;
    let connector = tokens[2].to_ascii_lowercase();
    if connector != "to" && connector != "in" {
        return None;
    }

    let from = lookup_rate(tokens[1])?;
    let to = lookup_rate(tokens[3])?;

    // amount in USD = amount / from.per_usd
    // amount in TO  = (amount / from.per_usd) * to.per_usd
    let usd = amount / from.per_usd;
    let out = usd * to.per_usd;

    Some(Calculation {
        expression: format!("{} {} to {}", format_number(amount), from.code, to.code),
        result: format!(
            "{} {} (rates from {})",
            format_number(out),
            to.code,
            RATES_SNAPSHOT_DATE
        ),
        kind: CalculationKind::CurrencyConversion,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usd_to_usd_is_identity() {
        let calc = detect("100 USD to USD").unwrap();
        assert!(calc.result.starts_with("100 USD"), "got: {}", calc.result);
    }

    #[test]
    fn usd_to_eur_applies_rate() {
        let calc = detect("100 USD to EUR").unwrap();
        assert!(
            calc.result.starts_with("92 EUR"),
            "expected 100 * 0.92 = 92, got: {}",
            calc.result
        );
    }

    #[test]
    fn eur_to_usd_reverses_rate() {
        let calc = detect("92 EUR to USD").unwrap();
        // 92 / 0.92 = 100
        assert!(calc.result.starts_with("100 USD"), "got: {}", calc.result);
    }

    #[test]
    fn result_includes_snapshot_date() {
        let calc = detect("10 USD to EUR").unwrap();
        assert!(
            calc.result.contains(RATES_SNAPSHOT_DATE),
            "snapshot date should appear in result; got: {}",
            calc.result
        );
    }

    #[test]
    fn lowercase_codes_are_accepted() {
        assert!(detect("100 usd to eur").is_some());
    }

    #[test]
    fn mixed_case_codes_are_accepted() {
        assert!(detect("100 Usd to Eur").is_some());
    }

    #[test]
    fn unknown_currency_is_not_detected() {
        assert!(detect("100 XYZ to EUR").is_none());
        assert!(detect("100 USD to XYZ").is_none());
    }

    #[test]
    fn in_is_accepted_as_connector() {
        assert!(detect("100 USD in EUR").is_some());
    }

    #[test]
    fn non_numeric_amount_is_not_detected() {
        assert!(detect("some USD to EUR").is_none());
    }

    #[test]
    fn wrong_token_count_is_not_detected() {
        assert!(detect("100 USD").is_none());
        assert!(detect("100 USD to EUR extra").is_none());
    }

    #[test]
    fn ordinary_search_queries_are_not_detected() {
        assert!(detect("usd").is_none());
        assert!(detect("exchange rate").is_none());
        assert!(detect("100 apples to oranges").is_none());
    }
}
