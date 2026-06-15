//! Barcode scanner component: getUserMedia → canvas capture → rxing decode.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    CanvasRenderingContext2d, HtmlCanvasElement, HtmlVideoElement, MediaStreamConstraints,
};

use crate::routes::catalogue::add::ScanResult;

// ── Scan response from API ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApiScanResponse {
    barcode: String,
    resolved_as: Option<String>,
    printing: Option<crate::routes::catalogue::add::PrintingView>,
    grading_verification: Option<GradingVerificationView>,
    needs_manual_entry: bool,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GradingVerificationView {
    grader: String,
    cert_number: String,
    grade: Option<String>,
}

// ── Camera component ───────────────────────────────────────────────────────

/// Renders a live camera preview that continuously tries to decode a barcode.
/// On success, calls `on_result` with the resolved identity.
/// On camera permission denial or any failure, renders a manual-entry fallback.
#[component]
pub fn ScanStep<F>(on_result: F) -> impl IntoView
where
    F: Fn(ScanResult) + Clone + Send + Sync + 'static,
{
    let video_ref = NodeRef::<leptos::html::Video>::new();
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();

    let (camera_state, set_camera_state) = signal(CameraState::Starting);
    let (decoded, set_decoded) = signal::<Option<String>>(None);
    let (scanning, set_scanning) = signal(false);
    let (scan_error, set_scan_error) = signal::<Option<String>>(None);

    // Start the camera stream on mount.
    let video_ref_clone = video_ref.clone();
    Effect::new(move |_| {
        let video = video_ref_clone.get()?;
        wasm_bindgen_futures::spawn_local(async move {
            match start_camera_stream(&video).await {
                Ok(()) => set_camera_state.set(CameraState::Active),
                Err(e) => set_camera_state.set(CameraState::Denied(e)),
            }
        });
        Some(())
    });

    // Poll for barcode decode every 400 ms once the camera is active.
    let video_ref_scan = video_ref.clone();
    let canvas_ref_scan = canvas_ref.clone();
    let on_result_scan = on_result.clone();
    Effect::new(move |_| {
        if camera_state.get() != CameraState::Active {
            return;
        }
        let Some(video) = video_ref_scan.get() else { return; };
        let Some(canvas) = canvas_ref_scan.get() else { return; };

        let on_result_inner = on_result_scan.clone();

        wasm_bindgen_futures::spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(400).await;

                let barcode = capture_and_decode(&video, &canvas);
                if let Some(code) = barcode {
                    set_decoded.set(Some(code.clone()));
                    set_scanning.set(true);

                    match resolve_barcode(&code).await {
                        Ok(result) => {
                            on_result_inner(result);
                            break;
                        }
                        Err(e) => {
                            set_scan_error.set(Some(e));
                            set_scanning.set(false);
                        }
                    }
                }
            }
        });
    });

    let on_result_manual = on_result.clone();
    let go_manual = move |_| {
        on_result_manual(ScanResult {
            barcode: String::new(),
            resolved_as: None,
            printing: None,
            grader: None,
            cert_number: None,
            grade: None,
            needs_manual_entry: true,
            error: Some("switched to manual entry".into()),
        });
    };

    view! {
        <div class="scan-step">
            <h2 class="step-title">"Scan barcode"</h2>

            {move || match camera_state.get() {
                CameraState::Starting => view! {
                    <p class="loading">"Starting camera…"</p>
                }.into_any(),

                CameraState::Denied(msg) => view! {
                    <div class="camera-denied">
                        <p class="error">"Camera unavailable: "{msg}</p>
                        <p>"Tip: check browser permissions, then reload."</p>
                        <button on:click=go_manual.clone() class="btn-secondary">
                            "Enter cert # or search instead"
                        </button>
                    </div>
                }.into_any(),

                CameraState::Active => view! {
                    <div class="camera-container">
                        // Hidden canvas used for frame capture.
                        <canvas
                            node_ref=canvas_ref
                            style="display:none"
                        ></canvas>

                        <video
                            node_ref=video_ref
                            autoplay=true
                            playsinline=true
                            class="camera-preview"
                        ></video>

                        <div class="scan-overlay">
                            <div class="scan-viewfinder"></div>
                            {move || decoded.get().map(|code| view! {
                                <p class="decoded-indicator">
                                    {if scanning.get() {
                                        format!("Decoded: {} — resolving…", code)
                                    } else {
                                        format!("Decoded: {}", code)
                                    }}
                                </p>
                            })}
                            {move || scan_error.get().map(|e| view! {
                                <p class="error scan-error">{e}</p>
                            })}
                        </div>

                        <p class="scan-hint">"Point camera at a graded slab barcode or product UPC"</p>
                        <button on:click=go_manual.clone() class="btn-ghost scan-manual">
                            "Can't scan? Enter manually"
                        </button>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}

// ── Camera state ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum CameraState {
    Starting,
    Active,
    Denied(String),
}

// ── Camera stream setup ────────────────────────────────────────────────────

async fn start_camera_stream(video: &HtmlVideoElement) -> Result<(), String> {
    let window = web_sys::window().ok_or("no window")?;
    let navigator = window.navigator();
    let media_devices = navigator.media_devices().map_err(|e| format!("{:?}", e))?;

    let mut constraints = MediaStreamConstraints::new();
    constraints.video(&JsValue::TRUE);

    let promise = media_devices
        .get_user_media_with_constraints(&constraints)
        .map_err(|e| format!("getUserMedia failed: {:?}", e))?;

    let stream = JsFuture::from(promise)
        .await
        .map_err(|e| format!("camera permission denied: {:?}", e))?;

    let stream: web_sys::MediaStream = stream
        .dyn_into()
        .map_err(|_| "stream cast failed".to_string())?;

    video.set_src_object(Some(&stream));
    video
        .play()
        .map_err(|e| format!("video.play() failed: {:?}", e))?;

    Ok(())
}

// ── Frame capture and barcode decode ──────────────────────────────────────

fn capture_and_decode(video: &HtmlVideoElement, canvas: &HtmlCanvasElement) -> Option<String> {
    let w = video.video_width();
    let h = video.video_height();

    if w == 0 || h == 0 {
        return None;
    }

    canvas.set_width(w);
    canvas.set_height(h);

    let ctx = canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<CanvasRenderingContext2d>()
        .ok()?;

    ctx.draw_image_with_html_video_element(video, 0.0, 0.0)
        .ok()?;

    let image_data = ctx.get_image_data(0.0, 0.0, w as f64, h as f64).ok()?;

    // web_sys::Uint8ClampedArray is re-exported from js_sys and has to_vec().
    let rgba: Vec<u8> = image_data.data().to_vec();

    // RGBA → luma (BT.601 coefficients, integer arithmetic).
    let luma: Vec<u8> = rgba
        .chunks_exact(4)
        .map(|px| ((px[0] as u32 * 299 + px[1] as u32 * 587 + px[2] as u32 * 114) / 1000) as u8)
        .collect();

    // rxing returns Result<RXingResult, Exceptions>; .ok() drops decode failures.
    rxing::helpers::detect_in_luma(luma, w, h, None)
        .ok()
        .map(|result| result.getText().to_string())
}

// ── API round-trip to resolve the barcode ─────────────────────────────────

#[derive(Serialize)]
struct ScanApiRequest<'a> {
    barcode: &'a str,
    kind: &'static str,
}

async fn resolve_barcode(barcode: &str) -> Result<ScanResult, String> {
    let body = ScanApiRequest {
        barcode,
        kind: "auto",
    };

    let resp = gloo_net::http::Request::post("/api/catalogue/scan")
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("scan API error: HTTP {}", resp.status()));
    }

    let api: ApiScanResponse = resp.json().await.map_err(|e| e.to_string())?;

    Ok(ScanResult {
        barcode: api.barcode,
        resolved_as: api.resolved_as,
        printing: api.printing,
        grader: api.grading_verification.as_ref().map(|g| g.grader.clone()),
        cert_number: api
            .grading_verification
            .as_ref()
            .map(|g| g.cert_number.clone()),
        grade: api
            .grading_verification
            .as_ref()
            .and_then(|g| g.grade.clone()),
        needs_manual_entry: api.needs_manual_entry,
        error: api.error,
    })
}
