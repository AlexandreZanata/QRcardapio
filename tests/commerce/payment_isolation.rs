use super::*;

#[test]
fn offline_payment_choice_rejects_saas_provider_and_platform_account() {
    for request in [
        json!({ "method": "asaas", "change_for_centavos": null }),
        json!({ "method": "cash", "provider_account": "platform", "change_for_centavos": null }),
    ] {
        assert!(serde_json::from_value::<OfflinePaymentChoice>(request).is_err());
    }
}

#[tokio::test]
async fn offline_payment_is_tenant_scoped_and_wrong_method_cash_change_is_rejected() {
    let (harness, owner, order, now) = setup("offline-payment-tenant").await;
    let (other, other_est) = support::owner(&harness.pool, "offline-foreign").await;
    let selected = choice(OfflineMethod::Cash, Some(5_000));
    let pool = &harness.pool;
    let est = &order.establishment_id;
    assert!(matches!(
        create_pending(pool, &other_est, &order.id, selected, now).await,
        Err(AppError::NotFound(_))
    ));
    for invalid in [
        choice(OfflineMethod::Cash, Some(order.total_centavos - 1)),
        choice(OfflineMethod::CardOnDelivery, Some(5_000)),
        choice(OfflineMethod::PixOnDelivery, Some(5_000)),
    ] {
        assert!(matches!(
            create_pending(pool, est, &order.id, invalid, now).await,
            Err(AppError::Validation("commerce.invalid_cash_change"))
        ));
    }
    let payment = create_pending(pool, est, &order.id, selected, now)
        .await
        .unwrap();
    let default_state: String = sqlx::query_scalar(
        "SELECT dflt_value FROM pragma_table_info('order_payments') WHERE name = 'state'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        default_state,
        format!("'{}'", definition(Machine::OrderPayment).initial.as_str())
    );
    assert_eq!(payment.state, OrderPaymentState::Pending);
    assert_eq!(payment.amount_centavos, order.total_centavos);
    assert_eq!(payment.change_for_centavos, Some(5_000));
    assert!(
        order_payments::get(pool, &other_est, &order.id, &payment.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        create_pending(pool, est, &order.id, selected, now).await,
        Err(AppError::Conflict(_))
    ));
    for (command, missing) in [
        (
            PaymentOperation {
                establishment_id: &other_est,
                ..operation(&payment, &other.account.id)
            },
            true,
        ),
        (
            PaymentOperation {
                order_id: &other_est,
                ..operation(&payment, &owner)
            },
            true,
        ),
        (operation(&payment, &other.account.id), false),
        (
            PaymentOperation {
                actor: Actor::Courier,
                ..operation(&payment, &owner)
            },
            false,
        ),
        (
            PaymentOperation {
                actor: Actor::Gateway,
                ..operation(&payment, &owner)
            },
            false,
        ),
    ] {
        let error = confirm(pool, command, OfflineMethod::Cash, now)
            .await
            .unwrap_err();
        if missing {
            assert!(matches!(error, AppError::NotFound(_)));
        } else {
            assert!(matches!(error, AppError::Forbidden(_)));
        }
    }

    let cancelled = cancel(pool, operation(&payment, &owner), now)
        .await
        .unwrap();
    assert_eq!(cancelled.state, OrderPaymentState::Cancelled);
    assert_eq!(cancelled.version, 2);
    let replacement = create_pending(
        pool,
        est,
        &order.id,
        choice(OfflineMethod::PixOnDelivery, None),
        now,
    )
    .await
    .unwrap();
    assert_ne!(replacement.id, cancelled.id);
    assert!(matches!(
        confirm(
            pool,
            operation(&cancelled, &owner),
            OfflineMethod::Cash,
            now
        )
        .await,
        Err(AppError::Conflict(_))
    ));
    let traces: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT from_state, to_state, actor_account_id FROM order_payment_transitions
         WHERE payment_id = ? AND establishment_id = ?",
    )
    .bind(&payment.id)
    .bind(est)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(traces, [("pending".into(), "cancelled".into(), owner)]);
    assert_eq!(
        domain_events::list_for_establishment(pool, est)
            .await
            .unwrap()
            .len(),
        1
    );
}
