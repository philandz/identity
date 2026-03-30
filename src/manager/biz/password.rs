use chrono::{Duration, Utc};
use tonic::Status;

use crate::manager::validate;
use crate::pb::service::identity::{
    ChangePasswordResponse, ForgotPasswordResponse, ResetPasswordResponse,
};

use super::token::hash_token;
use super::IdentityBiz;

impl IdentityBiz {
    /// Change password for an authenticated user.
    ///
    /// Verifies the current password before accepting the new one.
    pub async fn change_password(
        &self,
        user_id: &str,
        current_password: &str,
        new_password: &str,
    ) -> Result<ChangePasswordResponse, Status> {
        validate::change_password_input(current_password, new_password)?;

        let db_user = self
            .repo
            .find_user_by_id(user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("User not found"))?;

        let valid = philand_crypto::verify_password(current_password, &db_user.password_hash)
            .map_err(Self::map_internal_error)?;
        if !valid {
            return Err(Status::unauthenticated("Current password is incorrect"));
        }

        let new_hash =
            philand_crypto::hash_password(new_password).map_err(Self::map_internal_error)?;
        self.repo
            .update_user_password(user_id, &new_hash)
            .await
            .map_err(Self::map_internal_error)?;

        Ok(ChangePasswordResponse {})
    }

    /// Initiate a password-reset flow.
    ///
    /// Generates a random token, stores its SHA-256 hash with a 1-hour TTL,
    /// and logs the raw token.  Email dispatch is deferred to the Notification
    /// service (future phase).
    pub async fn forgot_password(&self, email: &str) -> Result<ForgotPasswordResponse, Status> {
        validate::forgot_password_input(email)?;

        // Always return success to prevent email enumeration
        let db_user = self
            .repo
            .find_user_by_email(email.trim())
            .await
            .map_err(Self::map_internal_error)?;

        if let Some(user) = db_user {
            let raw_token = generate_random_token();
            let token_hash = hash_token(&raw_token);
            let id = uuid::Uuid::new_v4().to_string();
            let expires_at = Utc::now() + Duration::hours(1);

            self.repo
                .insert_password_reset_token(&id, &user.id, &token_hash, expires_at)
                .await
                .map_err(Self::map_internal_error)?;

            // TODO: emit event for Notification service to send email
            tracing::info!(
                "Password reset token for {}: {} (expires {})",
                email,
                raw_token,
                expires_at
            );

            self.enqueue_notification(super::NotificationEvent::PasswordReset {
                email: email.to_string(),
                token: raw_token,
            })
            .await;
        } else {
            tracing::debug!("Forgot-password for unknown email: {}", email);
        }

        Ok(ForgotPasswordResponse {
            message: "If that email exists, a reset link has been sent.".to_string(),
        })
    }

    /// Complete a password reset using the token from the forgot-password email.
    pub async fn reset_password(
        &self,
        raw_token: &str,
        new_password: &str,
    ) -> Result<ResetPasswordResponse, Status> {
        validate::reset_password_input(raw_token, new_password)?;

        let token_hash = hash_token(raw_token.trim());

        let reset_record = self
            .repo
            .find_valid_password_reset_token(&token_hash)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::invalid_argument("Invalid or expired reset token"))?;

        let new_hash =
            philand_crypto::hash_password(new_password).map_err(Self::map_internal_error)?;

        self.repo
            .update_user_password(&reset_record.user_id, &new_hash)
            .await
            .map_err(Self::map_internal_error)?;

        self.repo
            .mark_password_reset_token_used(&reset_record.id)
            .await
            .map_err(Self::map_internal_error)?;

        Ok(ResetPasswordResponse {})
    }
}

/// Generate a cryptographically random 32-byte hex token.
fn generate_random_token() -> String {
    philand_random::random_string(64)
}
