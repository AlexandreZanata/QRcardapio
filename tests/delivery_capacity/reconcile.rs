use std::time::{Duration, Instant};

use sqlx::SqlitePool;

use crate::fixture::{DELIVERY_FEE, SUBTOTAL};

pub async fn check(pool: &SqlitePool, delivery: usize, pickup: usize) {
    let count = i64::try_from(delivery + pickup).unwrap();
    let expected: Vec<(String, i64, i64)> = [
        ("delivery", delivery, SUBTOTAL + DELIVERY_FEE),
        ("pickup", pickup, SUBTOTAL),
    ]
    .map(|(mode, orders, unit_total)| {
        let orders = i64::try_from(orders).unwrap();
        (mode.to_owned(), orders, orders * unit_total)
    })
    .into();
    let orders: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT fulfillment, COUNT(*), SUM(total_centavos)
         FROM orders GROUP BY fulfillment ORDER BY fulfillment",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(orders, expected, "orders and centavos by fulfillment");

    let inconsistent: i64 = sqlx::query_scalar(
        r"
        SELECT COUNT(*) FROM orders o
        LEFT JOIN order_payments p
          ON p.establishment_id = o.establishment_id AND p.order_id = o.id
        LEFT JOIN checkout_attempts a
          ON a.establishment_id = o.establishment_id AND a.order_id = o.id
        LEFT JOIN delivery_order_metadata d
          ON d.establishment_id = o.establishment_id AND d.order_id = o.id
        LEFT JOIN order_delivery_details dd
          ON dd.establishment_id = o.establishment_id AND dd.order_id = o.id
        LEFT JOIN order_items i
          ON i.establishment_id = o.establishment_id AND i.order_id = o.id
        LEFT JOIN order_item_options opt
          ON opt.establishment_id = i.establishment_id AND opt.order_item_id = i.id
        WHERE o.catalog_channel <> 'storefront' OR o.channel <> o.fulfillment
           OR o.state <> 'completed' OR o.version <> 4 OR o.completed_at IS NULL
           OR o.items_subtotal_centavos <> ?1 OR o.discount_centavos <> 0
           OR o.delivery_fee_centavos <> CASE WHEN o.fulfillment = 'delivery' THEN ?2 ELSE 0 END
           OR o.total_centavos <> o.items_subtotal_centavos + o.delivery_fee_centavos
           OR p.id IS NULL OR p.requested_method <> 'cash' OR p.method <> 'cash'
           OR p.state <> 'pending' OR p.amount_centavos <> o.total_centavos
           OR p.refunded_centavos <> 0 OR p.refund_due_centavos <> 0
           OR a.order_id IS NULL OR d.order_id IS NULL OR d.cancel_token_hash IS NULL
           OR (o.fulfillment = 'delivery' AND (d.zone_id IS NULL OR dd.order_id IS NULL))
           OR (o.fulfillment = 'pickup' AND (d.zone_id IS NOT NULL OR dd.order_id IS NOT NULL))
           OR d.pix_receipt_key IS NOT NULL OR d.coupon_id IS NOT NULL
           OR i.id IS NULL OR i.product_id <> o.establishment_id || '-product'
           OR i.variation_id IS NOT (o.establishment_id || '-variation')
           OR i.quantity <> 2 OR i.line_total_centavos <> o.items_subtotal_centavos
           OR opt.option_id IS NOT (o.establishment_id || '-addon') OR opt.quantity <> 2
        ",
    )
    .bind(SUBTOTAL)
    .bind(DELIVERY_FEE)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(inconsistent, 0, "order totals and linked snapshots");

    let ownership: (i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT COUNT(DISTINCT public_id) FROM orders),
           (SELECT COUNT(*) FROM order_payments),
           (SELECT COUNT(*) FROM checkout_attempts),
           (SELECT COUNT(*) FROM delivery_order_metadata),
           (SELECT COUNT(*) FROM order_delivery_details),
           (SELECT COUNT(*) FROM order_items),
           (SELECT COUNT(*) FROM order_item_options),
           (SELECT COUNT(*) FROM order_transitions)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        ownership,
        (
            count,
            count,
            count,
            count,
            i64::try_from(delivery).unwrap(),
            count,
            count,
            3 * count
        ),
        "one placement and no duplicate effects per idempotency key"
    );

    let events: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT d.event_type, COUNT(*), COUNT(DISTINCT o.id)
         FROM domain_events d
         JOIN orders o ON o.establishment_id = d.establishment_id AND o.id = d.aggregate_id
         WHERE d.aggregate_type = 'order' GROUP BY d.event_type ORDER BY d.event_type",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        events,
        [
            "OrderAccepted",
            "OrderCompleted",
            "OrderCreated",
            "OrderReady",
        ]
        .map(|event| (event.to_owned(), count, count)),
        "exactly one event of each expected kind per order"
    );
    let wrong_event: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM domain_events d
         LEFT JOIN orders o ON o.establishment_id = d.establishment_id AND o.id = d.aggregate_id
         WHERE d.aggregate_type = 'order' AND (
           o.id IS NULL OR d.payload LIKE '%Cliente sintético%' OR d.payload LIKE '%Rua Sintética%'
           OR (d.event_type = 'OrderCreated'
             AND json_extract(d.payload, '$.total_centavos') IS NOT o.total_centavos)
         )",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(wrong_event, 0, "outbox tenant and centavo reconciliation");
    let foreign_key_check = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(pool)
        .await
        .unwrap();
    assert!(foreign_key_check.is_empty());
    println!(
        "reconciled {count} placements, {count} pending offline payments, {} transitions, {} order events, no duplicate or monetary discrepancies",
        3 * count,
        4 * count
    );
}

pub async fn check_publications(pool: &SqlitePool, orders: usize) {
    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let pending: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM event_dispatch_publications WHERE published_at IS NULL",
            )
            .fetch_one(pool)
            .await
            .unwrap();
            if pending == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("outbox publications did not drain after traffic stopped");
    let published: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM domain_events e
         JOIN event_dispatch_publications p ON p.event_seq = e.seq
         WHERE e.aggregate_type = 'order' AND p.published_at IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let notified: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event_consumer_receipts r
         JOIN domain_events e ON e.event_id = r.event_id
         WHERE e.event_type = 'OrderCreated' AND r.consumer = 'commerce.notifications'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(published, i64::try_from(orders).unwrap() * 4);
    assert_eq!(notified, i64::try_from(orders).unwrap());
    println!(
        "outbox: {published} order events published and {notified} notification receipts reconciled in {:.3}s after ledger reconciliation",
        started.elapsed().as_secs_f64()
    );
}
