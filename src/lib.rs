//! Generates a monthly, hours-based PDF invoice from a YAML config file.

pub mod config;
pub mod fonts;
pub mod invoice;
pub mod layout;
pub mod money;
pub mod pdf;

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use jiff::civil::Date;

pub use config::Config;
pub use invoice::Invoice;

/// An error with a message meant for the person running picoinvoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub fn new(message: impl Into<String>) -> Error {
        Error(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Renders an invoice as PDF bytes.
pub fn render(invoice: &Invoice, config: &Config) -> Result<Vec<u8>, Error> {
    let fonts = fonts::Fonts::embedded();
    let ops = layout::layout(invoice, config, &fonts);
    pdf::render(&ops, &fonts, &format!("Invoice {}", invoice.number))
}

/// An invoice with its number assigned, ready to be saved.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub config: Config,
    pub invoice: Invoice,
    pub path: PathBuf,
}

/// Loads the config and works out the next invoice without writing anything.
/// `hours` defaults to the config's `default_hours`; `home` expands `~` in `output`.
pub fn prepare(
    config_path: &Path,
    issue_date: Date,
    hours: Option<u32>,
    home: Option<&Path>,
) -> Result<Prepared, Error> {
    let config = Config::load(config_path)?;
    let dir = config.output_dir(home)?;
    let number = invoice::next_number(&dir, issue_date.year())?;
    let hours = hours.unwrap_or(config.default_hours);
    let invoice = Invoice::new(&config, number, issue_date, hours)?;
    let path = dir.join(invoice.file_name());
    Ok(Prepared { config, invoice, path })
}

impl Prepared {
    /// Writes the PDF, creating the output folder if needed. Never overwrites a file.
    pub fn save(&self) -> Result<(), Error> {
        let pdf = render(&self.invoice, &self.config)?;
        let cannot_write = |e: std::io::Error| Error::new(format!("Cannot write {}: {e}", self.path.display()));
        if let Some(dir) = self.path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(cannot_write)?;
        }
        let mut file =
            std::fs::OpenOptions::new().write(true).create_new(true).open(&self.path).map_err(|e| match e.kind() {
                std::io::ErrorKind::AlreadyExists => {
                    Error::new(format!("{} already exists, not overwriting it", self.path.display()))
                }
                _ => cannot_write(e),
            })?;
        if let Err(e) = file.write_all(&pdf).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = std::fs::remove_file(&self.path);
            return Err(cannot_write(e));
        }
        Ok(())
    }
}
