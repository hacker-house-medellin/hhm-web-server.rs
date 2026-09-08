//! HHaus Medellín public web and secure account-entry server.

pub mod anti_abuse;
pub mod auth;
pub mod config;
pub mod flags;
pub mod views;

use std::sync::Arc;

use anti_abuse::{TurnstileError, TurnstileVerifier};
use auth::{ProductPermission, SessionClient, SessionError};
use axum::{
    Form, Json, Router,
    body::Body,
    extract::{
        DefaultBodyLimit, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{self, ORIGIN},
    },
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::{SinkExt, StreamExt};
use rand::{RngCore, rngs::OsRng};
use sea_orm::{Database, DatabaseConnection};
use serde::Deserialize;
use subtle::ConstantTimeEq;
use tokio::sync::{RwLock, broadcast};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use config::Config;

const ENTRY_CSRF_COOKIE: &str = "__Host-hhaus-entry-csrf";
const ENTRY_CSRF_MAX_AGE_SECONDS: u16 = 600;
const MAX_CSRF_BYTES: usize = 128;
const MAX_TITLE_BYTES: usize = 120;
const MAX_DETAIL_BYTES: usize = 1_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Individual,
    Organization,
}

impl EntryKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "individual",
            Self::Organization => "organization",
        }
    }

    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::Individual => "/join/individual",
            Self::Organization => "/join/organization",
        }
    }

    #[must_use]
    pub const fn onboarding_path(self) -> &'static str {
        match self {
            Self::Individual => "/onboarding/individual",
            Self::Organization => "/onboarding/organization",
        }
    }

    const fn permission(self) -> ProductPermission {
        match self {
            Self::Individual => ProductPermission::IndividualOnboarding,
            Self::Organization => ProductPermission::OrganizationOnboarding,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    database: Option<DatabaseConnection>,
    sessions: SessionClient,
    turnstile: TurnstileVerifier,
    items: Arc<RwLock<Vec<Item>>>,
    events: broadcast::Sender<RealtimeEvent>,
}

#[derive(Clone)]
pub struct Item {
    pub id: Uuid,
    pub title: String,
    pub detail: String,
    owner_subject: Option<String>,
}

#[derive(Clone)]
struct RealtimeEvent {
    subject: String,
    message: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthStartForm {
    kind: EntryKind,
    csrf: String,
    #[serde(rename = "cf-turnstile-response")]
    turnstile_response: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewItem {
    title: String,
    detail: String,
}

impl AppState {
    /// Builds all outbound security clients and optionally connects the legacy database.
    ///
    /// # Errors
    ///
    /// Startup fails if client construction or a configured database connection fails.
    pub async fn from_config(config: Config) -> anyhow::Result<Self> {
        let database = match config.database_url.as_deref() {
            Some(url) => Some(Database::connect(url).await?),
            None => None,
        };
        Self::new(config, database)
    }

    fn new(config: Config, database: Option<DatabaseConnection>) -> anyhow::Result<Self> {
        let sessions = SessionClient::try_new(config.shared_auth.clone())?;
        let turnstile = TurnstileVerifier::try_new(config.turnstile.clone())?;
        let (events, _) = broadcast::channel(256);
        Ok(Self {
            config: Arc::new(config),
            database,
            sessions,
            turnstile,
            items: Arc::new(RwLock::new(seed_items())),
            events,
        })
    }
}

/// Serves the HHaus web application until Ctrl-C.
///
/// # Errors
///
/// Returns an error when startup, binding, or serving fails.
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let bind_address = config.bind_address;
    let state = AppState::from_config(config).await?;
    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    tracing::info!(address = %listener.local_addr()?, "HHaus web listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(landing))
        .route("/join/individual", get(join_individual))
        .route("/join/organization", get(join_organization))
        .route("/auth/start", post(start_auth))
        .route("/onboarding/individual", get(onboarding_individual))
        .route("/onboarding/organization", get(onboarding_organization))
        .route("/healthz", get(health))
        .route("/readyz", get(readiness))
        .route(
            "/partials/reservations",
            get(items_partial).post(create_item),
        )
        .route("/ws", get(ws_upgrade))
        .route("/ws/chat", get(ws_upgrade))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn landing() -> Html<String> {
    Html(views::landing().into_string())
}

async fn join_individual(State(state): State<AppState>) -> Response {
    join(state, EntryKind::Individual)
}

async fn join_organization(State(state): State<AppState>) -> Response {
    join(state, EntryKind::Organization)
}

fn join(state: AppState, kind: EntryKind) -> Response {
    let csrf = random_token();
    let mut response =
        Html(views::entry(kind, &csrf, &state.config.turnstile.site_key).into_string())
            .into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        csrf_cookie(&csrf, ENTRY_CSRF_MAX_AGE_SECONDS),
    );
    response
}

async fn start_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<AuthStartForm>,
) -> Response {
    let retry = form.kind.path();
    if !valid_origin(&state.config, &headers) {
        return with_cleared_csrf(error_response(
            StatusCode::FORBIDDEN,
            "Request not accepted",
            "The page origin could not be verified. No sign-in was started.",
            Some(retry),
        ));
    }
    let csrf_cookie_value = match unique_cookie(&headers, ENTRY_CSRF_COOKIE) {
        Ok(Some(value)) => value,
        Ok(None) | Err(()) => {
            return with_cleared_csrf(error_response(
                StatusCode::FORBIDDEN,
                "Session check expired",
                "Reload this page to start a new secure entry check.",
                Some(retry),
            ));
        }
    };
    if !valid_csrf(&csrf_cookie_value, &form.csrf) {
        return with_cleared_csrf(error_response(
            StatusCode::FORBIDDEN,
            "Session check did not match",
            "Reload this page to start a new secure entry check.",
            Some(retry),
        ));
    }
    match state.turnstile.verify(&form.turnstile_response).await {
        Ok(()) => {}
        Err(TurnstileError::Rejected) => {
            return with_cleared_csrf(error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Human verification was not accepted",
                "Please reload the page and complete the verification again.",
                Some(retry),
            ));
        }
        Err(TurnstileError::Unavailable) => {
            return with_cleared_csrf(error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Secure entry is temporarily unavailable",
                "HHaus did not start sign-in because the anti-abuse service could not be verified.",
                Some(retry),
            ));
        }
    }
    let destination = shared_auth_sign_in(&state.config, form.kind);
    with_cleared_csrf(Redirect::to(&destination).into_response())
}

