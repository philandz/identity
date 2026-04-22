use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone)]
pub struct GoogleOAuth2Config {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub auth_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub scopes: Vec<String>,
    pub timeout: Duration,
    pub max_retries: u8,
}

impl GoogleOAuth2Config {
    pub fn default_google(client_id: String, client_secret: String, redirect_uri: String) -> Self {
        Self {
            client_id,
            client_secret,
            redirect_uri,
            auth_endpoint: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_endpoint: "https://oauth2.googleapis.com/token".to_string(),
            userinfo_endpoint: "https://openidconnect.googleapis.com/v1/userinfo".to_string(),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            timeout: Duration::from_secs(10),
            max_retries: 2,
        }
    }

    pub fn validate(&self) -> Result<(), OAuth2Error> {
        if self.client_id.trim().is_empty() {
            return Err(OAuth2Error::InvalidConfig("client_id is empty".to_string()));
        }
        if self.client_secret.trim().is_empty() {
            return Err(OAuth2Error::InvalidConfig(
                "client_secret is empty".to_string(),
            ));
        }
        if self.redirect_uri.trim().is_empty() {
            return Err(OAuth2Error::InvalidConfig(
                "redirect_uri is empty".to_string(),
            ));
        }
        if self.scopes.is_empty() {
            return Err(OAuth2Error::InvalidConfig("scopes are empty".to_string()));
        }

        Url::parse(&self.auth_endpoint)?;
        Url::parse(&self.token_endpoint)?;
        Url::parse(&self.userinfo_endpoint)?;
        Url::parse(&self.redirect_uri)?;
        Ok(())
    }

    pub fn auth_url(&self, state: &str) -> Result<Url, OAuth2Error> {
        self.auth_url_with_pkce(state, None)
    }

    pub fn auth_url_with_pkce(
        &self,
        state: &str,
        pkce: Option<&PkcePair>,
    ) -> Result<Url, OAuth2Error> {
        self.validate()?;
        let mut url = Url::parse(&self.auth_endpoint)?;
        let mut qp = url.query_pairs_mut();
        qp.append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("scope", &self.scopes.join(" "))
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .append_pair("state", state);

        if let Some(pkce) = pkce {
            qp.append_pair("code_challenge", &pkce.code_challenge)
                .append_pair("code_challenge_method", "S256");
        }

        drop(qp);
        Ok(url)
    }
}

