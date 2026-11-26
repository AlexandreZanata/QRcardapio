use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::application::auth::AuthSuccess;
use qrcardapio::application::operator::{GrantRequest, grant as grant_offer};
use qrcardapio::domain::billing::billing_now;
use qrcardapio::domain::commerce::business_hours::{CommercialSettings, Windows};
use qrcardapio::domain::commerce::order::OrderItemSelection;
use qrcardapio::domain::commercial::{CommercialCatalog, catalog};
use qrcardapio::domain::entitlements::GrantKind;
use qrcardapio::infrastructure::database::queries::{
    business_hours, catalog_snapshots, commercial_catalog, delivery_zones, entitlements,
    find_account_by_email,
};

use crate::{order_fixtures, support};
use support::{Harness, config, post_form};

pub const ORIGIN: &str = "https://menu.example.test";
const OPERATOR: &str = "delivery-operator@example.test";
pub const PUBLISH: &str = "/dashboard/delivery/publish";
pub const UNPUBLISH: &str = "/dashboard/delivery/unpublish";
pub const SETTINGS: &str = "/dashboard/delivery/settings";
pub const STOREFRONT_CHANNEL: &str = "storefront";
pub const CUSTOMER: &str = "Cliente Teste Entrega";
pub const PHONE: &str = "+5511998877665";
pub const STREET: &str = "Rua Sigilosa QA";
pub const PRIVATE_NOTE: &str = "Nota privada para a cozinha";

pub struct Shop {
    pub slug: &'static str,
    pub id: String,
    pub owner: AuthSuccess,
    pub item: OrderItemSelection,
}

impl Shop {
    pub async fn create(h: &Harness, slug: &'static str, minimum_centavos: i64) -> Self {
        let (owner, id) = support::owner(&h.pool, slug).await;
        business_hours::save_settings(
            &h.pool,
            &id,
            &CommercialSettings {
                delivery: true,
                pickup: true,
                minimum_centavos,
                ..CommercialSettings::default()
            },
        )
        .await
        .unwrap();
        let always_open: Windows = "00:00-24:00".parse().unwrap();
        business_hours::save_weekly(&h.pool, &id, &std::array::from_fn(|_| always_open.clone()))
            .await
            .unwrap();
        let catalog =
            order_fixtures::catalog(h, &id, order_fixtures::at("2026-11-25T15:00:00Z")).await;
        Self {
            slug,
            id,
            owner,
            item: catalog.items.into_iter().next().unwrap(),
        }
    }

    pub fn storefront(&self) -> String {
        format!("/s/{}/", self.slug)
    }

    pub fn menu(&self) -> String {
        format!("/c/{}", self.slug)
    }

    pub fn api(&self, path: &str) -> String {
        format!("/api/storefront/v1/{}/{path}", self.slug)
    }

    pub async fn catalog_version(&self, h: &Harness) -> i64 {
        catalog_snapshots::current(&h.pool, &self.id, STOREFRONT_CHANNEL)
            .await
            .unwrap()
            .unwrap()
            .version
    }

    pub async fn publish(&self, app: &Router) {
        assert_eq!(
            submit(app, &self.owner, PUBLISH, "").await.status(),
            StatusCode::SEE_OTHER
        );
    }

    pub async fn create_zone(&self, h: &Harness, app: &Router) -> String {
        let response = submit(
            app,
            &self.owner,
            "/dashboard/delivery/zones",
            "neighborhood=Centro&city=Sao+Paulo&fee=5%2C00&min_order=30%2C00&eta_min_minutes=20&eta_max_minutes=40&is_active=1",
        )
        .await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        delivery_zones::list_active_for_establishment(&h.pool, &self.id)
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .id
    }

    pub fn quote(&self, version: i64, zone: Option<&str>, coupon: Option<&str>) -> Value {
        json!({
            "catalog_version": version,
            "fulfillment": if zone.is_some() { "delivery" } else { "pickup" },
            "zone_id": zone,
            "coupon_code": coupon,
            "lines": [{
                "product_id": self.item.product_id,
                "variation_id": self.item.variation_id,
                "quantity": self.item.quantity,
                "options": self.item.addons.iter().map(|option| json!({
                    "option_id": option.option_id,
                    "quantity": option.quantity,
                })).collect::<Vec<_>>(),
                "note": PRIVATE_NOTE,
            }],
        })
    }
}

pub fn preview_catalog() -> CommercialCatalog {
    catalog().clone()
}

