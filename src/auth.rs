//! Shared Auth browser-session and product-delegation boundary.
//!
//! The browser supplies only one configured, host-only session cookie. This
//! server resolves it over the private Shared Auth back channel and delegates
//! fixed product scopes. Raw session and delegated tokens never enter HTML,
//! JSON responses, URLs, logs, product tenant selection, or browser script.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::http::{HeaderMap, header::COOKIE};
use reqwest::StatusCode;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::config::SharedAuthConfig;

const MAX_COOKIE_HEADER_BYTES: usize = 32 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_AUTH_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_ROLES: usize = 32;
const MAX_ROLE_BYTES: usize = 128;
const MAX_EMAIL_BYTES: usize = 320;
const MAX_CLOCK_SKEW_SECONDS: u64 = 60;
const MAX_DELEGATED_TTL_SECONDS: u64 = 900;

#[derive(Clone)]
pub struct SessionClient {
    http: reqwest::Client,
    config: SharedAuthConfig,
}

pub struct AuthenticatedSession {
    pub subject: String,
    pub email: Option<String>,
    /// Shared Auth roles are authentication metadata, not HHaus product roles.
    _shared_auth_roles: Vec<String>,
    session_token: SecretString,
}

pub struct DelegatedAccessToken(SecretString);

impl DelegatedAccessToken {
    #[must_use]
    pub(crate) fn expose_for_backchannel(&self) -> &str {
        self.0.expose_secret()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductPermission {
    IndividualOnboarding,
    OrganizationOnboarding,
    ReservationRead,
    ReservationWrite,
    ChatConnect,
}

impl ProductPermission {
    #[must_use]
    pub const fn scope(self) -> &'static str {
        match self {
            Self::IndividualOnboarding => "hhm:onboarding:individual",
            Self::OrganizationOnboarding => "hhm:onboarding:organization",
            Self::ReservationRead => "hhm:reservation:read",
            Self::ReservationWrite => "hhm:reservation:write",
            Self::ChatConnect => "hhm:chat:connect",
        }
    }
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum SessionError {
    #[error("authentication is required")]
    Missing,
    #[error("authentication is invalid")]
    Invalid,
    #[error("authentication is not authorized")]
    Forbidden,
    #[error("authentication authority is unavailable")]
    Unavailable,
}

#[derive(Deserialize)]
struct BrowserSessionClaims {
    user_id: String,
    provider: String,
    provider_tenant: String,
    roles: Vec<String>,
    aal: u8,
    #[serde(default)]
    auth_time: Option<u64>,
    #[serde(default)]
    amr: Vec<String>,
    #[serde(default)]
    acr: Option<String>,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    azp: Option<String>,
    #[serde(default)]
    cred: Option<String>,
}

#[derive(Serialize)]
struct DelegateRequest<'a> {
    client_id: &'a str,
    audience: &'a str,
    scopes: [&'a str; 1],
}

#[derive(Deserialize)]
struct DelegateResponse {
    access_token: String,
    token_type: String,
    expires_at: u64,
    audience: String,
    scope: String,
}

impl SessionClient {
    /// Builds a redirect-free client with bounded timeouts.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn try_new(config: SharedAuthConfig) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(8))
                .user_agent("hhm-web-server/shared-auth-bff")
                .build()?,
            config,
        })
    }

    /// Resolves the exact Shared Auth browser cookie into a bounded identity.
    ///
    /// Provider tenant and Shared Auth roles are retained only as verified
    /// authentication provenance. They are not product-organization authority.
    pub async fn resolve(&self, headers: &HeaderMap) -> Result<AuthenticatedSession, SessionError> {
        let token = unique_cookie(headers, &self.config.session_cookie_name)?
            .ok_or(SessionError::Missing)?;
        let endpoint = self
            .config
            .base_url
            .join("auth/browser/session")
            .map_err(|_| SessionError::Unavailable)?;
        let response = self
            .http
            .get(endpoint)
            .header(
                COOKIE,
                format!("{}={token}", self.config.session_cookie_name),
            )
            .send()
            .await
            .map_err(|_| SessionError::Unavailable)?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(SessionError::Invalid);
            }
            status if !status.is_success() => return Err(SessionError::Unavailable),
            _ => {}
        }
        let claims: BrowserSessionClaims = bounded_json(response).await?;
        validate_claims(&claims, &self.config)?;
        Ok(AuthenticatedSession {
            subject: claims.user_id,
            email: claims.email,
            _shared_auth_roles: claims.roles,
            session_token: SecretString::from(token.into_boxed_str()),
        })
    }

    /// Exchanges the current interactive session for exactly one product scope.
    ///
    /// The returned bearer is deliberately opaque and is usable only by this
    /// process for a server-to-server HHM API call.
    pub async fn delegate(
        &self,
        session: &AuthenticatedSession,
        permission: ProductPermission,
    ) -> Result<DelegatedAccessToken, SessionError> {
        let endpoint = self
            .config
            .base_url
            .join("auth/delegate")
            .map_err(|_| SessionError::Unavailable)?;
        let expected_scope = permission.scope();
        let response = self
            .http
            .post(endpoint)
            .bearer_auth(session.session_token.expose_secret())
            .json(&DelegateRequest {
                client_id: &self.config.delegation_client_id,
                audience: &self.config.api_audience,
                scopes: [expected_scope],
            })
            .send()
            .await
            .map_err(|_| SessionError::Unavailable)?;
        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(SessionError::Invalid),
            StatusCode::FORBIDDEN => return Err(SessionError::Forbidden),
            status if !status.is_success() => return Err(SessionError::Unavailable),
            _ => {}
        }
        let delegated: DelegateResponse = bounded_json(response).await?;
        let now = now_seconds();
        let scopes = delegated.scope.split_ascii_whitespace().collect::<Vec<_>>();
        if delegated.token_type != "Bearer"
            || delegated.audience != self.config.api_audience
            || scopes.as_slice() != [expected_scope]
            || !valid_token(&delegated.access_token)
            || delegated.expires_at <= now
            || delegated.expires_at
                > now.saturating_add(MAX_DELEGATED_TTL_SECONDS + MAX_CLOCK_SKEW_SECONDS)
        {
            return Err(SessionError::Invalid);
        }
        Ok(DelegatedAccessToken(SecretString::from(
            delegated.access_token.into_boxed_str(),
        )))
    }
}

