use std::path::{Path, PathBuf};

use assert_cmd::Command;
use jiff::civil::date;
use tempfile::TempDir;

const CONFIG: &str = "
from:
  name: Jana Nováková
  address:
    city: Bratislava
bill_to:
  name: Acme Software Inc.
item:
  description: Software Engineering
  unit_price: EUR 40.00
";

struct Setup {
    dir: TempDir,
}

impl Setup {
    /// A temporary folder with `picoinvoice.yml` writing to `<dir>/invoices`.
    fn new() -> Setup {
        Setup::with_config("")
    }

    fn with_config(extra: &str) -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let setup = Setup { dir };
        let output = setup.invoices();
        setup.write_config(&format!("output: {}\n{CONFIG}{extra}", output.display()));
        setup
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn config_path(&self) -> PathBuf {
        self.path().join("picoinvoice.yml")
    }

    fn write_config(&self, text: &str) {
        std::fs::write(self.config_path(), text).unwrap();
    }

    fn invoices(&self) -> PathBuf {
        self.path().join("invoices")
    }

    fn seed(&self, file: &str) {
        std::fs::create_dir_all(self.invoices()).unwrap();
        std::fs::write(self.invoices().join(file), "placeholder").unwrap();
    }

    /// The command with an isolated environment and `--config` pointing at the setup.
    fn cmd(&self) -> Command {
        let mut cmd = self.bare_cmd();
        cmd.arg("--config").arg(self.config_path());
        cmd
    }

    fn bare_cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("picoinvoice").unwrap();
        cmd.env_remove("XDG_CONFIG_HOME").env("HOME", self.path().join("home")).env_remove("NO_COLOR");
        cmd
    }
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn assert_pdf(path: &Path) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert!(bytes.starts_with(b"%PDF-"), "{} is not a PDF", path.display());
}

#[test]
fn generates_the_next_invoice_with_a_summary() {
    let setup = Setup::new();
    setup.seed("2026001.pdf");

    let output = setup.cmd().args(["--hours", "5", "--date", "2026-01-27"]).output().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));

    let expected = setup.invoices().join("2026002.pdf");
    assert_pdf(&expected);
    assert_eq!(
        stdout(&output),
        format!(
            "\nInvoice summary\nTotal hours: 5 h\nHourly rate: EUR 40.00\nDaily rate (8h): EUR 320.00\n\
             Dates: 2025-12-01 to 2025-12-31\n\nInvoice generated: {}\n",
            expected.display()
        )
    );
}

#[test]
fn numbers_restart_each_year() {
    let setup = Setup::new();
    setup.seed("2026007.pdf");
    setup.cmd().args(["--date", "2027-01-04"]).assert().success();
    assert_pdf(&setup.invoices().join("2027001.pdf"));
}

#[test]
fn defaults_hours_to_160() {
    let setup = Setup::new();
    let output = setup.cmd().args(["--date", "2026-02-01"]).output().unwrap();
    assert!(stdout(&output).contains("Total hours: 160 h"), "{}", stdout(&output));
}

#[test]
fn defaults_hours_to_the_config() {
    let setup = Setup::with_config("default_hours: 120\n");
    let output = setup.cmd().args(["--date", "2026-02-01"]).output().unwrap();
    assert!(stdout(&output).contains("Total hours: 120 h"), "{}", stdout(&output));
}

#[test]
fn defaults_date_to_today() {
    let setup = Setup::new();
    let output = setup.cmd().output().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));

    let today = jiff::Zoned::now().date();
    assert_pdf(&setup.invoices().join(format!("{}001.pdf", today.year())));
    let period_end = today.first_of_month().yesterday().unwrap();
    let dates = format!("Dates: {} to {}", period_end.first_of_month(), period_end);
    assert!(stdout(&output).contains(&dates), "{}", stdout(&output));
}

#[test]
fn reads_the_config_from_xdg_config_home() {
    let setup = Setup::new();
    let output = setup.bare_cmd().env("XDG_CONFIG_HOME", setup.path()).arg("--date=2026-05-01").output().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_pdf(&setup.invoices().join("2026001.pdf"));
}

#[test]
fn reads_the_config_from_home_dot_config() {
    let setup = Setup::new();
    let config_dir = setup.path().join("home/.config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::rename(setup.config_path(), config_dir.join("picoinvoice.yml")).unwrap();

    let output = setup.bare_cmd().arg("--date=2026-05-01").output().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_pdf(&setup.invoices().join("2026001.pdf"));
}

#[test]
fn expands_home_in_output() {
    let setup = Setup::new();
    setup.write_config(&format!("output: ~/Invoices\n{CONFIG}"));
    let output = setup.cmd().arg("--date=2026-05-01").output().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_pdf(&setup.path().join("home/Invoices/2026001.pdf"));
}

#[test]
fn rejects_invalid_hours() {
    let setup = Setup::new();
    for hours in ["0", "-1", "2.5", "many"] {
        let output = setup.cmd().args(["--hours", hours, "--date", "2026-05-01"]).output().unwrap();
        assert!(!output.status.success(), "--hours {hours} was accepted");
        assert!(stderr(&output).contains("--hours"), "{}", stderr(&output));
    }
    assert!(!setup.invoices().exists());
}

#[test]
fn rejects_invalid_dates() {
    let setup = Setup::new();
    for date in ["2026-02-30", "2026-1-5", "tomorrow"] {
        let output = setup.cmd().args(["--date", date]).output().unwrap();
        assert!(!output.status.success(), "--date {date} was accepted");
        assert!(stderr(&output).contains("expected YYYY-MM-DD"), "{}", stderr(&output));
    }
    assert!(!setup.invoices().exists());
}

#[test]
fn reports_a_missing_config() {
    let setup = Setup::new();
    let output = setup.bare_cmd().arg("--date=2026-05-01").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let expected = format!("error: No config file at {}", setup.path().join("home/.config/picoinvoice.yml").display());
    assert!(stderr(&output).starts_with(&expected), "{}", stderr(&output));
}

#[test]
fn reports_an_invalid_config() {
    let setup = Setup::new();
    setup.write_config("output: ./invoices\nfrom: {}\n");
    let output = setup.cmd().arg("--date=2026-05-01").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let expected = format!("error: Invalid config {}", setup.config_path().display());
    assert!(stderr(&output).starts_with(&expected), "{}", stderr(&output));
}

#[test]
fn prints_the_version() {
    let output = Setup::new().bare_cmd().arg("--version").output().unwrap();
    assert_eq!(stdout(&output), format!("picoinvoice {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn prepares_without_writing_and_never_overwrites() {
    let setup = Setup::new();
    let prepared = picoinvoice::prepare(&setup.config_path(), date(2026, 3, 2), Some(10), None).unwrap();
    assert_eq!(prepared.path, setup.invoices().join("2026001.pdf"));
    assert_eq!(prepared.invoice.total.to_string(), "EUR 400.00");
    assert!(!setup.invoices().exists(), "prepare must not write anything");

    prepared.save().unwrap();
    assert_pdf(&prepared.path);
    let err = prepared.save().unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
}
