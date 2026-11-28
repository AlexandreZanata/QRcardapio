pub mod api;
pub mod orders;
pub mod public;
mod push;
pub mod settings;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::from_fn;
use axum::routing::{get, post};

use crate::app::AppState;
use crate::http::middleware::security_headers::no_store;

const STOREFRONT_API_BODY_LIMIT: usize = 32 * 1024;

pub fn router() -> Router<AppState> {
    let consumer_api = Router::new()
        .route("/api/storefront/v1/{slug}/quotes", post(api::post_quote))
        .route("/api/storefront/v1/{slug}/orders", post(api::post_order))
        .route(
            "/api/storefront/v1/{slug}/orders/{order_id}",
            get(api::get_order),
        )
        .route(
            "/api/storefront/v1/{slug}/orders/{order_id}/cancel",
            post(api::cancel_order),
        )
        .layer(DefaultBodyLimit::max(STOREFRONT_API_BODY_LIMIT))
        .layer(from_fn(no_store));

    let dashboard = Router::new()
        .route(
            "/dashboard/delivery/settings",
            get(settings::settings_page).post(settings::save_settings_submit),
        )
        .route(
            "/dashboard/delivery/zones",
            post(settings::create_zone_submit),
        )
        .route(
            "/dashboard/delivery/zones/{id}",
            post(settings::update_zone_submit),
        )
        .route(
            "/dashboard/delivery/zones/{id}/delete",
            post(settings::delete_zone_submit),
        )
        .route(
            "/dashboard/delivery/coupons",
            post(settings::create_coupon_submit),
        )
        .route(
            "/dashboard/delivery/coupons/{id}",
            post(settings::update_coupon_submit),
        )
        .route(
            "/dashboard/delivery/coupons/{id}/delete",
            post(settings::delete_coupon_submit),
        )
        .route(
            "/dashboard/delivery/customers/block",
            post(settings::customer_block_submit),
        )
        .route(
            "/dashboard/delivery/publish",
            post(settings::publish_submit),
        )
        .route(
            "/dashboard/delivery/unpublish",
            post(settings::unpublish_submit),
        )
        .route("/dashboard/delivery/orders", get(orders::orders_page))
        .route(
            "/dashboard/delivery/manifest.webmanifest",
            get(push::manifest),
        )
        .route("/dashboard/delivery/sw.js", get(push::worker))
        .route(
            "/dashboard/delivery/push-subscriptions",
            post(push::subscribe),
        )
        .route(
            "/dashboard/delivery/push-subscriptions/{id}/revoke",
            post(push::revoke),
        )
        .route("/dashboard/delivery/orders.csv", get(orders::orders_csv))
        .route(
            "/dashboard/delivery/orders/{id}/transition",
            post(orders::transition_submit),
        )
        .route(
            "/dashboard/delivery/orders/{id}/payments/{payment_id}/confirm",
            post(orders::confirm_payment_submit),
        )
        .route(
            "/dashboard/delivery/orders/{id}/refunds",
            post(orders::refund_submit),
        )
        .layer(from_fn(no_store));

    Router::new()
        .merge(dashboard)
        .route("/s/{slug}", get(public::redirect_trailing_slash))
        .route("/s/{slug}/", get(public::storefront_index))
        .route("/s/{slug}/{*file}", get(public::storefront_asset))
        .merge(consumer_api)
}
