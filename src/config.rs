//! Fail-closed runtime configuration for the public web and authentication edge.

use std::{fmt, net::SocketAddr};

use secrecy::{ExposeSecret, SecretString};
use url::Url;

use crate::flags;

const MAX_IDENTIFIER_BYTES: usize = 128;
const MAX_SECRET_BYTES: usize = 512;
const MIN_SECRET_BYTES: usize = 16;

#[derive(Clone)]
pub struct Config {
    pub bind_address: SocketAddr,
    pub database_url: Option<String>,
    pub supabase_url: Option<Url>,
    pub public_origin: Url,
    pub shared_auth: SharedAuthConfig,
    pub turnstile: TurnstileConfig,
}

#[derive(Clone)]
pub struct SharedAuthConfig {
    pub base_url: Url,
    pub browser_prefix: String,
    pub session_cookie_name: String,
    pub expected_provider: String,
    /// Credential-provider namespace only. It is never an HHaus organization.
    pub expected_provider_tenant: String,
    pub delegation_client_id: String,
    pub api_audience: String,
}

#[derive(Clone)]
pub struct TurnstileConfig {
    pub site_key: String,
    pub(crate) secret: SecretString,
    pub verify_url: Url,
    pub expected_hostname: String,
}

impl TurnstileConfig {
    #[must_use]
    pub(crate) fn secret(&self) -> &str {
        self.secret.expose_secret()
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Config")
            .field("bind_address", &self.bind_address)
            .field("database_configured", &self.database_url.is_some())
            .field("supabase_url", &self.supabase_url)
            .field("public_origin", &self.public_origin)
            .field("shared_auth", &self.shared_auth)
            .field("turnstile", &self.turnstile)
            .finish()
    }
}

impl fmt::Debug for SharedAuthConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedAuthConfig")
            .field("base_url", &self.base_url)
            .field("browser_prefix", &self.browser_prefix)
            .field("session_cookie_name", &self.session_cookie_name)
            .field("expected_provider", &self.expected_provider)
            .field("expected_provider_tenant", &self.expected_provider_tenant)
            .field("delegation_client_id", &self.delegation_client_id)
            .field("api_audience", &self.api_audience)
            .finish()
    }
}

