//! Cert barcode scanner — wraps the device camera and decodes the cert barcode.
//!
//! PSA cert barcodes are Code 128 / QR codes that encode the cert number.
//! This component opens the rear camera and delegates decoding to the
//! `BarcodeDetector` Web API (where available) with a fallback to a text
//! entry prompt.
//!
//! The catalogue stream's camera/decoder is reused here via the same
//! BarcodeDetector path.

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use web_sys::HtmlInputElement;

/// Called when a cert barcode is successfully decoded.
pub type OnScanCallback = Callback<(String, String), ()>;

/// Camera-based cert scanner.
///
/// Emits `on_scan(grader, cert_number)` when a cert barcode is decoded.
/// Falls back to a text input if the BarcodeDetector API is unavailable.
#[component]
pub fn CertScanner(
    /// Callback invoked with (grader, cert_number) on successful decode.
    on_scan: OnScanCallback,
    #[prop(default = false)] disabled: bool,
) -> impl IntoView {
    let (manual_grader, set_manual_grader) = signal("PSA".to_string());
    let (manual_cert, set_manual_cert) = signal(String::new());
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (scanning, set_scanning) = signal(false);

    // Check BarcodeDetector availability in the browser.
    let has_barcode_detector = js_sys::Reflect::has(
        &web_sys::window().unwrap().into(),
        &JsValue::from_str("BarcodeDetector"),
    )
    .unwrap_or(false);

    let do_submit = {
        let on_scan = on_scan.clone();
        move || {
            let grader = manual_grader.get();
            let cert = manual_cert.get();
            if cert.trim().is_empty() {
                set_error_msg.set(Some("Please enter a cert number.".into()));
                return;
            }
            set_error_msg.set(None);
            on_scan.run((grader, cert.trim().to_string()));
        }
    };
    let submit_manual = { let do_submit = do_submit.clone(); move |_| do_submit() };

    let start_scan = {
        let set_scanning = set_scanning.clone();
        let on_scan = on_scan.clone();
        let set_error_msg = set_error_msg.clone();
        move |_| {
            set_scanning.set(true);
            // Initiate camera scan via JS interop.
            // In production this spawns a Tokio task via wasm_bindgen_futures.
            wasm_bindgen_futures::spawn_local({
                let set_scanning = set_scanning.clone();
                let on_scan = on_scan.clone();
                let set_error_msg = set_error_msg.clone();
                async move {
                    match scan_from_camera().await {
                        Ok((grader, cert)) => {
                            set_scanning.set(false);
                            on_scan.run((grader, cert));
                        }
                        Err(e) => {
                            set_scanning.set(false);
                            set_error_msg.set(Some(e));
                        }
                    }
                }
            });
        }
    };

    view! {
        <div class="cert-scanner">
            // ── Manual entry (always shown) ─────────────────────────────
            <div class="cert-scanner__manual">
                <div class="cert-scanner__field-group">
                    <label class="cert-scanner__label" for="grader-select">"Grader"</label>
                    <select
                        id="grader-select"
                        class="cert-scanner__select"
                        disabled=disabled
                        on:change=move |ev| {
                            let el = event_target::<web_sys::HtmlSelectElement>(&ev);
                            set_manual_grader.set(el.value());
                        }
                    >
                        <option value="PSA">"PSA"</option>
                        <option value="CGC">"CGC"</option>
                        <option value="BGS">"BGS"</option>
                    </select>
                </div>
                <div class="cert-scanner__field-group">
                    <label class="cert-scanner__label" for="cert-input">"Cert #"</label>
                    <input
                        id="cert-input"
                        type="text"
                        class="cert-scanner__input"
                        placeholder="e.g. 12345678"
                        disabled=disabled
                        on:input=move |ev| {
                            let el = event_target::<HtmlInputElement>(&ev);
                            set_manual_cert.set(el.value());
                        }
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" {
                                do_submit();
                            }
                        }
                    />
                </div>
                <button
                    type="button"
                    class="cert-scanner__submit"
                    disabled=disabled
                    on:click=submit_manual
                >
                    "Look Up"
                </button>
            </div>

            // ── Camera scan button (only when BarcodeDetector is available) ──
            {has_barcode_detector.then(|| view! {
                <div class="cert-scanner__divider">
                    <span>"or"</span>
                </div>
                <button
                    type="button"
                    class="cert-scanner__scan-btn"
                    disabled=move || disabled || scanning.get()
                    on:click=start_scan
                >
                    {move || if scanning.get() { "Scanning…" } else { "Scan Barcode" }}
                </button>
            })}

            // ── Error message ────────────────────────────────────────────
            {move || error_msg.get().map(|e| view! {
                <p class="cert-scanner__error" role="alert">{e}</p>
            })}
        </div>
    }
}

