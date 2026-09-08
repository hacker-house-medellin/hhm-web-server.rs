use hhm_orm_core::IntakePrefill;
use maud::{DOCTYPE, Markup, PreEscaped, html};
use uuid::Uuid;

#[must_use]
pub fn home(email: Option<&str>, points: i64) -> Markup {
    layout(
        "Your HHaus account",
        "/",
        html! {
            section class="hero" {
                p class="eyebrow" { "HHaus · Medellín" }
                h1 { "Build the next chapter with us." }
                @if let Some(email) = email {
                    p class="lede" { "Signed in as " strong { (email) } ". Your applications are tied to this verified account." }
                } @else {
                    p class="lede" { "Your verified HHaus session is active." }
                }
            }
            section class="score" aria-label="HHaus points" {
                div { span class="score-number" { (points) } span class="score-label" { "HHaus points" } }
                p { "Points are an internal participation score. Every change is recorded in the append-only points ledger." }
            }
            section class="grid" {
                (action_card("Pre-interest", "Tell us the venture you want to build and whether three or six months fits your plan.", "/submit-pre-interest", "Share your idea"))
                (action_card("Full application", "Apply with your background, timing, resume, and private age-verification document.", "/submit-application", "Start application"))
                (action_card("Refer a builder", "Nominate someone you know, with their permission. Referrals are always tied to your account.", "/submit-referral", "Make a referral"))
            }
        },
    )
}

#[must_use]
pub fn pre_interest(prefill: &IntakePrefill, nonce: Uuid) -> Markup {
    layout(
        "Pre-interest",
        "/submit-pre-interest",
        html! {
            (intro("Pre-interest", "Start with the venture", "This takes about five minutes. We use it to understand the problem you want to solve and which residency length fits your work."))
            form method="post" action="/submit-pre-interest" class="panel form-stack" {
                input type="hidden" name="submission_nonce" value=(nonce);
                (field("Email", "We will use this for application updates.", html! {
                    input type="email" name="email" value=(&prefill.email) autocomplete="email" maxlength="320" required;
                }))
                (field("LinkedIn profile", "Use your personal /in/ profile URL.", html! {
                    input type="url" name="linkedin_url" value=(&prefill.linkedin_url) autocomplete="url" placeholder="https://www.linkedin.com/in/your-name" required;
                }))
                (field("Entrepreneurship idea", "Describe the customer, problem, and what you hope to build. 40–4,000 characters.", html! {
                    textarea name="entrepreneurship_idea" minlength="40" maxlength="4000" rows="8" required { (&prefill.entrepreneurship_idea) }
                }))
                fieldset {
                    legend { "Preferred stay" }
                    label class="choice" { input type="radio" name="stay_preference" value="three_months" checked[prefill.stay_preference != "six_months"]; span { strong { "3 months" } " · Focused build sprint" } }
                    label class="choice" { input type="radio" name="stay_preference" value="six_months" checked[prefill.stay_preference == "six_months"]; span { strong { "6 months" } " · Longer residency and community arc" } }
                }
                (privacy_consent())
                button class="primary" type="submit" { "Submit pre-interest" }
            }
        },
    )
}

