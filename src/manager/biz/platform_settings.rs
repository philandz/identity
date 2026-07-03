//! Platform-wide settings (Super Admin → Global Settings).
//!
//! Two tables back this module:
//!   * `platform_settings`         — encrypted secrets (Resend API key), AES-GCM ciphertext.
//!   * `platform_settings_public`  — non-secret JSON config (mail config + system env).
//!
//! The 4 system-environment fields (`app_public_base_url`, `support_email`,
//! `default_locale`, `mail_from_address`) live in a `LiveConfig` overlay
//! inside `IdentityBiz`. The overlay is seeded from env at startup and
//! reloaded from the DB by the Super Admin → System Config RPCs — no service
//! restart needed when the admin updates them.

use serde::{Deserialize, Serialize};
use tonic::Status;

use crate::manager::validate;
use crate::pb::service::identity::{
    GetResendConfigResponse, GetSystemConfigResponse, TestResendConfigResponse,
    UpdateResendConfigResponse, UpdateSystemConfigResponse,
};

use super::IdentityBiz;

/// Key used to store the Resend API key in the encrypted `platform_settings`
/// table. Used as the AAD for AES-GCM so a ciphertext can't be replayed into
/// a future `smtp_password` row.
const RESEND_API_KEY: &str = "resend_api_key";

/// Key used to store the public mail config JSON in
/// `platform_settings_public`.
const MAIL_CONFIG: &str = "mail_config";

/// Key used to store the public system-environment config (URLs, support
/// email, locale). Non-secret so it lives in `platform_settings_public`.
const SYSTEM_CONFIG: &str = "system_config";

/// Source a particular system-config field is being read from. Reported back
/// to the UI so the admin knows which fields are DB-managed vs. env-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    Db,
    Env,
    Default,
}

impl ConfigSource {
    pub fn as_label(self) -> &'static str {
        match self {
            ConfigSource::Db => "db",
            ConfigSource::Env => "env",
            ConfigSource::Default => "default",
        }
    }
}

/// Resolved value for a single system-config field.
#[derive(Debug, Clone)]
pub struct ResolvedField<T> {
    pub value: T,
    pub source: ConfigSource,
}

impl<T: ToString> ResolvedField<T> {
    pub fn source_label(&self) -> String {
        self.source.as_label().to_string()
    }
}

/// JSON shape stored in `platform_settings_public[system_config]`. All fields
/// are optional in the persisted blob — any missing field falls back to env
/// (and then a hard-coded default).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemConfigBlob {
    #[serde(default)]
    pub app_public_base_url: Option<String>,
    #[serde(default)]
    pub support_email: Option<String>,
    #[serde(default)]
    pub default_locale: Option<String>,
    #[serde(default)]
    pub mail_from_address: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MailPublicConfig {
    pub from_address: String,
    #[serde(default)]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResendKeySource {
    Db,
    Env,
    None,
}

fn source_label(s: ResendKeySource) -> &'static str {
    match s {
        ResendKeySource::Db => "db",
        ResendKeySource::Env => "env",
        ResendKeySource::None => "none",
    }
}

fn mask_key(key: &str) -> String {
    if key.len() <= 6 {
        return "***".to_string();
    }
    let last4 = &key[key.len() - 4..];
    format!("***{last4}")
}

impl IdentityBiz {
    // -----------------------------------------------------------------------
    // Resend configuration (encrypted API key + public mail config)
    // -----------------------------------------------------------------------

    /// Read the public Resend config + masked key, plus which source is in
    /// use. Always returns a value; never exposes the raw key.
    pub async fn get_resend_config(&self) -> Result<GetResendConfigResponse, Status> {
        let public = self
            .repo
            .get_platform_setting_public(MAIL_CONFIG)
            .await
            .map_err(Self::map_internal_error)?;
        let cfg = public
            .and_then(|r| serde_json::from_value::<MailPublicConfig>(r.value_json).ok())
            .unwrap_or_default();

        let (source, masked) = match self.repo.get_platform_setting(RESEND_API_KEY).await {
            Ok(Some(row)) => match self.decrypt_api_key(&row.value_ciphertext) {
                Ok(plain) => (ResendKeySource::Db, Some(mask_key(&plain))),
                Err(_) => (ResendKeySource::Db, Some("re_***".to_string())),
            },
            Ok(None) | Err(_) => match std::env::var("RESEND_API_KEY") {
                Ok(v) if !v.trim().is_empty() => (ResendKeySource::Env, Some(mask_key(&v))),
                _ => (ResendKeySource::None, None),
            },
        };

        Ok(GetResendConfigResponse {
            configured: source != ResendKeySource::None,
            source: source_label(source).to_string(),
            masked_key: masked.unwrap_or_default(),
            from_address: cfg.from_address,
            reply_to: cfg.reply_to.unwrap_or_default(),
        })
    }