#[derive(Debug, Error)]
pub enum OAuth2Error {
    #[error("url parse error")]
    Url(#[from] url::ParseError),
    #[error("http request failed")]
    Http(#[from] reqwest::Error),
    #[error("invalid oauth config: {0}")]
    InvalidConfig(String),
    #[error("invalid oauth input: {0}")]
    InvalidInput(String),
    #[error("provider error {status}: {message}")]
    Provider { status: u16, message: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserInfo {
    #[serde(default)]
    pub sub: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: Option<bool>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub given_name: Option<String>,
    #[serde(default)]
    pub family_name: Option<String>,
    #[serde(default)]
    pub picture: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProviderErrorBody {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PkcePair {
    pub code_verifier: String,
    pub code_challenge: String,
}

pub fn generate_state(len: usize) -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), len.max(24))
}

pub fn generate_pkce_pair() -> PkcePair {
    let verifier = Alphanumeric.sample_string(&mut rand::thread_rng(), 64);
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    let challenge = URL_SAFE_NO_PAD.encode(digest);
    PkcePair {
        code_verifier: verifier,
        code_challenge: challenge,
    }
}

pub async fn exchange_authorization_code(
    client: &reqwest::Client,
    cfg: &GoogleOAuth2Config,
    code: &str,
) -> Result<TokenResponse, OAuth2Error> {
    exchange_authorization_code_with_pkce(client, cfg, code, None).await
}

pub async fn exchange_authorization_code_with_pkce(
    client: &reqwest::Client,
    cfg: &GoogleOAuth2Config,
    code: &str,
    code_verifier: Option<&str>,
) -> Result<TokenResponse, OAuth2Error> {
    cfg.validate()?;
    if code.trim().is_empty() {
        return Err(OAuth2Error::InvalidInput(
            "authorization code is empty".to_string(),
        ));
    }

    let mut params: Vec<(&str, &str)> = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", cfg.client_id.as_str()),
        ("client_secret", cfg.client_secret.as_str()),
        ("redirect_uri", cfg.redirect_uri.as_str()),
    ];

    if let Some(verifier) = code_verifier {
        if verifier.trim().is_empty() {
            return Err(OAuth2Error::InvalidInput(
                "code_verifier is empty".to_string(),
            ));
        }
        params.push(("code_verifier", verifier));
    }

    post_form_retry(client, &cfg.token_endpoint, &params, cfg.max_retries).await
}

pub async fn refresh_access_token(
    client: &reqwest::Client,
    cfg: &GoogleOAuth2Config,
    refresh_token: &str,
) -> Result<TokenResponse, OAuth2Error> {
    cfg.validate()?;
    if refresh_token.trim().is_empty() {
        return Err(OAuth2Error::InvalidInput(
            "refresh_token is empty".to_string(),
        ));
    }

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", cfg.client_id.as_str()),
        ("client_secret", cfg.client_secret.as_str()),
    ];

    post_form_retry(client, &cfg.token_endpoint, &params, cfg.max_retries).await
}

pub async fn fetch_user_info(
    client: &reqwest::Client,
    cfg: &GoogleOAuth2Config,
    access_token: &str,
) -> Result<UserInfo, OAuth2Error> {
    cfg.validate()?;
    if access_token.trim().is_empty() {
        return Err(OAuth2Error::InvalidInput(
            "access_token is empty".to_string(),
        ));
    }

    let resp = client
        .get(&cfg.userinfo_endpoint)
        .bearer_auth(access_token)
        .timeout(cfg.timeout)
        .send()
        .await?;

    if resp.status().is_success() {
        return Ok(resp.json::<UserInfo>().await?);
    }

    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    let parsed = serde_json::from_str::<ProviderErrorBody>(&body).ok();
    let message = parsed
        .and_then(|p| p.error_description.or(p.error))
        .unwrap_or_else(|| body.if_empty_then("Unknown provider error"));

    Err(OAuth2Error::Provider { status, message })
}

async fn post_form_retry(
    client: &reqwest::Client,
    endpoint: &str,
    params: &[(&str, &str)],
    max_retries: u8,
) -> Result<TokenResponse, OAuth2Error> {
    let mut attempts: u8 = 0;
    loop {
        let resp = client.post(endpoint).form(params).send().await;
        match resp {
            Ok(resp) if resp.status().is_success() => {
                return Ok(resp.json::<TokenResponse>().await?);
            }
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                if attempts < max_retries && status >= 500 {
                    attempts += 1;
                    continue;
                }

                let parsed = serde_json::from_str::<ProviderErrorBody>(&body).ok();
                let message = parsed
                    .and_then(|p| p.error_description.or(p.error))
                    .unwrap_or_else(|| body.if_empty_then("Unknown provider error"));
                return Err(OAuth2Error::Provider { status, message });
            }
            Err(err) => {
                if attempts < max_retries {
                    attempts += 1;
                    continue;
                }
                return Err(OAuth2Error::Http(err));
            }
        }
    }
}

trait EmptyFallback {
    fn if_empty_then(self, fallback: &str) -> String;
}

