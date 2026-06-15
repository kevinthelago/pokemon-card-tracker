#[cfg(feature = "ssr")]
mod app;
#[cfg(feature = "ssr")]
mod routes;

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, render_app_to_stream, LeptosRoutes};
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(app::App);

    let app = Router::new()
        .leptos_routes(&leptos_options, routes, app::App)
        .fallback(render_app_to_stream(app::App))
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await.unwrap();
    tracing::info!("web server listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    use leptos::prelude::*;
    #[cfg(feature = "csr")]
    {
        console_error_panic_hook::set_once();
        mount_to_body(app::App);
    }
}

// Keep module visibility for non-ssr builds
#[cfg(not(feature = "ssr"))]
mod app;
#[cfg(not(feature = "ssr"))]
mod routes;
