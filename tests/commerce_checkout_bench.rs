#[path = "commercial/support.rs"]
mod support;

use std::time::{Duration, Instant};

use axum::{body::Body, http::Request};
use chrono::Utc;
use sqlx::SqlitePool;
use tokio::{sync::mpsc, task::JoinSet};
use tower::ServiceExt;
use uuid::Uuid;

use qrcardapio::application::commerce::{checkout, order_transitions, worker};
use qrcardapio::application::demo_menu::seed_public_demo;
use qrcardapio::domain::commerce::business_hours::ServiceMode;
use qrcardapio::domain::commerce::checkout::{
    CheckoutCart, CheckoutOutcome, CheckoutRequest, CheckoutTarget,
};
use qrcardapio::domain::commerce::customer::CustomerProfile;
use qrcardapio::domain::commerce::order::{OrderAction, OrderItemSelection, TransitionOrder};
use qrcardapio::domain::commerce::order_payment::{OfflineMethod, OfflinePaymentChoice};
use qrcardapio::domain::commerce::state_machine::Actor;
use qrcardapio::infrastructure::database::queries::{catalog_snapshots, orders};

const ESTABLISHMENTS: usize = 200;
const CLIENTS: usize = 64;
const CHANNEL: &str = "pickup";
const EXPECTED_CENTAVOS: i64 = 1500;

struct Tenant {
    id: String,
    account_id: String,
    cart: CheckoutCart,
}