async fn onboarding_individual(State(state): State<AppState>, headers: HeaderMap) -> Response {
    onboarding(state, headers, EntryKind::Individual).await
}

async fn onboarding_organization(State(state): State<AppState>, headers: HeaderMap) -> Response {
    onboarding(state, headers, EntryKind::Organization).await
}

async fn onboarding(state: AppState, headers: HeaderMap, kind: EntryKind) -> Response {
    let session = match state.sessions.resolve(&headers).await {
        Ok(session) => session,
        Err(SessionError::Missing | SessionError::Invalid) => {
            return Redirect::to(kind.path()).into_response();
        }
        Err(SessionError::Forbidden) => {
            return error_response(
                StatusCode::FORBIDDEN,
                "Access not available",
                "Your account is valid, but this onboarding path is not authorized.",
                Some(kind.path()),
            );
        }
        Err(SessionError::Unavailable) => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Sign-in could not be verified",
                "HHaus did not show account information because Shared Auth is unavailable.",
                Some(kind.path()),
            );
        }
    };
    let delegated = match state.sessions.delegate(&session, kind.permission()).await {
        Ok(token) => token,
        Err(SessionError::Missing | SessionError::Invalid) => {
            return Redirect::to(kind.path()).into_response();
        }
        Err(SessionError::Forbidden) => {
            return error_response(
                StatusCode::FORBIDDEN,
                "Onboarding not authorized",
                "HHaus could not issue the narrow permission required for this onboarding path.",
                Some(kind.path()),
            );
        }
        Err(SessionError::Unavailable) => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Onboarding is temporarily unavailable",
                "No account or organization change was made.",
                Some(kind.path()),
            );
        }
    };
    let _credential_is_server_only = delegated.expose_for_backchannel().len();
    Html(views::onboarding(kind, session.email.as_deref()).into_string()).into_response()
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "hhm-web-server",
        "status": "ok",
        "database_configured": state.database.is_some(),
        "supabase_configured": state.config.supabase_url.is_some(),
        "auth_configured": true
    }))
}

async fn readiness() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "hhm-web-server",
        "status": "ready",
        "scope": "process-and-configuration"
    }))
}

