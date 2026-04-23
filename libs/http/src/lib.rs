use serde::de::DeserializeOwned;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("request failed")]
    Request(#[from] reqwest::Error),
    #[error("invalid response status: {0}")]
    Status(u16),
}

pub async fn get_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, HttpError> {
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(HttpError::Status(resp.status().as_u16()));
    }
    Ok(resp.json::<T>().await?)
}

pub async fn post_json<TReq: serde::Serialize, TRes: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    body: &TReq,
) -> Result<TRes, HttpError> {
    let resp = client.post(url).json(body).send().await?;
    if !resp.status().is_success() {
        return Err(HttpError::Status(resp.status().as_u16()));
    }
    Ok(resp.json::<TRes>().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize)]
    struct Resp {
        ok: bool,
    }

    #[derive(Debug, Serialize)]
    struct Req {
        name: String,
    }

    #[tokio::test]
    async fn get_json_success() {
        let server = MockServer::start();
        let _m = server.mock(|when, then| {
            when.method(GET).path("/health");
            then.status(200)
                .json_body_obj(&serde_json::json!({"ok": true}));
        });
        let client = reqwest::Client::new();
        let out: Resp = get_json(&client, &format!("{}/health", server.base_url()))
            .await
            .expect("must pass");
        assert!(out.ok);
    }

    #[tokio::test]
    async fn post_json_success() {
        let server = MockServer::start();
        let _m = server.mock(|when, then| {
            when.method(POST).path("/echo");
            then.status(200)
                .json_body_obj(&serde_json::json!({"ok": true}));
        });
        let client = reqwest::Client::new();
        let out: Resp = post_json(
            &client,
            &format!("{}/echo", server.base_url()),
            &Req {
                name: "x".to_string(),
            },
        )
        .await
        .expect("must pass");
        assert!(out.ok);
    }
}
