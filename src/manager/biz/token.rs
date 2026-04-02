use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tonic::Status;

use crate::converters::DbUser;

use super::IdentityBiz;

/// JWT claims per Philand spec: sub (user ID), email, org_id, exp.
/// See infra/.ai/skills/02-backend-rust/jwt-and-org.md
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub org_id: String,
    pub exp: usize,
}

/// Compute the SHA-256 hex digest of an arbitrary string (JWT, reset token, etc.).
pub fn hash_token(token: &str) -> String {
    philand_crypto::sha256_hex(token)
}

impl IdentityBiz {
    /// Issue a JWT containing the standard Philand claims.
    ///
    /// `org_id` is the currently-selected organization. At login time this
    /// defaults to the first org the user belongs to. An empty string is
    /// used when the user has no organizations yet.
    #[allow(clippy::result_large_err)]
    pub(super) fn issue_jwt(&self, db_user: &DbUser, org_id: &str) -> Result<String, Status> {
        let claims = Claims {
            sub: db_user.id.clone(),
            email: db_user.email.clone(),
            org_id: org_id.to_owned(),
            exp: (Utc::now() + Duration::hours(24)).timestamp() as usize,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.config.jwt_secret.as_bytes()),
        )
        .map_err(Self::map_internal_error)
    }

    /// Verify and decode a JWT, returning the claims.
    ///
    /// Also checks the token-revocation blacklist so that logged-out
    /// tokens are immediately rejected.
    #[allow(clippy::result_large_err)]
    pub async fn verify_jwt(&self, token: &str) -> Result<Claims, Status> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|e| Status::unauthenticated(format!("Invalid token: {e}")))?;

        // Check revocation blacklist
        let token_hash = hash_token(token);
        let revoked = self
            .repo
            .is_token_revoked(&token_hash)
            .await
            .map_err(Self::map_internal_error)?;
        if revoked {
            return Err(Status::unauthenticated("Token has been revoked"));
        }

        Ok(token_data.claims)
    }
}