impl fmt::Debug for TurnstileConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TurnstileConfig")
            .field("site_key", &self.site_key)
            .field("secret", &"[REDACTED]")
            .field("verify_url", &self.verify_url)
            .field("expected_hostname", &self.expected_hostname)
            .finish()
    }
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum ConfigError {
    #[error("required configuration is missing: {0}")]
    Missing(&'static str),
    #[error("configuration is invalid: {0}")]
    Invalid(&'static str),
}

impl Config {
    /// Resolves runtime values through the repository's audited flags-2-env boundary.
    ///
    /// # Errors
    ///
    /// Missing, ambiguous, unsafe, or unbounded identity configuration is rejected.
    pub fn from_runtime() -> Result<Self, ConfigError> {
        Self::from_resolver(|name| flags::var(name).ok())
    }

    pub(crate) fn from_resolver<F>(mut lookup: F) -> Result<Self, ConfigError>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let host = optional(&mut lookup, "HOST").unwrap_or_else(|| "0.0.0.0".into());
        let port = optional(&mut lookup, "PORT").unwrap_or_else(|| "8081".into());
        let bind_address = format!("{host}:{port}")
            .parse()
            .map_err(|_| ConfigError::Invalid("HOST or PORT"))?;

        let public_origin = parse_origin(&mut lookup, "PUBLIC_ORIGIN")?;
        let expected_hostname = public_origin
            .host_str()
            .ok_or(ConfigError::Invalid("PUBLIC_ORIGIN"))?
            .to_ascii_lowercase();
        let shared_auth_base_url = parse_service_url(&mut lookup, "SHARED_AUTH_BASE_URL")?;
        let turnstile_verify_url = parse_turnstile_url(&mut lookup)?;
        let session_cookie_name = required(&mut lookup, "SHARED_AUTH_SESSION_COOKIE_NAME")?;
        if !valid_host_cookie_name(&session_cookie_name) {
            return Err(ConfigError::Invalid("SHARED_AUTH_SESSION_COOKIE_NAME"));
        }
        let browser_prefix = required(&mut lookup, "SHARED_AUTH_BROWSER_PREFIX")?;
        if !valid_local_prefix(&browser_prefix) {
            return Err(ConfigError::Invalid("SHARED_AUTH_BROWSER_PREFIX"));
        }
        let turnstile_site_key = required(&mut lookup, "TURNSTILE_SITE_KEY")?;
        if !valid_public_key(&turnstile_site_key) {
            return Err(ConfigError::Invalid("TURNSTILE_SITE_KEY"));
        }
        let turnstile_secret = required(&mut lookup, "TURNSTILE_SECRET")?;
        if !valid_secret(&turnstile_secret) {
            return Err(ConfigError::Invalid("TURNSTILE_SECRET"));
        }

        Ok(Self {
            bind_address,
            database_url: optional(&mut lookup, "DATABASE_URL"),
            supabase_url: optional(&mut lookup, "SUPABASE_URL")
                .map(|value| Url::parse(&value))
                .transpose()
                .map_err(|_| ConfigError::Invalid("SUPABASE_URL"))?,
            public_origin,
            shared_auth: SharedAuthConfig {
                base_url: shared_auth_base_url,
                browser_prefix,
                session_cookie_name,
                expected_provider: bounded_identifier(
                    "SHARED_AUTH_PROVIDER",
                    required(&mut lookup, "SHARED_AUTH_PROVIDER")?,
                )?,
                expected_provider_tenant: bounded_identifier(
                    "SHARED_AUTH_PROVIDER_TENANT",
                    required(&mut lookup, "SHARED_AUTH_PROVIDER_TENANT")?,
                )?,
                delegation_client_id: bounded_identifier(
                    "SHARED_AUTH_DELEGATION_CLIENT_ID",
                    required(&mut lookup, "SHARED_AUTH_DELEGATION_CLIENT_ID")?,
                )?,
                api_audience: bounded_identifier(
                    "SHARED_AUTH_API_AUDIENCE",
                    required(&mut lookup, "SHARED_AUTH_API_AUDIENCE")?,
                )?,
            },
            turnstile: TurnstileConfig {
                site_key: turnstile_site_key,
                secret: SecretString::from(turnstile_secret.into_boxed_str()),
                verify_url: turnstile_verify_url,
                expected_hostname,
            },
        })
    }
}

fn parse_origin<F>(lookup: &mut F, name: &'static str) -> Result<Url, ConfigError>
where
    F: FnMut(&str) -> Option<String>,
{
    let url = Url::parse(&required(lookup, name)?).map_err(|_| ConfigError::Invalid(name))?;
    if url.scheme() != "https"
        || url.username() != ""
        || url.password().is_some()
        || url.port().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ConfigError::Invalid(name));
    }
    Ok(url)
}

fn parse_service_url<F>(lookup: &mut F, name: &'static str) -> Result<Url, ConfigError>
where
    F: FnMut(&str) -> Option<String>,
{
    let url = Url::parse(&required(lookup, name)?).map_err(|_| ConfigError::Invalid(name))?;
    let host = url.host_str().ok_or(ConfigError::Invalid(name))?;
    let cluster_http = url.scheme() == "http" && is_loopback_or_cluster_host(host);
    if !(url.scheme() == "https" || cluster_http)
        || url.username() != ""
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ConfigError::Invalid(name));
    }
    Ok(url)
}

fn parse_turnstile_url<F>(lookup: &mut F) -> Result<Url, ConfigError>
where
    F: FnMut(&str) -> Option<String>,
{
    let name = "TURNSTILE_VERIFY_URL";
    let url = Url::parse(&required(lookup, name)?).map_err(|_| ConfigError::Invalid(name))?;
    let host = url.host_str().ok_or(ConfigError::Invalid(name))?;
    let official = url.scheme() == "https"
        && host.eq_ignore_ascii_case("challenges.cloudflare.com")
        && url.path() == "/turnstile/v0/siteverify";
    let test_loopback = url.scheme() == "http" && is_loopback_host(host);
    if !(official || test_loopback)
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ConfigError::Invalid(name));
    }
    Ok(url)
}

fn is_loopback_or_cluster_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host.ends_with(".svc")
        || host.ends_with(".svc.cluster.local")
}

