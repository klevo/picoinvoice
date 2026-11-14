use std::path::Path;

use jiff::ToSpan;
use jiff::civil::Date;

use crate::Error;
use crate::config::Config;
use crate::money::Money;

/// Hours in the working day used for the daily rate in the summary.
pub const HOURS_PER_DAY: u32 = 8;

/// One invoice: a single line item billing `hours` for the month before the issue date.
#[derive(Debug, Clone, PartialEq)]
pub struct Invoice {
    pub number: String,
    pub issue_date: Date,
    pub due_date: Date,
    pub period_start: Date,
    pub period_end: Date,
    /// The item description with the billed month appended, e.g. `Consulting (May 2026)`.
    pub description: String,
    pub hours: u32,
    pub unit_price: Money,
    pub total: Money,
}

impl Invoice {
    pub fn new(config: &Config, number: String, issue_date: Date, hours: u32) -> Result<Invoice, Error> {
        if hours == 0 {
            return Err(Error::new("--hours must be a positive integer"));
        }
        let period_end = issue_date.first_of_month().yesterday().map_err(|_| Error::new("--date is too early"))?;
        let period_start = period_end.first_of_month();
        let due_date = issue_date
            .checked_add(i64::from(config.payment_days).days())
            .map_err(|_| Error::new("the due date is out of range"))?;
        let unit_price = config.item.unit_price.clone();
        let total = unit_price.times(hours).ok_or_else(|| Error::new("the invoice total is too large"))?;
        let description = format!("{} ({})", config.item.description, period_end.strftime("%B %Y"));

        Ok(Invoice { number, issue_date, due_date, period_start, period_end, description, hours, unit_price, total })
    }

    pub fn daily_rate(&self) -> Money {
        self.unit_price.times(HOURS_PER_DAY).expect("daily rate fits when the unit price does")
    }

    pub fn file_name(&self) -> String {
        format!("{}.pdf", self.number)
    }
}

/// The next invoice number for `year`: `{year}{NNN}`, one more than the highest
/// `{year}NNN.pdf` already in `dir`. Other files are ignored.
pub fn next_number(dir: &Path, year: i16) -> Result<String, Error> {
    let prefix = year.to_string();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(format!("{prefix}001")),
        Err(e) => return Err(Error::new(format!("Cannot read {}: {e}", dir.display()))),
    };

    let mut highest = 0;
    for entry in entries {
        let entry = entry.map_err(|e| Error::new(format!("Cannot read {}: {e}", dir.display())))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(sequence) = sequence_number(name, &prefix) {
            highest = highest.max(sequence);
        }
    }

    if highest >= 999 {
        return Err(Error::new(format!("Invoice numbers for {year} are used up ({prefix}999 exists)")));
    }
    Ok(format!("{prefix}{:03}", highest + 1))
}

fn sequence_number(file_name: &str, prefix: &str) -> Option<u32> {
    let (stem, extension) = file_name.rsplit_once('.')?;
    if !extension.eq_ignore_ascii_case("pdf") {
        return None;
    }
    let digits = stem.strip_prefix(prefix)?;
    if digits.len() != 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn config() -> Config {
        Config::parse(include_str!("../examples/picoinvoice.yml")).unwrap()
    }

    #[test]
    fn bills_the_previous_month() {
        let invoice = Invoice::new(&config(), "2026009".into(), date(2026, 10, 1), 168).unwrap();
        assert_eq!(invoice.description, "Software Engineering (September 2026)");
        assert_eq!(invoice.period_start, date(2026, 9, 1));
        assert_eq!(invoice.period_end, date(2026, 9, 30));
        assert_eq!(invoice.due_date, date(2026, 10, 15));
        assert_eq!(invoice.total.to_string(), "EUR 7,560.00");
        assert_eq!(invoice.daily_rate().to_string(), "EUR 360.00");
        assert_eq!(invoice.file_name(), "2026009.pdf");
    }

    #[test]
    fn january_bills_december_of_the_previous_year() {
        let invoice = Invoice::new(&config(), "2027001".into(), date(2027, 1, 27), 160).unwrap();
        assert_eq!(invoice.description, "Software Engineering (December 2026)");
        assert_eq!(invoice.period_start, date(2026, 12, 1));
        assert_eq!(invoice.period_end, date(2026, 12, 31));
    }

    #[test]
    fn handles_month_ends() {
        let invoice = Invoice::new(&config(), "2028001".into(), date(2028, 3, 31), 1).unwrap();
        assert_eq!(invoice.period_start, date(2028, 2, 1));
        assert_eq!(invoice.period_end, date(2028, 2, 29));
        assert_eq!(invoice.due_date, date(2028, 4, 14));
    }

    #[test]
    fn uses_configured_payment_days() {
        let mut config = config();
        config.payment_days = 30;
        let invoice = Invoice::new(&config, "2026001".into(), date(2026, 12, 15), 1).unwrap();
        assert_eq!(invoice.due_date, date(2027, 1, 14));
    }

    #[test]
    fn rejects_zero_hours() {
        assert!(Invoice::new(&config(), "2026001".into(), date(2026, 1, 1), 0).is_err());
    }

    fn dir_with(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for file in files {
            std::fs::write(dir.path().join(file), "").unwrap();
        }
        dir
    }

    #[test]
    fn numbers_start_at_one() {
        let dir = dir_with(&[]);
        assert_eq!(next_number(dir.path(), 2026).unwrap(), "2026001");
        assert_eq!(next_number(&dir.path().join("missing"), 2026).unwrap(), "2026001");
    }

    #[test]
    fn numbers_follow_the_highest_of_the_year() {
        let dir = dir_with(&["2026001.pdf", "2026007.pdf", "2026003.pdf", "2025012.pdf", "2027001.pdf"]);
        assert_eq!(next_number(dir.path(), 2026).unwrap(), "2026008");
        assert_eq!(next_number(dir.path(), 2025).unwrap(), "2025013");
        assert_eq!(next_number(dir.path(), 2028).unwrap(), "2028001");
    }

    #[test]
    fn numbers_ignore_other_files() {
        let dir = dir_with(&[
            "OF250004.PDF",
            "OF2026005.pdf",
            "2026004.txt",
            "20260099.pdf",
            "202601.pdf",
            "2026abc.pdf",
            "2026002 copy.pdf",
            ".DS_Store",
        ]);
        assert_eq!(next_number(dir.path(), 2026).unwrap(), "2026001");
        let dir = dir_with(&["2026002.PDF"]);
        assert_eq!(next_number(dir.path(), 2026).unwrap(), "2026003");
    }

    #[test]
    fn numbers_run_out_after_999() {
        let dir = dir_with(&["2026999.pdf"]);
        assert!(next_number(dir.path(), 2026).unwrap_err().to_string().contains("used up"));
    }
}
