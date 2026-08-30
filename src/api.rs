use hhm_interfaces::intake::{
    ApplicationCreate, ReferralCreate, SubmissionReceipt, UploadCompleteCreate,
    UploadCompletionReceipt, UploadIntentCreate, UploadIntentReceipt, UploadKind,
};
use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use url::Url;
use uuid::Uuid;

use crate::config::Config;

const MAX_API_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    api_base_url: Url,
    storage_origin: url::Origin,
    origin_header: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ApiError {
    #[error("the intake request is invalid or conflicts with existing state")]
    Invalid,
    #[error("the authenticated intake session was rejected")]
    Unauthorized,
    #[error("the intake service is unavailable")]
    Unavailable,
}

pub struct IntakeFile<'a> {
    pub kind: UploadKind,
    pub file_name: &'a str,
    pub content_type: &'a str,
    pub bytes: &'a [u8],
    pub sha256: &'a str,
}

impl ApiClient {
    /// Builds the internal API and signed-upload client.
    ///
    /// # Errors
    ///
    /// Returns a transport error if static client configuration is invalid.
    pub fn from_config(config: &Config) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(std::time::Duration::from_secs(2))
                .timeout(std::time::Duration::from_secs(30))
                .user_agent("hhm-user-web/0.1")
                .build()?,
            api_base_url: config.api_base_url.clone(),
            storage_origin: config.supabase_url.origin(),
            origin_header: config.public_origin.origin().ascii_serialization(),
        })
    }

    /// Submits an authenticated pre-interest record.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError`] if authorization, validation, or dual persistence fails.
    pub async fn submit_pre_interest(
        &self,
        token: &SecretString,
        idempotency_key: &str,
        input: &hhm_interfaces::intake::PreInterestCreate,
    ) -> Result<SubmissionReceipt, ApiError> {
        self.post_json("v1/pre-interests", token, idempotency_key, input)
            .await
    }

    /// Submits an authenticated application after both private uploads verify.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError`] if authorization, validation, or dual persistence fails.
    pub async fn submit_application(
        &self,
        token: &SecretString,
        idempotency_key: &str,
        input: &ApplicationCreate,
    ) -> Result<SubmissionReceipt, ApiError> {
        self.post_json("v1/applications", token, idempotency_key, input)
            .await
    }

    /// Submits a referral under the current verified subject.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError`] if authorization, validation, or dual persistence fails.
    pub async fn submit_referral(
        &self,
        token: &SecretString,
        idempotency_key: &str,
        input: &ReferralCreate,
    ) -> Result<SubmissionReceipt, ApiError> {
        self.post_json("v1/referrals", token, idempotency_key, input)
            .await
    }

    /// Reserves, uploads, and verifies one private intake file.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError`] unless the object and both database records verify.
    pub async fn upload_intake_file(
        &self,
        token: &SecretString,
        idempotency_prefix: &str,
        file: IntakeFile<'_>,
    ) -> Result<Uuid, ApiError> {
        let intent: UploadIntentReceipt = self
            .post_json(
                "v1/intake/uploads",
                token,
                &format!("{idempotency_prefix}:intent"),
                &UploadIntentCreate {
                    kind: file.kind,
                    file_name: file.file_name.to_owned(),
                    content_type: file.content_type.to_owned(),
                    size_bytes: u64::try_from(file.bytes.len()).map_err(|_| ApiError::Invalid)?,
                    sha256: file.sha256.to_owned(),
                    turnstile_token: None,
                },
            )
            .await?;
        if intent.upload_url.origin() != self.storage_origin
            || intent.upload_url.scheme() != "https"
            || !intent
                .allowed_content_types
                .iter()
                .any(|candidate| candidate == file.content_type)
            || intent.maximum_bytes < file.bytes.len() as u64
        {
            return Err(ApiError::Unavailable);
        }
        let upload_attempt = self
            .http
            .put(intent.upload_url)
            .header(reqwest::header::CONTENT_TYPE, file.content_type)
            .header("x-upsert", "false")
            .body(file.bytes.to_vec())
            .send()
            .await;
        if let Ok(response) = upload_attempt
            && response.status().is_success()
        {
            drain_bounded(response).await?;
        }

        // A retried idempotent intent can find the same object already stored,
        // and an upload transport error can be ambiguous. Completion is the
        // authority: the API streams the private object and verifies its exact
        // size, MIME type, digest, owner, and metadata in both databases.
        let completed: UploadCompletionReceipt = self
            .post_json(
                &format!("v1/intake/uploads/{}/complete", intent.upload_id),
                token,
                &format!("{idempotency_prefix}:complete"),
                &UploadCompleteCreate {
                    sha256: file.sha256.to_owned(),
                    turnstile_token: None,
                },
            )
            .await?;
        if completed.upload_id != intent.upload_id {
            return Err(ApiError::Unavailable);
        }
        Ok(completed.upload_id)
    }

    async fn post_json<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        token: &SecretString,
        idempotency_key: &str,
        input: &T,
    ) -> Result<R, ApiError> {
        let endpoint = self
            .api_base_url
            .join(path)
            .map_err(|_| ApiError::Unavailable)?;
        let response = self
            .http
            .post(endpoint)
            .header(reqwest::header::ORIGIN, &self.origin_header)
            .header("idempotency-key", idempotency_key)
            .bearer_auth(token.expose_secret())
            .json(input)
            .send()
            .await
            .map_err(|_| ApiError::Unavailable)?;
        match response.status() {
            status if status.is_success() => bounded_json(response).await,
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                Err(ApiError::Unauthorized)
            }
            reqwest::StatusCode::CONFLICT
            | reqwest::StatusCode::BAD_REQUEST
            | reqwest::StatusCode::UNPROCESSABLE_ENTITY => Err(ApiError::Invalid),
            _ => Err(ApiError::Unavailable),
        }
    }
}

async fn bounded_json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, ApiError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_API_RESPONSE_BYTES as u64)
    {
        return Err(ApiError::Unavailable);
    }
    let body = response.bytes().await.map_err(|_| ApiError::Unavailable)?;
    if body.len() > MAX_API_RESPONSE_BYTES {
        return Err(ApiError::Unavailable);
    }
    serde_json::from_slice(&body).map_err(|_| ApiError::Unavailable)
}

async fn drain_bounded(response: reqwest::Response) -> Result<(), ApiError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_API_RESPONSE_BYTES as u64)
    {
        return Err(ApiError::Unavailable);
    }
    let body = response.bytes().await.map_err(|_| ApiError::Unavailable)?;
    if body.len() > MAX_API_RESPONSE_BYTES {
        return Err(ApiError::Unavailable);
    }
    Ok(())
}
