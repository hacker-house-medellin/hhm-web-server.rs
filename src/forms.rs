use std::collections::BTreeMap;

use axum::extract::Multipart;
use chrono::NaiveDate;
use hhm_interfaces::intake::{
    ApplicationCreate, MAX_UPLOAD_BYTES, PRIVACY_NOTICE_VERSION, PreInterestCreate, ProjectStage,
    ReferralCreate, RoommatePreference, SensitivityLevel, StayPreference, UploadKind,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreInterestForm {
    email: String,
    linkedin_url: String,
    entrepreneurship_idea: String,
    stay_preference: String,
    privacy_accepted: Option<String>,
    submission_nonce: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferralForm {
    referee_name: String,
    referee_email: String,
    referee_linkedin_url: String,
    relationship: String,
    rationale: String,
    stay_preference: Option<String>,
    nominee_consent_confirmed: Option<String>,
    submission_nonce: String,
}

pub struct ParsedApplication {
    pub input: ApplicationCreate,
    pub resume: UploadedFile,
    pub photo_id: UploadedFile,
    pub nonce: Uuid,
}

pub struct UploadedFile {
    pub file_name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("the submitted form is invalid")]
pub struct FormError;

impl PreInterestForm {
    /// Converts a browser form into the versioned API contract.
    ///
    /// # Errors
    ///
    /// Returns [`FormError`] for invalid consent, nonce, or contract values.
    pub fn into_contract(self) -> Result<(PreInterestCreate, Uuid), FormError> {
        require_checked(self.privacy_accepted.as_deref())?;
        let nonce = parse_nonce(&self.submission_nonce)?;
        let input = PreInterestCreate {
            email: trimmed(&self.email),
            linkedin_url: trimmed(&self.linkedin_url),
            entrepreneurship_idea: trimmed(&self.entrepreneurship_idea),
            stay_preference: parse_stay(&self.stay_preference)?,
            privacy_notice_version: PRIVACY_NOTICE_VERSION.into(),
            turnstile_token: None,
        };
        input.validate().map_err(|_| FormError)?;
        Ok((input, nonce))
    }
}

impl ReferralForm {
    /// Converts a referral form into the authenticated API contract.
    ///
    /// # Errors
    ///
    /// Returns [`FormError`] for invalid consent, nonce, or contract values.
    pub fn into_contract(self) -> Result<(ReferralCreate, Uuid), FormError> {
        let nonce = parse_nonce(&self.submission_nonce)?;
        let input = ReferralCreate {
            referee_name: trimmed(&self.referee_name),
            referee_email: trimmed(&self.referee_email),
            referee_linkedin_url: trimmed(&self.referee_linkedin_url),
            relationship: trimmed(&self.relationship),
            rationale: trimmed(&self.rationale),
            stay_preference: self
                .stay_preference
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(parse_stay)
                .transpose()?,
            nominee_consent_confirmed: checked(self.nominee_consent_confirmed.as_deref()),
        };
        input.validate().map_err(|_| FormError)?;
        Ok((input, nonce))
    }
}

/// Parses one bounded multipart application and rejects duplicates or unknowns.
///
/// # Errors
///
/// Returns [`FormError`] for malformed text, files, dates, or contract values.
pub async fn parse_application(mut multipart: Multipart) -> Result<ParsedApplication, FormError> {
    let (mut values, resume, photo_id) = collect_application_parts(&mut multipart).await?;

    let nonce = parse_nonce(&take(&mut values, "submission_nonce")?)?;
    let privacy = take(&mut values, "privacy_accepted")?;
    require_checked(Some(&privacy))?;
    let preferred_month = take(&mut values, "preferred_start_month")?;
    let preferred_start_month =
        NaiveDate::parse_from_str(&format!("{}-01", preferred_month.trim()), "%Y-%m-%d")
            .map_err(|_| FormError)?;
    let attestation = take(&mut values, "age_and_identity_attestation")?;
    let accommodation_consent = take(&mut values, "accommodation_data_consent")?;
    let input = ApplicationCreate {
        email: trimmed(&take(&mut values, "email")?),
        linkedin_url: trimmed(&take(&mut values, "linkedin_url")?),
        legal_name: trimmed(&take(&mut values, "legal_name")?),
        date_of_birth: NaiveDate::parse_from_str(
            take(&mut values, "date_of_birth")?.trim(),
            "%Y-%m-%d",
        )
        .map_err(|_| FormError)?,
        nationality: trimmed(&take(&mut values, "nationality")?),
        phone: trimmed(&take(&mut values, "phone")?),
        current_city: trimmed(&take(&mut values, "current_city")?),
        github_url: optional(&take(&mut values, "github_url")?),
        portfolio_url: optional(&take(&mut values, "portfolio_url")?),
        entrepreneurship_idea: trimmed(&take(&mut values, "entrepreneurship_idea")?),
        project_stage: parse_stage(&take(&mut values, "project_stage")?)?,
        stay_preference: parse_stay(&take(&mut values, "stay_preference")?)?,
        preferred_start_month,
        community_contribution: trimmed(&take(&mut values, "community_contribution")?),
        accessibility_or_accommodation_notes: optional(&take(
            &mut values,
            "accessibility_or_accommodation_notes",
        )?),
        allergy_notes: optional(&take(&mut values, "allergy_notes")?),
        noise_sensitivity: parse_sensitivity(&take(&mut values, "noise_sensitivity")?)?,
        light_sensitivity: parse_sensitivity(&take(&mut values, "light_sensitivity")?)?,
        room_preference_notes: optional(&take(&mut values, "room_preference_notes")?),
        roommate_preference: parse_roommate_preference(&take(&mut values, "roommate_preference")?)?,
        preferred_room_occupancy: parse_room_occupancy(&take(
            &mut values,
            "preferred_room_occupancy",
        )?)?,
        roommate_for_lower_cost: checked(values.remove("roommate_for_lower_cost").as_deref()),
        roommate_for_social_connection: checked(
            values.remove("roommate_for_social_connection").as_deref(),
        ),
        accommodation_data_consent: checked(Some(&accommodation_consent)),
        resume_upload_id: Uuid::nil(),
        photo_id_upload_id: Uuid::nil(),
        age_and_identity_attestation: checked(Some(&attestation)),
        privacy_notice_version: PRIVACY_NOTICE_VERSION.into(),
        turnstile_token: None,
    };
    if !values.is_empty() {
        return Err(FormError);
    }
    // Upload IDs are assigned only after server-side private upload verification,
    // so validate the remaining fields here and the complete contract later.
    if !input.age_and_identity_attestation {
        return Err(FormError);
    }
    if !input.accommodation_data_consent {
        return Err(FormError);
    }
    let mut validation_probe = input.clone();
    validation_probe.resume_upload_id = Uuid::new_v4();
    validation_probe.photo_id_upload_id = Uuid::new_v4();
    validation_probe.validate().map_err(|_| FormError)?;
    Ok(ParsedApplication {
        input,
        resume: resume.ok_or(FormError)?,
        photo_id: photo_id.ok_or(FormError)?,
        nonce,
    })
}

async fn collect_application_parts(
    multipart: &mut Multipart,
) -> Result<
    (
        BTreeMap<String, String>,
        Option<UploadedFile>,
        Option<UploadedFile>,
    ),
    FormError,
> {
    let mut values = BTreeMap::new();
    let mut resume = None;
    let mut photo_id = None;
    while let Some(field) = multipart.next_field().await.map_err(|_| FormError)? {
        let name = field.name().ok_or(FormError)?.to_owned();
        match name.as_str() {
            "resume" | "photo_id" => {
                let kind = if name == "resume" {
                    UploadKind::Resume
                } else {
                    UploadKind::PhotoId
                };
                let file_name = field.file_name().ok_or(FormError)?.to_owned();
                let content_type = field.content_type().ok_or(FormError)?.to_owned();
                let bytes = field.bytes().await.map_err(|_| FormError)?.to_vec();
                let file = validate_file(kind, file_name, content_type, bytes)?;
                let slot = if kind == UploadKind::Resume {
                    &mut resume
                } else {
                    &mut photo_id
                };
                if slot.replace(file).is_some() {
                    return Err(FormError);
                }
            }
            "email"
            | "linkedin_url"
            | "legal_name"
            | "date_of_birth"
            | "nationality"
            | "phone"
            | "current_city"
            | "github_url"
            | "portfolio_url"
            | "entrepreneurship_idea"
            | "project_stage"
            | "stay_preference"
            | "preferred_start_month"
            | "community_contribution"
            | "accessibility_or_accommodation_notes"
            | "allergy_notes"
            | "noise_sensitivity"
            | "light_sensitivity"
            | "room_preference_notes"
            | "roommate_preference"
            | "preferred_room_occupancy"
            | "roommate_for_lower_cost"
            | "roommate_for_social_connection"
            | "accommodation_data_consent"
            | "age_and_identity_attestation"
            | "privacy_accepted"
            | "submission_nonce" => {
                let value = field.text().await.map_err(|_| FormError)?;
                if values.insert(name, value).is_some() {
                    return Err(FormError);
                }
            }
            _ => return Err(FormError),
        }
    }
    Ok((values, resume, photo_id))
}

fn validate_file(
    kind: UploadKind,
    file_name: String,
    content_type: String,
    bytes: Vec<u8>,
) -> Result<UploadedFile, FormError> {
    if bytes.is_empty()
        || bytes.len() as u64 > MAX_UPLOAD_BYTES
        || file_name.is_empty()
        || file_name.len() > 255
        || file_name.contains(['/', '\\', '\0'])
        || !signature_matches(kind, &content_type, &bytes)
    {
        return Err(FormError);
    }
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    Ok(UploadedFile {
        file_name,
        content_type,
        bytes,
        sha256,
    })
}

fn signature_matches(kind: UploadKind, content_type: &str, bytes: &[u8]) -> bool {
    match (kind, content_type) {
        (UploadKind::Resume | UploadKind::PhotoId, "application/pdf") => {
            bytes.starts_with(b"%PDF-")
        }
        (
            UploadKind::Resume,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ) => bytes.starts_with(b"PK\x03\x04"),
        (UploadKind::PhotoId, "image/jpeg") => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        (UploadKind::PhotoId, "image/png") => {
            bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a])
        }
        (UploadKind::PhotoId, "image/heic") => {
            bytes.len() >= 12
                && &bytes[4..8] == b"ftyp"
                && matches!(
                    &bytes[8..12],
                    b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1"
                )
        }
        _ => false,
    }
}

