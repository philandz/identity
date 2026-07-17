//! Two-step password change for logged-in users.
//!
//! Step 1 — [`IdentityBiz::request_password_change_otp`]: user submits
//! their current password + the new password. Server verifies current,
//! generates a 6-digit code, stores its SHA-256 hash, and emails the code.
//!
//! Step 2 — [`IdentityBiz::confirm_password_change_otp`]: user submits the
//! code. Server validates (constant-time hash compare, attempt counter,
//! expiry) and applies the new password on success.
//!
//! The intermediate OTP state lives in `password_change_otps`; only one
//! active OTP per user is allowed at a time so a brute-forced entry can be
//! cleanly invalidated by issuing a fresh code.

use chrono::{Duration as ChronoDuration, Utc};
use rand::Rng;
use tonic::Status;

use crate::manager::validate;
use crate::pb::service::identity::{
    ConfirmPasswordChangeOtpRequest, ConfirmPasswordChangeOtpResponse,
    RequestPasswordChangeOtpRequest, RequestPasswordChangeOtpResponse,
};

use super::token::hash_token;
use super::{IdentityBiz, NotificationEvent};

const OTP_TTL_SECONDS: i64 = 600; // 10 minutes
const OTP_MAX_ATTEMPTS: u8 = 5;
const OTP_DIGITS: usize = 6;

impl IdentityBiz {
    /// Step 1 of the in-app password change. Verifies the current password,
    /// generates a 6-digit OTP, stores its hash, and emails the code.
    pub async fn request_password_change_otp(
        &self,
        caller_user_id: &str,
        req: RequestPasswordChangeOtpRequest,
    ) -> Result<RequestPasswordChangeOtpResponse, Status> {
        validate::password_change_request(&req.current_password, &req.new_password)?;

        let db_user = self
            .repo
            .find_user_by_id(caller_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("User not found"))?;

        let valid = philand_crypto::verify_password(&req.current_password, &db_user.password_hash)
            .map_err(Self::map_internal_error)?;
        if !valid {
            return Err(Status::unauthenticated("Current password is incorrect"));
        }

        // Invalidate any existing pending OTP for this user so the new code
        // is the only one that can succeed.
        self.repo
            .invalidate_pending_otps_for_user(caller_user_id)
            .await
            .map_err(Self::map_internal_error)?;

        let code = generate_otp(OTP_DIGITS);
        let code_hash = hash_token(&code);
        let id = uuid::Uuid::new_v4().to_string();
        let expires_at = Utc::now() + ChronoDuration::seconds(OTP_TTL_SECONDS);

        // max_attempts is hard-coded today; expose via config in a follow-up.
        let _ = OTP_MAX_ATTEMPTS;

        self.repo
            .insert_password_change_otp(&id, caller_user_id, &code_hash, expires_at)
            .await
            .map_err(Self::map_internal_error)?;

        let display_name = match db_user
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(s) => Some(s.to_string()),
            None => None,
        };

        self.enqueue_notification(NotificationEvent::PasswordChangeOtp {
            email: db_user.email,
            display_name,
            code,
            expires_at,
        })
        .await;

        Ok(RequestPasswordChangeOtpResponse {
            message: "If your account matches, a code has been sent.".to_string(),
            ttl_seconds: OTP_TTL_SECONDS as i32,
        })
    }

    /// Step 2 of the in-app password change. Validates the OTP and applies
    /// the new password on success.
    pub async fn confirm_password_change_otp(
        &self,
        caller_user_id: &str,
        req: ConfirmPasswordChangeOtpRequest,
    ) -> Result<ConfirmPasswordChangeOtpResponse, Status> {
        validate::otp_code(&req.otp)?;
        validate::password_value(&req.new_password)?;

        let active = self
            .repo
            .find_active_password_change_otp(caller_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| {
                Status::invalid_argument(
                    "No active password-change request. Please request a new code.",
                )
            })?;

        // Increment attempts first so brute-force progress is captured even
        // when we return early.
        let attempts = self
            .repo
            .increment_otp_attempts(&active.id)
            .await
            .map_err(Self::map_internal_error)?;

        if attempts > active.max_attempts {
            // Lock the row so no further attempts succeed.
            self.repo
                .mark_password_change_otp_used(&active.id)
                .await
                .map_err(Self::map_internal_error)?;
            return Err(Status::invalid_argument(
                "Too many attempts. Please request a new code.",
            ));
        }

        let provided_hash = hash_token(req.otp.trim());
        if !constant_time_eq(provided_hash.as_bytes(), active.otp_hash.as_bytes()) {
            return Err(Status::invalid_argument("Invalid code"));
        }

        // Success — apply new password and mark the OTP used.
        let new_hash =
            philand_crypto::hash_password(&req.new_password).map_err(Self::map_internal_error)?;
        self.repo
            .update_user_password(caller_user_id, &new_hash)
            .await
            .map_err(Self::map_internal_error)?;
        self.repo
            .mark_password_change_otp_used(&active.id)
            .await
            .map_err(Self::map_internal_error)?;

        // The user must sign in again everywhere — emit a security alert
        // (deferred; this is a hook for a future "auth_events" table).

        Ok(ConfirmPasswordChangeOtpResponse {})
    }
}

/// Generate a fixed-width zero-padded numeric OTP.
fn generate_otp(digits: usize) -> String {
    let max: u32 = 10_u32.pow(digits as u32);
    let n = rand::thread_rng().gen_range(0..max);
    format!("{n:0digits$}")
}

/// Constant-time string comparison. Lengths differ → fast path; otherwise
/// XOR-compare every byte.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_otp_has_expected_length_and_is_digit_only() {
        for _ in 0..50 {
            let code = generate_otp(OTP_DIGITS);
            assert_eq!(code.len(), OTP_DIGITS);
            assert!(code.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn constant_time_eq_matches_unequal_lengths() {
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"a"));
    }

    #[test]
    fn constant_time_eq_handles_equal_and_unequal() {
        assert!(constant_time_eq(b"123456", b"123456"));
        assert!(!constant_time_eq(b"123456", b"654321"));
    }
}
