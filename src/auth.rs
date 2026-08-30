use axum::http::{HeaderMap, header::COOKIE};
use hhm_orm_core::VerifiedSubject;
use reqwest::StatusCode;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::config::Config;

const MAX_COOKIE_HEADER_BYTES: usize = 32 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_AUTH_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct SessionClient {
    http: reqwest::Client,
    auth_base_url: url::Url,
    cookie_name: String,
    delegation_client_id: String,
    api_audience: String,
}

pub struct AuthenticatedSession {
    pub subject: VerifiedSubject,
    pub email: Option<String>,
    session_token: SecretString,
}

#[derive(Clone, Deserialize)]
struct BrowserSessionClaims {
    user_id: String,
    provider: String,
    provider_tenant: String,
    #[serde(default)]
    email: Option<String>,
    aal: u8,
    #[serde(default)]
    cred: Option<String>,
}

#[derive(Serialize)]
struct DelegateRequest<'a> {
    client_id: &'a str,
    audience: &'a str,
    scopes: [&'static str; 1],
}

#[derive(Deserialize)]
struct DelegateResponse {
    access_token: String,
    token_type: String,
    audience: String,
    scope: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SessionError {
    #[error("authentication is required")]
    Missing,
    #[error("authentication is invalid")]
    Invalid,
    #[error("authentication authority is unavailable")]
    Unavailable,
}

impl SessionClient {
    /// Constructs a redirect-free, bounded Shared Auth session client.
    ///
    /// # Errors
    ///
    /// Returns a transport error only if static client configuration fails.
    pub fn from_config(config: &Config) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(std::time::Duration::from_secs(2))
                .timeout(std::time::Duration::from_secs(8))
                .user_agent("hhm-user-web/0.1")
                .build()?,
            auth_base_url: config.shared_auth_base_url.clone(),
            cookie_name: config.shared_auth_cookie_name.clone(),
            delegation_client_id: config.delegation_client_id.clone(),
            api_audience: config.api_audience.clone(),
        })
    }

    /// Resolves exactly one configured host-only Shared Auth session cookie.
    ///
    /// # Errors
    ///
    /// Missing and invalid sessions are distinct from an authority outage.
    pub async fn resolve(&self, headers: &HeaderMap) -> Result<AuthenticatedSession, SessionError> {
        let token = unique_cookie(headers, &self.cookie_name)?.ok_or(SessionError::Missing)?;
        let endpoint = self
            .auth_base_url
            .join("auth/browser/session")
            .map_err(|_| SessionError::Unavailable)?;
        let response = self
            .http
            .get(endpoint)
            .header(COOKIE, format!("{}={token}", self.cookie_name))
            .send()
            .await
            .map_err(|_| SessionError::Unavailable)?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(SessionError::Invalid);
        }
        if !response.status().is_success() {
            return Err(SessionError::Unavailable);
        }
        let claims: BrowserSessionClaims = bounded_json(response).await?;
        if claims.aal == 0
            || claims.cred.is_some()
            || claims.provider.is_empty()
            || claims.provider_tenant.is_empty()
        {
            return Err(SessionError::Invalid);
        }
        let subject = VerifiedSubject::from_verified_claim(claims.user_id)
            .map_err(|_| SessionError::Invalid)?;
        Ok(AuthenticatedSession {
            subject,
            email: claims.email,
            session_token: SecretString::from(token),
        })
    }

    /// Delegates the interactive session to a short-lived, write-only API token.
    ///
    /// # Errors
    ///
    /// Denied or malformed delegation fails closed without exposing credentials.
    pub async fn delegate_for_intake(
        &self,
        session: &AuthenticatedSession,
    ) -> Result<SecretString, SessionError> {
        let endpoint = self
            .auth_base_url
            .join("auth/delegate")
            .map_err(|_| SessionError::Unavailable)?;
        let response = self
            .http
            .post(endpoint)
            .bearer_auth(session.session_token.expose_secret())
            .json(&DelegateRequest {
                client_id: &self.delegation_client_id,
                audience: &self.api_audience,
                scopes: ["hhm:intake:write"],
            })
            .send()
            .await
            .map_err(|_| SessionError::Unavailable)?;
        if matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            return Err(SessionError::Invalid);
        }
        if !response.status().is_success() {
            return Err(SessionError::Unavailable);
        }
        let delegated: DelegateResponse = bounded_json(response).await?;
        if delegated.token_type != "Bearer"
            || delegated.audience != self.api_audience
            || !delegated
                .scope
                .split_ascii_whitespace()
                .any(|scope| scope == "hhm:intake:write")
            || delegated.access_token.is_empty()
            || delegated.access_token.len() > MAX_TOKEN_BYTES
            || delegated.access_token.contains(char::is_whitespace)
        {
            return Err(SessionError::Invalid);
        }
        Ok(SecretString::from(delegated.access_token))
    }
}

async fn bounded_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, SessionError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_AUTH_RESPONSE_BYTES as u64)
    {
        return Err(SessionError::Unavailable);
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| SessionError::Unavailable)?;
    if body.len() > MAX_AUTH_RESPONSE_BYTES {
        return Err(SessionError::Unavailable);
    }
    serde_json::from_slice(&body).map_err(|_| SessionError::Unavailable)
}

fn unique_cookie(headers: &HeaderMap, name: &str) -> Result<Option<String>, SessionError> {
    let values = headers.get_all(COOKIE).iter().collect::<Vec<_>>();
    if values.len() > 1 {
        return Err(SessionError::Invalid);
    }
    let Some(raw) = values.first() else {
        return Ok(None);
    };
    let raw = raw.to_str().map_err(|_| SessionError::Invalid)?;
    if raw.len() > MAX_COOKIE_HEADER_BYTES || raw.contains(['\r', '\n', '\0']) {
        return Err(SessionError::Invalid);
    }
    let mut found = None;
    for pair in raw.split(';').map(str::trim) {
        let Some((candidate, value)) = pair.split_once('=') else {
            return Err(SessionError::Invalid);
        };
        if candidate == name {
            if found.is_some()
                || value.is_empty()
                || value.len() > MAX_TOKEN_BYTES
                || value.contains(char::is_whitespace)
            {
                return Err(SessionError::Invalid);
            }
            found = Some(value.to_owned());
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_parser_rejects_ambiguity() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            "other=x; __Host-hhaus-user=opaque.jwt".parse().unwrap(),
        );
        assert_eq!(
            unique_cookie(&headers, "__Host-hhaus-user").unwrap(),
            Some("opaque.jwt".into())
        );
        headers.insert(
            COOKIE,
            "__Host-hhaus-user=one; __Host-hhaus-user=two"
                .parse()
                .unwrap(),
        );
        assert_eq!(
            unique_cookie(&headers, "__Host-hhaus-user"),
            Err(SessionError::Invalid)
        );
    }
}