fn take(values: &mut BTreeMap<String, String>, name: &str) -> Result<String, FormError> {
    values.remove(name).ok_or(FormError)
}

fn parse_nonce(value: &str) -> Result<Uuid, FormError> {
    let parsed = Uuid::parse_str(value).map_err(|_| FormError)?;
    if parsed.is_nil() || parsed.to_string() != value {
        return Err(FormError);
    }
    Ok(parsed)
}

fn parse_stay(value: &str) -> Result<StayPreference, FormError> {
    match value.trim() {
        "three_months" => Ok(StayPreference::ThreeMonths),
        "six_months" => Ok(StayPreference::SixMonths),
        _ => Err(FormError),
    }
}

fn parse_stage(value: &str) -> Result<ProjectStage, FormError> {
    match value.trim() {
        "idea" => Ok(ProjectStage::Idea),
        "prototype" => Ok(ProjectStage::Prototype),
        "early_revenue" => Ok(ProjectStage::EarlyRevenue),
        "growing" => Ok(ProjectStage::Growing),
        "nonprofit_or_open_source" => Ok(ProjectStage::NonprofitOrOpenSource),
        _ => Err(FormError),
    }
}

fn parse_sensitivity(value: &str) -> Result<SensitivityLevel, FormError> {
    match value.trim() {
        "none" => Ok(SensitivityLevel::None),
        "low" => Ok(SensitivityLevel::Low),
        "moderate" => Ok(SensitivityLevel::Moderate),
        "high" => Ok(SensitivityLevel::High),
        "prefer_not_to_say" => Ok(SensitivityLevel::PreferNotToSay),
        _ => Err(FormError),
    }
}

