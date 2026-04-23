use regex::Regex;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("{0} is empty")]
    Empty(&'static str),
    #[error("invalid email")]
    InvalidEmail,
    #[error("invalid url")]
    InvalidUrl,
    #[error("weak password")]
    WeakPassword,
}

pub fn non_empty(name: &'static str, value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return Err(ValidationError::Empty(name));
    }
    Ok(())
}

pub fn email(value: &str) -> Result<(), ValidationError> {
    let re = Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").expect("valid regex");
    if re.is_match(value) {
        Ok(())
    } else {
        Err(ValidationError::InvalidEmail)
    }
}

pub fn url(value: &str) -> Result<(), ValidationError> {
    if value.starts_with("http://") || value.starts_with("https://") {
        Ok(())
    } else {
        Err(ValidationError::InvalidUrl)
    }
}

pub fn strong_password(value: &str) -> Result<(), ValidationError> {
    let has_upper = value.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = value.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = value.chars().any(|c| c.is_ascii_digit());
    let has_symbol = value.chars().any(|c| !c.is_ascii_alphanumeric());
    if value.len() >= 8 && has_upper && has_lower && has_digit && has_symbol {
        Ok(())
    } else {
        Err(ValidationError::WeakPassword)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_email() {
        assert!(email("a@b.com").is_ok());
        assert!(email("bad").is_err());
    }

    #[test]
    fn validates_password_strength() {
        assert!(strong_password("Aa@12345").is_ok());
        assert_eq!(
            strong_password("short").err(),
            Some(ValidationError::WeakPassword)
        );
    }
}
