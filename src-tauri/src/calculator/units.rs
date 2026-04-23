//! Unit conversion for WAT-203.
//!
//! Supports three dimensions: temperature, length, weight. Parses queries
//! of the form `<number> <from> (to|in) <to>`. Both units must be known
//! AND belong to the same dimension — otherwise we fall through and the
//! caller tries arithmetic (which will also fail, so the query falls to
//! fuzzy search).
//!
//! Conversion approach: each dimension has a reference unit (Kelvin,
//! meters, grams). Every known unit knows how to convert to/from its
//! reference. Cross-unit conversion is therefore: input → reference →
//! output, which keeps the table linear in the number of units.

use super::{format_number, Calculation, CalculationKind};

/// Parsed `<amount> <from> (to|in) <to>` pattern.
struct ParsedQuery<'a> {
    amount: f64,
    from: &'a str,
    to: &'a str,
}

fn parse(query: &str) -> Option<ParsedQuery<'_>> {
    // Split on whitespace; expect exactly 4 tokens: <number> <from_unit>
    // <"to"|"in"> <to_unit>. If there are more, the query is too complex
    // for our narrow pattern — fall through.
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.len() != 4 {
        return None;
    }
    let amount: f64 = tokens[0].parse().ok()?;
    let connector = tokens[2].to_ascii_lowercase();
    if connector != "to" && connector != "in" {
        return None;
    }
    Some(ParsedQuery {
        amount,
        from: tokens[1],
        to: tokens[3],
    })
}

pub fn detect(query: &str) -> Option<Calculation> {
    let parsed = parse(query)?;

    // Try each dimension. First match wins. If the dimensions disagree
    // (e.g. `5 m to kg`) neither will claim it and we fall through.
    let (result_value, canonical_from, canonical_to) =
        if let Some(r) = convert_temperature(parsed.amount, parsed.from, parsed.to) {
            r
        } else if let Some(r) = convert_length(parsed.amount, parsed.from, parsed.to) {
            r
        } else if let Some(r) = convert_weight(parsed.amount, parsed.from, parsed.to) {
            r
        } else {
            return None;
        };

    Some(Calculation {
        expression: format!(
            "{} {} to {}",
            format_number(parsed.amount),
            canonical_from,
            canonical_to
        ),
        result: format!("{} {}", format_number(result_value), canonical_to),
        kind: CalculationKind::UnitConversion,
    })
}

// --- Temperature ---

/// Match case-insensitively. Accept common variants — `C`, `°C`, `celsius`,
/// `celcius` (common typo). Returns canonical display form.
fn canonicalize_temperature(unit: &str) -> Option<&'static str> {
    let lower = unit.to_ascii_lowercase();
    match lower.trim_start_matches('°') {
        "c" | "celsius" | "celcius" => Some("°C"),
        "f" | "fahrenheit" => Some("°F"),
        "k" | "kelvin" => Some("K"),
        _ => None,
    }
}

fn convert_temperature(amount: f64, from: &str, to: &str) -> Option<(f64, &'static str, &'static str)> {
    let from_c = canonicalize_temperature(from)?;
    let to_c = canonicalize_temperature(to)?;
    // Convert input to Kelvin then Kelvin to output.
    let kelvin = match from_c {
        "°C" => amount + 273.15,
        "°F" => (amount - 32.0) * 5.0 / 9.0 + 273.15,
        "K" => amount,
        _ => unreachable!(),
    };
    let out = match to_c {
        "°C" => kelvin - 273.15,
        "°F" => (kelvin - 273.15) * 9.0 / 5.0 + 32.0,
        "K" => kelvin,
        _ => unreachable!(),
    };
    Some((out, from_c, to_c))
}

// --- Length ---

/// Returns (meters_per_unit, canonical_display_form) or None.
fn length_in_meters(unit: &str) -> Option<(f64, &'static str)> {
    let lower = unit.to_ascii_lowercase();
    Some(match lower.as_str() {
        "mm" | "millimeter" | "millimeters" => (0.001, "mm"),
        "cm" | "centimeter" | "centimeters" => (0.01, "cm"),
        "m" | "meter" | "meters" => (1.0, "m"),
        "km" | "kilometer" | "kilometers" => (1000.0, "km"),
        "in" | "inch" | "inches" => (0.0254, "in"),
        "ft" | "foot" | "feet" => (0.3048, "ft"),
        "yd" | "yard" | "yards" => (0.9144, "yd"),
        "mi" | "mile" | "miles" => (1609.344, "mi"),
        _ => return None,
    })
}

fn convert_length(amount: f64, from: &str, to: &str) -> Option<(f64, &'static str, &'static str)> {
    let (from_m, from_c) = length_in_meters(from)?;
    let (to_m, to_c) = length_in_meters(to)?;
    Some((amount * from_m / to_m, from_c, to_c))
}

