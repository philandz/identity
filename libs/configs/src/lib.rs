use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing environment variable: {0}")]
    MissingVar(String),
    #[error("invalid value in {name}: {value}")]
    InvalidValue { name: String, value: String },
    #[error("http request failed")]
    Http(#[from] reqwest::Error),
    #[error("invalid consul payload: {0}")]
    InvalidConsulPayload(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub database_url: String,
    pub jwt_secret: String,
    pub harbor_host: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuth2GoogleConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityServiceConfig {
    pub database_url: String,
    pub grpc_host: String,
    pub grpc_port: u16,
    pub http_host: String,
    pub http_port: u16,
    pub jwt_secret: String,
    pub consul_addr: String,
    pub service_name: String,
    pub super_admin_email: String,
    pub super_admin_password: String,
    pub google_client_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IdentityTransportMode {
    ProxyHttp,
    GrpcTranscode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayServiceConfig {
    pub upstream_url: String,
    pub identity_url: String,
    pub identity_grpc_url: String,
    pub identity_transport: IdentityTransportMode,
    pub host: String,
    pub port: u16,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            jwt_secret: required("JWT_SECRET")?,
            harbor_host: env::var("HARBOR_HOST").ok(),
        })
    }
}

impl OAuth2GoogleConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            client_id: required("OAUTH2_GOOGLE_CLIENT_ID")?,
            client_secret: required("OAUTH2_GOOGLE_CLIENT_SECRET")?,
            redirect_uri: required("OAUTH2_GOOGLE_REDIRECT_URI")?,
        })
    }
}

impl IdentityServiceConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            grpc_host: read_or_default("GRPC_HOST", "127.0.0.1"),
            grpc_port: parse_u16_or_default("GRPC_PORT", 50051),
            http_host: read_or_default("HTTP_HOST", "127.0.0.1"),
            http_port: parse_u16_or_default("HTTP_PORT", 3001),
            jwt_secret: required("JWT_SECRET")?,
            consul_addr: read_or_default("CONSUL_ADDR", "http://127.0.0.1:8500"),
            service_name: read_or_default("SERVICE_NAME", "identity"),
            super_admin_email: read_or_default("SUPER_ADMIN_EMAIL", "laphi1612@gmail.com"),
            super_admin_password: read_or_default("SUPER_ADMIN_PASSWORD", "Aa@123456"),
        })
    }

    pub async fn register_consul(&self) -> Result<(), ConfigError> {
        let registration = serde_json::json!({
            "ID": format!("{}-{}", self.service_name, self.grpc_port),
            "Name": self.service_name,
            "Address": "127.0.0.1",
            "Port": self.grpc_port,
            "Check": {
                "HTTP": format!("http://127.0.0.1:{}/health", self.http_port),
                "Interval": "10s"
            }
        });

        let url = format!("{}/v1/agent/service/register", self.consul_addr);
        reqwest::Client::new()
            .put(url)
            .json(&registration)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn read_consul_kv(&self) -> Result<HashMap<String, String>, ConfigError> {
        let url = format!(
            "{}/v1/kv/config/{}/?recurse",
            self.consul_addr, self.service_name
        );

        let entries = reqwest::Client::new()
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<serde_json::Value>>()
            .await?;

        let mut kv = HashMap::new();
        for entry in entries {
            let key = entry
                .get("Key")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| ConfigError::InvalidConsulPayload("missing Key".to_string()))?;

            let value_b64 = entry
                .get("Value")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| ConfigError::InvalidConsulPayload("missing Value".to_string()))?;

            let decoded = base64_decode(value_b64)?;
            let short_key = key
                .strip_prefix(&format!("config/{}/", self.service_name))
                .unwrap_or(key);
            kv.insert(short_key.to_string(), decoded);
        }

        Ok(kv)
    }
}

impl IdentityTransportMode {
    pub fn from_env_value(value: &str) -> Self {
        match value {
            "grpc_transcode" => Self::GrpcTranscode,
            _ => Self::ProxyHttp,
        }
    }
}

impl GatewayServiceConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let identity_transport = IdentityTransportMode::from_env_value(&read_or_default(
            "IDENTITY_TRANSPORT",
            "grpc_transcode",
        ));

        Ok(Self {
            upstream_url: required("UPSTREAM_URL")?,
            identity_url: read_or_default("IDENTITY_URL", "http://127.0.0.1:3001"),
            identity_grpc_url: required("IDENTITY_GRPC_URL")?,
            identity_transport,
            host: read_or_default("HOST", "0.0.0.0"),
            port: parse_u16_or_default("PORT", 3000),
        })
    }
}

fn required(name: &str) -> Result<String, ConfigError> {
    env::var(name).map_err(|_| ConfigError::MissingVar(name.to_string()))
}

fn read_or_default(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

fn parse_u16_or_default(name: &str, default: u16) -> u16 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(default)
}

fn base64_decode(input: &str) -> Result<String, ConfigError> {
    use base64::Engine;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| ConfigError::InvalidConsulPayload(e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| ConfigError::InvalidConsulPayload(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().expect("lock")
    }

    #[test]
    fn reads_app_config_from_env() {
        let _guard = env_guard();
        std::env::set_var("DATABASE_URL", "mysql://u:p@localhost:3306/philand");
        std::env::set_var("JWT_SECRET", "secret");

        let cfg = AppConfig::from_env().expect("must parse");
        assert_eq!(cfg.database_url, "mysql://u:p@localhost:3306/philand");
        assert_eq!(cfg.jwt_secret, "secret");
    }

    #[test]
    fn requires_google_oauth_env() {
        let _guard = env_guard();
        std::env::remove_var("OAUTH2_GOOGLE_CLIENT_ID");
        let err = OAuth2GoogleConfig::from_env().expect_err("must fail");
        assert!(matches!(err, ConfigError::MissingVar(_)));
    }

    #[test]
    fn identity_requires_database_and_jwt() {
        let _guard = env_guard();
        std::env::remove_var("DATABASE_URL");
        std::env::remove_var("JWT_SECRET");
        let err = IdentityServiceConfig::from_env().expect_err("must fail");
        assert!(matches!(err, ConfigError::MissingVar(_)));
    }

    #[test]
    fn identity_service_config_applies_defaults() {
        let _guard = env_guard();
        std::env::set_var("DATABASE_URL", "mysql://u:p@localhost:3306/philand");
        std::env::set_var("JWT_SECRET", "secret");
        std::env::remove_var("GRPC_PORT");
        std::env::remove_var("HTTP_PORT");
        std::env::remove_var("SERVICE_NAME");

        let cfg = IdentityServiceConfig::from_env().expect("must parse");
        assert_eq!(cfg.grpc_port, 50051);
        assert_eq!(cfg.http_port, 3001);
        assert_eq!(cfg.service_name, "identity");
    }

    #[test]
    fn gateway_transport_parsing_defaults_to_proxy_http() {
        assert_eq!(
            IdentityTransportMode::from_env_value("grpc_transcode"),
            IdentityTransportMode::GrpcTranscode
        );
        assert_eq!(
            IdentityTransportMode::from_env_value("anything_else"),
            IdentityTransportMode::ProxyHttp
        );
    }

    #[test]
    fn gateway_requires_upstream_and_identity_grpc() {
        let _guard = env_guard();
        std::env::remove_var("UPSTREAM_URL");
        std::env::remove_var("IDENTITY_GRPC_URL");
        let err = GatewayServiceConfig::from_env().expect_err("must fail");
        assert!(matches!(err, ConfigError::MissingVar(_)));
    }
}
