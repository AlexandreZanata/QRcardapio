use axum::Router;
use axum::routing::{get, post};

use crate::app::AppState;
use crate::application::entitlements::authorize;
use crate::domain::billing::billing_now;
use crate::domain::entitlements::Operation;
use crate::error::AppResult;
use crate::http::middleware::auth::TenantContext;

mod catalog;
mod customers;
mod hours;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/dashboard/customers",
            get(customers::index).post(customers::create),
        )
        .route(
            "/dashboard/customers/{id}",
            get(customers::detail).post(customers::update),
        )
        .route("/dashboard/customers/{id}/export", post(customers::export))
        .route("/dashboard/customers/{id}/erase", post(customers::erase))
        .route(
            "/dashboard/customers/{id}/addresses",
            post(customers::save_address),
        )
        .route(
            "/dashboard/customers/{id}/addresses/{address_id}/delete",
            post(customers::remove_address),
        )
        .route("/dashboard/hours", get(hours::page).post(hours::settings))
        .route("/dashboard/hours/week", post(hours::weekly))
        .route("/dashboard/hours/exceptions", post(hours::exception))
        .route(
            "/dashboard/hours/exceptions/delete",
            post(hours::remove_exception),
        )
        .route("/dashboard/hours/pause", post(hours::pause))
        .route(
            "/dashboard/menu/addons",
            get(catalog::groups).post(catalog::save_group),
        )
        .route(
            "/dashboard/menu/addons/public-options",
            post(catalog::set_public_options),
        )
        .route("/dashboard/menu/addons/{id}", get(catalog::group))
        .route(
            "/dashboard/menu/addons/{id}/delete",
            post(catalog::delete_group),
        )
        .route(
            "/dashboard/menu/addons/{id}/options",
            post(catalog::save_option),
        )
        .route(
            "/dashboard/menu/addons/{id}/options/{option_id}/delete",
            post(catalog::delete_option),
        )
        .route(
            "/dashboard/products/{id}/options",
            get(catalog::product).post(catalog::save_variation),
        )
        .route(
            "/dashboard/products/{id}/options/{variation_id}/delete",
            post(catalog::delete_variation),
        )
        .route(
            "/dashboard/products/{id}/addon-groups",
            post(catalog::set_link),
        )
}

async fn allowed(state: &AppState, tenant: &TenantContext, operation: Operation) -> AppResult<()> {
    authorize(
        &state.pool,
        state.billing.enforcement(),
        &tenant.establishment.id,
        tenant.role,
        operation,
        billing_now().date_naive(),
    )
    .await?
    .map_err(|denial| denial.into_error())?;
    Ok(())
}