fn parse_roommate_preference(value: &str) -> Result<RoommatePreference, FormError> {
    match value.trim() {
        "private_room" => Ok(RoommatePreference::PrivateRoom),
        "open_to_roommates" => Ok(RoommatePreference::OpenToRoommates),
        "prefer_roommates" => Ok(RoommatePreference::PreferRoommates),
        "flexible" => Ok(RoommatePreference::Flexible),
        _ => Err(FormError),
    }
}

fn parse_room_occupancy(value: &str) -> Result<i32, FormError> {
    match value.trim() {
        "1" => Ok(1),
        "2" => Ok(2),
        "3" => Ok(3),
        _ => Err(FormError),
    }
}

fn checked(value: Option<&str>) -> bool {
    matches!(value, Some("on" | "yes" | "true" | "1"))
}

fn require_checked(value: Option<&str>) -> Result<(), FormError> {
    if checked(value) {
        Ok(())
    } else {
        Err(FormError)
    }
}

fn trimmed(value: &str) -> String {
    value.trim().to_owned()
}

fn optional(value: &str) -> Option<String> {
    let value = trimmed(value);
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_signatures_are_kind_specific() {
        assert!(signature_matches(
            UploadKind::PhotoId,
            "image/png",
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        ));
        assert!(!signature_matches(
            UploadKind::Resume,
            "image/png",
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        ));
    }

    #[test]
    fn nonce_is_canonical_and_nonzero() {
        assert!(parse_nonce("00000000-0000-0000-0000-000000000000").is_err());
        assert!(parse_nonce(&Uuid::new_v4().to_string()).is_ok());
    }

    #[test]
    fn placement_choices_are_closed_and_allow_three_person_rooms() {
        assert_eq!(parse_room_occupancy("3"), Ok(3));
        assert!(parse_room_occupancy("4").is_err());
        assert_eq!(
            parse_sensitivity("prefer_not_to_say"),
            Ok(SensitivityLevel::PreferNotToSay)
        );
        assert!(parse_sensitivity("extreme").is_err());
        assert_eq!(
            parse_roommate_preference("prefer_roommates"),
            Ok(RoommatePreference::PreferRoommates)
        );
        assert!(parse_roommate_preference("anyone").is_err());
    }

    #[test]
    fn referral_allows_an_unspecified_stay() {
        let (input, _) = ReferralForm {
            referee_name: "Nominee Builder".into(),
            referee_email: "nominee@example.com".into(),
            referee_linkedin_url: "https://www.linkedin.com/in/nominee".into(),
            relationship: "Former teammate".into(),
            rationale: "A thoughtful operator who consistently helps teams turn ambiguous problems into durable systems.".into(),
            stay_preference: Some(String::new()),
            nominee_consent_confirmed: Some("on".into()),
            submission_nonce: Uuid::new_v4().to_string(),
        }
        .into_contract()
        .unwrap();
        assert_eq!(input.stay_preference, None);
    }
}