#[must_use]
pub fn application(prefill: &IntakePrefill, nonce: Uuid) -> Markup {
    layout(
        "Application",
        "/submit-application",
        html! {
            (intro("Application", "Apply to live and build at HHaus", "Your resume and identity document are placed in private storage. The identity document is reviewed only for age and identity eligibility."))
            form method="post" action="/submit-application" enctype="multipart/form-data" class="panel form-stack" {
                input type="hidden" name="submission_nonce" value=(nonce);
                div class="two-col" {
                    (field("Email", "Application updates go here.", html! { input type="email" name="email" value=(&prefill.email) autocomplete="email" maxlength="320" required; }))
                    (field("Legal name", "Match the name on your ID.", html! { input type="text" name="legal_name" autocomplete="name" maxlength="200" required; }))
                    (field("LinkedIn profile", "Your personal /in/ URL.", html! { input type="url" name="linkedin_url" value=(&prefill.linkedin_url) placeholder="https://www.linkedin.com/in/your-name" required; }))
                    (field("Date of birth", "Applicants must be 18 or older.", html! { input type="date" name="date_of_birth" autocomplete="bday" min="1900-01-01" required; }))
                    (field("Nationality", "Used for program and logistics planning.", html! { input type="text" name="nationality" autocomplete="country-name" maxlength="120" required; }))
                    (field("Phone", "Include country code.", html! { input type="tel" name="phone" autocomplete="tel" maxlength="32" required; }))
                    (field("Current city", "City and country.", html! { input type="text" name="current_city" autocomplete="address-level2" maxlength="160" required; }))
                    (field("Preferred start month", "Choose the month you could arrive.", html! { input type="month" name="preferred_start_month" required; }))
                    (field("GitHub URL", "Optional.", html! { input type="url" name="github_url" placeholder="https://github.com/you"; }))
                    (field("Portfolio URL", "Optional.", html! { input type="url" name="portfolio_url" placeholder="https://your-site.example"; }))
                }
                (field("Entrepreneurship idea", "Explain the problem, customer, your approach, and what progress at HHaus would look like. 80–8,000 characters.", html! {
                    textarea name="entrepreneurship_idea" minlength="80" maxlength="8000" rows="10" required { (&prefill.entrepreneurship_idea) }
                }))
                div class="two-col" {
                    (field("Project stage", "Choose the closest fit.", html! {
                        select name="project_stage" required {
                            option value="idea" { "Idea" }
                            option value="prototype" { "Prototype" }
                            option value="early_revenue" { "Early revenue" }
                            option value="growing" { "Growing" }
                            option value="nonprofit_or_open_source" { "Nonprofit or open source" }
                        }
                    }))
                    (field("Preferred stay", "Three or six months.", html! {
                        select name="stay_preference" required {
                            option value="three_months" selected[prefill.stay_preference != "six_months"] { "3 months" }
                            option value="six_months" selected[prefill.stay_preference == "six_months"] { "6 months" }
                        }
                    }))
                }
                (field("Community contribution", "How will you contribute to the house, other founders, and Medellín? 40–4,000 characters.", html! {
                    textarea name="community_contribution" minlength="40" maxlength="4000" rows="7" required {}
                }))
                (field("Accessibility or accommodation notes", "Optional. Share only what you want our review team to consider.", html! {
                    textarea name="accessibility_or_accommodation_notes" maxlength="4000" rows="4" {}
                }))
                (room_placement_fields())
                section class="upload-grid" aria-labelledby="documents-heading" {
                    h2 id="documents-heading" { "Private documents" }
                    (field("Resume", "PDF or DOCX, up to 10 MB.", html! {
                        input type="file" name="resume" accept="application/pdf,application/vnd.openxmlformats-officedocument.wordprocessingml.document" required;
                    }))
                    (field("Photo ID", "PDF, JPEG, PNG, or HEIC, up to 10 MB. Used only for age and identity verification.", html! {
                        input type="file" name="photo_id" accept="application/pdf,image/jpeg,image/png,image/heic" required;
                    }))
                }
                label class="consent" {
                    input type="checkbox" name="age_and_identity_attestation" required;
                    span { "I attest that I am at least 18 and the identity document belongs to me." }
                }
                (privacy_consent())
                button class="primary" type="submit" { "Submit application securely" }
                p class="fine-print" { "Uploading and verifying two private documents can take a moment. Keep this page open until you see your receipt." }
            }
        },
    )
}