    /// Persist a new Resend config. `api_key` is encrypted at rest and never
    /// returned through any RPC.
    pub async fn update_resend_config(
        &self,
        caller_user_id: &str,
        api_key: Option<&str>,
        from_address: &str,
        reply_to: Option<&str>,
    ) -> Result<UpdateResendConfigResponse, Status> {
        self.require_permission(caller_user_id, crate::manager::biz::authz::Permission::ManageAnyUser)
            .await?;

        validate::resend_public_config(from_address, reply_to)?;
        if let Some(k) = api_key {
            validate::resend_api_key(k)?;
        }

        if let Some(k) = api_key {
            let envelope = philand_crypto::aes_gcm_encrypt_or_err(
                k.as_bytes(),
                &self.config.platform_master_key,
                RESEND_API_KEY.as_bytes(),
            )
            .map_err(Self::map_internal_error)?;
            self.repo
                .upsert_platform_setting(RESEND_API_KEY, &envelope, Some(caller_user_id))
                .await
                .map_err(Self::map_internal_error)?;
        }

        let existing = self
            .repo
            .get_platform_setting_public(MAIL_CONFIG)
            .await
            .map_err(Self::map_internal_error)?
            .and_then(|r| serde_json::from_value::<MailPublicConfig>(r.value_json).ok())
            .unwrap_or_default();

        let enabled = api_key.is_some() || !existing.from_address.is_empty();
        let next = MailPublicConfig {
            from_address: from_address.trim().to_string(),
            reply_to: reply_to.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            enabled,
        };
        self.repo
            .upsert_platform_setting_public(
                MAIL_CONFIG,
                &serde_json::to_value(&next).map_err(Self::map_internal_error)?,
                Some(caller_user_id),
            )
            .await
            .map_err(Self::map_internal_error)?;

        let current = self.get_resend_config().await?;
        Ok(UpdateResendConfigResponse {
            current: Some(current),
        })
    }

