use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer};

use crate::Error;
use crate::money::Money;

pub const FILE_NAME: &str = "picoinvoice.yml";

/// The contents of `picoinvoice.yml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Folder the invoices are saved to. `~` expands to the home folder.
    pub output: String,
    pub from: Sender,
    pub bill_to: Party,
    pub item: Item,
    #[serde(default = "default_payment_days")]
    pub payment_days: u32,
    #[serde(default = "default_hours")]
    pub default_hours: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sender {
    pub name: String,
    #[serde(default)]
    pub address: Address,
    pub registration_number: Option<String>,
    pub tax_number: Option<String>,
    /// Extra lines printed under the sender's details.
    #[serde(default)]
    pub notes: Vec<String>,
    pub bank_details: Option<BankDetails>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Party {
    pub name: String,
    #[serde(default)]
    pub address: Address,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Address {
    pub street: Option<String>,
    pub city: Option<String>,
    pub province: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BankDetails {
    pub bank_name: Option<String>,
    pub account_holder_name: Option<String>,
    pub account_number: Option<String>,
    pub swift_code: Option<String>,
    pub address: Option<Address>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub description: String,
    #[serde(deserialize_with = "deserialize_money")]
    pub unit_price: Money,
}

fn default_payment_days() -> u32 {
    14
}

fn default_hours() -> u32 {
    160
}

fn deserialize_money<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Money, D::Error> {
    let value = String::deserialize(deserializer)?;
    Money::parse(&value).map_err(serde::de::Error::custom)
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, Error> {
        let text = std::fs::read_to_string(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::new(format!(
                "No config file at {}. Create one from examples/picoinvoice.yml, or pass --config PATH.",
                path.display()
            )),
            _ => Error::new(format!("Cannot read {}: {e}", path.display())),
        })?;
        Config::parse(&text).map_err(|e| Error::new(format!("Invalid config {}: {e}", path.display())))
    }

    pub fn parse(text: &str) -> Result<Config, String> {
        let config: Config = serde_saphyr::from_str(text).map_err(|e| e.to_string())?;
        if config.output.trim().is_empty() {
            return Err("`output` must not be empty".into());
        }
        if config.default_hours == 0 {
            return Err("`default_hours` must be a positive integer".into());
        }
        Ok(config)
    }

    /// The output folder, with a leading `~` expanded to `home`.
    pub fn output_dir(&self, home: Option<&Path>) -> Result<PathBuf, Error> {
        let output = self.output.trim();
        let rest = match output.strip_prefix('~') {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => rest.trim_start_matches('/'),
            _ => return Ok(PathBuf::from(output)),
        };
        let home = home.ok_or_else(|| Error::new("Cannot expand `~` in `output`: HOME is not set"))?;
        Ok(if rest.is_empty() { home.to_path_buf() } else { home.join(rest) })
    }
}

/// `$XDG_CONFIG_HOME/picoinvoice.yml`, falling back to `~/.config/picoinvoice.yml`.
/// A relative `XDG_CONFIG_HOME` is ignored, as the XDG spec requires.
pub fn default_path(xdg_config_home: Option<OsString>, home: Option<OsString>) -> Result<PathBuf, Error> {
    if let Some(dir) = xdg_config_home.map(PathBuf::from).filter(|dir| dir.is_absolute()) {
        return Ok(dir.join(FILE_NAME));
    }
    match home.filter(|home| !home.is_empty()) {
        Some(home) => Ok(PathBuf::from(home).join(".config").join(FILE_NAME)),
        None => Err(Error::new("Cannot find the config file: neither XDG_CONFIG_HOME nor HOME is set")),
    }
}

impl Address {
    /// Street, `city, province, postal code`, and country; empty parts are skipped.
    pub fn lines(&self) -> Vec<String> {
        fn part(value: &Option<String>) -> Option<&str> {
            value.as_deref().map(str::trim).filter(|v| !v.is_empty())
        }
        let mut lines = Vec::new();
        lines.extend(part(&self.street).map(String::from));
        let city_line: Vec<&str> =
            [&self.city, &self.province, &self.postal_code].into_iter().filter_map(part).collect();
        if !city_line.is_empty() {
            lines.push(city_line.join(", "));
        }
        lines.extend(part(&self.country).map(String::from));
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "
output: ~/Invoices
from:
  name: Jana
bill_to:
  name: Acme
item:
  description: Consulting
  unit_price: EUR 40.00
";

    #[test]
    fn parses_example_config() {
        let config = Config::parse(include_str!("../examples/picoinvoice.yml")).unwrap();
        assert_eq!(config.from.name, "Jana Nováková");
        assert_eq!(config.from.registration_number.as_deref(), Some("12345678"));
        assert_eq!(config.from.notes, ["Reverse charge / VAT not applicable"]);
        assert_eq!(config.bill_to.address.province.as_deref(), Some("ON"));
        assert_eq!(config.item.unit_price, Money { currency: "EUR".into(), cents: 4_500 });
        assert_eq!(config.payment_days, 14);
        assert_eq!(config.default_hours, 160);
    }

    #[test]
    fn applies_defaults() {
        let config = Config::parse(MINIMAL).unwrap();
        assert_eq!(config.payment_days, 14);
        assert_eq!(config.default_hours, 160);
        assert!(config.from.notes.is_empty());
        assert!(config.from.bank_details.is_none());
        assert!(config.bill_to.address.lines().is_empty());
    }

    #[test]
    fn keeps_unquoted_scalars_as_written() {
        let text = MINIMAL.replace(
            "  name: Acme\n",
            "  name: Acme\n  address:\n    province: ON\n    postal_code: 01001\n    country: yes\n",
        ) + "\n";
        let text = text.replace("  name: Jana\n", "  name: Jana\n  registration_number: 87654321\n");
        let config = Config::parse(&text).unwrap();
        assert_eq!(config.from.registration_number.as_deref(), Some("87654321"));
        assert_eq!(config.bill_to.address.lines(), ["ON, 01001", "yes"]);
    }

    #[test]
    fn rejects_unknown_keys() {
        let err = Config::parse(&format!("{MINIMAL}paymnet_days: 30\n")).unwrap_err();
        assert!(err.contains("paymnet_days"), "{err}");
    }

    #[test]
    fn rejects_missing_output() {
        let err = Config::parse(&MINIMAL.replace("output: ~/Invoices\n", "")).unwrap_err();
        assert!(err.contains("output"), "{err}");
        let err = Config::parse(&MINIMAL.replace("~/Invoices", "\"\"")).unwrap_err();
        assert!(err.contains("output"), "{err}");
    }

    #[test]
    fn rejects_invalid_values() {
        let err = Config::parse(&MINIMAL.replace("EUR 40.00", "forty")).unwrap_err();
        assert!(err.contains("forty"), "{err}");
        let err = Config::parse(&format!("{MINIMAL}default_hours: 0\n")).unwrap_err();
        assert!(err.contains("default_hours"), "{err}");
    }

    #[test]
    fn expands_home_in_output() {
        let config = |output: &str| Config::parse(&MINIMAL.replace("~/Invoices", output)).unwrap();
        let home = Path::new("/home/jana");
        assert_eq!(config("~/Invoices").output_dir(Some(home)).unwrap(), Path::new("/home/jana/Invoices"));
        assert_eq!(config("\"~\"").output_dir(Some(home)).unwrap(), Path::new("/home/jana"));
        assert_eq!(config("/srv/invoices").output_dir(None).unwrap(), Path::new("/srv/invoices"));
        assert_eq!(config("invoices").output_dir(None).unwrap(), Path::new("invoices"));
        assert_eq!(config("~jana/x").output_dir(Some(home)).unwrap(), Path::new("~jana/x"));
        assert!(config("~/Invoices").output_dir(None).is_err());
    }

    #[test]
    fn finds_default_path() {
        let os = |s: &str| Some(OsString::from(s));
        assert_eq!(default_path(os("/xdg"), os("/home/jana")).unwrap(), Path::new("/xdg/picoinvoice.yml"));
        assert_eq!(default_path(None, os("/home/jana")).unwrap(), Path::new("/home/jana/.config/picoinvoice.yml"));
        assert_eq!(
            default_path(os("relative"), os("/home/jana")).unwrap(),
            Path::new("/home/jana/.config/picoinvoice.yml")
        );
        assert!(default_path(None, None).is_err());
    }

    #[test]
    fn reports_missing_config_file() {
        let err = Config::load(Path::new("/nonexistent/picoinvoice.yml")).unwrap_err();
        assert!(err.to_string().contains("No config file at /nonexistent/picoinvoice.yml"), "{err}");
    }

    #[test]
    fn formats_address_lines() {
        let address = Address {
            street: Some("100 King St W".into()),
            city: Some("Toronto".into()),
            province: Some("ON".into()),
            postal_code: Some("M5X 1A9".into()),
            country: Some("Canada".into()),
        };
        assert_eq!(address.lines(), ["100 King St W", "Toronto, ON, M5X 1A9", "Canada"]);

        let address = Address { city: Some("Berlin".into()), postal_code: Some(" ".into()), ..Default::default() };
        assert_eq!(address.lines(), ["Berlin"]);
    }
}
