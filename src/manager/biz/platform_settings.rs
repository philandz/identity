//! Platform-wide settings (Super Admin → Global Settings).
//!
//! The Resend API key lives encrypted in `platform_settings`, while the
//! non-secret mail config (From address, Reply-To, enabled flag) lives in
//! `platform_settings_public` so the rest of the system can read it without
//! holding the master key.

use serde::{Deserialize, Serialize};
use tonic::Status;

use crate::manager::validate;
use crate::pb::service::identity::{
    GetResendConfigResponse, TestResendConfigResponse, UpdateResendConfigResponse,
};

use super::IdentityBiz;

/// Key used to store the Resend API key in the encrypted `platform_settings`
/// table. Used as the AAD for AES-GCM so a ciphertext can't be replayed into
/// a future `smtp_password` row.
const RESEND_API_KEY: &str = "resend_api_key";

/// Key used to store the public mail config JSON in
/// `platform_settings_public`.
const MAIL_CONFIG: &str = "mail_config";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MailPublicConfig {
    pub from_address: String,
    #[serde(default)]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

impl IdentityBiz {
    /// Read the public Resend config + masked key, plus which source is in
    /// use. Always returns a value; never exposes the raw key.
    pub async fn get_resend_config(&self) -> Result<GetResendConfigResponse, Status> {
        // Public config (from, reply-to, enabled).
        let public = self
            .repo
            .get_platform_setting_public(MAIL_CONFIG)
            .await
            .map_err(Self::map_internal_error)?;
        let cfg = public
            .and_then(|r| serde_json::from_value::<MailPublicConfig>(r.value_json).ok())
            .unwrap_or_default();

        // Determine source + masked key by trying DB first, then env.
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
        self.require_permission(
            caller_user_id,
            crate::manager::biz::authz::Permission::ManageAnyUser,
        )
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

        // If we're updating only the public side (no api_key passed), make sure
        // a previously-saved key is still here. We don't delete it; admin can
        // call DeleteResendConfig in a follow-up.
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
            reply_to: reply_to
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
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

    /// Send a one-off test message using the current Resend config. The
    /// returned `message_id` lets the admin confirm Resend accepted the
    /// payload.
    pub async fn test_resend_config(
        &self,
        caller_user_id: &str,
        recipient_email: &str,
    ) -> Result<TestResendConfigResponse, Status> {
        self.require_permission(
            caller_user_id,
            crate::manager::biz::authz::Permission::ManageAnyUser,
        )
        .await?;
        validate::email(recipient_email)?;

        let cfg = self.get_mail_public_config().await?;
        let from = if !cfg.from_address.is_empty() {
            cfg.from_address
        } else {
            self.config.mail_from_address.clone()
        };
        let reply_to = cfg
            .reply_to
            .clone()
            .or_else(|| Some(self.config.support_email.clone()));

        let rendered =
            philand_notify::render_password_change_otp(philand_notify::PasswordChangeOtpVars {
                display_name: None,
                code: "000000",
                ttl_human: "10 minutes",
                expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
                support_email: &self.config.support_email,
            });
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

    fn decrypt_api_key(&self, envelope: &str) -> Result<String, philand_crypto::CryptoError> {
        let bytes = philand_crypto::aes_gcm_decrypt_or_err(
            envelope,
            &self.config.platform_master_key,
            RESEND_API_KEY.as_bytes(),
        )?;
        String::from_utf8(bytes).map_err(|_| philand_crypto::CryptoError::BadEnvelope("utf8"))
    }
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
    // Format: "re_****last4" so admins can verify which key is in use
    // without revealing it.
    if key.len() <= 6 {
        return "***".to_string();
    }
    let last4 = &key[key.len() - 4..];
    format!("***{last4}")
}

/// Convenience for `main.rs` to construct the [`philand_notify::ApiKeySource`]
/// from the live DB state, falling back to env if the DB is empty.
pub async fn build_api_key_source(
    biz: &IdentityBiz,
) -> Result<philand_notify::ApiKeySource, Status> {
    match biz.resolve_resend_api_key().await? {
        Some(key) => Ok(philand_notify::ApiKeySource::Db(
            philand_notify::DbKeyResolver::new(move || Some(key.clone())),
        )),
        None => match std::env::var("RESEND_API_KEY") {
            Ok(v) if !v.trim().is_empty() => Ok(philand_notify::ApiKeySource::Env(v)),
            _ => Ok(philand_notify::ApiKeySource::Db(
                philand_notify::DbKeyResolver::new(|| None),
            )),
        },
    }
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
}
