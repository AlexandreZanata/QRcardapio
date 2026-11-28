//! Run the real Delivery router against local synthetic SQLite state, without a network listener.
#[path = "delivery_capacity/fixture.rs"]
mod fixture;
#[path = "delivery_capacity/http.rs"]
mod http;
#[path = "delivery_capacity/reconcile.rs"]
mod reconcile;
#[path = "delivery_capacity/staff.rs"]
mod staff;
#[path = "commercial/support.rs"]
mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use uuid::Uuid;

use qrcardapio::application::commerce::worker;

use fixture::{MENU_PATH, Tenant};
use http::{Mode, Visitor, assert_quote, order_input, post, quote_input, storefront_get};
use staff::StaffOrder;

const ESTABLISHMENTS: usize = 200;
const CLIENTS: usize = 64;
const CADENCE: Duration = Duration::from_millis(10);

struct Sample {
    quote: Vec<Duration>,
    checkout: Vec<Duration>,
    replay: Vec<Duration>,
    delivery: usize,
    pickup: usize,
}

impl Sample {
    fn new() -> Self {
        Self {
            quote: Vec::new(),
            checkout: Vec::new(),
            replay: Vec::new(),
            delivery: 0,
            pickup: 0,
        }
    }

    fn merge(&mut self, other: Self) {
        self.quote.extend(other.quote);
        self.checkout.extend(other.checkout);
        self.replay.extend(other.replay);
        self.delivery += other.delivery;
        self.pickup += other.pickup;
    }
}

async fn read_stream(
    app: Router,
    tenants: Arc<Vec<Tenant>>,
    seconds: Duration,
    storefront: bool,
) -> Vec<Duration> {
    let start = Instant::now();
    let mut samples = Vec::new();
    let mut cadence = tokio::time::interval(CADENCE);
    cadence.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    while start.elapsed() < seconds {
        cadence.tick().await;
        let path = if storefront {
            &tenants[samples.len() % tenants.len()].index_url
        } else {
            MENU_PATH
        };
        samples.push(storefront_get(&app, path).await);
    }
    samples
}

struct Percentiles {
    p95: f64,
    p99: f64,
}

fn percentiles(samples: &mut [Duration], name: &str) -> Percentiles {
    assert!(!samples.is_empty(), "no samples from {name}");
    samples.sort_unstable();
    let millis = |p: usize| samples[(samples.len() * p).div_ceil(100) - 1].as_secs_f64() * 1000.0;
    Percentiles {
        p95: millis(95),
        p99: millis(99),
    }
}

fn report(samples: &mut [Duration], name: &str, elapsed: Duration) -> Percentiles {
    let percentiles = percentiles(samples, name);
    println!(
        "{name}: {} requests, {:.2} req/s, p95 {:.3} ms, p99 {:.3} ms",
        samples.len(),
        samples.len() as f64 / elapsed.as_secs_f64(),
        percentiles.p95,
        percentiles.p99
    );
    percentiles
}

