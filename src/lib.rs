pub mod api;
pub mod auth;
pub mod config;
pub mod forms;
pub mod views;

use axum::{
    Form, Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header::ORIGIN},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use hhm_interfaces::intake::UploadKind;
use hhm_orm_core::{IntakePrefill, ReadContext};
use secrecy::ExposeSecret;
use tower_http::trace::TraceLayer;
use tracing::info;
use uuid::Uuid;

use crate::{
    api::{ApiClient, ApiError, IntakeFile},
    auth::{AuthenticatedSession, SessionClient, SessionError},
    config::Config,
    forms::{PreInterestForm, ReferralForm},
};

const APPLICATION_BODY_LIMIT: usize = (20 * 1024 * 1024) + (512 * 1024);
const _: () = assert!(APPLICATION_BODY_LIMIT > 20 * 1024 * 1024);
const _: () = assert!(APPLICATION_BODY_LIMIT < 21 * 1024 * 1024);

#[derive(Clone)]
pub struct AppState {
    config: Config,
    reads: ReadContext,
    sessions: SessionClient,
    api: ApiClient,
}

impl AppState {
    /// Initializes read-only data and outbound identity/intake capabilities.
    ///
    /// # Errors
    ///
    /// Startup fails unless the database role proves read-only and all clients
    /// can be constructed safely.
    pub async fn from_config(config: Config) -> anyhow::Result<Self> {
        let reads = ReadContext::connect(config.read_database_url.expose_secret()).await?;
        let sessions = SessionClient::from_config(&config)?;
        let api = ApiClient::from_config(&config)?;
        Ok(Self {
            config,
            reads,
            sessions,
            api,
        })
    }
}

/// Runs the authenticated `HHaus` user server until shutdown.
///
/// # Errors
///
/// Returns an error when startup, binding, or serving fails.
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let bind_address = config.bind_address;
    let state = AppState::from_config(config).await?;
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    info!(address = %listener.local_addr()?, "HHaus user web listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/healthz", get(health))
        .route("/readyz", get(readiness))
        .route(
            "/submit-pre-interest",
            get(show_pre_interest).post(submit_pre_interest),
        )
        .route(
            "/submit-application",
            get(show_application)
                .post(submit_application)
                .layer(DefaultBodyLimit::max(APPLICATION_BODY_LIMIT)),
        )
        .route("/submit-referral", get(show_referral).post(submit_referral))
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"service":"hhm-user-web","status":"ok"}))
}

async fn readiness(State(state): State<AppState>) -> Response {
    match state.reads.ping().await {
        Ok(()) => {
            Json(serde_json::json!({"service":"hhm-user-web","status":"ready"})).into_response()
        }
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"service":"hhm-user-web","status":"unavailable"})),
        )
            .into_response(),
    }
}

async fn home(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session_for_get(&state, &headers, "/").await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let points = match state.reads.user_points(&session.subject).await {
        Ok(points) => points.map_or(0, |snapshot| snapshot.balance),
        Err(_) => {
            return unavailable(
                "Account unavailable",
                "We could not load your HHaus account right now.",
                Some("/"),
            );
        }
    };
    html(
        StatusCode::OK,
        views::home(session.email.as_deref(), points),
    )
}

async fn show_pre_interest(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session_for_get(&state, &headers, "/submit-pre-interest").await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let prefill = match load_prefill(&state, &session).await {
        Ok(prefill) => prefill,
        Err(response) => return response,
    };
    html(
        StatusCode::OK,
        views::pre_interest(&prefill, Uuid::new_v4()),
    )
}

async fn submit_pre_interest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PreInterestForm>,
) -> Response {
    if !valid_post_origin(&state, &headers) {
        return invalid(
            "Invalid submission",
            "The form origin could not be verified.",
            Some("/submit-pre-interest"),
        );
    }
    let session = match require_session_for_post(&state, &headers, "/submit-pre-interest").await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Ok((input, nonce)) = form.into_contract() else {
        return invalid(
            "Check your answers",
            "One or more pre-interest fields are incomplete or invalid.",
            Some("/submit-pre-interest"),
        );
    };
    let token = match state.sessions.delegate_for_intake(&session).await {
        Ok(token) => token,
        Err(error) => return auth_failure(error, "/submit-pre-interest"),
    };
    match state
        .api
        .submit_pre_interest(&token, &format!("pre-interest:{nonce}"), &input)
        .await
    {
        Ok(receipt) => html(
            StatusCode::CREATED,
            views::success("Pre-interest", receipt.submission_id),
        ),
        Err(error) => api_failure(error, "/submit-pre-interest"),
    }
}

