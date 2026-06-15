#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use axum::{Extension, Router};
    use cardguard_api::{NullPosProvider, PosProvider};
    use cardguard_web::app::App;
    use dotenvy::dotenv;
    use leptos::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use sqlx::postgres::PgPoolOptions;
    use std::{net::SocketAddr, sync::Arc};

    dotenv().ok();
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // NullPosProvider until the connect-pos stream lands its real implementation.
    let provider: Arc<dyn PosProvider> = Arc::new(NullPosProvider);

    let conf = get_configuration(None).await?;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let pool_ctx = pool.clone();
    let provider_ctx = Arc::clone(&provider);

    let app = Router::new()
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            move || {
                // Leptos use_context — accessible inside components and server fns.
                provide_context(pool_ctx.clone());
                provide_context(Arc::clone(&provider_ctx));
            },
            App,
        )
        // Axum extensions — accessible via leptos_axum::extract.
        .layer(Extension(pool.clone()))
        .layer(Extension(provider.clone()))
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    let addr: SocketAddr = "0.0.0.0:3000".parse()?;
    tracing::info!("Web server listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(feature = "ssr")]
fn shell(options: leptos::LeptosOptions) -> impl leptos::IntoView {
    use leptos::*;
    use cardguard_web::app::App;
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[cfg(not(feature = "ssr"))]
fn main() {}
