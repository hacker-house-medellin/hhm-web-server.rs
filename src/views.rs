//! Server-rendered public and authenticated entry surfaces.

use maud::{DOCTYPE, Markup, PreEscaped, html};

use crate::{EntryKind, Item};

#[must_use]
pub fn landing() -> Markup {
    page(
        "Live well. Build boldly.",
        "/",
        html! {
            section class="hero" aria-labelledby="hero-title" {
                div class="hero-copy" {
                    p class="eyebrow" { "HHaus · Medellín, Colombia" }
                    h1 id="hero-title" { "A home base for people building what comes next." }
                    p class="lede" { "Thoughtful coliving, dependable workspaces, and a community designed for founders, remote teams, and independent makers." }
                    div class="hero-actions" aria-label="Choose how to join" {
                        a class="button button-primary" href="/join/individual" { "Join as an individual" }
                        a class="button button-secondary" href="/join/organization" { "Bring your organization" }
                    }
                    p class="microcopy" { "Secure passwordless entry · No card required to get started" }
                }
                aside class="hero-card" aria-label="What your stay can include" {
                    div class="status-pill" { span aria-hidden="true" { "●" } " Designed for deep work and real community" }
                    h2 { "Your Medellín operating system" }
                    ul class="feature-list" {
                        li { span class="feature-number" { "01" } div { strong { "Live" } p { "Flexible rooms, clear arrival plans, and a calm place to land." } } }
                        li { span class="feature-number" { "02" } div { strong { "Work" } p { "Reliable connectivity and spaces for focus, calls, and collaboration." } } }
                        li { span class="feature-number" { "03" } div { strong { "Belong" } p { "A curated community with room for both momentum and rest." } } }
                    }
                }
            }
            section class="trust-strip" aria-label="HHaus principles" {
                p { strong { "One place." } " Personal stays and team programs." }
                p { strong { "One identity." } " Passwordless, privacy-minded access." }
                p { strong { "One boundary." } " Your organization permissions stay server-controlled." }
            }
            section class="section" aria-labelledby="path-title" {
                div class="section-heading" {
                    p class="eyebrow" { "Choose your path" }
                    h2 id="path-title" { "Built for one person—and for a whole team." }
                    p { "Start with the experience that matches you. You can talk with us before committing to a stay or a program." }
                }
                div class="path-grid" {
                    article class="path-card individual" {
                        p class="card-kicker" { "For individuals" }
                        h3 { "Make Medellín your next chapter." }
                        p { "A simple path for a first visit, a focused residency, or a longer season in the city." }
                        ul class="check-list" {
                            li { "Plan a one-time or recurring stay" }
                            li { "Explore rooms, workspace, and community" }
                            li { "Keep one private HHaus account" }
                        }
                        a class="text-link" href="/join/individual" { "Start individual onboarding " span aria-hidden="true" { "→" } }
                    }
                    article class="path-card organization" {
                        p class="card-kicker" { "For organizations" }
                        h3 { "Give your people a better place to gather." }
                        p { "Create a home base for offsites, remote teams, founder cohorts, or employee mobility." }
                        ul class="check-list" {
                            li { "Coordinate multiple member accounts" }
                            li { "Separate organization and member access" }
                            li { "Keep roles and invitations centrally governed" }
                        }
                        a class="text-link" href="/join/organization" { "Start organization onboarding " span aria-hidden="true" { "→" } }
                    }
                }
            }
            section id="experience" class="section experience" aria-labelledby="experience-title" {
                div {
                    p class="eyebrow" { "More than a room" }
                    h2 id="experience-title" { "Everything you need to arrive ready." }
                }
                div class="experience-grid" {
                    (experience_card("Stay", "Flexible living plans with a clear, human arrival experience."))
                    (experience_card("Work", "Space and connectivity designed around the reality of remote work."))
                    (experience_card("Connect", "A considered community—not forced networking or a crowded lobby."))
                    (experience_card("Operate", "One account for future bookings, visits, and house services."))
                }
            }
            section class="closing" aria-labelledby="closing-title" {
                p class="eyebrow" { "Ready when you are" }
                h2 id="closing-title" { "Find your place at HHaus." }
                p { "Tell us whether you are planning for yourself or an organization. We will guide the rest." }
                div class="hero-actions centered" {
                    a class="button button-light" href="/join/individual" { "I’m joining myself" }
                    a class="button button-outline-light" href="/join/organization" { "I’m planning for a team" }
                }
            }
        },
    )
}