async fn show_application(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session_for_get(&state, &headers, "/submit-application").await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let prefill = match load_prefill(&state, &session).await {
        Ok(prefill) => prefill,
        Err(response) => return response,
    };
    html(StatusCode::OK, views::application(&prefill, Uuid::new_v4()))
}

async fn submit_application(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Response {
    if !valid_post_origin(&state, &headers) {
        return invalid(
            "Invalid submission",
            "The form origin could not be verified.",
            Some("/submit-application"),
        );
    }
    let session = match require_session_for_post(&state, &headers, "/submit-application").await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Ok(mut parsed) = forms::parse_application(multipart).await else {
        return invalid(
            "Check your application",
            "A field or private document is missing, unsupported, or invalid.",
            Some("/submit-application"),
        );
    };
    let token = match state.sessions.delegate_for_intake(&session).await {
        Ok(token) => token,
        Err(error) => return auth_failure(error, "/submit-application"),
    };
    let resume_id = match state
        .api
        .upload_intake_file(
            &token,
            &format!("application:{}:resume", parsed.nonce),
            IntakeFile {
                kind: UploadKind::Resume,
                file_name: &parsed.resume.file_name,
                content_type: &parsed.resume.content_type,
                bytes: &parsed.resume.bytes,
                sha256: &parsed.resume.sha256,
            },
        )
        .await
    {
        Ok(id) => id,
        Err(error) => return api_failure(error, "/submit-application"),
    };
    let photo_id = match state
        .api
        .upload_intake_file(
            &token,
            &format!("application:{}:photo-id", parsed.nonce),
            IntakeFile {
                kind: UploadKind::PhotoId,
                file_name: &parsed.photo_id.file_name,
                content_type: &parsed.photo_id.content_type,
                bytes: &parsed.photo_id.bytes,
                sha256: &parsed.photo_id.sha256,
            },
        )
        .await
    {
        Ok(id) => id,
        Err(error) => return api_failure(error, "/submit-application"),
    };
    parsed.input.resume_upload_id = resume_id;
    parsed.input.photo_id_upload_id = photo_id;
    if parsed.input.validate().is_err() {
        return invalid(
            "Check your application",
            "One or more application fields are invalid.",
            Some("/submit-application"),
        );
    }
    match state
        .api
        .submit_application(
            &token,
            &format!("application:{}:submit", parsed.nonce),
            &parsed.input,
        )
        .await
    {
        Ok(receipt) => html(
            StatusCode::CREATED,
            views::success("Application", receipt.submission_id),
        ),
        Err(error) => api_failure(error, "/submit-application"),
    }
}

async fn show_referral(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_session_for_get(&state, &headers, "/submit-referral").await {
        return response;
    }
    html(StatusCode::OK, views::referral(Uuid::new_v4()))
}

async fn submit_referral(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ReferralForm>,
) -> Response {
    if !valid_post_origin(&state, &headers) {
        return invalid(
            "Invalid submission",
            "The form origin could not be verified.",
            Some("/submit-referral"),
        );
    }
    let session = match require_session_for_post(&state, &headers, "/submit-referral").await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Ok((input, nonce)) = form.into_contract() else {
        return invalid(
            "Check your referral",
            "The referral is incomplete or the nominee consent was not confirmed.",
            Some("/submit-referral"),
        );
    };
    let token = match state.sessions.delegate_for_intake(&session).await {
        Ok(token) => token,
        Err(error) => return auth_failure(error, "/submit-referral"),
    };
    match state
        .api
        .submit_referral(&token, &format!("referral:{nonce}"), &input)
        .await
    {
        Ok(receipt) => html(
            StatusCode::CREATED,
            views::success("Referral", receipt.submission_id),
        ),
        Err(error) => api_failure(error, "/submit-referral"),
    }
}

#[allow(clippy::result_large_err)]
async fn load_prefill(
    state: &AppState,
    session: &AuthenticatedSession,
) -> Result<IntakePrefill, Response> {
    match state.reads.latest_intake_prefill(&session.subject).await {
        Ok(Some(prefill)) => Ok(prefill),
        Ok(None) => Ok(IntakePrefill {
            email: session.email.clone().unwrap_or_default(),
            linkedin_url: String::new(),
            entrepreneurship_idea: String::new(),
            stay_preference: "three_months".into(),
        }),
        Err(_) => Err(unavailable(
            "Prefill unavailable",
            "We could not safely load your existing intake details.",
            None,
        )),
    }
}

#[allow(clippy::result_large_err)]
async fn require_session_for_get(
    state: &AppState,
    headers: &HeaderMap,
    return_to: &'static str,
) -> Result<AuthenticatedSession, Response> {
    match state.sessions.resolve(headers).await {
        Ok(session) => Ok(session),
        Err(SessionError::Missing | SessionError::Invalid) => {
            Err(login_redirect(&state.config, return_to))
        }
        Err(SessionError::Unavailable) => Err(unavailable(
            "Sign-in unavailable",
            "HHaus could not verify your session. No private account information was shown.",
            None,
        )),
    }
}

#[allow(clippy::result_large_err)]
async fn require_session_for_post(
    state: &AppState,
    headers: &HeaderMap,
    retry_path: &'static str,
) -> Result<AuthenticatedSession, Response> {
    state
        .sessions
        .resolve(headers)
        .await
        .map_err(|error| auth_failure(error, retry_path))
}

fn auth_failure(error: SessionError, retry_path: &'static str) -> Response {
    match error {
        SessionError::Missing | SessionError::Invalid => html(
            StatusCode::UNAUTHORIZED,
            views::error(
                "Sign in again",
                "Your session is missing or expired. Sign in, then reopen the form before submitting again.",
                Some(retry_path),
            ),
        ),
        SessionError::Unavailable => unavailable(
            "Sign-in unavailable",
            "HHaus could not make a safe authentication decision. Your submission was not accepted.",
            Some(retry_path),
        ),
    }
}

fn api_failure(error: ApiError, retry_path: &'static str) -> Response {
    match error {
        ApiError::Invalid => invalid(
            "Submission not accepted",
            "The intake service rejected a field or detected a conflicting replay.",
            Some(retry_path),
        ),
        ApiError::Unauthorized => auth_failure(SessionError::Invalid, retry_path),
        ApiError::Unavailable => unavailable(
            "Submission not completed",
            "The intake service could not confirm both HHaus database copies. Please retry with the same information.",
            Some(retry_path),
        ),
    }
}

fn valid_post_origin(state: &AppState, headers: &HeaderMap) -> bool {
    let expected = state.config.public_origin.origin().ascii_serialization();
    headers
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected)
}

fn login_redirect(config: &Config, return_to: &'static str) -> Response {
    Redirect::to(&format!(
        "{}/auth/browser/sign-in?return={return_to}",
        config.shared_auth_browser_prefix
    ))
    .into_response()
}

fn invalid(title: &str, message: &str, retry: Option<&str>) -> Response {
    html(
        StatusCode::UNPROCESSABLE_ENTITY,
        views::error(title, message, retry),
    )
}

fn unavailable(title: &str, message: &str, retry: Option<&str>) -> Response {
    html(
        StatusCode::SERVICE_UNAVAILABLE,
        views::error(title, message, retry),
    )
}

fn html(status: StatusCode, markup: maud::Markup) -> Response {
    (status, Html(markup.into_string())).into_response()
}

async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    headers.insert(
        axum::http::header::PRAGMA,
        HeaderValue::from_static("no-cache"),
    );
    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        axum::http::header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        axum::http::header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    headers.insert(
        axum::http::header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; style-src 'unsafe-inline'; img-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    response
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
