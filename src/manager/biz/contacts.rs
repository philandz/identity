//! Internal contact lookup used by the notification service to send email.
//!
//! Not user-facing: callers authenticate with the shared `x-internal-token` instead of a JWT and
//! the gateway never routes this RPC.

use tonic::metadata::MetadataMap;
use tonic::Status;

use crate::pb::service::identity::{GetUserContactsResponse, UserContact};

use super::IdentityBiz;

const MAX_IDS: usize = 100;

/// Compares the `x-internal-token` metadata with `INTERNAL_SERVICE_TOKEN` in constant time.
/// When the variable is unset the RPC is disabled.
pub fn check_internal_token(meta: &MetadataMap) -> Result<(), Status> {
    let expected = std::env::var("INTERNAL_SERVICE_TOKEN").unwrap_or_default();
    verify_token(meta, &expected)
}

fn verify_token(meta: &MetadataMap, expected: &str) -> Result<(), Status> {
    let given = meta
        .get("x-internal-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if expected.is_empty() || !constant_time_eq(given.as_bytes(), expected.as_bytes()) {
        return Err(Status::unauthenticated("invalid internal token"));
    }
    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Only "vi" and "en" are supported in emails; anything else falls back to English.
fn normalize_locale(raw: &str) -> &'static str {
    if raw.trim().to_ascii_lowercase().starts_with("vi") {
        "vi"
    } else {
        "en"
    }
}

impl IdentityBiz {
    pub async fn get_user_contacts(
        &self,
        user_ids: &[String],
    ) -> Result<GetUserContactsResponse, Status> {
        if user_ids.len() > MAX_IDS {
            return Err(Status::invalid_argument("too many user_ids (max 100)"));
        }
        let mut contacts = Vec::with_capacity(user_ids.len());
        for id in user_ids {
            let id = id.trim();
            if id.is_empty() {
                continue;
            }
            let Some(user) = self
                .repo
                .find_user_by_id(id)
                .await
                .map_err(Self::map_internal_error)?
            else {
                continue;
            };
            if user.deleted_at.is_some() || user.email.is_empty() {
                continue;
            }
            contacts.push(UserContact {
                user_id: user.id,
                email: user.email,
                display_name: user.display_name.unwrap_or_default(),
                locale: normalize_locale(&user.locale).to_string(),
            });
        }
        Ok(GetUserContactsResponse { contacts })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(token: Option<&str>) -> MetadataMap {
        let mut m = MetadataMap::new();
        if let Some(t) = token {
            m.insert("x-internal-token", t.parse().unwrap());
        }
        m
    }

    #[test]
    fn token_must_match_and_be_configured() {
        assert!(verify_token(&meta(Some("s3cret")), "s3cret").is_ok());
        assert!(verify_token(&meta(Some("wrong")), "s3cret").is_err());
        assert!(verify_token(&meta(None), "s3cret").is_err());
        // Disabled when the server has no token, even for an empty client token.
        assert!(verify_token(&meta(Some("")), "").is_err());
        assert!(verify_token(&meta(None), "").is_err());
    }

    #[test]
    fn locale_falls_back_to_english() {
        assert_eq!(normalize_locale("vi"), "vi");
        assert_eq!(normalize_locale("vi-VN"), "vi");
        assert_eq!(normalize_locale("EN"), "en");
        assert_eq!(normalize_locale(""), "en");
        assert_eq!(normalize_locale("fr"), "en");
    }
}