fn validate_claims(
    claims: &BrowserSessionClaims,
    config: &SharedAuthConfig,
) -> Result<(), SessionError> {
    let valid_email = claims.email.as_deref().is_none_or(|email| {
        !email.is_empty()
            && email.len() <= MAX_EMAIL_BYTES
            && email.contains('@')
            && !email
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    });
    let valid_roles = claims.roles.len() <= MAX_ROLES
        && claims.roles.iter().all(|role| valid_identifier(role))
        && unique(&claims.roles);
    let valid_assurance = claims.aal >= 1
        && claims.amr.len() <= MAX_ROLES
        && claims.amr.iter().all(|method| valid_identifier(method))
        && claims.acr.as_deref().is_none_or(valid_identifier);
    let interactive = claims.cred.is_none() && claims.azp.is_none() && claims.scope.is_empty();
    let provider_matches = claims.provider == config.expected_provider
        && claims.provider_tenant == config.expected_provider_tenant
        && claims
            .project
            .as_deref()
            .is_none_or(|project| project == claims.provider_tenant);

    if !valid_identifier(&claims.user_id)
        || !valid_email
        || !valid_roles
        || !valid_assurance
        || !interactive
        || !provider_matches
        || claims.auth_time == Some(0)
    {
        return Err(SessionError::Invalid);
    }
    Ok(())
}

async fn bounded_json<T: serde::de::DeserializeOwned>(
    mut response: reqwest::Response,
) -> Result<T, SessionError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_AUTH_RESPONSE_BYTES as u64)
    {
        return Err(SessionError::Unavailable);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| SessionError::Unavailable)?
    {
        if body.len().saturating_add(chunk.len()) > MAX_AUTH_RESPONSE_BYTES {
            return Err(SessionError::Unavailable);
        }
        body.extend_from_slice(&chunk);
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
            if found.is_some() || !valid_token(value) {
                return Err(SessionError::Invalid);
            }
            found = Some(value.to_owned());
        }
    }
    Ok(found)
}

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TOKEN_BYTES
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ROLE_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.' | b'/')
        })
}