async fn simulate_visitor(
    app: Router,
    tenants: Arc<Vec<Tenant>>,
    staff: mpsc::Sender<StaffOrder>,
    client: usize,
    deadline: Instant,
) -> Sample {
    let mut samples = Sample::new();
    let mut sequence = 0;
    while Instant::now() < deadline {
        let tenant_index = (client + sequence) % tenants.len();
        let shop = &tenants[tenant_index];
        let mode = if (client + sequence).is_multiple_of(2) {
            Mode::Delivery
        } else {
            Mode::Pickup
        };
        let visitor = Visitor::synthetic(client, sequence);
        let input = quote_input(shop, mode);
        let (price, quote_latency) = post(&app, &shop.quote_url, visitor, &input, None).await;
        assert_quote(&price, shop, mode);
        samples.quote.push(quote_latency);
        let order = order_input(shop, mode, input, visitor);
        let key = Uuid::new_v4();
        let (receipt, order_latency) =
            post(&app, &shop.order_url, visitor, &order, Some(key)).await;
        samples.checkout.push(order_latency);
        let public_id = receipt["order_id"].as_str().unwrap();
        assert!(public_id.len() == 32 && public_id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(
            receipt["tracking_path"],
            format!("{}/{public_id}", shop.order_url)
        );
        assert!(receipt["cancel_token"].is_string());
        if sequence.is_multiple_of(9) {
            let (replay, replay_latency) =
                post(&app, &shop.order_url, visitor, &order, Some(key)).await;
            assert_eq!(replay, receipt, "idempotent HTTP 201 body");
            samples.replay.push(replay_latency);
        }
        staff
            .send(StaffOrder {
                tenant_index,
                public_id: public_id.to_owned(),
                submitted_at: Instant::now(),
            })
            .await
            .unwrap();
        match mode {
            Mode::Delivery => samples.delivery += 1,
            Mode::Pickup => samples.pickup += 1,
        }
        sequence += 1;
    }
    samples
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "local pinned release benchmark; run scripts/bench_commerce.sh delivery"]
async fn sustained_delivery_capacity() {
    let seconds = std::env::var("COMMERCE_BENCH_SECONDS")
        .ok()
        .map_or(600, |value| value.parse::<u64>().unwrap());
    assert!(seconds > 0);
    let duration = Duration::from_secs(seconds);
    let baseline_duration = Duration::from_secs(if seconds >= 600 { 30 } else { seconds.min(5) });
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let sync: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(pool)
        .await
        .unwrap();
    let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(sync, 2, "SQLite synchronous must be FULL");
    assert_eq!(journal, "wal");
    let tenants = Arc::new(fixture::seed(&harness, ESTABLISHMENTS).await);
    println!(
        "Delivery capacity: {ESTABLISHMENTS} establishments, {CLIENTS} clients, {seconds}s, router in-process, WAL/FULL"
    );

    // Use warmed caches for both measured phases. Otherwise the reference
    // includes cache fills the mixed phase will inherit.
    storefront_get(&harness.app, MENU_PATH).await;
    for tenant in tenants.iter() {
        storefront_get(&harness.app, &tenant.index_url).await;
    }

    let baseline_menu = tokio::spawn(read_stream(
        harness.app.clone(),
        tenants.clone(),
        baseline_duration,
        false,
    ));
    let baseline_storefront = tokio::spawn(read_stream(
        harness.app.clone(),
        tenants.clone(),
        baseline_duration,
        true,
    ));
    let (mut menu_baseline, mut storefront_baseline) = (
        baseline_menu.await.unwrap(),
        baseline_storefront.await.unwrap(),
    );
    let menu_before = report(&mut menu_baseline, "baseline GET /c", baseline_duration);
    let storefront_before = report(
        &mut storefront_baseline,
        "baseline GET /s",
        baseline_duration,
    );

    let worker_pool = pool.clone();
    let dispatcher = tokio::spawn(async move { worker::run(&worker_pool, None).await });
    let started = Instant::now();
    let deadline = started + duration;
    let menu = tokio::spawn(read_stream(
        harness.app.clone(),
        tenants.clone(),
        duration,
        false,
    ));
    let storefront = tokio::spawn(read_stream(
        harness.app.clone(),
        tenants.clone(),
        duration,
        true,
    ));
    let (staff_send, staff_receive) = mpsc::channel(CLIENTS);
    let staff = tokio::spawn(staff::process(
        harness.app.clone(),
        pool.clone(),
        tenants.clone(),
        staff_receive,
        started,
    ));
    let mut workers = JoinSet::new();
    for client in 0..CLIENTS {
        workers.spawn(simulate_visitor(
            harness.app.clone(),
            tenants.clone(),
            staff_send.clone(),
            client,
            deadline,
        ));
    }
    drop(staff_send);

    let mut samples = Sample::new();
    while let Some(sample) = workers.join_next().await {
        samples.merge(sample.unwrap());
    }
    let staff = staff.await.unwrap();
    let completed = staff.completed();
    let elapsed = started.elapsed();
    let (mut menu_live, mut storefront_live) = (menu.await.unwrap(), storefront.await.unwrap());
    assert_eq!(completed, samples.delivery + samples.pickup);
    assert_eq!(samples.checkout.len(), completed);
    assert_eq!(samples.quote.len(), completed);
    assert!(samples.delivery > 0 && samples.pickup > 0);
    assert!(!samples.replay.is_empty());

    let quotes = report(&mut samples.quote, "POST quotes", elapsed);
    let checkouts = report(&mut samples.checkout, "POST new orders", elapsed);
    report(&mut samples.replay, "POST replayed orders", elapsed);
    staff.report(elapsed);
    let menu_after = report(&mut menu_live, "mixed GET /c", duration);
    let storefront_after = report(&mut storefront_live, "mixed GET /s", duration);
    let orders_per_second = completed as f64 / elapsed.as_secs_f64();
    let menu_ratio = menu_after.p95 / menu_before.p95;
    let storefront_ratio = storefront_after.p95 / storefront_before.p95;
    println!(
        "staff-inclusive throughput: {orders_per_second:.2} completed/s; {} delivery, {} pickup, {:.2}s including drain",
        samples.delivery,
        samples.pickup,
        elapsed.as_secs_f64()
    );
    println!("mixed/static p95 ratios: /c {menu_ratio:.3}x, /s {storefront_ratio:.3}x");

    reconcile::check(pool, samples.delivery, samples.pickup).await;
    reconcile::check_publications(pool, completed).await;
    dispatcher.abort();
    let _ = dispatcher.await;
    if seconds >= 600 {
        let budgets = [
            ("throughput >= 30 orders/s", orders_per_second >= 30.0),
            ("quote p95 <= 50 ms", quotes.p95 <= 50.0),
            ("checkout p95 <= 300 ms", checkouts.p95 <= 300.0),
            ("checkout p99 <= 1000 ms", checkouts.p99 <= 1000.0),
            ("GET /c p95 <= 2x baseline", menu_ratio <= 2.0),
            ("GET /s p95 <= 2x baseline", storefront_ratio <= 2.0),
        ];
        for (name, passed) in budgets {
            println!(
                "capacity gate {name}: {}",
                if passed { "pass" } else { "fail" }
            );
        }
        let failed: Vec<_> = budgets
            .iter()
            .filter_map(|(name, passed)| (!passed).then_some(name))
            .collect();
        assert!(failed.is_empty(), "capacity budgets failed: {failed:?}");
    } else {
        println!("short smoke sample; production capacity gates not evaluated");
    }
}