fn room_placement_fields() -> Markup {
    html! {
        section class="panel form-stack" aria-labelledby="room-placement-heading" {
            h2 id="room-placement-heading" { "Room placement and living preferences" }
            p class="fine-print" { "These answers are used only for accommodation and room placement—not admission scoring. You may choose ‘Prefer not to say’ for sensory questions." }
            (field("Potential allergies", "Optional. Share food, material, pet, or environmental allergies that may affect a shared living space.", html! {
                textarea name="allergy_notes" maxlength="2000" rows="4" {}
            }))
            div class="two-col" {
                (field("Noise sensitivity", "Choose the closest fit.", html! {
                    select name="noise_sensitivity" required {
                        option value="none" { "None" }
                        option value="low" { "Low" }
                        option value="moderate" { "Moderate" }
                        option value="high" { "High" }
                        option value="prefer_not_to_say" { "Prefer not to say" }
                    }
                }))
                (field("Light sensitivity", "Choose the closest fit.", html! {
                    select name="light_sensitivity" required {
                        option value="none" { "None" }
                        option value="low" { "Low" }
                        option value="moderate" { "Moderate" }
                        option value="high" { "High" }
                        option value="prefer_not_to_say" { "Prefer not to say" }
                    }
                }))
                (field("Room-sharing preference", "Some HHaus rooms have two or three beds.", html! {
                    select name="roommate_preference" required {
                        option value="private_room" { "Private room" }
                        option value="open_to_roommates" { "Open to roommates" }
                        option value="prefer_roommates" { "Prefer roommates" }
                        option value="flexible" { "Flexible / no strong preference" }
                    }
                }))
                (field("Preferred room occupancy", "Total residents in the room, including you.", html! {
                    select name="preferred_room_occupancy" required {
                        option value="1" { "1 person / private room" }
                        option value="2" { "2 people / one roommate" }
                        option value="3" { "3 people / two roommates" }
                    }
                }))
            }
            (field("Other room preferences", "Optional. Tell us about room location, natural light, stairs, layout, schedule, or other placement considerations.", html! {
                textarea name="room_preference_notes" maxlength="2000" rows="4" {}
            }))
            label class="consent" {
                input type="checkbox" name="roommate_for_lower_cost";
                span { "I am interested in roommates for a lower-cost stay." }
            }
            label class="consent" {
                input type="checkbox" name="roommate_for_social_connection";
                span { "I am interested in roommates for more social connection." }
            }
            label class="consent" {
                input type="checkbox" name="accommodation_data_consent" required;
                span { "I consent to HHaus using these allergy, sensory, accommodation, and room-preference answers only for accommodation and placement planning." }
            }
        }
    }
}

#[must_use]
pub fn referral(nonce: Uuid) -> Markup {
    layout(
        "Referral",
        "/submit-referral",
        html! {
            (intro("Referral", "Nominate a thoughtful builder", "Referrals are tied to your verified account. Confirm the nominee has agreed to be referred before you submit."))
            form method="post" action="/submit-referral" class="panel form-stack" {
                input type="hidden" name="submission_nonce" value=(nonce);
                div class="two-col" {
                    (field("Nominee name", "Their full name.", html! { input type="text" name="referee_name" maxlength="200" required; }))
                    (field("Nominee email", "We may use this to invite them to apply.", html! { input type="email" name="referee_email" maxlength="320" required; }))
                    (field("Nominee LinkedIn", "Their personal /in/ profile.", html! { input type="url" name="referee_linkedin_url" placeholder="https://www.linkedin.com/in/their-name" required; }))
                    (field("Your relationship", "How do you know them?", html! { input type="text" name="relationship" maxlength="120" required; }))
                }
                (field("Why HHaus should meet them", "Describe their character, work, and likely contribution. 40–4,000 characters.", html! {
                    textarea name="rationale" minlength="40" maxlength="4000" rows="8" required {}
                }))
                (field("Likely stay preference", "Optional.", html! {
                    select name="stay_preference" {
                        option value="" { "Not sure" }
                        option value="three_months" { "3 months" }
                        option value="six_months" { "6 months" }
                    }
                }))
                label class="consent" {
                    input type="checkbox" name="nominee_consent_confirmed" required;
                    span { "The nominee has given me permission to share this information with HHaus." }
                }
                button class="primary" type="submit" { "Submit referral" }
            }
        },
    )
}

#[must_use]
pub fn success(kind: &str, receipt_id: Uuid) -> Markup {
    layout(
        "Submission received",
        "",
        html! {
            section class="receipt panel" {
                div class="check" aria-hidden="true" { "✓" }
                p class="eyebrow" { "Stored in both HHaus systems" }
                h1 { (kind) " received" }
                p class="lede" { "Your submission is safely recorded in the HHaus primary database and Supabase mirror." }
                dl {
                    dt { "Receipt ID" }
                    dd { code { (receipt_id) } }
                }
                a class="primary button-link" href="/" { "Back to your account" }
            }
        },
    )
}

#[must_use]
pub fn error(title: &str, message: &str, retry: Option<&str>) -> Markup {
    layout(
        title,
        "",
        html! {
            section class="panel error-panel" {
                p class="eyebrow" { "We could not complete that request" }
                h1 { (title) }
                p class="lede" { (message) }
                @if let Some(path) = retry {
                    a class="primary button-link" href=(path) { "Try again" }
                }
                a class="quiet-link" href="/" { "Return to your account" }
            }
        },
    )
}