    /// Resolve the current API key (DB → env → None) by decrypting the DB
    /// row. Used by the notify worker to construct the Resend client.
    pub async fn resolve_resend_api_key(&self) -> Result<Option<String>, Status> {
        if let Ok(Some(row)) = self.repo.get_platform_setting(RESEND_API_KEY).await {
            match self.decrypt_api_key(&row.value_ciphertext) {
                Ok(plain) => return Ok(Some(plain)),
                Err(e) => {
                    tracing::warn!(
                        "failed to decrypt platform_settings.resend_api_key (master key mismatch?): {e}"
                    );
                }
            }
        }
        Ok(std::env::var("RESEND_API_KEY")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()))
    }

    /// Read the public mail config (from address, reply-to, enabled).
    pub async fn get_mail_public_config(&self) -> Result<MailPublicConfig, Status> {
        let row = self
            .repo
            .get_platform_setting_public(MAIL_CONFIG)
            .await
            .map_err(Self::map_internal_error)?;
        Ok(row
            .and_then(|r| serde_json::from_value::<MailPublicConfig>(r.value_json).ok())
            .unwrap_or_default())
    }

    /// Send a one-off test message using the current Resend config.
    pub async fn test_resend_config(
        &self,
        caller_user_id: &str,
        recipient_email: &str,
    ) -> Result<TestResendConfigResponse, Status> {
        self.require_permission(caller_user_id, crate::manager::biz::authz::Permission::ManageAnyUser)
            .await?;
        validate::email(recipient_email)?;

        let cfg = self.get_mail_public_config().await?;
        let live = self.live_config_snapshot().await;
        let from = if !cfg.from_address.is_empty() {
            cfg.from_address
        } else {
            live.mail_from_address.clone()
        };
        let reply_to = cfg
            .reply_to
            .clone()
            .or_else(|| Some(live.support_email.clone()));

        let rendered = philand_notify::render_password_change_otp(
            philand_notify::PasswordChangeOtpVars {
                display_name: None,
                code: "000000",
                ttl_human: "10 minutes",
                expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
                support_email: &live.support_email,
            },
        );
        let msg = rendered.into_mail(recipient_email.to_string(), from, reply_to);
        let receipt = self
            .mailer
            .send(msg)
            .await
            .map_err(|e| Status::failed_precondition(format!("mailer send failed: {e}")))?;
        Ok(TestResendConfigResponse {
            message_id: receipt.message_id,
        })
    }

    // -----------------------------------------------------------------------
    // System configuration — Super Admin → Global Settings → System
    // -----------------------------------------------------------------------

    /// Read the live system config (DB-stored values + env fallback) and
    /// serialize it for the gRPC/REST layer.
    pub async fn get_system_config(&self) -> Result<GetSystemConfigResponse, Status> {
        let blob = self.load_system_config_blob().await?;
        let env = &self.config;
        let live = self.live_config_snapshot().await;

        let url = resolve_field(
            blob.app_public_base_url.as_deref(),
            Some(env.app_public_base_url.as_str()),
            Some(live.app_public_base_url.as_str()),
            "http://localhost:3000",
        );
        let support = resolve_field(
            blob.support_email.as_deref(),
            Some(env.support_email.as_str()),
            Some(live.support_email.as_str()),
            "support@philandz.com",
        );
        let locale = resolve_field(
            blob.default_locale.as_deref(),
            Some(env.default_locale.as_str()),
            Some(live.default_locale.as_str()),
            "en",
        );
        let mail_from = resolve_field(
            blob.mail_from_address.as_deref(),
            Some(env.mail_from_address.as_str()),
            Some(live.mail_from_address.as_str()),
            "Philandz <noreply@philandz.com>",
        );

        Ok(GetSystemConfigResponse {
            app_public_base_url: url.0,
            support_email: support.0,
            default_locale: locale.0,
            mail_from_address: mail_from.0,
            source_app_public_base_url: url.1,
            source_support_email: support.1,
            source_default_locale: locale.1,
            source_mail_from_address: mail_from.1,
        })
    }

    /// Persist a new system config. The DB-stored values take precedence over
    /// env on next read; no service restart needed.
    pub async fn update_system_config(
        &self,
        caller_user_id: &str,
        app_public_base_url: String,
        support_email: String,
        default_locale: String,
        mail_from_address: String,
    ) -> Result<UpdateSystemConfigResponse, Status> {
        self.require_permission(caller_user_id, crate::manager::biz::authz::Permission::ManageAnyUser)
            .await?;
        validate::system_config(
            &app_public_base_url,
            &support_email,
            &default_locale,
            &mail_from_address,
        )?;

        let blob = SystemConfigBlob {
            app_public_base_url: Some(app_public_base_url),
            support_email: Some(support_email),
            default_locale: Some(default_locale),
            mail_from_address: Some(mail_from_address),
        };
        self.repo
            .upsert_platform_setting_public(
                SYSTEM_CONFIG,
                &serde_json::to_value(&blob).map_err(Self::map_internal_error)?,
                Some(caller_user_id),
            )
            .await
            .map_err(Self::map_internal_error)?;

        // Live-reload so the notify worker + every subsequent read pick up
        // the new value without a service restart.
        self.reload_system_config_into_live().await?;

        Ok(UpdateSystemConfigResponse {
            current: Some(self.get_system_config().await?),
        })
    }

    /// Load the DB-stored system config and overlay it onto the live
    /// in-memory config so subsequent reads (e.g. the notify worker building
    /// email links) use the freshly-saved values.
    async fn reload_system_config_into_live(&self) -> Result<(), Status> {
        let blob = self.load_system_config_blob().await?;
        let env = &self.config;
        let mut live = self.live_config.write().await;

        if let Some(v) = blob.app_public_base_url.filter(|s| !s.is_empty()) {
            live.app_public_base_url = v;
        } else if !env.app_public_base_url.is_empty() {
            live.app_public_base_url = env.app_public_base_url.clone();
        }
        if let Some(v) = blob.support_email.filter(|s| !s.is_empty()) {
            live.support_email = v;
        } else if !env.support_email.is_empty() {
            live.support_email = env.support_email.clone();
        }
        if let Some(v) = blob.default_locale.filter(|s| !s.is_empty()) {
            live.default_locale = v;
        } else if !env.default_locale.is_empty() {
            live.default_locale = env.default_locale.clone();
        }
        if let Some(v) = blob.mail_from_address.filter(|s| !s.is_empty()) {
            live.mail_from_address = v;
        } else if !env.mail_from_address.is_empty() {
            live.mail_from_address = env.mail_from_address.clone();
        }
        Ok(())
    }

    /// Called once at startup. Same as [`reload_system_config_into_live`]
    /// but exposed as a method so `main.rs` can invoke it before the notify
    /// worker starts.
    pub async fn apply_db_system_config_at_startup(&self) {
        if let Err(e) = self.reload_system_config_into_live().await {
            tracing::warn!("could not load system_config from DB at startup: {e}");
        }
    }

    /// Read the persisted SystemConfigBlob from the DB (or `None` if no row).
    async fn load_system_config_blob(&self) -> Result<SystemConfigBlob, Status> {
        match self
            .repo
            .get_platform_setting_public(SYSTEM_CONFIG)
            .await
            .map_err(Self::map_internal_error)?
        {
            Some(row) => serde_json::from_value::<SystemConfigBlob>(row.value_json)
                .map_err(|e| Status::internal(format!("system_config JSON: {e}"))),
            None => Ok(SystemConfigBlob::default()),
        }
    }

    fn decrypt_api_key(&self, envelope: &str) -> Result<String, philand_crypto::CryptoError> {
        let bytes = philand_crypto::aes_gcm_decrypt_or_err(
            envelope,
            &self.config.platform_master_key,
            RESEND_API_KEY.as_bytes(),
        )?;
        String::from_utf8(bytes).map_err(|_| philand_crypto::CryptoError::BadEnvelope("utf8"))
    }
}