// ── Camera scan implementation ─────────────────────────────────────────────

/// Open the device camera, capture a frame, and run BarcodeDetector on it.
/// Returns `(grader, cert_number)` on success, or an error string.
async fn scan_from_camera() -> Result<(String, String), String> {
    let window = web_sys::window().ok_or("no window")?;
    let navigator = window.navigator();
    let media_devices = navigator
        .media_devices()
        .map_err(|_| "MediaDevices not available".to_string())?;

    let constraints = web_sys::MediaStreamConstraints::new();
    let video_constraints = js_sys::Object::new();
    js_sys::Reflect::set(
        &video_constraints,
        &JsValue::from_str("facingMode"),
        &JsValue::from_str("environment"),
    )
    .ok();
    constraints.set_video(&video_constraints.into());

    let stream_promise = media_devices
        .get_user_media_with_constraints(&constraints)
        .map_err(|_| "Failed to request camera".to_string())?;

    let stream_value = wasm_bindgen_futures::JsFuture::from(stream_promise)
        .await
        .map_err(|_| "Camera access denied or unavailable".to_string())?;

    let stream = web_sys::MediaStream::from(stream_value);

    // Create a video element and play the stream.
    let document = window.document().ok_or("no document")?;
    let video = document
        .create_element("video")
        .map_err(|_| "failed to create video")?;
    let video: web_sys::HtmlVideoElement = video.unchecked_into();
    video.set_src_object(Some(&stream));
    let _ = video.play().map_err(|_| "failed to play video");

    // Wait a moment for the camera to stabilize.
    let _ = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::new(&mut |resolve, _| {
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 800)
            .ok();
    }))
    .await;

    // Capture a frame via canvas.
    let canvas = document
        .create_element("canvas")
        .map_err(|_| "failed to create canvas")?;
    let canvas: web_sys::HtmlCanvasElement = canvas.unchecked_into();
    let width = video.video_width();
    let height = video.video_height();
    canvas.set_width(width);
    canvas.set_height(height);

    let ctx: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .ok_or("no canvas context")?
        .unchecked_into();

    ctx.draw_image_with_html_video_element(&video, 0.0, 0.0)
        .map_err(|_| "failed to capture frame")?;

    // Stop the camera.
    for track in stream.get_video_tracks().iter() {
        let track: web_sys::MediaStreamTrack = track.unchecked_into();
        track.stop();
    }

    // Run BarcodeDetector.
    let barcode_detector_class =
        js_sys::Reflect::get(&window.into(), &JsValue::from_str("BarcodeDetector"))
            .map_err(|_| "BarcodeDetector not available")?;

    let formats = js_sys::Array::new();
    formats.push(&JsValue::from_str("code_128"));
    formats.push(&JsValue::from_str("qr_code"));
    let opts = js_sys::Object::new();
    js_sys::Reflect::set(&opts, &JsValue::from_str("formats"), &formats).ok();

    let detector = js_sys::Reflect::construct(
        &barcode_detector_class.unchecked_into::<js_sys::Function>(),
        &js_sys::Array::of1(&opts),
    )
    .map_err(|_| "failed to create BarcodeDetector")?;

    let detect_fn = js_sys::Reflect::get(&detector, &JsValue::from_str("detect"))
        .map_err(|_| "detect method not found")?;
    let detect_fn: js_sys::Function = detect_fn.unchecked_into();

    let promise = detect_fn
        .call1(&detector, &canvas)
        .map_err(|_| "detect call failed")?;

    let results = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|_| "BarcodeDetector failed")?;

    let barcodes = js_sys::Array::from(&results);
    if barcodes.length() == 0 {
        return Err("No barcode found in frame. Try again with better lighting.".into());
    }

    let first = barcodes.get(0);
    let raw_value = js_sys::Reflect::get(&first, &JsValue::from_str("rawValue"))
        .map_err(|_| "no rawValue")?
        .as_string()
        .ok_or("rawValue is not a string")?;

    // Parse "PSA-12345678" or plain "12345678" (assume PSA).
    Ok(parse_cert_barcode(&raw_value))
}

/// Attempt to split a cert barcode value into (grader, cert_number).
fn parse_cert_barcode(barcode: &str) -> (String, String) {
    if let Some((prefix, rest)) = barcode.split_once('-') {
        let prefix_upper = prefix.to_uppercase();
        if ["PSA", "BGS", "CGC", "SGC"].contains(&prefix_upper.as_str()) {
            return (prefix_upper, rest.to_string());
        }
    }
    // Numeric-only → assume PSA.
    if barcode.chars().all(|c| c.is_ascii_digit()) {
        return ("PSA".into(), barcode.to_string());
    }
    ("PSA".into(), barcode.to_string())
}