fn intro(eyebrow: &str, heading: &str, copy: &str) -> Markup {
    html! {
        section class="page-intro" {
            p class="eyebrow" { (eyebrow) }
            h1 { (heading) }
            p class="lede" { (copy) }
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
fn field(label: &str, help: &str, control: Markup) -> Markup {
    html! {
        label class="field" {
            span class="label" { (label) }
            span class="help" { (help) }
            (control)
        }
    }
}

fn privacy_consent() -> Markup {
    html! {
        label class="consent" {
            input type="checkbox" name="privacy_accepted" required;
            span { "I have read the HHaus intake privacy notice (version 2026-08-30) and consent to dual storage in the HHaus PostgreSQL and Supabase systems." }
        }
    }
}

fn action_card(title: &str, copy: &str, href: &str, action: &str) -> Markup {
    html! {
        article class="card" {
            h2 { (title) }
            p { (copy) }
            a href=(href) { (action) " →" }
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
fn layout(title: &str, current: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="color-scheme" content="light";
                title { (title) " · HHaus" }
                style { (PreEscaped(CSS)) }
            }
            body {
                header class="site-header" {
                    a class="brand" href="/" aria-label="HHaus account home" { span class="brand-mark" { "H" } span { "HHaus" } }
                    nav aria-label="Account" {
                        a aria-current=[(current == "/submit-pre-interest").then_some("page")] href="/submit-pre-interest" { "Pre-interest" }
                        a aria-current=[(current == "/submit-application").then_some("page")] href="/submit-application" { "Application" }
                        a aria-current=[(current == "/submit-referral").then_some("page")] href="/submit-referral" { "Referral" }
                    }
                }
                main { (content) }
                footer { "HHaus Medellín · Founder residency" }
            }
        }
    }
}

const CSS: &str = r"
:root{--ink:#18231d;--muted:#5c6b63;--paper:#f7f5ed;--card:#fffdf7;--line:#d9d7ca;--green:#1f5a3d;--lime:#d4f47b;--rust:#b84e2b;--shadow:0 18px 45px rgba(25,38,30,.08)}
*{box-sizing:border-box}body{margin:0;background:var(--paper);color:var(--ink);font:16px/1.55 Inter,ui-sans-serif,system-ui,-apple-system,sans-serif}a{color:inherit}.site-header{max-width:1180px;margin:auto;padding:1.2rem 2rem;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid var(--line)}.brand{display:flex;align-items:center;gap:.65rem;text-decoration:none;font-weight:800;letter-spacing:-.03em}.brand-mark{display:grid;place-items:center;width:2.2rem;height:2.2rem;border-radius:.65rem;background:var(--ink);color:var(--lime)}nav{display:flex;gap:1.25rem}nav a{text-decoration:none;color:var(--muted);font-size:.92rem}nav a[aria-current=page],nav a:hover{color:var(--ink)}main{max-width:1040px;margin:0 auto;padding:4.5rem 2rem 6rem}.hero,.page-intro{max-width:780px;margin-bottom:2.5rem}.eyebrow{text-transform:uppercase;letter-spacing:.14em;font-size:.75rem;font-weight:800;color:var(--green)}h1{font:700 clamp(2.5rem,7vw,5.8rem)/.98 Georgia,serif;letter-spacing:-.055em;margin:.3rem 0 1.2rem}h2{font:700 1.45rem/1.15 Georgia,serif;margin:.2rem 0 .8rem}.lede{font-size:1.18rem;color:var(--muted);max-width:730px}.grid{display:grid;grid-template-columns:repeat(3,1fr);gap:1rem}.card,.panel,.score{background:var(--card);border:1px solid var(--line);border-radius:1.25rem;box-shadow:var(--shadow)}.card{padding:1.55rem;display:flex;min-height:250px;flex-direction:column}.card p{color:var(--muted)}.card a{margin-top:auto;color:var(--green);font-weight:750;text-decoration:none}.score{display:flex;align-items:center;gap:2rem;padding:1.4rem 1.7rem;margin:0 0 1.4rem}.score-number{display:block;font:700 2.4rem/1 Georgia,serif}.score-label{font-size:.8rem;text-transform:uppercase;letter-spacing:.1em;color:var(--muted)}.score p{margin:0;color:var(--muted)}.panel{padding:clamp(1.35rem,4vw,3rem)}.form-stack{display:grid;gap:1.7rem}.two-col{display:grid;grid-template-columns:1fr 1fr;gap:1.4rem}.field{display:grid;gap:.45rem}.label,legend{font-weight:750}.help{font-size:.88rem;color:var(--muted)}input,textarea,select{width:100%;border:1px solid #b9beb8;background:#fff;border-radius:.7rem;padding:.82rem .9rem;color:var(--ink);font:inherit}textarea{resize:vertical}input:focus,textarea:focus,select:focus{outline:3px solid rgba(31,90,61,.18);border-color:var(--green)}fieldset{border:0;padding:0;margin:0;display:grid;gap:.65rem}.choice,.consent{display:flex;align-items:flex-start;gap:.7rem;border:1px solid var(--line);border-radius:.8rem;padding:.85rem 1rem;background:#fff}.choice input,.consent input{width:auto;margin-top:.28rem}.primary{border:0;border-radius:.8rem;background:var(--ink);color:#fff;font-weight:800;padding:1rem 1.25rem;cursor:pointer}.primary:hover{background:var(--green)}.button-link{display:inline-block;text-decoration:none}.upload-grid{border:1px dashed #9da99f;border-radius:1rem;padding:1.3rem;background:#f4f8ef}.upload-grid h2{margin-top:0}.fine-print{color:var(--muted);font-size:.85rem;margin:0}.receipt{text-align:center;max-width:720px;margin:2rem auto}.check{display:grid;place-items:center;width:4rem;height:4rem;margin:0 auto 1rem;border-radius:50%;background:var(--lime);font-size:2rem}.receipt h1,.error-panel h1{font-size:clamp(2.5rem,6vw,4.5rem)}dl{margin:2rem 0}dt{font-size:.75rem;text-transform:uppercase;letter-spacing:.12em;color:var(--muted)}dd{margin:.4rem 0;overflow-wrap:anywhere}.quiet-link{display:block;margin-top:1.2rem;color:var(--muted)}footer{border-top:1px solid var(--line);padding:2rem;text-align:center;color:var(--muted);font-size:.85rem}
@media(max-width:760px){.site-header{align-items:flex-start;padding:1rem}.site-header nav{display:grid;gap:.25rem;text-align:right}main{padding:3rem 1rem 4rem}.grid,.two-col{grid-template-columns:1fr}.score{align-items:flex-start;flex-direction:column;gap:.7rem}.card{min-height:0}}
";

#[cfg(test)]
mod tests {
    use super::*;

    fn prefill() -> IntakePrefill {
        IntakePrefill {
            email: "builder@example.com".into(),
            linkedin_url: "https://www.linkedin.com/in/builder".into(),
            entrepreneurship_idea: "A deliberately prefilled founder proposal.".into(),
            stay_preference: "six_months".into(),
        }
    }

    #[test]
    fn application_renders_private_uploads_without_browser_bearers() {
        let output = application(&prefill(), Uuid::new_v4()).into_string();
        for expected in [
            "enctype=\"multipart/form-data\"",
            "name=\"resume\"",
            "name=\"photo_id\"",
            "name=\"age_and_identity_attestation\"",
            "name=\"allergy_notes\"",
            "name=\"noise_sensitivity\"",
            "name=\"light_sensitivity\"",
            "name=\"roommate_preference\"",
            "name=\"preferred_room_occupancy\"",
            "name=\"roommate_for_lower_cost\"",
            "name=\"roommate_for_social_connection\"",
            "name=\"accommodation_data_consent\"",
            "name=\"privacy_accepted\"",
            "builder@example.com",
        ] {
            assert!(output.contains(expected), "missing {expected}");
        }
        assert!(!output.contains("turnstile"));
        assert!(!output.contains("access_token"));
        assert!(!output.contains("Authorization"));
    }

    #[test]
    fn all_authenticated_intake_routes_render() {
        assert!(
            pre_interest(&prefill(), Uuid::new_v4())
                .into_string()
                .contains("action=\"/submit-pre-interest\"")
        );
        assert!(
            referral(Uuid::new_v4())
                .into_string()
                .contains("action=\"/submit-referral\"")
        );
    }
}