/// Convenience for `main.rs` to construct the [`philand_notify::ApiKeySource`]
/// from the live DB state, falling back to env if the DB is empty.
pub async fn build_api_key_source(
    biz: &IdentityBiz,
) -> Result<philand_notify::ApiKeySource, Status> {
    match biz.resolve_resend_api_key().await? {
        Some(key) => Ok(philand_notify::ApiKeySource::Db(philand_notify::DbKeyResolver::new(
            move || Some(key.clone()),
        ))),
        None => match std::env::var("RESEND_API_KEY") {
            Ok(v) if !v.trim().is_empty() => Ok(philand_notify::ApiKeySource::Env(v)),
            _ => Ok(philand_notify::ApiKeySource::Db(philand_notify::DbKeyResolver::new(
                || None,
            ))),
        },
    }
}

/// Resolve a single field through the precedence chain: DB blob → live
/// overlay → env value → hard default. Returns (resolved_value, source_label).
fn resolve_field(
    blob: Option<&str>,
    live: Option<&str>,
    env_val: Option<&str>,
    hard_default: &str,
) -> (String, String) {
    if let Some(v) = blob.map(str::trim).filter(|s| !s.is_empty()) {
        return (v.to_string(), ConfigSource::Db.as_label().to_string());
    }
    if let Some(v) = live.map(str::trim).filter(|s| !s.is_empty()) {
        return (v.to_string(), ConfigSource::Env.as_label().to_string());
    }
    if let Some(v) = env_val.map(str::trim).filter(|s| !s.is_empty()) {
        return (v.to_string(), ConfigSource::Env.as_label().to_string());
    }
    (hard_default.to_string(), ConfigSource::Default.as_label().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_key_returns_last_four() {
        assert_eq!(mask_key("re_T0pS3cretK3y_AAAABBBB"), "***BBBB");
    }

    #[test]
    fn mask_key_short_input() {
        assert_eq!(mask_key("abc"), "***");
    }

    #[test]
    fn source_label_maps_correctly() {
        assert_eq!(source_label(ResendKeySource::Db), "db");
        assert_eq!(source_label(ResendKeySource::Env), "env");
        assert_eq!(source_label(ResendKeySource::None), "none");
    }

    #[test]
    fn mail_public_config_default_is_disabled_empty() {
        let cfg = MailPublicConfig::default();
        assert!(cfg.from_address.is_empty());
        assert!(cfg.reply_to.is_none());
        assert!(!cfg.enabled);
    }

    #[test]
    fn resolve_field_prefers_blob_over_env_over_default() {
        let (v, s) = resolve_field(Some("from-db"), None, Some("from-env"), "hard");
        assert_eq!(v, "from-db");
        assert_eq!(s, "db");

        let (v, s) = resolve_field(None, None, Some("from-env"), "hard");
        assert_eq!(v, "from-env");
        assert_eq!(s, "env");

        let (v, s) = resolve_field(None, Some("from-live"), Some("from-env"), "hard");
        assert_eq!(v, "from-live");
        assert_eq!(s, "env");

        let (v, s) = resolve_field(None, None, None, "hard");
        assert_eq!(v, "hard");
        assert_eq!(s, "default");

        let (v, _) = resolve_field(Some(""), None, Some("env-val"), "hard");
        assert_eq!(v, "env-val");
    }

    #[test]
    fn config_source_label_is_stable() {
        assert_eq!(ConfigSource::Db.as_label(), "db");
        assert_eq!(ConfigSource::Env.as_label(), "env");
        assert_eq!(ConfigSource::Default.as_label(), "default");
    }
}