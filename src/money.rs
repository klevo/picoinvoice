use std::fmt;

/// An amount of money in a currency, stored as integer cents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    pub currency: String,
    pub cents: i64,
}

impl Money {
    /// Parses `EUR 40.00`, `EUR 1,250` or `USD 95.5`: a currency code, a space
    /// and an amount with at most two decimal places.
    pub fn parse(value: &str) -> Result<Money, String> {
        let invalid = || format!("invalid amount `{value}`, expected a currency and an amount like `EUR 40.00`");
        let mut parts = value.split_whitespace();
        let (Some(currency), Some(amount), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(invalid());
        };
        if currency.chars().any(|c| c.is_ascii_digit()) {
            return Err(invalid());
        }

        let amount = amount.replace(',', "");
        let (whole, fraction) = amount.split_once('.').unwrap_or((&amount, ""));
        let all_digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
        if whole.is_empty() || !all_digits(whole) || !all_digits(fraction) {
            return Err(invalid());
        }
        if fraction.len() > 2 {
            return Err(format!("invalid amount `{value}`, use at most two decimal places"));
        }

        let whole: i64 = whole.parse().map_err(|_| invalid())?;
        let fraction: i64 = format!("{fraction:0<2}").parse().map_err(|_| invalid())?;
        let cents = whole.checked_mul(100).and_then(|c| c.checked_add(fraction)).ok_or_else(invalid)?;

        Ok(Money { currency: currency.to_string(), cents })
    }

    pub fn times(&self, factor: u32) -> Option<Money> {
        Some(Money { currency: self.currency.clone(), cents: self.cents.checked_mul(i64::from(factor))? })
    }

    /// Zero in this currency, written without decimals (`EUR 0`).
    pub fn zero_label(&self) -> String {
        format!("{} 0", self.currency)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.currency, format_amount(self.cents))
    }
}

/// Formats cents as `1,250.00`: comma thousands separators and two decimals.
fn format_amount(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let cents = cents.unsigned_abs();
    let digits = (cents / 100).to_string();
    let mut grouped = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{sign}{grouped}.{:02}", cents % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eur(cents: i64) -> Money {
        Money { currency: "EUR".into(), cents }
    }

    #[test]
    fn formats_with_thousands_separators() {
        assert_eq!(eur(0).to_string(), "EUR 0.00");
        assert_eq!(eur(9_900).to_string(), "EUR 99.00");
        assert_eq!(eur(125_000).to_string(), "EUR 1,250.00");
        assert_eq!(eur(672_000).to_string(), "EUR 6,720.00");
        assert_eq!(eur(2_101_000).to_string(), "EUR 21,010.00");
        assert_eq!(eur(100_000_005).to_string(), "EUR 1,000,000.05");
        assert_eq!(eur(-125_050).to_string(), "EUR -1,250.50");
    }

    #[test]
    fn parses_amounts() {
        assert_eq!(Money::parse("EUR 40.00"), Ok(eur(4_000)));
        assert_eq!(Money::parse("EUR 40"), Ok(eur(4_000)));
        assert_eq!(Money::parse("EUR 95.5"), Ok(eur(9_550)));
        assert_eq!(Money::parse("EUR 1,250.00"), Ok(eur(125_000)));
        assert_eq!(Money::parse("  USD   12.34 "), Ok(Money { currency: "USD".into(), cents: 1_234 }));
    }

    #[test]
    fn rejects_invalid_amounts() {
        for value in ["", "EUR", "40.00", "EUR forty", "EUR 40.00 extra", "EUR -40", "EUR .50", "EUR 4.0.0", "1 2"] {
            assert!(Money::parse(value).is_err(), "{value:?} should be rejected");
        }
        assert!(Money::parse("EUR 40.001").unwrap_err().contains("two decimal places"));
    }

    #[test]
    fn multiplies() {
        assert_eq!(eur(4_000).times(168), Some(eur(672_000)));
        assert_eq!(eur(i64::MAX).times(2), None);
    }

    #[test]
    fn zero_label_has_no_decimals() {
        assert_eq!(eur(4_000).zero_label(), "EUR 0");
    }
}
