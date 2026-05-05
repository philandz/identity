use chrono::{DateTime, Utc};
use serde::Deserialize;
use tonic::Status;

#[derive(Debug, Deserialize)]
struct GoogleTokenInfo {
    sub: String,           // Google user ID
    email: String,
    email_verified: Option<String>,
    name: Option<String>,
    picture: Option<String>,
    aud: String,           // audience (client ID)
}

use crate::converters::user_type_from_db;
use crate::manager::validate;
use crate::pb::common::base::BaseStatus;
use crate::pb::service::identity::{
    LoginResponse, LogoutResponse, OrganizationSummary, RefreshTokenResponse, RegisterResponse,
};
use crate::pb::shared::organization::{MemberStatus, OrgRole};
use crate::pb::shared::user::UserType;

use super::token::hash_token;
use super::IdentityBiz;

impl IdentityBiz {
    /// Seed the initial super-admin user on startup.
    ///
    /// Runs once after migrations.  If a user with the configured email
    /// already exists the step is silently skipped — making the operation
    /// idempotent across restarts.
    pub async fn init_super_admin(&self) -> Result<(), Status> {
        let email = self.config.super_admin_email.trim();
        let password = &self.config.super_admin_password;

        if email.is_empty() || password.is_empty() {
            tracing::warn!("SUPER_ADMIN_EMAIL or SUPER_ADMIN_PASSWORD not set — skipping init");
            return Ok(());
        }

        // Already exists? Skip.
        let existing = self
            .repo
            .find_user_by_email(email)
            .await
            .map_err(Self::map_internal_error)?;

        if existing.is_some() {
            tracing::info!(
                "Super-admin user ({}) already exists — skipping init",
                email
            );
            return Ok(());
        }

        let user_id = uuid::Uuid::new_v4().to_string();
        let password_hash =
            philand_crypto::hash_password(password).map_err(Self::map_internal_error)?;

        self.repo
            .insert_user(
                &user_id,
                email,
                &password_hash,
                "Super Admin",
                UserType::UtSuperAdmin,
                BaseStatus::BsActive,
            )
            .await
            .map_err(Self::map_internal_error)?;

        tracing::info!("Created init super-admin user: {} (id={})", email, user_id);
        Ok(())
    }

    pub async fn register(
        &self,
        email: &str,
        password: &str,
        display_name: &str,
    ) -> Result<RegisterResponse, Status> {
        validate::register_input(email, password, display_name)?;

        let user_id = uuid::Uuid::new_v4().to_string();
        let org_id = uuid::Uuid::new_v4().to_string();
        let org_name = format!("{}'s Organization", display_name.trim());
        let password_hash =
            philand_crypto::hash_password(password).map_err(Self::map_internal_error)?;

        let db_user = self
            .repo
            .create_user_with_default_organization(
                &user_id,
                email.trim(),
                &password_hash,
                display_name.trim(),
                UserType::UtNormal,
                BaseStatus::BsActive,
                &org_id,
                &org_name,
                OrgRole::OrOwner,
                MemberStatus::MsActive,
            )
            .await
            .map_err(|error| {
                if self.repo.is_unique_violation(&error) {
                    Status::already_exists("Email already exists")
                } else {
                    Self::map_internal_error(error)
                }
            })?;

        Ok(RegisterResponse {
            user: Some(db_user.into()),
        })
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<LoginResponse, Status> {
        validate::login_input(email, password)?;

        let db_user = self
            .repo
            .find_user_by_email(email.trim())
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::unauthenticated("Invalid credentials"))?;

        let valid = philand_crypto::verify_password(password, &db_user.password_hash)
            .map_err(Self::map_internal_error)?;
        if !valid {
            return Err(Status::unauthenticated("Invalid credentials"));
        }

        let org_rows = self
            .repo
            .find_user_org_summaries(&db_user.id)
            .await
            .map_err(Self::map_internal_error)?;

        // Default org_id is the first org the user belongs to (empty if none)
        let default_org_id = org_rows.first().map(|r| r.id.as_str()).unwrap_or("");
        let token = self.issue_jwt(&db_user, default_org_id)?;

        let organizations = org_rows
            .into_iter()
            .map(|r| OrganizationSummary {
                id: r.id,
                name: r.name,
                role: r.role as i32,
            })
            .collect();

        let user_type_enum = user_type_from_db(&db_user.user_type);

        Ok(LoginResponse {
            access_token: token,
            user_type: user_type_enum as i32,
            organizations,
        })
    }

    /// Logout: revoke the current JWT so it can no longer be used.
    pub async fn logout(
        &self,
        raw_token: &str,
        user_id: &str,
        claims_exp: usize,
    ) -> Result<LogoutResponse, Status> {
        let token_hash = hash_token(raw_token);
        let expires_at =
            DateTime::<Utc>::from_timestamp(claims_exp as i64, 0).unwrap_or_else(Utc::now);

        self.repo
            .insert_revoked_token(&token_hash, user_id, expires_at)
            .await
            .map_err(Self::map_internal_error)?;

        Ok(LogoutResponse {})
    }

