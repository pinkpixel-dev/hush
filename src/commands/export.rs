use std::io::Write;

use anyhow::{Result, bail};

use super::scope;
use crate::cli::Target;
use crate::paths::Paths;
use crate::vault::store;

/// Prints the project's secrets in .env format on stdout. hush never writes
/// the plaintext file itself, so where it ends up is the caller's choice.
pub fn run(paths: &Paths, target: Target) -> Result<()> {
    let (project, env) = scope(target)?;
    let (vault, _) = store::open(paths)?;
    let Some(secrets) = vault.secrets(&project, &env).filter(|s| !s.is_empty()) else {
        bail!("no secrets in {project}/{env} to export");
    };
    let mut out = std::io::stdout().lock();
    for (key, value) in secrets {
        writeln!(out, "{key}={}", quote(value))?;
    }
    out.flush()?;
    Ok(())
}

/// Quotes a value so dotenv parsers (including `hush import`) read it back
/// exactly: bare when it's plain, single quotes when that's enough, and double
/// quotes with escapes for values holding `'` or line breaks.
fn quote(value: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "_-./:@+,=%".contains(c);
    if !value.is_empty() && value.chars().all(plain) {
        return value.to_string();
    }
    if !value.contains(['\'', '\n', '\r']) {
        return format!("'{value}'");
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for c in value.chars() {
        match c {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '$' => quoted.push_str("\\$"),
            '\n' => quoted.push_str("\\n"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_values_round_trip_through_dotenvy() {
        let values = [
            "plain-value_1.2:3@x+y,z=%",
            "",
            "has spaces",
            "has # hash",
            "$HOME and ${PATH}",
            r"back\slash",
            r#"double "quotes""#,
            "it's single",
            "it's $both \"kinds\" \\ here",
            "line one\nline two",
            "crlf\r\nend",
            " leading and trailing ",
            "emoji ✓ ok",
        ];
        let mut file = String::new();
        for (i, value) in values.iter().enumerate() {
            file.push_str(&format!("K{i}={}\n", quote(value)));
        }
        let parsed: Vec<(String, String)> =
            dotenvy::from_read_iter(file.as_bytes()).map(|item| item.unwrap()).collect();
        assert_eq!(parsed.len(), values.len(), "{file}");
        for (i, value) in values.iter().enumerate() {
            assert_eq!(parsed[i], (format!("K{i}"), value.to_string()), "in:\n{file}");
        }
    }

    #[test]
    fn plain_values_stay_bare() {
        assert_eq!(quote("sk-test_123"), "sk-test_123");
        assert_eq!(quote("postgres://u@host:5432/db"), "postgres://u@host:5432/db");
        assert_eq!(quote("two words"), "'two words'");
    }
}
