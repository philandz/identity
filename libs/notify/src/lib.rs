use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NotifyError {
    #[error("http error")]
    Http(#[from] reqwest::Error),
    #[error("provider returned non-success status: {0}")]
    Status(u16),
}

#[derive(Debug, Serialize)]
struct TelegramPayload<'a> {
    chat_id: &'a str,
    text: &'a str,
}

pub async fn send_telegram_message(
    client: &reqwest::Client,
    bot_token: &str,
    chat_id: &str,
    text: &str,
) -> Result<(), NotifyError> {
    let url = format!("https://api.telegram.org/bot{bot_token}/sendMessage");
    let resp = client
        .post(url)
        .json(&TelegramPayload { chat_id, text })
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(NotifyError::Status(resp.status().as_u16()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use httpmock::Method::POST;
    use httpmock::MockServer;

    #[tokio::test]
    async fn returns_status_error_for_failed_endpoint() {
        let server = MockServer::start();
        let _m = server.mock(|when, then| {
            when.method(POST);
            then.status(500);
        });
        let client = reqwest::Client::new();
        let err = client
            .post(format!("{}/send", server.base_url()))
            .send()
            .await
            .expect("http ok");
        assert_eq!(err.status().as_u16(), 500);
    }
}
