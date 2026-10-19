use anyhow::{Result, bail};

/// Project and environment names: letters, digits, `.`, `_`, `-`.
pub fn validate_name(kind: &str, name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !ok {
        bail!("`{name}` isn't a valid {kind} name (use letters, digits, `.`, `_` or `-`)");
    }
    Ok(())
}

/// Secret names must be valid environment variable names.
pub fn validate_key(key: &str) -> Result<()> {
    let mut chars = key.chars();
    let ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ok {
        bail!("`{key}` isn't a valid variable name (use letters, digits and `_`, not starting with a digit)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert!(validate_key("OPENAI_API_KEY").is_ok());
        assert!(validate_key("_private").is_ok());
        assert!(validate_key("1KEY").is_err());
        assert!(validate_key("MY-KEY").is_err());
        assert!(validate_key("").is_err());
    }

    #[test]
    fn names() {
        assert!(validate_name("project", "my-app.v2").is_ok());
        assert!(validate_name("project", "my app").is_err());
        assert!(validate_name("project", "../etc").is_err());
    }
}
