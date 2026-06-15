//! Standalone verify-a-card page.
//!
//! Route: /verify
//!
//! Allows a user to enter or scan a grader + cert# and see the verification
//! result without needing to add the card to the catalogue.

use leptos::prelude::*;
use gloo_net::http::Request;

use super::{
    result_card::VerificationResultCard,
    scanner::{CertScanner, OnScanCallback},
    types::{VerifyRequest, VerifyResponse},
};

const API_BASE: &str = "/api/grading/verify";

#[component]
pub fn VerifyPage() -> impl IntoView {
    let (result, set_result) = signal(Option::<VerifyResponse>::None);
    let (loading, set_loading) = signal(false);
    let (api_error, set_api_error) = signal(Option::<String>::None);

    let run_verify = move |grader: String, cert_number: String| {
        set_loading.set(true);
        set_api_error.set(None);
        set_result.set(None);

        let req = VerifyRequest {
            grader,
            cert_number,
            claimed_card_name: None,
            claimed_set_name: None,
            claimed_number: None,
        };

        wasm_bindgen_futures::spawn_local(async move {
            let response = Request::post(API_BASE)
                .json(&req)
                .expect("failed to serialize request")
                .send()
                .await;

            set_loading.set(false);

            match response {
                Ok(resp) if resp.ok() => {
                    match resp.json::<VerifyResponse>().await {
                        Ok(data) => set_result.set(Some(data)),
                        Err(e) => {
                            set_api_error.set(Some(format!("Failed to parse response: {e}")));
                        }
                    }
                }
                Ok(resp) => {
                    set_api_error.set(Some(format!(
                        "Server error (HTTP {})",
                        resp.status()
                    )));
                }
                Err(e) => {
                    set_api_error.set(Some(format!("Network error: {e}")));
                }
            }
        });
    };

    let on_scan: OnScanCallback = Callback::new(move |(grader, cert_number): (String, String)| {
        run_verify(grader, cert_number);
    });

    view! {
        <main class="verify-page">
            <header class="verify-page__header">
                <h1 class="verify-page__title">"Verify a Graded Card"</h1>
                <p class="verify-page__subtitle">
                    "Enter or scan a cert number to confirm authenticity."
                </p>
            </header>

            <section class="verify-page__form" aria-label="Cert lookup">
                <CertScanner on_scan=on_scan disabled=loading.get() />
            </section>

            // ── Loading indicator ───────────────────────────────────────
            {move || loading.get().then(|| view! {
                <div class="verify-page__loading" role="status" aria-live="polite">
                    <span class="verify-page__spinner" aria-hidden="true" />
                    <span>"Looking up cert…"</span>
                </div>
            })}

            // ── API error ───────────────────────────────────────────────
            {move || api_error.get().map(|e| view! {
                <div class="verify-page__api-error" role="alert">
                    <p><strong>"Error:"</strong> " " {e}</p>
                </div>
            })}

            // ── Verification result ─────────────────────────────────────
            {move || result.get().map(|r| view! {
                <section class="verify-page__result" aria-live="polite">
                    <VerificationResultCard result=r />
                </section>
            })}
        </main>
    }
}

/// Inline verify step — for embedding in the add-card cataloguing flow.
///
/// Takes a grader + cert# (already known from the add-card form), runs
/// verification, and reports the result without navigation.
///
/// The catalogue-a-card stream wires this into its add-card flow once both
/// streams have landed on develop.
#[component]
pub fn InlineVerifyStep(
    grader: String,
    cert_number: String,
    /// Called with the verification result when lookup completes.
    #[prop(optional)]
    on_result: Option<Callback<VerifyResponse, ()>>,
) -> impl IntoView {
    let (result, set_result) = signal(Option::<VerifyResponse>::None);
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    // Auto-run verification when the component mounts.
    let g = grader.clone();
    let c = cert_number.clone();
    Effect::new(move |_| {
        let grader = g.clone();
        let cert_number = c.clone();
        let on_result = on_result.clone();

        set_loading.set(true);

        let req = VerifyRequest {
            grader,
            cert_number,
            claimed_card_name: None,
            claimed_set_name: None,
            claimed_number: None,
        };

        wasm_bindgen_futures::spawn_local(async move {
            let resp = Request::post(API_BASE)
                .json(&req)
                .expect("serialize")
                .send()
                .await;

            set_loading.set(false);

            match resp {
                Ok(r) if r.ok() => {
                    if let Ok(data) = r.json::<VerifyResponse>().await {
                        if let Some(cb) = on_result {
                            cb.run(data.clone());
                        }
                        set_result.set(Some(data));
                    }
                }
                Ok(r) => {
                    set_error.set(Some(format!("HTTP {}", r.status())));
                }
                Err(e) => {
                    set_error.set(Some(e.to_string()));
                }
            }
        });
    });

    view! {
        <div class="inline-verify">
            {move || loading.get().then(|| view! {
                <div class="inline-verify__loading" role="status">
                    <span class="inline-verify__spinner" aria-hidden="true" />
                    <span>"Verifying cert…"</span>
                </div>
            })}
            {move || error.get().map(|e| view! {
                <p class="inline-verify__error" role="alert">
                    "Verification failed: " {e}
                </p>
            })}
            {move || result.get().map(|r| view! {
                <div class="inline-verify__result">
                    <VerificationResultCard result=r />
                </div>
            })}
        </div>
    }
}