pub fn development_catalog() -> CommercialCatalog {
    let mut modules: Value =
        serde_json::from_str(include_str!("../../contracts/commercial/modules.json")).unwrap();
    let delivery = modules["modules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|module| module["code"] == "delivery")
        .unwrap();
    delivery["lifecycle"] = "development".into();
    CommercialCatalog::load(
        &modules.to_string(),
        include_str!("../../contracts/commercial/bundles.json"),
        include_str!("../../contracts/commercial/addons.json"),
        include_str!("../../contracts/commercial/billing-cycles.json"),
    )
    .unwrap()
}

pub async fn preview_router(h: &Harness) -> Router {
    let catalog = preview_catalog();
    commercial_catalog::sync_registry(&h.pool, &catalog)
        .await
        .unwrap();
    build_router(
        AppState::new(config(&h.dir, false), h.pool.clone()).with_commercial_catalog(catalog),
    )
}

pub async fn grant_preview(h: &Harness, shop: &Shop) -> String {
    ensure_operator(h).await;
    let today = billing_now().date_naive();
    grant_offer(
        &h.pool,
        GrantRequest {
            operator_email: OPERATOR,
            establishment_slug: shop.slug,
            offer_code: "delivery",
            kind: GrantKind::InternalPreview,
            starts_on: today - Duration::days(1),
            ends_on: today + Duration::days(1),
            reason: "Testing Delivery preview access",
        },
    )
    .await
    .unwrap()
}

async fn ensure_operator(h: &Harness) {
    if find_account_by_email(&h.pool, OPERATOR)
        .await
        .unwrap()
        .is_none()
    {
        support::admin(&h.pool, OPERATOR).await;
    }
}

pub async fn force_grant(h: &Harness, shop: &Shop, kind: &str) -> String {
    ensure_operator(h).await;
    let operator = find_account_by_email(&h.pool, OPERATOR)
        .await
        .unwrap()
        .unwrap();
    let id = Uuid::new_v4().to_string();
    let today = billing_now().date_naive();
    let mut tx = h.pool.begin().await.unwrap();
    assert!(
        entitlements::insert_grant(
            &mut tx,
            &entitlements::NewGrant {
                id: &id,
                establishment_id: &shop.id,
                offer_kind: "module",
                offer_code: "delivery",
                grant_kind: kind,
                starts_on: today - Duration::days(1),
                ends_on: today + Duration::days(1),
                reason: "Testing Delivery preview access",
                granted_by: &operator.id,
                now: &Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap()
    );
    tx.commit().await.unwrap();
    id
}

pub async fn submit(app: &Router, owner: &AuthSuccess, path: &str, fields: &str) -> Response {
    post_form(
        app,
        owner,
        path,
        format!("_csrf={}&{fields}", owner.csrf_token),
    )
    .await
}

pub async fn get_public(app: &Router, path: &str, etag: Option<&str>) -> Response {
    let mut request = Request::get(path);
    if let Some(etag) = etag {
        request = request.header(header::IF_NONE_MATCH, etag);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

pub async fn post_json(
    app: &Router,
    path: &str,
    payload: &Value,
    origin: &str,
    key: Option<Uuid>,
) -> Response {
    let mut request = Request::post(path)
        .header(header::ORIGIN, origin)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(key) = key {
        request = request.header("Idempotency-Key", key.to_string());
    }
    app.clone()
        .oneshot(request.body(Body::from(payload.to_string())).unwrap())
        .await
        .unwrap()
}

pub async fn body(response: Response) -> String {
    String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}

pub async fn expect_json(response: Response, status: StatusCode) -> Value {
    let actual = response.status();
    let text = body(response).await;
    assert_eq!(actual, status, "{text}");
    serde_json::from_str(&text).unwrap()
}

pub fn checkout(quote: &Value, totals: &Value, zone: Option<&str>) -> Value {
    let mut payload = quote.clone();
    payload["expected_total_centavos"] = totals["total_centavos"].clone();
    payload["expected_totals"] = json!({
        "items_subtotal_centavos": totals["items_subtotal_centavos"],
        "discount_centavos": totals["discount_centavos"],
        "delivery_fee_centavos": totals["delivery_fee_centavos"],
        "total_centavos": totals["total_centavos"],
    });
    payload["customer"] = json!({ "name": CUSTOMER, "phone": PHONE });
    payload["address"] = zone.map_or(Value::Null, |zone_id| {
        json!({
            "street": STREET,
            "number": "77",
            "complement": "Andar QA",
            "neighborhood_zone_id": zone_id,
            "city": "São Paulo",
            "uf": "SP",
            "cep": "01001000",
            "reference": "Referencia QA",
        })
    });
    payload["payment"] = json!({ "method": "cash" });
    payload["note"] = json!(PRIVATE_NOTE);
    payload
}
