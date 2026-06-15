use super::{
    adapter::{PosAdapter, Provider},
    clover::CloverAdapter,
    shopify::ShopifyAdapter,
    square::SquareAdapter,
};

/// Build the adapter for the given provider, reading credentials from env.
pub fn get_adapter(provider: Provider) -> Box<dyn PosAdapter> {
    match provider {
        Provider::Square => Box::new(SquareAdapter::new(
            std::env::var("SQUARE_CLIENT_ID").unwrap_or_default(),
            std::env::var("SQUARE_CLIENT_SECRET").unwrap_or_default(),
        )),
        Provider::Shopify => Box::new(ShopifyAdapter::new(
            std::env::var("SHOPIFY_API_KEY").unwrap_or_default(),
            std::env::var("SHOPIFY_API_SECRET").unwrap_or_default(),
            std::env::var("SHOPIFY_SHOP_DOMAIN").unwrap_or_default(),
        )),
        Provider::Clover => Box::new(CloverAdapter::new(
            std::env::var("CLOVER_CLIENT_ID").unwrap_or_default(),
            std::env::var("CLOVER_CLIENT_SECRET").unwrap_or_default(),
        )),
    }
}

/// Returns the `pos::connect::routes()` router (alias).
pub fn routes() -> axum::Router<crate::app::AppState> {
    super::routes::routes()
}