async fn items_partial(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match state.sessions.resolve(&headers).await {
        Ok(session) => session,
        Err(SessionError::Missing | SessionError::Invalid | SessionError::Forbidden) => {
            return error_response(
                StatusCode::UNAUTHORIZED,
                "Sign in required",
                "Reservation details are available only to a verified HHaus account.",
                Some("/join/individual"),
            );
        }
        Err(SessionError::Unavailable) => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Reservations are temporarily unavailable",
                "HHaus could not make a safe authentication decision.",
                None,
            );
        }
    };
    let delegated = match state
        .sessions
        .delegate(&session, ProductPermission::ReservationRead)
        .await
    {
        Ok(token) => token,
        Err(SessionError::Missing | SessionError::Invalid | SessionError::Forbidden) => {
            return error_response(
                StatusCode::FORBIDDEN,
                "Reservation access not authorized",
                "No reservation details were shown.",
                None,
            );
        }
        Err(SessionError::Unavailable) => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Reservations are temporarily unavailable",
                "No reservation details were shown.",
                None,
            );
        }
    };
    let _credential_is_server_only = delegated.expose_for_backchannel().len();
    let items = visible_items(&state, &session.subject).await;
    Html(views::reservations(&items).into_string()).into_response()
}

async fn create_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(input): Form<NewItem>,
) -> Response {
    if !valid_origin(&state.config, &headers) {
        return error_response(
            StatusCode::FORBIDDEN,
            "Request not accepted",
            "The reservation request origin could not be verified.",
            None,
        );
    }
    let session = match state.sessions.resolve(&headers).await {
        Ok(session) => session,
        Err(SessionError::Missing | SessionError::Invalid | SessionError::Forbidden) => {
            return error_response(
                StatusCode::UNAUTHORIZED,
                "Sign in required",
                "A verified HHaus account is required before creating a reservation.",
                Some("/join/individual"),
            );
        }
        Err(SessionError::Unavailable) => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Reservations are temporarily unavailable",
                "HHaus could not make a safe authentication decision.",
                None,
            );
        }
    };
    let delegated = match state
        .sessions
        .delegate(&session, ProductPermission::ReservationWrite)
        .await
    {
        Ok(token) => token,
        Err(SessionError::Missing | SessionError::Invalid | SessionError::Forbidden) => {
            return error_response(
                StatusCode::FORBIDDEN,
                "Reservation not authorized",
                "No reservation was created.",
                None,
            );
        }
        Err(SessionError::Unavailable) => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Reservations are temporarily unavailable",
                "No reservation was created.",
                None,
            );
        }
    };
    let _credential_is_server_only = delegated.expose_for_backchannel().len();
    let title = input.title.trim();
    let detail = input.detail.trim();
    if title.is_empty()
        || title.len() > MAX_TITLE_BYTES
        || detail.is_empty()
        || detail.len() > MAX_DETAIL_BYTES
    {
        return error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Reservation details were not accepted",
            "Use a short title and a bounded description.",
            None,
        );
    }
    let item = Item {
        id: Uuid::new_v4(),
        title: title.to_owned(),
        detail: detail.to_owned(),
        owner_subject: Some(session.subject.clone()),
    };
    state.items.write().await.push(item.clone());
    let _ = state.events.send(RealtimeEvent {
        subject: session.subject.clone(),
        message: format!("reservation-refresh:{}", item.id),
    });
    let items = visible_items(&state, &session.subject).await;
    (
        StatusCode::CREATED,
        Html(views::reservations(&items).into_string()),
    )
        .into_response()
}

async fn ws_upgrade(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    match authorize_realtime(&state, &headers).await {
        Ok(subject) => ws
            .on_upgrade(move |socket| websocket(socket, state, subject))
            .into_response(),
        Err(response) => response,
    }
}

