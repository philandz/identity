use rusty_s3::{
    actions::{GetObject, PutObject},
    Bucket, BucketError, Credentials, S3Action, UrlStyle,
};
use sqlx::mysql::{MySqlPoolOptions, MySqlSslMode};
use sqlx::MySqlPool;
use std::time::Duration;
use thiserror::Error;
use url::Url;

pub mod repo;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlx error")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration error")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("invalid endpoint url")]
    Url(#[from] url::ParseError),
    #[error("invalid bucket config")]
    Bucket(#[from] BucketError),
    #[error("invalid storage config: {0}")]
    InvalidConfig(String),
    #[error("invalid object key: {0}")]
    InvalidKey(String),
}

#[derive(Debug, Clone)]
pub struct MySqlConfig {
    pub database_url: String,
    pub min_connections: u32,
    pub max_connections: u32,
    pub connect_timeout: Duration,
    pub max_retries: u8,
    pub retry_backoff: Duration,
}

impl Default for MySqlConfig {
    fn default() -> Self {
        Self {
            database_url: "mysql://root:root@localhost:3306/philand".to_string(),
            min_connections: 1,
            max_connections: 10,
            connect_timeout: Duration::from_secs(10),
            max_retries: 2,
            retry_backoff: Duration::from_millis(250),
        }
    }
}

impl MySqlConfig {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.database_url.trim().is_empty() {
            return Err(StorageError::InvalidConfig(
                "database_url must not be empty".to_string(),
            ));
        }
        if self.min_connections > self.max_connections {
            return Err(StorageError::InvalidConfig(
                "min_connections must be <= max_connections".to_string(),
            ));
        }
        Ok(())
    }
}

pub async fn mysql_pool(cfg: &MySqlConfig) -> Result<MySqlPool, StorageError> {
    cfg.validate()?;
    Ok(MySqlPoolOptions::new()
        .min_connections(cfg.min_connections)
        .max_connections(cfg.max_connections)
        .acquire_timeout(cfg.connect_timeout)
        .connect_with(
            cfg.database_url
                .parse::<sqlx::mysql::MySqlConnectOptions>()?
                .ssl_mode(MySqlSslMode::Preferred),
        )
        .await?)
}

pub async fn mysql_pool_with_retry(cfg: &MySqlConfig) -> Result<MySqlPool, StorageError> {
    let mut attempts: u8 = 0;
    loop {
        match mysql_pool(cfg).await {
            Ok(pool) => return Ok(pool),
            Err(err) => {
                if attempts >= cfg.max_retries {
                    return Err(err);
                }
                attempts += 1;
                tokio::time::sleep(cfg.retry_backoff).await;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    pub force_path_style: bool,
}

impl S3Config {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.endpoint.trim().is_empty() {
            return Err(StorageError::InvalidConfig(
                "endpoint must not be empty".to_string(),
            ));
        }
        if self.region.trim().is_empty() {
            return Err(StorageError::InvalidConfig(
                "region must not be empty".to_string(),
            ));
        }
        if self.bucket.trim().is_empty() {
            return Err(StorageError::InvalidConfig(
                "bucket must not be empty".to_string(),
            ));
        }
        if self.access_key.trim().is_empty() || self.secret_key.trim().is_empty() {
            return Err(StorageError::InvalidConfig(
                "access_key/secret_key must not be empty".to_string(),
            ));
        }
        Url::parse(&self.endpoint)?;
        Ok(())
    }

    pub fn credentials(&self) -> Credentials {
        Credentials::new(self.access_key.clone(), self.secret_key.clone())
    }

    pub fn bucket_def(&self) -> Result<Bucket, StorageError> {
        self.validate()?;
        let endpoint = Url::parse(&self.endpoint)?;
        let style = if self.force_path_style {
            UrlStyle::Path
        } else {
            UrlStyle::VirtualHost
        };
        Ok(Bucket::new(
            endpoint,
            style,
            self.bucket.clone(),
            self.region.clone(),
        )?)
    }
}

pub fn normalize_key(key: &str) -> Result<String, StorageError> {
    let trimmed = key.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return Err(StorageError::InvalidKey("key is empty".to_string()));
    }
    if trimmed.contains("..") {
        return Err(StorageError::InvalidKey(
            "key must not contain '..'".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn presign_get_object(
    cfg: &S3Config,
    key: &str,
    expires_in: Duration,
) -> Result<Url, StorageError> {
    let key = normalize_key(key)?;
    let bucket = cfg.bucket_def()?;
    let creds = cfg.credentials();
    let action = GetObject::new(&bucket, Some(&creds), &key);
    Ok(action.sign(expires_in))
}

pub fn presign_put_object(
    cfg: &S3Config,
    key: &str,
    expires_in: Duration,
) -> Result<Url, StorageError> {
    let key = normalize_key(key)?;
    let bucket = cfg.bucket_def()?;
    let creds = cfg.credentials();
    let action = PutObject::new(&bucket, Some(&creds), &key);
    Ok(action.sign(expires_in))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_default_uses_philand_db() {
        let cfg = MySqlConfig::default();
        assert!(cfg.database_url.ends_with("/philand"));
    }

    #[test]
    fn mysql_validation_checks_connection_bounds() {
        let cfg = MySqlConfig {
            min_connections: 10,
            max_connections: 2,
            ..MySqlConfig::default()
        };
        let err = cfg.validate().expect_err("must fail");
        assert!(matches!(err, StorageError::InvalidConfig(_)));
    }

    #[test]
    fn normalizes_and_validates_s3_key() {
        assert_eq!(
            normalize_key("/avatars/u.png").expect("ok"),
            "avatars/u.png"
        );
        let err = normalize_key("../x").expect_err("must fail");
        assert!(matches!(err, StorageError::InvalidKey(_)));
    }

    #[test]
    fn can_create_presigned_url_for_custom_endpoint() {
        let cfg = S3Config {
            endpoint: "http://127.0.0.1:9000".to_string(),
            region: "ap-southeast-1".to_string(),
            access_key: "minio".to_string(),
            secret_key: "minio123".to_string(),
            bucket: "images".to_string(),
            force_path_style: true,
        };

        let signed = presign_get_object(&cfg, "avatars/user.png", Duration::from_secs(600))
            .expect("must sign");
        let signed_str = signed.as_str();
        assert!(signed_str.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(
            signed_str.contains("avatars%2Fuser.png") || signed_str.contains("avatars/user.png")
        );
    }

    #[test]
    fn can_create_presigned_put_url_for_custom_endpoint() {
        let cfg = S3Config {
            endpoint: "http://127.0.0.1:9000".to_string(),
            region: "ap-southeast-1".to_string(),
            access_key: "minio".to_string(),
            secret_key: "minio123".to_string(),
            bucket: "images".to_string(),
            force_path_style: true,
        };

        let signed = presign_put_object(&cfg, "/avatars/new.png", Duration::from_secs(900))
            .expect("must sign");
        let signed_str = signed.as_str();
        assert!(signed_str.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(signed_str.contains("avatars%2Fnew.png") || signed_str.contains("avatars/new.png"));
    }
}