// --- Weight (mass, pedantically) ---

fn weight_in_grams(unit: &str) -> Option<(f64, &'static str)> {
    let lower = unit.to_ascii_lowercase();
    Some(match lower.as_str() {
        "g" | "gram" | "grams" => (1.0, "g"),
        "kg" | "kilogram" | "kilograms" => (1000.0, "kg"),
        "mg" | "milligram" | "milligrams" => (0.001, "mg"),
        "lb" | "lbs" | "pound" | "pounds" => (453.592, "lb"),
        "oz" | "ounce" | "ounces" => (28.3495, "oz"),
        _ => return None,
    })
}

fn convert_weight(amount: f64, from: &str, to: &str) -> Option<(f64, &'static str, &'static str)> {
    let (from_g, from_c) = weight_in_grams(from)?;
    let (to_g, to_c) = weight_in_grams(to)?;
    Some((amount * from_g / to_g, from_c, to_c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(query: &str) -> String {
        detect(query)
            .unwrap_or_else(|| panic!("expected '{query}' to be detected as unit conversion"))
            .result
    }

    // --- temperature ---

    #[test]
    fn celsius_to_fahrenheit() {
        assert_eq!(r("100 C to F"), "212 °F");
        assert_eq!(r("0 C to F"), "32 °F");
        assert_eq!(r("-40 C to F"), "-40 °F");
    }

    #[test]
    fn fahrenheit_to_celsius() {
        assert_eq!(r("32 F to C"), "0 °C");
        assert_eq!(r("212 F to C"), "100 °C");
    }

    #[test]
    fn celsius_to_kelvin_absolute_zero() {
        assert_eq!(r("-273.15 C to K"), "0 K");
    }

    #[test]
    fn temperature_accepts_long_forms() {
        assert!(detect("100 celsius to fahrenheit").is_some());
        // Tolerate the common typo "celcius" — user doesn't need to know
        // they misspelled it. The output still shows the correct "°C".
        let calc = detect("100 celcius to F").unwrap();
        assert!(calc.expression.contains("°C"));
    }

    #[test]
    fn temperature_with_in_keyword() {
        assert_eq!(r("100 C in F"), "212 °F");
    }

    // --- length ---

    #[test]
    fn miles_to_km() {
        let calc = detect("1 mi to km").unwrap();
        assert_eq!(calc.result, "1.609344 km");
    }

    #[test]
    fn km_to_m() {
        assert_eq!(r("5 km to m"), "5000 m");
    }

    #[test]
    fn inches_to_cm() {
        assert_eq!(r("1 in to cm"), "2.54 cm");
    }

    #[test]
    fn length_accepts_long_forms() {
        assert!(detect("5 miles to kilometers").is_some());
        assert!(detect("1 foot to inches").is_some());
    }

    // --- weight ---

    #[test]
    fn kg_to_lb() {
        let calc = detect("1 kg to lb").unwrap();
        // 1 kg = 2.20462... lb; not asserting exact decimal.
        assert!(calc.result.starts_with("2.20"), "got: {}", calc.result);
    }

    #[test]
    fn oz_to_g() {
        let calc = detect("1 oz to g").unwrap();
        assert_eq!(calc.result, "28.3495 g");
    }

    // --- dimension mismatch / noise ---

    #[test]
    fn cross_dimension_conversion_is_not_detected() {
        // 5 meters → kilograms is nonsense; we must not silently pretend.
        assert!(detect("5 m to kg").is_none());
    }

    #[test]
    fn unknown_units_are_not_detected() {
        assert!(detect("5 foobar to m").is_none());
        assert!(detect("5 m to bazqux").is_none());
    }

    #[test]
    fn wrong_token_count_is_not_detected() {
        // Our parser is strict — 4 whitespace-separated tokens exactly.
        assert!(detect("5 miles").is_none());
        assert!(detect("5 miles to km somewhere").is_none());
        assert!(detect("5mi to km").is_none(), "no space between number and unit");
    }

    #[test]
    fn missing_connector_is_not_detected() {
        assert!(detect("5 m kg").is_none());
        assert!(detect("5 miles and km").is_none());
    }

    #[test]
    fn non_numeric_amount_is_not_detected() {
        assert!(detect("many miles to km").is_none());
    }

    #[test]
    fn ordinary_search_queries_are_not_detected() {
        // Pin zero-false-positive: common queries.
        assert!(detect("apple").is_none());
        assert!(detect("to do list").is_none());
        assert!(detect("go to sleep").is_none());
    }

    #[test]
    fn temperature_case_insensitive() {
        assert!(detect("100 c to f").is_some());
        assert!(detect("100 C to F").is_some());
        assert!(detect("100 CELSIUS to FAHRENHEIT").is_some());
    }
}