#[allow(clippy::result_large_err)]
async fn authorize_realtime(state: &AppState, headers: &HeaderMap) -> Result<String, Response> {
    if !valid_origin(&state.config, headers) {
        return Err(error_response(
            StatusCode::FORBIDDEN,
            "Realtime request not accepted",
            "The WebSocket origin could not be verified.",
            None,
        ));
    }
    let session = match state.sessions.resolve(headers).await {
        Ok(session) => session,
        Err(SessionError::Missing | SessionError::Invalid | SessionError::Forbidden) => {
            return Err(error_response(
                StatusCode::UNAUTHORIZED,
                "Sign in required",
                "Realtime access starts only after HHaus verifies your session.",
                Some("/join/individual"),
            ));
        }
        Err(SessionError::Unavailable) => {
            return Err(error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Realtime is temporarily unavailable",
                "HHaus did not open a connection because session readiness could not be verified.",
                None,
            ));
        }
    };
    let delegated = match state
        .sessions
        .delegate(&session, ProductPermission::ChatConnect)
        .await
    {
        Ok(token) => token,
        Err(SessionError::Missing | SessionError::Invalid | SessionError::Forbidden) => {
            return Err(error_response(
                StatusCode::FORBIDDEN,
                "Realtime not authorized",
                "No realtime connection was opened.",
                None,
            ));
        }
        Err(SessionError::Unavailable) => {
            return Err(error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Realtime is temporarily unavailable",
                "No realtime connection was opened.",
                None,
            ));
        }
    };
    let _credential_is_server_only = delegated.expose_for_backchannel().len();
    Ok(session.subject)
}

async fn websocket(socket: WebSocket, state: AppState, subject: String) {
    let (mut sender, mut receiver) = socket.split();
    let mut events = state.events.subscribe();
    loop {
        tokio::select! {
            message = receiver.next() => match message {
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            },
            event = events.recv() => match event {
                Ok(event) if event.subject == subject => {
                    if sender.send(Message::Text(event.message.into())).await.is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            },
        }
    }
}

async fn not_found() -> Response {
    error_response(
        StatusCode::NOT_FOUND,
        "Page not found",
        "That HHaus page does not exist.",
        None,
    )
}

fn shared_auth_sign_in(config: &Config, kind: EntryKind) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("return", kind.onboarding_path())
        .finish();
    format!(
        "{}/auth/browser/sign-in?{query}",
        config.shared_auth.browser_prefix
    )
}

fn valid_origin(config: &Config, headers: &HeaderMap) -> bool {
    let origins = headers.get_all(ORIGIN).iter().collect::<Vec<_>>();
    if origins.len() != 1 {
        return false;
    }
    let expected = config.public_origin.origin().ascii_serialization();
    origins[0].to_str().is_ok_and(|origin| origin == expected)
}

fn valid_csrf(cookie: &str, form: &str) -> bool {
    !cookie.is_empty()
        && cookie.len() <= MAX_CSRF_BYTES
        && cookie.len() == form.len()
        && bool::from(cookie.as_bytes().ct_eq(form.as_bytes()))
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn unique_cookie(headers: &HeaderMap, name: &str) -> Result<Option<String>, ()> {
    let values = headers.get_all(header::COOKIE).iter().collect::<Vec<_>>();
    if values.len() > 1 {
        return Err(());
    }
    let Some(raw) = values.first() else {
        return Ok(None);
    };
    let raw = raw.to_str().map_err(|_| ())?;
    if raw.len() > 32 * 1024 || raw.contains(['\r', '\n', '\0']) {
        return Err(());
    }
    let mut found = None;
    for pair in raw.split(';').map(str::trim) {
        let Some((candidate, value)) = pair.split_once('=') else {
            return Err(());
        };
        if candidate == name {
            if found.is_some()
                || value.is_empty()
                || value.len() > MAX_CSRF_BYTES
                || value
                    .bytes()
                    .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
            {
                return Err(());
            }
            found = Some(value.to_owned());
        }
    }
    Ok(found)
}

fn csrf_cookie(value: &str, max_age: u16) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{ENTRY_CSRF_COOKIE}={value}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={max_age}"
    ))
    .expect("bounded CSRF cookie")
}

fn clear_csrf_cookie() -> HeaderValue {
    HeaderValue::from_static(
        "__Host-hhaus-entry-csrf=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0",
    )
}

fn with_cleared_csrf(mut response: Response) -> Response {
    response
        .headers_mut()
        .append(header::SET_COOKIE, clear_csrf_cookie());
    response
}

fn error_response(status: StatusCode, title: &str, message: &str, retry: Option<&str>) -> Response {
    (
        status,
        Html(views::error(title, message, retry).into_string()),
    )
        .into_response()
}

async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'; img-src 'self' data:; style-src 'unsafe-inline'; script-src https://challenges.cloudflare.com; frame-src https://challenges.cloudflare.com; connect-src 'self' https://challenges.cloudflare.com",
        ),
    );
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );
    headers.insert(
        header::HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        header::HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    response
}

