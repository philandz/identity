use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub environment: String,
}

impl AppInfo {
    pub fn user_agent(&self) -> String {
        format!("{}/{} ({})", self.name, self.version, self.environment)
    }
}

pub fn from_env_with_prefix(prefix: &str) -> AppInfo {
    let key = |suffix: &str| format!("{}_{}", prefix, suffix);

    let name = std::env::var(key("NAME")).unwrap_or_else(|_| "app".to_string());
    let version = std::env::var(key("VERSION")).unwrap_or_else(|_| "0.1.0".to_string());
    let environment = std::env::var(key("ENV")).unwrap_or_else(|_| "dev".to_string());

    AppInfo {
        name,
        version,
        environment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_user_agent() {
        let app = AppInfo {
            name: "identity".to_string(),
            version: "1.2.3".to_string(),
            environment: "sandbox".to_string(),
        };

        assert_eq!(app.user_agent(), "identity/1.2.3 (sandbox)");
    }

    #[test]
    fn uses_defaults_when_env_missing() {
        let app = from_env_with_prefix("TEST_APP_DEFAULTS");
        assert_eq!(app.name, "app");
        assert_eq!(app.version, "0.1.0");
        assert_eq!(app.environment, "dev");
    }
}
