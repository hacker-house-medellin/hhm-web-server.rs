use std::{env, net::SocketAddr};

use secrecy::SecretString;
use url::Url;

#[derive(Clone)]
pub struct Config {
    pub bind_address: SocketAddr,
    pub read_database_url: SecretString,
    pub api_base_url: Url,
    pub supabase_url: Url,
    pub shared_auth_base_url: Url,
    pub shared_auth_cookie_name: String,
    pub shared_auth_browser_prefix: String,
    pub delegation_client_id: String,
    pub api_audience: String,
    pub public_origin: Url,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("required configuration is missing: {0}")]
    Missing(&'static str),
    #[error("configuration is invalid: {0}")]
    Invalid(&'static str),
}

impl Config {
    /// Loads the authenticated web tier's fail-closed runtime configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if an identity, network, or database boundary is
    /// absent or unsafe.
    pub fn from_env() -> Result<Self, ConfigError> {
        let host = optional("HOST").unwrap_or_else(|| "0.0.0.0".into());
        let port = optional("PORT").unwrap_or_else(|| "8081".into());
        let bind_address = format!("{host}:{port}")
            .parse()
            .map_err(|_| ConfigError::Invalid("HOST or PORT"))?;
        let shared_auth_cookie_name = required("SHARED_AUTH_SESSION_COOKIE_NAME")?;
        if !valid_cookie_name(&shared_auth_cookie_name) {
            return Err(ConfigError::Invalid("SHARED_AUTH_SESSION_COOKIE_NAME"));
        }
        let shared_auth_browser_prefix = required("SHARED_AUTH_BROWSER_PREFIX")?;
        if !valid_local_prefix(&shared_auth_browser_prefix) {
            return Err(ConfigError::Invalid("SHARED_AUTH_BROWSER_PREFIX"));
        }
        let public_origin = parse_url("PUBLIC_ORIGIN", false)?;
        if public_origin.as_str() != "https://user.hhaus.org/" {
            return Err(ConfigError::Invalid("PUBLIC_ORIGIN"));
        }

        Ok(Self {
            bind_address,
            read_database_url: SecretString::from(required("READ_DATABASE_URL")?),
            api_base_url: parse_url("API_BASE_URL", true)?,
            supabase_url: parse_url("SUPABASE_URL", false)?,
            shared_auth_base_url: parse_url("SHARED_AUTH_BASE_URL", true)?,
            shared_auth_cookie_name,
            shared_auth_browser_prefix,
            delegation_client_id: bounded_identifier(
                "SHARED_AUTH_DELEGATION_CLIENT_ID",
                required("SHARED_AUTH_DELEGATION_CLIENT_ID")?,
            )?,
            api_audience: bounded_identifier(
                "SHARED_AUTH_API_AUDIENCE",
                required("SHARED_AUTH_API_AUDIENCE")?,
            )?,
            public_origin,
        })
    }
}

fn parse_url(name: &'static str, allow_cluster_http: bool) -> Result<Url, ConfigError> {
    let url = Url::parse(&required(name)?).map_err(|_| ConfigError::Invalid(name))?;
    let host = url.host_str().ok_or(ConfigError::Invalid(name))?;
    let normalized_host = host.to_ascii_lowercase();
    let labels = normalized_host.split('.').collect::<Vec<_>>();
    let cluster = normalized_host == "localhost"
        || normalized_host == "127.0.0.1"
        || labels.last() == Some(&"svc")
        || labels.ends_with(&["svc", "cluster", "local"]);
    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || !(url.scheme() == "https" || (allow_cluster_http && url.scheme() == "http" && cluster))
    {
        return Err(ConfigError::Invalid(name));
    }
    Ok(url)
}

fn bounded_identifier(name: &'static str, value: String) -> Result<String, ConfigError> {
    if value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.'))
    {
        return Err(ConfigError::Invalid(name));
    }
    Ok(value)
}

fn valid_cookie_name(value: &str) -> bool {
    value.starts_with("__Host-")
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_local_prefix(value: &str) -> bool {
    value.starts_with('/')
        && !value.starts_with("//")
        && !value.ends_with('/')
        && value.len() <= 128
        && !value
            .chars()
            .any(|character| matches!(character, '\\' | '\r' | '\n' | '\0' | '?' | '#'))
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    optional(name).ok_or(ConfigError::Missing(name))
}

fn optional(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_and_prefix_boundaries_are_host_only() {
        assert!(valid_cookie_name("__Host-hhaus-user"));
        assert!(!valid_cookie_name("hhaus-user"));
        assert!(valid_local_prefix("/shared-auth-ui"));
        assert!(!valid_local_prefix("//foreign.example"));
    }
}