fn unique(values: &[String]) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(index, value)| !values[..index].contains(value))
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use axum::{
        Json, Router,
        routing::{get, post},
    };
    use serde_json::json;

    use super::*;

    async fn mock_shared_auth(
        session: serde_json::Value,
        delegate: serde_json::Value,
    ) -> (SharedAuthConfig, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route(
                "/auth/browser/session",
                get({
                    let session = session.clone();
                    move || {
                        let session = session.clone();
                        async move { Json(session) }
                    }
                }),
            )
            .route(
                "/auth/delegate",
                post(move || {
                    let delegate = delegate.clone();
                    async move { Json(delegate) }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (
            SharedAuthConfig {
                base_url: format!("http://{address}/").parse().unwrap(),
                browser_prefix: "/shared-auth-ui".into(),
                session_cookie_name: "__Host-hhaus-user".into(),
                expected_provider: "supabase".into(),
                expected_provider_tenant: "provider-project".into(),
                delegation_client_id: "hhm-web".into(),
                api_audience: "hhm-api".into(),
            },
            task,
        )
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            "theme=dark; __Host-hhaus-user=opaque.session.token"
                .parse()
                .unwrap(),
        );
        headers
    }

    fn valid_session() -> serde_json::Value {
        json!({
            "user_id": "shared-user-01",
            "provider": "supabase",
            "provider_tenant": "provider-project",
            "project": "provider-project",
            "roles": ["authenticated"],
            "aal": 1,
            "auth_time": now_seconds(),
            "amr": ["email_otp"],
            "email": "builder@example.test"
        })
    }

    #[test]
    fn cookie_parser_rejects_duplicate_or_ambiguous_credentials() {
        assert_eq!(
            unique_cookie(&headers(), "__Host-hhaus-user").unwrap(),
            Some("opaque.session.token".into())
        );
        let mut duplicate = HeaderMap::new();
        duplicate.insert(
            COOKIE,
            "__Host-hhaus-user=one; __Host-hhaus-user=two"
                .parse()
                .unwrap(),
        );
        assert_eq!(
            unique_cookie(&duplicate, "__Host-hhaus-user"),
            Err(SessionError::Invalid)
        );
    }

    #[tokio::test]
    async fn resolves_server_claims_and_delegates_one_fixed_scope() {
        let expires_at = now_seconds() + 300;
        let (config, task) = mock_shared_auth(
            valid_session(),
            json!({
                "access_token": "short-lived-api-bearer",
                "token_type": "Bearer",
                "expires_at": expires_at,
                "audience": "hhm-api",
                "scope": "hhm:onboarding:organization"
            }),
        )
        .await;
        let client = SessionClient::try_new(config).unwrap();
        let session = client.resolve(&headers()).await.unwrap();
        assert_eq!(session.subject, "shared-user-01");
        assert_eq!(session.email.as_deref(), Some("builder@example.test"));
        let token = client
            .delegate(&session, ProductPermission::OrganizationOnboarding)
            .await
            .unwrap();
        assert_eq!(token.expose_for_backchannel(), "short-lived-api-bearer");
        task.abort();
    }

    #[tokio::test]
    async fn provider_tenant_never_falls_through_as_product_authority() {
        let mut claims = valid_session();
        claims["provider_tenant"] = json!("foreign-provider-project");
        let (config, task) = mock_shared_auth(claims, json!({})).await;
        let client = SessionClient::try_new(config).unwrap();
        assert!(matches!(
            client.resolve(&headers()).await,
            Err(SessionError::Invalid)
        ));
        task.abort();
    }

    #[tokio::test]
    async fn delegated_or_sandboxed_browser_sessions_fail_closed() {
        let mut claims = valid_session();
        claims["cred"] = json!("ssh_key");
        let (config, task) = mock_shared_auth(claims, json!({})).await;
        let client = SessionClient::try_new(config).unwrap();
        assert!(matches!(
            client.resolve(&headers()).await,
            Err(SessionError::Invalid)
        ));
        task.abort();
    }
}
