//! Verification result card — 4 visually distinct states: verified, mismatch,
//! not_found, unavailable.

use leptos::prelude::*;

use super::types::{MismatchDetail, VerifyResponse};

/// Displays the verification outcome for a cert lookup.
/// Each status has a distinct color scheme, icon, and messaging.
#[component]
pub fn VerificationResultCard(result: VerifyResponse) -> impl IntoView {
    match result.status.as_str() {
        "verified" => view! {
            <div class="result-card result-card--verified">
                <div class="result-card__header">
                    <span class="result-card__icon" aria-hidden="true">"✓"</span>
                    <span class="result-card__status">"Verified"</span>
                </div>
                <div class="result-card__body">
                    <VerifiedBody result=result />
                </div>
            </div>
        }.into_any(),

        "mismatch" => view! {
            <div class="result-card result-card--mismatch">
                <div class="result-card__header">
                    <span class="result-card__icon" aria-hidden="true">"⚠"</span>
                    <span class="result-card__status">"Identity Mismatch"</span>
                </div>
                <div class="result-card__body">
                    <MismatchBody
                        grader=result.grader.clone()
                        cert_number=result.cert_number.clone()
                        detail=result.mismatch_detail.clone()
                    />
                </div>
            </div>
        }.into_any(),

        "not_found" => view! {
            <div class="result-card result-card--not-found">
                <div class="result-card__header">
                    <span class="result-card__icon" aria-hidden="true">"✗"</span>
                    <span class="result-card__status">"Not Found — Possible Counterfeit"</span>
                </div>
                <div class="result-card__body">
                    <NotFoundBody
                        grader=result.grader.clone()
                        cert_number=result.cert_number.clone()
                    />
                </div>
            </div>
        }.into_any(),

        _ => view! {
            <div class="result-card result-card--unavailable">
                <div class="result-card__header">
                    <span class="result-card__icon" aria-hidden="true">"…"</span>
                    <span class="result-card__status">"Service Unavailable"</span>
                </div>
                <div class="result-card__body">
                    <UnavailableBody
                        grader=result.grader.clone()
                        cert_number=result.cert_number.clone()
                        reason=result.unavailable_reason.clone()
                    />
                </div>
            </div>
        }.into_any(),
    }
}

// ── Sub-components ─────────────────────────────────────────────────────────

#[component]
fn VerifiedBody(result: VerifyResponse) -> impl IntoView {
    view! {
        <dl class="result-card__fields">
            <FieldRow label="Grader" value=result.grader.clone() />
            <FieldRow label="Cert #" value=result.cert_number.clone() />
            {result.grade.as_deref().map(|g| view! {
                <FieldRow label="Grade" value=g.to_string() />
            })}
            {result.card_name.as_deref().map(|n| view! {
                <FieldRow label="Card" value=n.to_string() />
            })}
            {result.set_name.as_deref().map(|s| view! {
                <FieldRow label="Set" value=s.to_string() />
            })}
            {result.year.as_deref().map(|y| view! {
                <FieldRow label="Year" value=y.to_string() />
            })}
        </dl>
        {result.cached.then(|| view! {
            <p class="result-card__cache-note">"Result served from cache."</p>
        })}
    }
}

#[component]
fn MismatchBody(grader: String, cert_number: String, detail: Option<MismatchDetail>) -> impl IntoView {
    view! {
        <div class="result-card__warning">
            <p>
                "This cert exists in the " {grader.clone()} " database, but the "
                "card identity does not match what was claimed. This may indicate a "
                <strong>"swapped or re-holdered slab"</strong>
                "."
            </p>
        </div>
        <dl class="result-card__fields">
            <FieldRow label="Grader" value=grader />
            <FieldRow label="Cert #" value=cert_number />
            {detail.map(|d| view! {
                <FieldRow label="Grader Reports" value=format!("{} — {}", d.returned_grade, d.returned_card_name) />
            })}
        </dl>
    }
}

#[component]
fn NotFoundBody(grader: String, cert_number: String) -> impl IntoView {
    view! {
        <div class="result-card__alert">
            <p>
                <strong>"Cert # " {cert_number.clone()} " was not found in the " {grader.clone()} " database."</strong>
            </p>
            <p>
                "This is a strong signal that the slab may be "
                <strong>"counterfeit or altered"</strong>
                ". Do not purchase or accept this card until the cert is independently verified."
            </p>
            <p class="result-card__flag-notice">
                "A counterfeit risk flag has been raised for review."
            </p>
        </div>
        <dl class="result-card__fields">
            <FieldRow label="Grader" value=grader />
            <FieldRow label="Cert #" value=cert_number />
        </dl>
    }
}

#[component]
fn UnavailableBody(grader: String, cert_number: String, reason: Option<String>) -> impl IntoView {
    view! {
        <div class="result-card__notice">
            <p>
                "The " {grader.clone()} " verification service is temporarily unavailable. "
                "Your card has been saved; verification can be re-run later."
            </p>
            {reason.map(|r| view! {
                <p class="result-card__tech-detail">"Technical detail: " {r}</p>
            })}
        </div>
        <dl class="result-card__fields">
            <FieldRow label="Grader" value=grader />
            <FieldRow label="Cert #" value=cert_number />
        </dl>
    }
}

#[component]
fn FieldRow(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="result-card__field">
            <dt class="result-card__label">{label}</dt>
            <dd class="result-card__value">{value}</dd>
        </div>
    }
}