#[must_use]
pub fn entry(kind: EntryKind, csrf: &str, site_key: &str) -> Markup {
    let (kicker, title, lede, steps, button) = match kind {
        EntryKind::Individual => (
            "Individual membership",
            "Your next home base starts here.",
            "Use your private HHaus identity to explore a stay, complete your profile, and return to plans later.",
            [
                ("Verify", "Continue to HHaus passwordless sign-in."),
                (
                    "Introduce yourself",
                    "Share your goals and timing after sign-in.",
                ),
                (
                    "Plan",
                    "Review the next available stay and workspace options.",
                ),
            ],
            "Continue as an individual",
        ),
        EntryKind::Organization => (
            "Organization programs",
            "Bring your team together in Medellín.",
            "Start the organization conversation with your own verified identity. HHaus will establish the organization and its permissions on the server.",
            [
                (
                    "Verify",
                    "Sign in as the person opening the organization request.",
                ),
                (
                    "Shape the program",
                    "Describe team size, timing, and desired experience.",
                ),
                (
                    "Invite",
                    "Add members only after HHaus confirms the organization.",
                ),
            ],
            "Continue for my organization",
        ),
    };
    page(
        title,
        kind.path(),
        html! {
            section class="entry-shell" aria-labelledby="entry-title" {
                div class="entry-story" {
                    a class="back-link" href="/" { span aria-hidden="true" { "←" } " Back to HHaus" }
                    p class="eyebrow" { (kicker) }
                    h1 id="entry-title" { (title) }
                    p class="lede" { (lede) }
                    ol class="step-list" {
                        @for (index, (step_title, text)) in steps.iter().enumerate() {
                            li {
                                span class="step-number" aria-hidden="true" { (index + 1) }
                                div { strong { (step_title) } p { (text) } }
                            }
                        }
                    }
                }
                div class="entry-panel" {
                    p class="panel-label" { "Secure account entry" }
                    h2 { "Continue with Shared Auth" }
                    p { "HHaus does not collect or process your password on this page. Shared Auth completes the passwordless ceremony on this same origin." }
                    form method="post" action="/auth/start" class="entry-form" {
                        input type="hidden" name="kind" value=(kind.as_str());
                        input type="hidden" name="csrf" value=(csrf);
                        div class="cf-turnstile" data-sitekey=(site_key) data-action="hhaus_entry" data-theme="light" {}
                        noscript {
                            p class="notice" role="status" { "JavaScript is required only for the anti-abuse check. No HHaus password or session token is handled by page script." }
                        }
                        button class="button button-primary button-wide" type="submit" { (button) }
                    }
                    div class="privacy-note" {
                        span class="lock" aria-hidden="true" { "◆" }
                        p { strong { "Private by design." } " Browser code never receives an HHaus API bearer. Organization and role access is decided from HHaus server records after sign-in." }
                    }
                }
            }
            script src="https://challenges.cloudflare.com/turnstile/v0/api.js" async defer {}
        },
    )
}

#[must_use]
pub fn onboarding(kind: EntryKind, email: Option<&str>) -> Markup {
    let (kicker, title, intro, next_steps) = match kind {
        EntryKind::Individual => (
            "Individual onboarding",
            "Welcome to your HHaus path.",
            "Your Shared Auth session is active. The next product slice will connect this shell to the existing residency intake without exposing your session to the browser.",
            [
                "Tell us what you are building and when you want to arrive.",
                "Review room and workspace preferences.",
                "Submit only after every private field is ready.",
            ],
        ),
        EntryKind::Organization => (
            "Organization onboarding",
            "Let’s shape the right team experience.",
            "Your own identity is verified. No organization, tenant, or role has been granted by this page; those are established from HHaus-owned records and reviewed invitations.",
            [
                "Share program timing and approximate team size.",
                "Confirm an HHaus organization and its accountable owner.",
                "Invite members with narrow, server-issued permissions.",
            ],
        ),
    };
    page(
        title,
        kind.onboarding_path(),
        html! {
            section class="account-shell" aria-labelledby="account-title" {
                div class="account-heading" {
                    p class="eyebrow" { (kicker) }
                    h1 id="account-title" { (title) }
                    p class="lede" { (intro) }
                    @if let Some(email) = email {
                        p class="identity-chip" { span aria-hidden="true" { "✓" } " Signed in as " strong { (email) } }
                    } @else {
                        p class="identity-chip" { span aria-hidden="true" { "✓" } " Verified HHaus identity" }
                    }
                }
                article class="next-card" {
                    p class="panel-label" { "What comes next" }
                    h2 { "A clear, reviewable onboarding flow" }
                    ol class="next-list" {
                        @for (index, step) in next_steps.iter().enumerate() {
                            li { span { (index + 1) } p { (step) } }
                        }
                    }
                    p class="notice" role="status" { "This safe slice verifies entry and authorization only. It does not create an organization, assign a role, accept payment, or claim a live reservation." }
                    a class="button button-secondary" href="/" { "Return to HHaus" }
                }
            }
        },
    )
}

