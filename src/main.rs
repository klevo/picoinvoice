use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use jiff::civil::Date;
use picoinvoice::{Error, Prepared, config};

/// Generate a monthly, hours-based PDF invoice.
///
/// The invoice bills the month before the issue date. Details come from
/// $XDG_CONFIG_HOME/picoinvoice.yml (default ~/.config/picoinvoice.yml), and the PDF is
/// saved to the config's `output` folder as {year}{NNN}.pdf, numbered after the
/// highest invoice of that year already there.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Hours billed [default: `default_hours` from the config, or 160]
    #[arg(long, allow_negative_numbers = true, value_parser = clap::value_parser!(u32).range(1..))]
    hours: Option<u32>,

    /// Issue date as YYYY-MM-DD [default: today]
    #[arg(long, value_parser = parse_date)]
    date: Option<Date>,

    /// Config file to use instead of the default location
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
}

fn parse_date(value: &str) -> Result<Date, String> {
    let bytes = value.as_bytes();
    let shape_ok = bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, b)| if i == 4 || i == 7 { *b == b'-' } else { b.is_ascii_digit() });
    let invalid = || format!("`{value}` is not a valid date, expected YYYY-MM-DD");
    if !shape_ok {
        return Err(invalid());
    }
    value.parse().map_err(|_| invalid())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Error> {
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
    let config_path = match cli.config {
        Some(path) => path,
        None => config::default_path(std::env::var_os("XDG_CONFIG_HOME"), home.clone().map(Into::into))?,
    };
    let date = cli.date.unwrap_or_else(|| jiff::Zoned::now().date());

    let prepared = picoinvoice::prepare(&config_path, date, cli.hours, home.as_deref())?;
    print_summary(&prepared);
    prepared.save()?;
    println!("Invoice generated: {}", prepared.path.display());
    Ok(())
}

fn print_summary(prepared: &Prepared) {
    let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
    let paint = |code: &str, text: &str| if color { format!("\x1b[{code}m{text}\x1b[0m") } else { text.to_string() };
    let label = |text: &str| paint("1;36", text);
    let value = |text: &str| paint("1;32", text);

    let invoice = &prepared.invoice;
    println!();
    println!("{}", paint("1;33", "Invoice summary"));
    println!("{} {}", label("Total hours:"), value(&format!("{} h", invoice.hours)));
    println!("{} {}", label("Hourly rate:"), value(&invoice.unit_price.to_string()));
    println!(
        "{} {}",
        label(&format!("Daily rate ({}h):", picoinvoice::invoice::HOURS_PER_DAY)),
        value(&invoice.daily_rate().to_string())
    );
    println!(
        "{} {} {} {}",
        label("Dates:"),
        value(&invoice.period_start.to_string()),
        label("to"),
        value(&invoice.period_end.to_string())
    );
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dates_strictly() {
        assert_eq!(parse_date("2026-01-27"), Ok(jiff::civil::date(2026, 1, 27)));
        for value in ["2026-1-27", "20260127", "2026-02-30", "27.01.2026", "2026-01-27T00:00", "Jan 27"] {
            assert!(parse_date(value).is_err(), "{value} should be rejected");
        }
    }

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