fn is_loopback_host(host: &str) -> bool {
    matches!(
        host.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}

fn bounded_identifier(name: &'static str, value: String) -> Result<String, ConfigError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.' | b'/')
        })
    {
        return Err(ConfigError::Invalid(name));
    }
    Ok(value)
}

fn valid_host_cookie_name(value: &str) -> bool {
    value.starts_with("__Host-")
        && value.len() <= MAX_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_local_prefix(value: &str) -> bool {
    value.starts_with('/')
        && !value.starts_with("//")
        && !value.ends_with('/')
        && value.len() <= MAX_IDENTIFIER_BYTES
        && !value
            .chars()
            .any(|character| matches!(character, '\\' | '\r' | '\n' | '\0' | '?' | '#'))
}

fn valid_public_key(value: &str) -> bool {
    (8..=MAX_IDENTIFIER_BYTES).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_secret(value: &str) -> bool {
    (MIN_SECRET_BYTES..=MAX_SECRET_BYTES).contains(&value.len())
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}

fn required<F>(lookup: &mut F, name: &'static str) -> Result<String, ConfigError>
where
    F: FnMut(&str) -> Option<String>,
{
    optional(lookup, name).ok_or(ConfigError::Missing(name))
}

fn optional<F>(lookup: &mut F, name: &str) -> Option<String>
where
    F: FnMut(&str) -> Option<String>,
{
    lookup(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn values() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("HOST".into(), "127.0.0.1".into()),
            ("PORT".into(), "31337".into()),
            ("PUBLIC_ORIGIN".into(), "https://user.hhaus.org".into()),
            (
                "SHARED_AUTH_BASE_URL".into(),
                "http://shared-auth.auth.svc.cluster.local:8080".into(),
            ),
            (
                "SHARED_AUTH_BROWSER_PREFIX".into(),
                "/shared-auth-ui".into(),
            ),
            (
                "SHARED_AUTH_SESSION_COOKIE_NAME".into(),
                "__Host-hhaus-user".into(),
            ),
            ("SHARED_AUTH_PROVIDER".into(), "supabase".into()),
            (
                "SHARED_AUTH_PROVIDER_TENANT".into(),
                "provider-project".into(),
            ),
            ("SHARED_AUTH_DELEGATION_CLIENT_ID".into(), "hhm-web".into()),
            ("SHARED_AUTH_API_AUDIENCE".into(), "hhm-api".into()),
            ("TURNSTILE_SITE_KEY".into(), "public-test-key".into()),
            (
                "TURNSTILE_SECRET".into(),
                "server-only-secret-with-enough-bytes".into(),
            ),
            (
                "TURNSTILE_VERIFY_URL".into(),
                "https://challenges.cloudflare.com/turnstile/v0/siteverify".into(),
            ),
        ])
    }

    #[test]
    fn accepts_audited_flags_and_redacts_turnstile_secret() {
        let values = values();
        let config = Config::from_resolver(|name| values.get(name).cloned()).unwrap();
        assert_eq!(config.bind_address.to_string(), "127.0.0.1:31337");
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("server-only-secret-with-enough-bytes"));
    }

    #[test]
    fn rejects_cross_site_cookies_and_public_plaintext_backchannels() {
        let mut bad_cookie = values();
        bad_cookie.insert(
            "SHARED_AUTH_SESSION_COOKIE_NAME".into(),
            "hhaus-user".into(),
        );
        assert_eq!(
            Config::from_resolver(|name| bad_cookie.get(name).cloned()).unwrap_err(),
            ConfigError::Invalid("SHARED_AUTH_SESSION_COOKIE_NAME")
        );

        let mut bad_url = values();
        bad_url.insert(
            "SHARED_AUTH_BASE_URL".into(),
            "http://auth.example.com".into(),
        );
        assert_eq!(
            Config::from_resolver(|name| bad_url.get(name).cloned()).unwrap_err(),
            ConfigError::Invalid("SHARED_AUTH_BASE_URL")
        );
    }

    #[test]
    fn provider_namespace_is_configuration_not_a_product_tenant() {
        let values = values();
        let config = Config::from_resolver(|name| values.get(name).cloned()).unwrap();
        assert_eq!(
            config.shared_auth.expected_provider_tenant,
            "provider-project"
        );
    }
}