#[must_use]
pub fn error(title: &str, message: &str, retry: Option<&str>) -> Markup {
    page(
        title,
        "",
        html! {
            section class="error-card" role="alert" aria-labelledby="error-title" {
                p class="eyebrow" { "HHaus entry" }
                h1 id="error-title" { (title) }
                p class="lede" { (message) }
                @if let Some(retry) = retry {
                    a class="button button-primary" href=(retry) { "Try again" }
                }
                a class="text-link standalone" href="/" { "Return home" }
            }
        },
    )
}

#[must_use]
pub fn reservations(items: &[Item]) -> Markup {
    html! {
        @for item in items {
            article class="reservation-card" data-id=(item.id) {
                h2 { (&item.title) }
                p { (&item.detail) }
            }
        }
    }
}

fn experience_card(title: &str, copy: &str) -> Markup {
    html! {
        article {
            span class="experience-mark" aria-hidden="true" { "✦" }
            h3 { (title) }
            p { (copy) }
        }
    }
}

fn page(title: &str, current: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="color-scheme" content="light";
                meta name="theme-color" content="#17362d";
                meta name="description" content="HHaus Medellín — considered coliving and workspace for builders, remote teams, and independent professionals.";
                title { (title) " · HHaus Medellín" }
                style { (PreEscaped(STYLES)) }
            }
            body {
                a class="skip-link" href="#main-content" { "Skip to content" }
                header class="site-header" {
                    a class="brand" href="/" aria-label="HHaus Medellín home" {
                        span class="brand-mark" aria-hidden="true" { "H" }
                        span { strong { "HHaus" } small { "Medellín" } }
                    }
                    nav aria-label="Primary navigation" {
                        a href="/#experience" { "Experience" }
                        a href="/join/individual" aria-current=[(current == "/join/individual").then_some("page")] { "Individuals" }
                        a href="/join/organization" aria-current=[(current == "/join/organization").then_some("page")] { "Organizations" }
                        a class="nav-cta" href="/join/individual" { "Get started" }
                    }
                }
                main id="main-content" { (content) }
                footer class="site-footer" {
                    div {
                        a class="brand footer-brand" href="/" { span class="brand-mark" aria-hidden="true" { "H" } span { strong { "HHaus" } small { "Medellín" } } }
                        p { "A considered home base for ambitious people in Medellín." }
                    }
                    p { "Secure account entry powered by Shared Auth. HHaus does not handle passwords on this site." }
                }
            }
        }
    }
}