async fn setup(pool: &SqlitePool) -> Vec<Tenant> {
    let mut tenants = Vec::with_capacity(ESTABLISHMENTS);
    let mut tx = pool.begin().await.unwrap();
    for index in 0..ESTABLISHMENTS {
        let id = format!("bench-commerce-{index:03}");
        let product = format!("{id}-product");
        let account_id = format!("{id}-account");
        sqlx::query(
            "INSERT INTO establishments (id, slug, name, created_at, updated_at)
            VALUES (?1, ?1, 'Benchmark', '2026-11-25', '2026-11-25')",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, email, password_hash, created_at, updated_at)
            VALUES (?1, ?1 || '@example.test', 'fixture', '2026-11-25', '2026-11-25')",
        )
        .bind(&account_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO establishment_members (establishment_id, account_id, created_at)
            VALUES (?, ?, '2026-11-25')",
        )
        .bind(&id)
        .bind(&account_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO commercial_settings (establishment_id, pickup, acceptance_minutes)
            VALUES (?, 1, 30)",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO business_hours (establishment_id, weekday, windows_json)
            SELECT ?, value, '[[0,1440]]' FROM json_each('[0,1,2,3,4,5,6]')",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO categories (id, establishment_id, name, position, created_at, updated_at)
            VALUES (?1 || '-cat', ?1, 'Lanches', 0, '2026-11-25', '2026-11-25')",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query("INSERT INTO products (id, establishment_id, category_id, name, price_centavos,
            position, created_at, updated_at) VALUES (?1, ?2, ?2 || '-cat', 'Lanche', ?3, 0, '2026-11-25', '2026-11-25')")
            .bind(&product).bind(&id).bind(EXPECTED_CENTAVOS).execute(&mut *tx).await.unwrap();
        tenants.push(Tenant {
            id,
            account_id,
            cart: CheckoutCart {
                catalog_version: 1,
                items: vec![OrderItemSelection {
                    product_id: product,
                    variation_id: None,
                    quantity: 1,
                    addons: vec![],
                }],
            },
        });
    }
    tx.commit().await.unwrap();
    for tenant in &tenants {
        catalog_snapshots::publish(pool, &tenant.id, CHANNEL, &Utc::now().to_rfc3339())
            .await
            .unwrap();
    }
    tenants
}

async fn place(pool: &SqlitePool, tenant: &Tenant, customer: usize, key: Uuid) -> String {
    let target = CheckoutTarget {
        establishment_id: &tenant.id,
        catalog_channel: CHANNEL,
        mode: ServiceMode::Pickup,
        delivery_fee_centavos: 0,
        discount: None,
    };
    let request = CheckoutRequest {
        idempotency_key: key,
        cart: tenant.cart.clone(),
        expected_total_centavos: EXPECTED_CENTAVOS,
        customer: CustomerProfile {
            name: "Benchmark".into(),
            phone_e164: format!("+556599999{customer:04}"),
            email: String::new(),
        },
        address: None,
        payment: OfflinePaymentChoice {
            method: OfflineMethod::Cash,
            change_for_centavos: None,
        },
    };
    match checkout::place_order(pool, target, request, Utc::now())
        .await
        .unwrap()
    {
        CheckoutOutcome::Placed(receipt) => receipt.public_id,
        other => panic!("unexpected checkout outcome: {other:?}"),
    }
}

async fn progress(pool: &SqlitePool, tenant: &Tenant, public_id: &str) {
    let id: String =
        sqlx::query_scalar("SELECT id FROM orders WHERE establishment_id = ? AND public_id = ?")
            .bind(&tenant.id)
            .bind(public_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let mut order = orders::get(pool, &tenant.id, &id).await.unwrap().unwrap();
    for action in [
        OrderAction::Accept {
            promised_at: Utc::now() + chrono::Duration::minutes(5),
        },
        OrderAction::MarkReady,
        OrderAction::Complete,
    ] {
        order = order_transitions::transition(
            pool,
            &tenant.id,
            &id,
            TransitionOrder {
                expected_state: order.state,
                expected_version: order.version,
                action,
                actor: Actor::Owner,
                actor_account_id: Some(&tenant.account_id),
                event_context: Default::default(),
            },
            Utc::now(),
        )
        .await
        .unwrap();
    }
}

fn percentile(samples: &mut [Duration], p: usize) -> f64 {
    samples.sort_unstable();
    samples[(samples.len() * p).div_ceil(100) - 1].as_secs_f64() * 1000.0
}

async fn sample_menu(
    app: &axum::Router,
    duration: Duration,
    cadence: Option<Duration>,
) -> Vec<Duration> {
    let deadline = Instant::now() + duration;
    let mut samples = Vec::new();
    while Instant::now() < deadline {
        let start = Instant::now();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/c/bistro-da-praca")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        assert!(!bytes.is_empty());
        samples.push(start.elapsed());
        if let Some(cadence) = cadence {
            tokio::time::sleep(cadence).await;
        }
    }
    samples
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "pinned release benchmark; run scripts/bench_commerce.sh"]
async fn sustained_checkout() {
    let seconds = std::env::var("COMMERCE_BENCH_SECONDS")
        .ok()
        .map_or(600, |value| value.parse::<u64>().unwrap());
    let mixed = std::env::var_os("COMMERCE_BENCH_MIXED").is_some();
    let duration = Duration::from_secs(seconds);
    let harness = support::harness(false).await;
    let pool = harness.pool;
    let tenants = std::sync::Arc::new(setup(&pool).await);
    let mut static_baseline = Vec::new();
    if mixed {
        seed_public_demo(&pool, &support::config(&harness.dir, false))
            .await
            .unwrap();
        static_baseline = sample_menu(
            &harness.app,
            Duration::from_secs(5),
            Some(Duration::from_millis(10)),
        )
        .await;
    }
    let worker_pool = pool.clone();
    let dispatcher = tokio::spawn(async move { worker::run(&worker_pool, None).await });
    let start = Instant::now();
    let finish = start + duration;
    let static_work = mixed.then(|| {
        tokio::spawn(async move {
            sample_menu(&harness.app, duration, Some(Duration::from_millis(10))).await
        })
    });
    let (staff_send, mut staff_receive) = mpsc::channel::<(usize, String)>(CLIENTS);
    let staff_pool = pool.clone();
    let staff_tenants = tenants.clone();
    let staff = tokio::spawn(async move {
        let mut count = 0;
        while let Some((index, id)) = staff_receive.recv().await {
            progress(&staff_pool, &staff_tenants[index], &id).await;
            count += 1;
        }
        count
    });
    let mut workers = JoinSet::new();
    for client in 0..CLIENTS {
        let tenants = tenants.clone();
        let pool = pool.clone();
        let staff_send = staff_send.clone();
        workers.spawn(async move {
            let mut placements = 0;
            let mut retries = 0;
            let mut latencies = Vec::new();
            while Instant::now() < finish {
                let index = (client + placements) % tenants.len();
                let tenant = &tenants[index];
                let key = Uuid::new_v4();
                let request_start = Instant::now();
                let id = place(&pool, tenant, client, key).await;
                latencies.push(request_start.elapsed());
                if placements % 9 == 0 {
                    let retry_start = Instant::now();
                    assert_eq!(id, place(&pool, tenant, client, key).await);
                    latencies.push(retry_start.elapsed());
                    retries += 1;
                }
                placements += 1;
                staff_send.send((index, id)).await.unwrap();
            }
            (placements, retries, latencies)
        });
    }
    drop(staff_send);
    let mut placements = 0;
    let mut retries = 0;
    let mut latencies = Vec::new();
    while let Some(result) = workers.join_next().await {
        let (placed, replayed, measurements) = result.unwrap();
        placements += placed;
        retries += replayed;
        latencies.extend(measurements);
    }
    assert_eq!(staff.await.unwrap(), placements);
    dispatcher.abort();
    let _ = dispatcher.await;
    let elapsed = start.elapsed().as_secs_f64();
    let totals: (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), count(DISTINCT public_id),
        sum(total_centavos) FROM orders WHERE state = 'completed'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        totals,
        (
            placements as i64,
            placements as i64,
            placements as i64 * EXPECTED_CENTAVOS
        )
    );
    let reconciliation: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orders o
        WHERE total_centavos != (SELECT sum(i.line_total_centavos) FROM order_items i
        WHERE i.establishment_id = o.establishment_id AND i.order_id = o.id)
        OR total_centavos != (SELECT p.amount_centavos FROM order_payments p
        WHERE p.establishment_id = o.establishment_id AND p.order_id = o.id)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(reconciliation, 0);
    let all_orders: i64 = sqlx::query_scalar("SELECT count(*) FROM orders")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(all_orders, placements as i64);
    let throughput = placements as f64 / elapsed;
    let p95 = percentile(&mut latencies, 95);
    let p99 = percentile(&mut latencies, 99);
    println!(
        "mixed={mixed} seconds={elapsed:.2} orders={placements} retries={retries} orders/s={throughput:.2} checkout_p95_ms={p95:.3} checkout_p99_ms={p99:.3} errors=0 duplicates=0 unreconciled=0"
    );
    if let Some(task) = static_work {
        let baseline = percentile(&mut static_baseline, 95);
        let mut mixed = task.await.unwrap();
        let menu_p95 = percentile(&mut mixed, 95);
        let ratio = menu_p95 / baseline;
        println!(
            "static_reads={} baseline_p95_ms={baseline:.3} mixed_p95_ms={menu_p95:.3} ratio={ratio:.3}",
            mixed.len()
        );
        assert!(
            ratio <= 2.0,
            "static menu p95 exceeded 2× its pinned baseline"
        );
    }
    assert!(throughput >= 30.0 && p95 <= 300.0 && p99 <= 1000.0);
}
