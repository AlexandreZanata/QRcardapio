use std::net::{Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::fixture::{DELIVERY_FEE, ORIGIN, SUBTOTAL, Tenant};
use crate::support::post_form;

#[derive(Clone, Copy)]
pub enum Mode {
    Delivery,
    Pickup,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Delivery => "delivery",
            Self::Pickup => "pickup",
        }
    }

    fn zone(self, tenant: &Tenant) -> Option<&str> {
        match self {
            Self::Delivery => Some(&tenant.zone),
            Self::Pickup => None,
        }
    }

    pub const fn fee(self) -> i64 {
        match self {
            Self::Delivery => DELIVERY_FEE,
            Self::Pickup => 0,
        }
    }

    pub const fn total(self) -> i64 {
        SUBTOTAL + self.fee()
    }
}

#[derive(Clone, Copy)]
pub struct Visitor {
    ip: SocketAddr,
    unique: u64,
}

impl Visitor {
    pub fn synthetic(client: usize, sequence: usize) -> Self {
        let client: u64 = client.try_into().unwrap();
        let sequence: u64 = sequence.try_into().unwrap();
        let unique = (sequence << 16) | client;
        // RFC 3849 documentation prefix. No outbound network or forwarding header.
        let ip = Ipv6Addr::from(0x2001_0db8_0000_0000_0000_0000_0000_0000 | u128::from(unique));
        Self {
            ip: SocketAddr::from((ip, 45_000)),
            unique,
        }
    }
}

pub fn quote_input(tenant: &Tenant, mode: Mode) -> Value {
    json!({
        "catalog_version": tenant.catalog_version,
        "fulfillment": mode.label(),
        "zone_id": mode.zone(tenant),
        "lines": [{
            "product_id": tenant.product,
            "variation_id": tenant.variation,
            "quantity": 2,
            "options": [{"option_id": tenant.addon, "quantity": 2}],
        }],
    })
}

pub fn order_input(tenant: &Tenant, mode: Mode, quote: Value, visitor: Visitor) -> Value {
    let mut order = quote;
    order["expected_total_centavos"] = json!(mode.total());
    order["expected_totals"] = totals(mode);
    order["customer"] = json!({
        "name": "Cliente sintético do benchmark",
        "phone": format!("+9998{:010}", visitor.unique),
    });
    order["address"] = mode.zone(tenant).map_or(Value::Null, |zone| {
        json!({
            "street": "Rua Sintética",
            "number": "1",
            "neighborhood_zone_id": zone,
            "city": "Sao Paulo",
            "uf": "SP",
        })
    });
    order["payment"] = json!({"method": "cash"});
    order
}

fn totals(mode: Mode) -> Value {
    json!({
        "items_subtotal_centavos": SUBTOTAL,
        "discount_centavos": 0,
        "delivery_fee_centavos": mode.fee(),
        "total_centavos": mode.total(),
    })
}

pub fn assert_quote(quoted: &Value, tenant: &Tenant, mode: Mode) {
    assert_eq!(quoted["catalog_version"], tenant.catalog_version);
    assert_eq!(quoted["open_now"], true);
    assert_eq!(quoted["issues"], json!([]));
    assert_eq!(
        quoted["lines"],
        json!([{
            "unit_price_centavos": SUBTOTAL / 2,
            "line_total_centavos": SUBTOTAL,
            "available": true,
        }])
    );
    for (key, value) in totals(mode).as_object().unwrap() {
        assert_eq!(&quoted[key], value, "quoted total component {key}");
    }
}

pub async fn post(
    app: &Router,
    path: &str,
    visitor: Visitor,
    body: &Value,
    key: Option<Uuid>,
) -> (Value, Duration) {
    let mut request = Request::post(path)
        .extension(ConnectInfo(visitor.ip))
        .header(header::HOST, "menu.example.test")
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(key) = key {
        request = request.header("Idempotency-Key", key.to_string());
    }
    let request = request.body(Body::from(body.to_string())).unwrap();
    let start = Instant::now();
    let response = app.clone().oneshot(request).await.unwrap();
    let expected = if key.is_some() {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let latency = start.elapsed();
    assert_eq!(
        status,
        expected,
        "{path}: {}",
        String::from_utf8_lossy(&bytes)
    );
    (serde_json::from_slice(&bytes).unwrap(), latency)
}

pub async fn storefront_get(app: &Router, path: &str) -> Duration {
    let request = Request::get(path).body(Body::empty()).unwrap();
    let start = Instant::now();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{path}");
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    assert!(!body.is_empty(), "{path}");
    start.elapsed()
}

pub async fn complete(app: &Router, pool: &SqlitePool, tenant: &Tenant, public_id: &str) {
    let (id, mut version): (String, i64) = sqlx::query_as(
        "SELECT id, version FROM orders WHERE establishment_id = ? AND public_id = ?",
    )
    .bind(&tenant.id)
    .bind(public_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let path = format!("/dashboard/delivery/orders/{id}/transition");
    for target in ["accepted", "ready", "completed"] {
        let answer = post_form(
            app,
            &tenant.login,
            &path,
            format!(
                "_csrf={}&to={target}&expected_version={version}&prep_minutes=15",
                tenant.login.csrf_token
            ),
        )
        .await;
        assert_eq!(answer.status(), StatusCode::SEE_OTHER, "{target} {id}");
        version += 1;
    }
}