fn seed_items() -> Vec<Item> {
    vec![Item {
        id: Uuid::new_v4(),
        title: "Account-first reservations".into(),
        detail: "Reservation writes now require an origin-checked Shared Auth session and a fixed HHM API delegation scope.".into(),
        owner_subject: None,
    }]
}

async fn visible_items(state: &AppState, subject: &str) -> Vec<Item> {
    state
        .items
        .read()
        .await
        .iter()
        .filter(|item| {
            item.owner_subject
                .as_deref()
                .is_none_or(|owner| owner == subject)
        })
        .cloned()
        .collect()
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{
        Json,
        routing::{get, post},
    };
    use http_body_util::BodyExt as _;
    use serde_json::json;
    use tower::ServiceExt as _;

    use super::*;

    async fn mock_dependencies() -> (String, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route(
                "/turnstile/v0/siteverify",
                post(|| async {
                    Json(json!({
                        "success": true,
                        "hostname": "user.hhaus.org",
                        "action": "hhaus_entry"
                    }))
                }),
            )
            .route(
                "/auth/browser/session",
                get(|| async {
                    Json(json!({
                        "user_id": "shared-user-01",
                        "provider": "supabase",
                        "provider_tenant": "provider-project",
                        "project": "provider-project",
                        "roles": ["authenticated"],
                        "aal": 1,
                        "auth_time": now_seconds(),
                        "amr": ["email_otp"],
                        "email": "builder@example.test"
                    }))
                }),
            )
            .route(
                "/auth/delegate",
                post(|Json(body): Json<serde_json::Value>| async move {
                    let scope = body["scopes"][0].as_str().unwrap_or_default();
                    Json(json!({
                        "access_token": "short-lived-api-bearer",
                        "token_type": "Bearer",
                        "expires_at": now_seconds() + 300,
                        "audience": "hhm-api",
                        "scope": scope
                    }))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}"), task)
    }

    fn now_seconds() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    async fn test_state() -> (AppState, tokio::task::JoinHandle<()>) {
        let (base_url, task) = mock_dependencies().await;
        let values = BTreeMap::from([
            ("HOST".to_owned(), "127.0.0.1".to_owned()),
            ("PORT".to_owned(), "8081".to_owned()),
            (
                "PUBLIC_ORIGIN".to_owned(),
                "https://user.hhaus.org".to_owned(),
            ),
            ("SHARED_AUTH_BASE_URL".to_owned(), format!("{base_url}/")),
            (
                "SHARED_AUTH_BROWSER_PREFIX".to_owned(),
                "/shared-auth-ui".to_owned(),
            ),
            (
                "SHARED_AUTH_SESSION_COOKIE_NAME".to_owned(),
                "__Host-hhaus-user".to_owned(),
            ),
            ("SHARED_AUTH_PROVIDER".to_owned(), "supabase".to_owned()),
            (
                "SHARED_AUTH_PROVIDER_TENANT".to_owned(),
                "provider-project".to_owned(),
            ),
            (
                "SHARED_AUTH_DELEGATION_CLIENT_ID".to_owned(),
                "hhm-web".to_owned(),
            ),
            ("SHARED_AUTH_API_AUDIENCE".to_owned(), "hhm-api".to_owned()),
            (
                "TURNSTILE_SITE_KEY".to_owned(),
                "public-test-key".to_owned(),
            ),
            (
                "TURNSTILE_SECRET".to_owned(),
                "server-only-secret-with-enough-bytes".to_owned(),
            ),
            (
                "TURNSTILE_VERIFY_URL".to_owned(),
                format!("{base_url}/turnstile/v0/siteverify"),
            ),
        ]);
        let config = Config::from_resolver(|name| values.get(name).cloned()).unwrap();
        (AppState::new(config, None).unwrap(), task)
    }

    async fn response_body(response: Response) -> String {
        String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn landing_and_entry_routes_are_accessible_and_token_free() {
        let (state, task) = test_state().await;
        let app = router(state);
        let landing = app
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(landing.status(), StatusCode::OK);
        assert!(
            landing
                .headers()
                .contains_key(header::CONTENT_SECURITY_POLICY)
        );
        let body = response_body(landing).await;
        assert!(body.contains("Join as an individual"));
        assert!(body.contains("Bring your organization"));

        let entry = app
            .oneshot(
                Request::builder()
                    .uri("/join/organization")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = entry
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        let body = response_body(entry).await;
        assert!(body.contains("Continue for my organization"));
        assert!(!body.contains("type=\"password\""));
        assert!(!body.contains("access_token"));
        task.abort();
    }

    #[tokio::test]
    async fn turnstile_entry_uses_exact_local_return_and_clears_csrf() {
        let (state, task) = test_state().await;
        let app = router(state);
        let entry = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/join/organization")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let set_cookie = entry
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();
        let csrf = set_cookie
            .strip_prefix(&format!("{ENTRY_CSRF_COOKIE}="))
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        let form = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("kind", "organization")
            .append_pair("csrf", csrf)
            .append_pair("cf-turnstile-response", "opaque-proof")
            .finish();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/start")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(ORIGIN, "https://user.hhaus.org")
                    .header(header::COOKIE, format!("{ENTRY_CSRF_COOKIE}={csrf}"))
                    .body(Body::from(form))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/shared-auth-ui/auth/browser/sign-in?return=%2Fonboarding%2Forganization"
        );
        assert!(
            response
                .headers()
                .get_all(header::SET_COOKIE)
                .iter()
                .any(|value| value.to_str().unwrap().contains("Max-Age=0"))
        );
        task.abort();
    }

    #[tokio::test]
    async fn origin_mismatch_fails_before_any_sign_in_redirect() {
        let (state, task) = test_state().await;
        let response = router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/start")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(ORIGIN, "https://evil.example")
                    .body(Body::from(
                        "kind=individual&csrf=x&cf-turnstile-response=proof",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(response.headers().get(header::LOCATION).is_none());
        task.abort();
    }

    #[tokio::test]
    async fn public_mutation_body_is_bounded_before_authentication() {
        let (state, task) = test_state().await;
        let oversized = "x".repeat(33 * 1024);
        let response = router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/start")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(ORIGIN, "https://user.hhaus.org")
                    .body(Body::from(oversized))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        task.abort();
    }

    #[tokio::test]
    async fn auth_entry_rejects_client_supplied_authority_fields() {
        let (state, task) = test_state().await;
        let response = router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/start")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(ORIGIN, "https://user.hhaus.org")
                    .header(header::COOKIE, format!("{ENTRY_CSRF_COOKIE}=csrf-token"))
                    .body(Body::from(
                        "kind=organization&csrf=csrf-token&cf-turnstile-response=proof&organization_id=attacker-chosen",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_client_error());
        assert_ne!(response.status(), StatusCode::SEE_OTHER);
        assert!(response.headers().get(header::LOCATION).is_none());
        task.abort();
    }

    #[tokio::test]
    async fn protected_organization_shell_uses_server_session_and_never_renders_bearer() {
        let (state, task) = test_state().await;
        let response = router(state)
            .oneshot(
                Request::builder()
                    .uri("/onboarding/organization")
                    .header(header::COOKIE, "__Host-hhaus-user=opaque.session.token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_body(response).await;
        assert!(body.contains("owner") || body.contains("builder@example.test"));
        assert!(body.contains("No organization, tenant, or role has been granted"));
        assert!(!body.contains("short-lived-api-bearer"));
        task.abort();
    }

    #[tokio::test]
    async fn missing_session_redirects_to_matching_entry_path() {
        let (state, task) = test_state().await;
        let response = router(state)
            .oneshot(
                Request::builder()
                    .uri("/onboarding/individual")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/join/individual"
        );
        task.abort();
    }

    #[tokio::test]
    async fn both_standard_realtime_routes_are_registered_and_readiness_denies_anonymous_use() {
        let (state, task) = test_state().await;
        let app = router(state.clone());
        for path in ["/ws", "/ws/chat"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header(ORIGIN, "https://user.hhaus.org")
                        .header(header::CONNECTION, "upgrade")
                        .header(header::UPGRADE, "websocket")
                        .header("sec-websocket-version", "13")
                        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED, "{path}");
        }
        let mut headers = HeaderMap::new();
        headers.insert(ORIGIN, "https://user.hhaus.org".parse().unwrap());
        let response = authorize_realtime(&state, &headers).await.unwrap_err();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        task.abort();
    }
}