    /// Login with Google ID token.
    ///
    /// Verifies the token against Google's tokeninfo endpoint, then:
    /// - If the Google account is already linked → load user, issue JWT
    /// - If the email exists → link Google to existing account, issue JWT
    /// - Otherwise → create new user + default org, issue JWT
    pub async fn login_with_google(&self, id_token: &str) -> Result<LoginResponse, Status> {
        if id_token.trim().is_empty() {
            return Err(Status::invalid_argument("id_token must not be empty"));
        }

        // Verify the ID token with Google
        let google_info = self.verify_google_id_token(id_token).await?;

        // Try to find existing user by google_id
        let db_user = if let Some(user) = self.repo
            .find_user_by_google_id(&google_info.sub)
            .await
            .map_err(Self::map_internal_error)?
        {
            user
        } else if let Some(mut user) = self.repo
            .find_user_by_email(&google_info.email)
            .await
            .map_err(Self::map_internal_error)?
        {
            // Link Google to existing email account
            self.repo
                .link_google_to_user(&user.id, &google_info.sub, &google_info.email)
                .await
                .map_err(Self::map_internal_error)?;
            // Refresh user row
            user = self.repo
                .find_user_by_id(&user.id)
                .await
                .map_err(Self::map_internal_error)?
                .ok_or_else(|| Status::internal("User disappeared after Google link"))?;
            user
        } else {
            // Create new user with default org
            let user_id = uuid::Uuid::new_v4().to_string();
            let org_id = uuid::Uuid::new_v4().to_string();
            let display_name = google_info.name.clone()
                .unwrap_or_else(|| google_info.email.split('@').next().unwrap_or("User").to_string());
            let org_name = format!("{}'s Organization", display_name.trim());

            use crate::pb::common::base::BaseStatus;
            use crate::pb::shared::organization::{MemberStatus, OrgRole};
            use crate::pb::shared::user::UserType;

            self.repo
                .create_google_user_with_default_organization(
                    &user_id,
                    &google_info.email,
                    &display_name,
                    google_info.picture.as_deref(),
                    &google_info.sub,
                    UserType::UtNormal,
                    BaseStatus::BsActive,
                    &org_id,
                    &org_name,
                    OrgRole::OrOwner,
                    MemberStatus::MsActive,
                )
                .await
                .map_err(Self::map_internal_error)?
        };

        let org_rows = self.repo
            .find_user_org_summaries(&db_user.id)
            .await
            .map_err(Self::map_internal_error)?;

        let default_org_id = org_rows.first().map(|r| r.id.as_str()).unwrap_or("");
        let token = self.issue_jwt(&db_user, default_org_id)?;

        use crate::converters::user_type_from_db;
        let user_type_enum = user_type_from_db(&db_user.user_type);

        let organizations = org_rows
            .into_iter()
            .map(|r| OrganizationSummary {
                id: r.id,
                name: r.name,
                role: r.role as i32,
            })
            .collect();

        Ok(LoginResponse {
            access_token: token,
            user_type: user_type_enum as i32,
            organizations,
        })
    }

    /// Verify a Google ID token using Google's tokeninfo endpoint.
    async fn verify_google_id_token(&self, id_token: &str) -> Result<GoogleTokenInfo, Status> {
        let url = format!(
            "https://oauth2.googleapis.com/tokeninfo?id_token={}",
            id_token
        );
        let resp = reqwest::get(&url)
            .await
            .map_err(|e| Status::internal(format!("Google tokeninfo request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(Status::unauthenticated("Invalid Google ID token"));
        }

        let info: GoogleTokenInfo = resp
            .json()
            .await
            .map_err(|e| Status::internal(format!("Failed to parse Google tokeninfo: {e}")))?;

        // Verify the token was issued for our client
        if !self.config.google_client_id.is_empty()
            && info.aud != self.config.google_client_id
        {
            return Err(Status::unauthenticated("Google token audience mismatch"));
        }

        if info.email.is_empty() {
            return Err(Status::unauthenticated("Google token missing email"));
        }

        Ok(info)
    }

    /// Refresh: issue a new JWT with a fresh 24h window, revoke the old one.
    pub async fn refresh_token(
        &self,
        raw_token: &str,
        user_id: &str,
        claims_exp: usize,
    ) -> Result<RefreshTokenResponse, Status> {
        // Fetch user to build fresh claims
        let db_user = self
            .repo
            .find_user_by_id(user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("User not found"))?;

        // Determine default org (same logic as login)
        let org_rows = self
            .repo
            .find_user_org_summaries(&db_user.id)
            .await
            .map_err(Self::map_internal_error)?;
        let default_org_id = org_rows.first().map(|r| r.id.as_str()).unwrap_or("");

        let new_token = self.issue_jwt(&db_user, default_org_id)?;

        // Revoke old token
        let token_hash = hash_token(raw_token);
        let expires_at =
            DateTime::<Utc>::from_timestamp(claims_exp as i64, 0).unwrap_or_else(Utc::now);
        self.repo
            .insert_revoked_token(&token_hash, user_id, expires_at)
            .await
            .map_err(Self::map_internal_error)?;

        Ok(RefreshTokenResponse {
            access_token: new_token,
        })
    }
}
