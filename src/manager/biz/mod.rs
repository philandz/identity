mod admin;
mod auth;
pub mod authz;
mod organization;
mod password;
pub mod password_change;
pub mod platform_settings;
mod profile;
mod token;

pub use token::Claims;

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tonic::Status;

use crate::manager::repository::IdentityRepository;

/// All inputs needed to render and send an outbound email. The notify worker
/// (in `identity/src/main.rs`) consumes this enum and dispatches each variant
/// to the right renderer + provider.
#[derive(Clone, Debug)]
pub enum NotificationEvent {
    /// "Reset your password" email. `raw_token` is the unhashed token to
    /// embed in the URL — kept in-memory only (not persisted) so the DB row
    /// is the only on-disk artefact and the worker can render the link.
    PasswordReset {
        email: String,
        raw_token: String,
        expires_at: DateTime<Utc>,
    },
    /// "You've been invited to {org}" email.
    OrgInvitation {
        email: String,
        org_id: String,
        org_name: String,
        inviter_display_name: String,
        org_role_human: String,
        raw_token: String,
        expires_at: DateTime<Utc>,
    },
    /// 6-digit OTP for the in-app "Update password" flow.
    PasswordChangeOtp {
        email: String,
        display_name: Option<String>,
        /// Plaintext 6-digit code, embedded in the email and shown once.
        code: String,
        expires_at: DateTime<Utc>,
    },
}

/// The 4 fields that the Super Admin → System Config page can edit live
/// (without a service restart). All other config stays env-only and is
/// read once at startup.
#[derive(Debug, Clone, Default)]
pub struct LiveConfig {
    pub app_public_base_url: String,
    pub support_email: String,
    pub default_locale: String,
    pub mail_from_address: String,
}

pub struct IdentityBiz {
    repo: IdentityRepository,
    /// Read-only at runtime — contains jwt_secret, master_key, db url, etc.
    /// Must NOT change after startup (rotating would invalidate every issued
    /// JWT or require a full ciphertext re-encrypt pass).
    config: philand_configs::IdentityServiceConfig,
    /// Live-reloadable overlay — Super Admin → System Config writes here and
    /// every subsequent read picks up the new value without a service restart.
    live_config: Arc<tokio::sync::RwLock<LiveConfig>>,
    notify_queue: Option<philand_queue::QueueSender<NotificationEvent>>,
    /// Provider-agnostic mailer. Held as an `Arc<dyn Mailer>` so the notify
    /// worker can be given the same handle without an extra Arc.
    pub mailer: Arc<dyn philand_notify::Mailer>,
}

impl IdentityBiz {
    pub fn new(
        repo: IdentityRepository,
        config: philand_configs::IdentityServiceConfig,
        notify_queue: Option<philand_queue::QueueSender<NotificationEvent>>,
        mailer: Arc<dyn philand_notify::Mailer>,
    ) -> Self {
        // Seed the live overlay with the env values from the static config.
        let live = LiveConfig {
            app_public_base_url: config.app_public_base_url.clone(),
            support_email: config.support_email.clone(),
            default_locale: config.default_locale.clone(),
            mail_from_address: config.mail_from_address.clone(),
        };
        Self {
            repo,
            config,
            live_config: Arc::new(tokio::sync::RwLock::new(live)),
            notify_queue,
            mailer,
        }
    }

    fn map_internal_error(error: impl ToString) -> Status {
        Status::internal(error.to_string())
    }

    /// Snapshot of the live config (URLs, support email, locale, From).
    /// Cheap: clone of a small struct under a read lock.
    pub async fn live_config_snapshot(&self) -> LiveConfig {
        self.live_config.read().await.clone()
    }

    async fn enqueue_notification(&self, event: NotificationEvent) {
        if let Some(tx) = &self.notify_queue {
            if let Err(err) = philand_queue::enqueue(tx, event).await {
                tracing::warn!("notification queue enqueue failed: {err}");
            }
        }
    }

    /// Central authorization gate. Call this at the top of every biz method that
    /// requires elevated access. Returns `Status::permission_denied` (→ HTTP 403)
    /// if the caller does not hold the required permission.
    pub async fn require_permission(
        &self,
        caller_user_id: &str,
        permission: authz::Permission,
    ) -> Result<(), Status> {
        use crate::converters::{base_status_from_db, user_type_from_db};
        use crate::pb::common::base::BaseStatus;
        use crate::pb::shared::user::UserType;

        let caller = self
            .repo
            .find_user_by_id(caller_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::unauthenticated("User not found"))?;

        if base_status_from_db(&caller.status) != BaseStatus::BsActive {
            return Err(Status::permission_denied("Account is disabled"));
        }

        match permission {
            authz::Permission::ManageAnyUser | authz::Permission::ManageAnyOrganization => {
                if user_type_from_db(&caller.user_type) != UserType::UtSuperAdmin {
                    return Err(Status::permission_denied("Super admin permission required"));
                }
            }
        }

        Ok(())
    }
}
