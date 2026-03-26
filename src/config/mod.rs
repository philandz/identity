use serde::Deserialize;
use std::env;

/// Typed application configuration loaded from environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    pub grpc_host: String,
    pub grpc_port: u16,
    pub http_host: String,
    pub http_port: u16,
    pub jwt_secret: String,
    pub consul_addr: String,
    pub service_name: String,
    /// Email for the initial super-admin user (created on first startup).
    pub super_admin_email: String,
    /// Password for the initial super-admin user.
    pub super_admin_password: String,
}

impl AppConfig {
    /// Load configuration from environment variables with sensible defaults.
    pub fn from_env() -> Self {
        Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "mysql://philand:philand@localhost:3306/philand".to_string()),
            grpc_host: env::var("GRPC_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            grpc_port: env::var("GRPC_PORT")
                .unwrap_or_else(|_| "50051".to_string())
                .parse()
                .unwrap_or(50051),
            http_host: env::var("HTTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: env::var("HTTP_PORT")
                .unwrap_or_else(|_| "3001".to_string())
                .parse()
                .unwrap_or(3001),
            jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string()),
            consul_addr: env::var("CONSUL_ADDR")
                .unwrap_or_else(|_| "http://127.0.0.1:8500".to_string()),
            service_name: "identity".to_string(),
            super_admin_email: env::var("SUPER_ADMIN_EMAIL")
                .unwrap_or_else(|_| "laphi1612@gmail.com".to_string()),
            super_admin_password: env::var("SUPER_ADMIN_PASSWORD")
                .unwrap_or_else(|_| "Aa@123456".to_string()),
        }
    }

    /// Register this service instance with Consul so the gateway can discover it.
    pub async fn register_consul(&self) -> anyhow::Result<()> {
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
        let resp = reqwest::Client::new()
            .put(&url)
            .json(&registration)
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                tracing::info!(
                    "Registered with Consul as {}-{}",
                    self.service_name,
                    self.grpc_port
                );
                Ok(())
            }
            Ok(r) => {
                tracing::warn!(
                    "Consul registration returned {}: continuing without Consul",
                    r.status()
                );
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    "Could not reach Consul at {}: {}. Continuing without Consul.",
                    self.consul_addr,
                    e
                );
                Ok(())
            }
        }
    }

    /// Read runtime configuration overrides from Consul KV (best-effort).
    /// Returns key-value pairs from `config/{service_name}/`.
    pub async fn read_consul_kv(&self) -> std::collections::HashMap<String, String> {
        let mut kv = std::collections::HashMap::new();
        let url = format!(
            "{}/v1/kv/config/{}/?recurse",
            self.consul_addr, self.service_name
        );

        let resp = match reqwest::Client::new().get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(_) => {
                tracing::debug!("No Consul KV keys found for {}", self.service_name);
                return kv;
            }
            Err(e) => {
                tracing::debug!("Consul KV read failed: {}", e);
                return kv;
            }
        };

        // Consul returns an array of {Key, Value (base64)} objects
        if let Ok(entries) = resp.json::<Vec<serde_json::Value>>().await {
            for entry in entries {
                if let (Some(key), Some(value_b64)) =
                    (entry["Key"].as_str(), entry["Value"].as_str())
                {
                    if let Ok(decoded) = base64_decode(value_b64) {
                        let short_key = key
                            .strip_prefix(&format!("config/{}/", self.service_name))
                            .unwrap_or(key);
                        kv.insert(short_key.to_string(), decoded);
                    }
                }
            }
        }

        tracing::info!("Loaded {} Consul KV entries", kv.len());
        kv
    }
}

/// Simple base64 decode (standard encoding). Consul stores values base64-encoded.
fn base64_decode(input: &str) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}
