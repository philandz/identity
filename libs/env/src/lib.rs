use thiserror::Error;

#[derive(Debug, Error)]
pub enum EnvError {
    #[error("missing env var: {0}")]
    Missing(String),
}

pub fn required(name: &str) -> Result<String, EnvError> {
    std::env::var(name).map_err(|_| EnvError::Missing(name.to_string()))
}

pub fn bool_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_flag_works() {
        std::env::set_var("X_FLAG", "true");
        assert!(bool_flag("X_FLAG", false));
    }
}