impl EmptyFallback for String {
    fn if_empty_then(self, fallback: &str) -> String {
        if self.trim().is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;

    #[test]
    fn builds_google_auth_url() {
        let cfg = GoogleOAuth2Config::default_google(
            "client-id".to_string(),
            "client-secret".to_string(),
            "https://app/callback".to_string(),
        );

        let auth = cfg.auth_url("state-123").expect("must build");
        let q = auth.query().expect("must have query");
        assert!(q.contains("response_type=code"));
        assert!(q.contains("client_id=client-id"));
        assert!(q.contains("state=state-123"));
        assert!(q.contains("access_type=offline"));
    }

    #[test]
    fn builds_auth_url_with_pkce() {
        let cfg = GoogleOAuth2Config::default_google(
            "client-id".to_string(),
            "client-secret".to_string(),
            "https://app/callback".to_string(),
        );

        let pkce = generate_pkce_pair();
        let auth = cfg
            .auth_url_with_pkce("state-xyz", Some(&pkce))
            .expect("must build");
        let q = auth.query().expect("must have query");
        assert!(q.contains("code_challenge="));
        assert!(q.contains("code_challenge_method=S256"));
    }

    #[test]
    fn state_generator_has_min_entropy() {
        let state = generate_state(4);
        assert!(state.len() >= 24);
    }

    #[test]
    fn pkce_generation_has_expected_lengths() {
        let pkce = generate_pkce_pair();
        assert_eq!(pkce.code_verifier.len(), 64);
        assert!(!pkce.code_challenge.is_empty());
    }

    #[test]
    fn invalid_config_fails_validation() {
        let cfg = GoogleOAuth2Config::default_google(
            "".to_string(),
            "secret".to_string(),
            "https://app/callback".to_string(),
        );
        let err = cfg.validate().expect_err("must fail");
        assert!(matches!(err, OAuth2Error::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn exchanges_authorization_code_successfully() {
        let server = MockServer::start();
        let token_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/token")
                .header("content-type", "application/x-www-form-urlencoded")
                .body_contains("grant_type=authorization_code")
                .body_contains("code=abc123");
            then.status(200).json_body_obj(&serde_json::json!({
                "access_token":"access-1",
                "refresh_token":"refresh-1",
                "token_type":"Bearer",
                "expires_in":3600
            }));
        });

        let mut cfg = GoogleOAuth2Config::default_google(
            "cid".to_string(),
            "secret".to_string(),
            "https://app/callback".to_string(),
        );
        cfg.token_endpoint = format!("{}/token", server.base_url());

        let client = reqwest::Client::new();
        let token = exchange_authorization_code(&client, &cfg, "abc123")
            .await
            .expect("must exchange code");

        token_mock.assert();
        assert_eq!(token.access_token, "access-1");
        assert_eq!(token.refresh_token.as_deref(), Some("refresh-1"));
    }

    #[tokio::test]
    async fn maps_provider_error_details() {
        let server = MockServer::start();
        let token_mock = server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(400).json_body_obj(&serde_json::json!({
                "error": "invalid_grant",
                "error_description": "Bad auth code"
            }));
        });

        let mut cfg = GoogleOAuth2Config::default_google(
            "cid".to_string(),
            "secret".to_string(),
            "https://app/callback".to_string(),
        );
        cfg.token_endpoint = format!("{}/token", server.base_url());
        cfg.max_retries = 0;

        let client = reqwest::Client::new();
        let err = exchange_authorization_code(&client, &cfg, "bad")
            .await
            .expect_err("must map provider error");
        token_mock.assert();

        match err {
            OAuth2Error::Provider { status, message } => {
                assert_eq!(status, 400);
                assert!(message.contains("Bad auth code"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn fetches_user_info() {
        let server = MockServer::start();
        let userinfo_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/userinfo")
                .header("authorization", "Bearer token-xyz");
            then.status(200).json_body_obj(&serde_json::json!({
                "sub":"123",
                "email":"user@example.com",
                "email_verified":true,
                "name":"User One"
            }));
        });

        let mut cfg = GoogleOAuth2Config::default_google(
            "cid".to_string(),
            "secret".to_string(),
            "https://app/callback".to_string(),
        );
        cfg.userinfo_endpoint = format!("{}/userinfo", server.base_url());

        let client = reqwest::Client::new();
        let user = fetch_user_info(&client, &cfg, "token-xyz")
            .await
            .expect("must fetch userinfo");

        userinfo_mock.assert();
        assert_eq!(user.sub.as_deref(), Some("123"));
        assert_eq!(user.email.as_deref(), Some("user@example.com"));
    }
}
