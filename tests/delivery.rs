//! QR Delivery integration tests against the real router and SQLite migrations.

#[path = "delivery/fixture.rs"]
mod fixture;
#[path = "commerce/order_fixtures.rs"]
mod order_fixtures;
#[path = "delivery/order_lifecycle.rs"]
mod order_lifecycle;
#[path = "delivery/orders.rs"]
mod orders;
#[path = "delivery/preview_completion.rs"]
mod preview_completion;
#[path = "delivery/publication.rs"]
mod publication;
#[path = "commercial/support.rs"]
mod support;
