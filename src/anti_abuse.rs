//! Cloudflare Turnstile verification for public authentication entry.

use std::time::Duration;

use serde::Deserialize;

use crate::config::TurnstileConfig;

const MAX_RESPONSE_TOKEN_BYTES: usize = 4 * 1024;
const MAX_VERIFY_RESPONSE_BYTES: usize = 32 * 1024;
pub const ENTRY_ACTION: &str = "hhaus_entry";

#[derive(Clone)]
pub struct TurnstileVerifier {
    http: reqwest::Client,
    config: TurnstileConfig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TurnstileError {
    #[error("human verification was rejected")]
    Rejected,
    #[error("human verification is unavailable")]
    Unavailable,
}

#[derive(Deserialize)]
struct VerifyResponse {
    success: bool,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    action: Option<String>,
}

impl TurnstileVerifier {
    /// Creates a redirect-free, bounded Turnstile client.
    ///
    /// # Errors
    ///
    /// Returns an error only if the HTTP client cannot be initialized.
    pub fn try_new(config: TurnstileConfig) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(8))
                .user_agent("hhm-web-server/turnstile")
                .build()?,
            config,
        })
    }

    /// Verifies one bounded browser proof against the expected hostname/action.
    pub async fn verify(&self, response_token: &str) -> Result<(), TurnstileError> {
        if response_token.is_empty()
            || response_token.len() > MAX_RESPONSE_TOKEN_BYTES
            || response_token
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
        {
            return Err(TurnstileError::Rejected);
        }
        let response = self
            .http
            .post(self.config.verify_url.clone())
            .form(&[
                ("secret", self.config.secret()),
                ("response", response_token),
            ])
            .send()
            .await
            .map_err(|_| TurnstileError::Unavailable)?;
        if !response.status().is_success() {
            return Err(TurnstileError::Unavailable);
        }
        let verdict: VerifyResponse = bounded_json(response).await?;
        let hostname_matches = verdict
            .hostname
            .as_deref()
            .is_some_and(|hostname| hostname.eq_ignore_ascii_case(&self.config.expected_hostname));
        if !verdict.success || !hostname_matches || verdict.action.as_deref() != Some(ENTRY_ACTION)
        {
            return Err(TurnstileError::Rejected);
        }
        Ok(())
    }
}

async fn bounded_json(mut response: reqwest::Response) -> Result<VerifyResponse, TurnstileError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_VERIFY_RESPONSE_BYTES as u64)
    {
        return Err(TurnstileError::Unavailable);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| TurnstileError::Unavailable)?
    {
        if body.len().saturating_add(chunk.len()) > MAX_VERIFY_RESPONSE_BYTES {
            return Err(TurnstileError::Unavailable);
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| TurnstileError::Unavailable)
}

#[cfg(test)]
mod tests {
    use axum::{Json, Router, routing::post};
    use secrecy::SecretString;
    use serde_json::json;

    use super::*;

    async fn verifier(body: serde_json::Value) -> (TurnstileVerifier, tokio::task::JoinHandle<()>) {
        let app = Router::new().route(
            "/turnstile/v0/siteverify",
            post(move || {
                let body = body.clone();
                async move { Json(body) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let verifier = TurnstileVerifier::try_new(TurnstileConfig {
            site_key: "public-test-key".into(),
            secret: SecretString::from("server-only-secret-with-enough-bytes"),
            verify_url: format!("http://{address}/turnstile/v0/siteverify")
                .parse()
                .unwrap(),
            expected_hostname: "user.hhaus.org".into(),
        })
        .unwrap();
        (verifier, task)
    }

    #[tokio::test]
    async fn binds_a_success_to_hhaus_hostname_and_entry_action() {
        let (verifier, task) = verifier(json!({
            "success": true,
            "hostname": "user.hhaus.org",
            "action": ENTRY_ACTION
        }))
        .await;
        assert_eq!(verifier.verify("opaque-turnstile-proof").await, Ok(()));
        task.abort();
    }

    #[tokio::test]
    async fn rejects_wrong_action_hostname_and_malformed_tokens() {
        let (verifier, task) = verifier(json!({
            "success": true,
            "hostname": "evil.example",
            "action": "hhaus_entry"
        }))
        .await;
        assert_eq!(
            verifier.verify("opaque-turnstile-proof").await,
            Err(TurnstileError::Rejected)
        );
        assert_eq!(
            verifier.verify("contains whitespace").await,
            Err(TurnstileError::Rejected)
        );
        task.abort();
    }
}