const STYLES: &str = r#"
:root{--forest:#17362d;--forest-2:#214b3e;--ink:#17221e;--paper:#f7f3ea;--cream:#fffdf7;--lime:#d9ed83;--mint:#d9eee3;--peach:#f4d5b8;--line:#d8ddd7;--muted:#627069;--shadow:0 24px 70px rgba(23,54,45,.12);--radius:1.35rem}*{box-sizing:border-box}html{scroll-behavior:smooth}body{margin:0;background:var(--cream);color:var(--ink);font:16px/1.6 Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}a{color:inherit}.skip-link{position:fixed;z-index:50;left:1rem;top:1rem;transform:translateY(-180%);background:#fff;color:#000;padding:.75rem 1rem;border-radius:.5rem}.skip-link:focus{transform:none}.site-header{position:sticky;top:0;z-index:20;display:flex;align-items:center;justify-content:space-between;gap:2rem;min-height:5rem;padding:.8rem clamp(1rem,4vw,4.5rem);border-bottom:1px solid rgba(23,54,45,.09);background:rgba(255,253,247,.93);backdrop-filter:blur(18px)}.brand{display:inline-flex;align-items:center;gap:.7rem;text-decoration:none}.brand-mark{display:grid;place-items:center;width:2.45rem;height:2.45rem;border-radius:.7rem;background:var(--forest);color:var(--lime);font:800 1.15rem/1 Georgia,serif}.brand span:last-child{display:grid;line-height:1.05}.brand strong{font:800 1.05rem/1.1 Georgia,serif;letter-spacing:.02em}.brand small{text-transform:uppercase;letter-spacing:.18em;font-size:.55rem;color:var(--muted);margin-top:.25rem}.site-header nav{display:flex;align-items:center;gap:clamp(.8rem,2vw,1.7rem)}.site-header nav a{text-decoration:none;font-size:.9rem;font-weight:650}.site-header nav a:not(.nav-cta):hover{text-decoration:underline;text-underline-offset:.35rem}.nav-cta{padding:.65rem 1rem;border-radius:999px;background:var(--forest);color:#fff}.hero{display:grid;grid-template-columns:minmax(0,1.08fr) minmax(320px,.72fr);gap:clamp(2rem,6vw,6rem);align-items:center;min-height:calc(100vh - 5rem);padding:clamp(4rem,9vw,8rem) clamp(1rem,6vw,7rem);background:radial-gradient(circle at 15% 30%,rgba(217,237,131,.5),transparent 32%),linear-gradient(135deg,#f9f6ed 0%,#f3eee3 100%)}.eyebrow{margin:0 0 1rem;color:#44705f;font-size:.76rem;font-weight:850;letter-spacing:.18em;text-transform:uppercase}.hero h1,.entry-story h1,.account-heading h1,.error-card h1{max-width:12ch;margin:0;font:500 clamp(3.15rem,7vw,7.2rem)/.92 Georgia,"Times New Roman",serif;letter-spacing:-.055em}.lede{max-width:42rem;margin:1.6rem 0 0;color:#485750;font-size:clamp(1.05rem,2vw,1.3rem);line-height:1.65}.hero-actions{display:flex;flex-wrap:wrap;gap:.75rem;margin-top:2rem}.button{display:inline-flex;align-items:center;justify-content:center;min-height:3.2rem;padding:.78rem 1.25rem;border:1px solid transparent;border-radius:999px;font:750 .94rem/1 system-ui;text-decoration:none;cursor:pointer}.button:focus-visible,.text-link:focus-visible,.site-header a:focus-visible,.back-link:focus-visible{outline:3px solid #85a51d;outline-offset:4px}.button-primary{background:var(--forest);color:#fff;box-shadow:0 10px 28px rgba(23,54,45,.18)}.button-primary:hover{background:var(--forest-2)}.button-secondary{border-color:#91a097;background:transparent;color:var(--forest)}.button-secondary:hover{background:#fff}.button-light{background:var(--lime);color:var(--forest)}.button-outline-light{border-color:rgba(255,255,255,.5);color:#fff}.button-wide{width:100%;border-radius:.85rem}.microcopy{color:#68776f;font-size:.82rem}.hero-card{padding:clamp(1.4rem,3vw,2.3rem);border:1px solid rgba(23,54,45,.12);border-radius:var(--radius);background:rgba(255,255,255,.68);box-shadow:var(--shadow);transform:rotate(1.2deg)}.hero-card h2{margin:1.35rem 0 1.6rem;font:500 clamp(1.7rem,3vw,2.5rem)/1.1 Georgia,serif}.status-pill{display:inline-flex;align-items:center;gap:.45rem;padding:.45rem .7rem;border-radius:999px;background:var(--mint);color:#285141;font-size:.72rem;font-weight:750}.status-pill span{color:#54a176}.feature-list{list-style:none;margin:0;padding:0}.feature-list li{display:grid;grid-template-columns:2.3rem 1fr;gap:1rem;padding:1rem 0;border-top:1px solid var(--line)}.feature-number{color:#74847b;font:650 .72rem/1.6 ui-monospace,monospace}.feature-list strong{font-size:1.05rem}.feature-list p{margin:.15rem 0 0;color:var(--muted);font-size:.9rem}.trust-strip{display:grid;grid-template-columns:repeat(3,1fr);gap:1rem;padding:1.35rem clamp(1rem,6vw,7rem);background:var(--forest);color:#dce8e1}.trust-strip p{margin:0;font-size:.86rem}.trust-strip strong{color:var(--lime)}.section{padding:clamp(4rem,8vw,7rem) clamp(1rem,6vw,7rem)}.section-heading{display:grid;grid-template-columns:1fr 1fr;gap:1.5rem 4rem;align-items:end;margin-bottom:2.5rem}.section-heading .eyebrow{grid-column:1/-1;margin:0}.section h2,.closing h2{max-width:15ch;margin:0;font:500 clamp(2.3rem,5vw,4.7rem)/1 Georgia,serif;letter-spacing:-.04em}.section-heading>p:last-child{max-width:35rem;margin:0;color:var(--muted)}.path-grid{display:grid;grid-template-columns:1fr 1fr;gap:1rem}.path-card{min-height:28rem;padding:clamp(1.6rem,4vw,3rem);border-radius:var(--radius);display:flex;flex-direction:column}.path-card.individual{background:var(--mint)}.path-card.organization{background:var(--peach)}.card-kicker,.panel-label{margin:0 0 1rem;color:#4f6259;font-size:.72rem;font-weight:850;letter-spacing:.15em;text-transform:uppercase}.path-card h3{max-width:14ch;margin:0;font:500 clamp(2rem,4vw,3.5rem)/1.03 Georgia,serif}.path-card>p:not(.card-kicker){max-width:34rem;color:#4a5a52}.check-list{display:grid;gap:.55rem;margin:1rem 0 2rem;padding:0;list-style:none}.check-list li::before{content:"✓";margin-right:.65rem;font-weight:900}.text-link{margin-top:auto;color:var(--forest);font-weight:800;text-decoration:none}.text-link:hover{text-decoration:underline;text-underline-offset:.3rem}.experience{background:var(--paper)}.experience>div:first-child{display:flex;align-items:end;justify-content:space-between;gap:2rem}.experience-grid{display:grid;grid-template-columns:repeat(4,1fr);gap:.7rem;margin-top:2.5rem}.experience-grid article{min-height:14rem;padding:1.5rem;border:1px solid var(--line);border-radius:1rem;background:var(--cream)}.experience-mark{color:#6a8b36}.experience-grid h3{margin:2.6rem 0 .5rem;font:600 1.35rem/1.2 Georgia,serif}.experience-grid p{margin:0;color:var(--muted);font-size:.9rem}.closing{padding:clamp(4rem,9vw,8rem) 1rem;text-align:center;background:var(--forest);color:#fff}.closing .eyebrow{color:var(--lime)}.closing h2{margin-inline:auto}.closing>p:not(.eyebrow){color:#bdd0c6}.centered{justify-content:center}.entry-shell{display:grid;grid-template-columns:minmax(0,1fr) minmax(340px,.78fr);gap:clamp(2rem,8vw,8rem);align-items:center;min-height:calc(100vh - 5rem);padding:clamp(2.5rem,6vw,6rem) clamp(1rem,7vw,8rem);background:radial-gradient(circle at 88% 12%,rgba(217,237,131,.42),transparent 24%),var(--paper)}.back-link{display:inline-flex;gap:.45rem;margin-bottom:3rem;color:var(--muted);font-weight:700;text-decoration:none}.entry-story h1{font-size:clamp(3rem,6vw,6rem)}.step-list{display:grid;gap:1rem;max-width:38rem;margin:2.2rem 0 0;padding:0;list-style:none}.step-list li{display:grid;grid-template-columns:2.3rem 1fr;gap:1rem}.step-number{display:grid;place-items:center;width:2rem;height:2rem;border-radius:50%;background:var(--forest);color:var(--lime);font-size:.78rem;font-weight:850}.step-list p{margin:.1rem 0 0;color:var(--muted);font-size:.9rem}.entry-panel,.next-card,.error-card{padding:clamp(1.5rem,4vw,3rem);border:1px solid rgba(23,54,45,.12);border-radius:var(--radius);background:#fff;box-shadow:var(--shadow)}.entry-panel h2,.next-card h2{margin:0;font:600 clamp(1.8rem,3vw,2.6rem)/1.1 Georgia,serif}.entry-panel>p:not(.panel-label){color:var(--muted)}.entry-form{display:grid;gap:1rem;margin-top:1.5rem}.cf-turnstile{min-height:65px}.privacy-note{display:grid;grid-template-columns:1.5rem 1fr;gap:.65rem;margin-top:1.3rem;padding-top:1.3rem;border-top:1px solid var(--line);color:var(--muted);font-size:.8rem}.privacy-note p{margin:0}.lock{color:#6c8a35}.notice{padding:.9rem 1rem;border-left:4px solid #89a83c;background:#f4f7e8;color:#49554f;font-size:.86rem}.account-shell{display:grid;grid-template-columns:1fr .8fr;gap:clamp(2rem,7vw,7rem);align-items:center;min-height:calc(100vh - 5rem);padding:clamp(4rem,8vw,8rem) clamp(1rem,7vw,8rem);background:linear-gradient(145deg,var(--cream),var(--paper))}.account-heading h1{font-size:clamp(3rem,6vw,6rem)}.identity-chip{display:inline-flex;gap:.5rem;margin-top:1.5rem;padding:.6rem .9rem;border-radius:999px;background:var(--mint);color:#285141;font-size:.86rem}.next-list{display:grid;gap:1rem;margin:1.5rem 0;padding:0;list-style:none}.next-list li{display:grid;grid-template-columns:2rem 1fr;gap:.8rem;align-items:start}.next-list span{display:grid;place-items:center;width:1.8rem;height:1.8rem;border-radius:.5rem;background:var(--forest);color:var(--lime);font-size:.72rem;font-weight:850}.next-list p{margin:0;color:#4d5b54}.error-card{max-width:46rem;margin:clamp(4rem,10vw,9rem) auto}.error-card h1{font-size:clamp(2.8rem,7vw,5.5rem)}.standalone{display:block;margin-top:1.5rem}.reservation-card{padding:1rem;border:1px solid var(--line);border-radius:1rem}.site-footer{display:grid;grid-template-columns:1fr 1fr;gap:2rem;align-items:end;padding:2.5rem clamp(1rem,6vw,7rem);border-top:1px solid var(--line);background:var(--cream);color:var(--muted);font-size:.82rem}.footer-brand{color:var(--ink)}.site-footer>p{text-align:right}@media(max-width:860px){.site-header nav a:not(.nav-cta){display:none}.hero,.entry-shell,.account-shell{grid-template-columns:1fr;min-height:auto}.hero{padding-top:5rem}.hero-card{transform:none}.trust-strip,.section-heading,.path-grid,.experience-grid{grid-template-columns:1fr 1fr}.section-heading .eyebrow{grid-column:auto}.section-heading{display:block}.section-heading h2{margin-bottom:1rem}.experience-grid{gap:1rem}.entry-story{padding-top:1.5rem}.entry-panel{max-width:42rem}.account-shell{padding-top:5rem}}@media(max-width:560px){.site-header{min-height:4.4rem}.nav-cta{padding:.6rem .85rem}.hero h1,.entry-story h1,.account-heading h1{font-size:3.35rem}.hero-actions{display:grid}.button{width:100%}.trust-strip,.path-grid,.experience-grid,.site-footer{grid-template-columns:1fr}.trust-strip{gap:.45rem}.path-card{min-height:0}.site-footer>p{text-align:left}.entry-shell{padding-top:2rem}.back-link{margin-bottom:2rem}}@media(prefers-reduced-motion:reduce){html{scroll-behavior:auto}*{transition:none!important}}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landing_has_distinct_responsive_b2c_and_b2b_paths() {
        let output = landing().into_string();
        assert!(output.contains("/join/individual"));
        assert!(output.contains("/join/organization"));
        assert!(output.contains("Skip to content"));
        assert!(output.contains("@media(max-width:560px)"));
    }

    #[test]
    fn entry_forms_handle_no_password_bearer_tenant_or_role_fields() {
        for kind in [EntryKind::Individual, EntryKind::Organization] {
            let output = entry(kind, "csrf-token", "public-site-key").into_string();
            assert!(output.contains("method=\"post\" action=\"/auth/start\""));
            assert!(output.contains("class=\"cf-turnstile\""));
            assert!(output.contains("data-action=\"hhaus_entry\""));
            for prohibited in [
                "type=\"password\"",
                "name=\"tenant",
                "name=\"role",
                "name=\"access_token",
                "name=\"organization_id",
            ] {
                assert!(!output.contains(prohibited), "found {prohibited}");
            }
        }
    }

    #[test]
    fn organization_shell_denies_client_side_authority_claims() {
        let output = onboarding(EntryKind::Organization, Some("owner@example.test")).into_string();
        assert!(output.contains("No organization, tenant, or role has been granted"));
        assert!(!output.contains("short-lived-api-bearer"));
    }
}
