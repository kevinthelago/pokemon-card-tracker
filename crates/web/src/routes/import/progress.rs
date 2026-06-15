use leptos::prelude::*;
use leptos::task::spawn_local;
use uuid::Uuid;

use super::api::ImportStatusResponse;

/// Polls GET /api/catalogue/import/{job_id} every 2 s until done or failed.
/// Calls `on_done` with the final status when the job completes.
#[component]
pub fn ImportProgress(
    job_id: Uuid,
    #[prop(into)] on_done: Callback<ImportStatusResponse>,
) -> impl IntoView {
    let (status, set_status) = signal(Option::<ImportStatusResponse>::None);
    let (poll_error, set_poll_error) = signal(Option::<String>::None);

    Effect::new(move |_| {
        let job_id = job_id;
        let on_done = on_done;
        spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(2_000).await;
                let url = format!("/api/catalogue/import/{job_id}");
                match gloo_net::http::Request::get(&url).send().await {
                    Err(e) => {
                        set_poll_error.set(Some(e.to_string()));
                        break;
                    }
                    Ok(resp) if !resp.ok() => {
                        set_poll_error.set(Some(format!("HTTP {}", resp.status())));
                        break;
                    }
                    Ok(resp) => match resp.json::<ImportStatusResponse>().await {
                        Err(e) => {
                            set_poll_error.set(Some(e.to_string()));
                            break;
                        }
                        Ok(s) => {
                            let terminal = s.status == "done" || s.status == "failed";
                            set_status.set(Some(s.clone()));
                            if terminal {
                                on_done.run(s);
                                break;
                            }
                        }
                    },
                }
            }
        });
    });

    view! {
        <div class="import-progress">
            {move || match poll_error.get() {
                Some(e) => view! { <p class="error">"Poll error: " {e}</p> }.into_any(),
                None => match status.get() {
                    None => view! { <p>"Starting…"</p> }.into_any(),
                    Some(s) => view! {
                        <p>"Status: " {s.status.clone()}</p>
                        {s.total_rows.map(|t| view! {
                            <progress max=t value=s.processed_rows></progress>
                            <p>
                                {s.processed_rows} " / " {t} " rows processed"
                                " (" {s.imported_rows} " imported, "
                                {s.skipped_rows} " skipped, "
                                {s.error_count} " errors)"
                            </p>
                        })}
                    }.into_any(),
                },
            }}
        </div>
    }
}
