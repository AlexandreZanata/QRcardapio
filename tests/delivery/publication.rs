use axum::http::{StatusCode, header};
use serde_json::Value;

use qrcardapio::application::publishing::{PublishLocks, publish_establishment};

use crate::fixture::{
    PUBLISH, SETTINGS, Shop, UNPUBLISH, body, expect_json, get_public, grant, grant_preview,
    preview_router, submit,
};
use crate::support::{config, get, harness, post_form};

#[tokio::test]
async fn delivery_access_requires_the_preview_lifecycle_and_a_scoped_explicit_grant() {
    let h = harness(false).await;
    let alpha = Shop::create(&h, "delivery-access-alpha", 0).await;
    let beta = Shop::create(&h, "delivery-access-beta", 0).await;

    // A grant still cannot activate a Development offer.
    grant_preview(&h, &alpha).await;
    assert_eq!(
        submit(&h.app, &alpha.owner, PUBLISH, "").await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get_public(&h.app, &alpha.storefront(), None).await.status(),
        StatusCode::NOT_FOUND
    );

    let preview = preview_router(&h).await;
    grant(&h, &beta, "courtesy").await;
    assert_eq!(
        submit(&preview, &beta.owner, PUBLISH, "").await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post_form(&preview, &alpha.owner, PUBLISH, "_csrf=wrong".into())
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get_public(&preview, SETTINGS, None).await.status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        get(&preview, &alpha.owner, SETTINGS).await.status(),
        StatusCode::OK
    );

    alpha.publish(&preview).await;
    assert_eq!(
        get_public(&preview, &alpha.storefront(), None)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        get_public(&preview, &beta.storefront(), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn published_storefront_and_scoped_assets_survive_without_sqlite_and_leave_the_menu_alone() {
    let h = harness(false).await;
    let preview = preview_router(&h).await;
    let alpha = Shop::create(&h, "delivery-static-alpha", 0).await;
    let beta = Shop::create(&h, "delivery-static-beta", 0).await;
    let cfg = config(&h.dir, false);
    let locks = PublishLocks::default();
    for shop in [&alpha, &beta] {
        grant_preview(&h, shop).await;
        publish_establishment(&h.pool, &cfg, &locks, &shop.id)
            .await
            .unwrap();
        shop.publish(&preview).await;
    }
    let alpha_catalog = format!(
        "{}catalog.v{}.json",
        alpha.storefront(),
        alpha.catalog_version(&h).await
    );
    let beta_catalog = format!(
        "{}catalog.v{}.json",
        beta.storefront(),
        beta.catalog_version(&h).await
    );
    let beta_json = expect_json(
        get_public(&preview, &beta_catalog, None).await,
        StatusCode::OK,
    )
    .await;
    assert!(beta_json.to_string().contains(&beta.item.product_id));
    assert!(!beta_json.to_string().contains(&alpha.item.product_id));

    let storefront_event: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM domain_events WHERE establishment_id = ? AND event_type = 'CatalogPublished'
         AND json_extract(payload, '$.channel') = 'storefront'",
    )
    .bind(&alpha.id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(storefront_event, 1);

    assert_eq!(
        submit(&preview, &beta.owner, UNPUBLISH, "").await.status(),
        StatusCode::SEE_OTHER
    );

    h.pool.close().await;
    let redirect = get_public(&preview, alpha.storefront().trim_end_matches('/'), None).await;
    assert_eq!(redirect.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        redirect.headers()[header::LOCATION].to_str().unwrap(),
        alpha.storefront()
    );

    let storefront = get_public(&preview, &alpha.storefront(), None).await;
    assert_eq!(storefront.status(), StatusCode::OK);
    assert_eq!(
        storefront.headers()[header::CACHE_CONTROL],
        "public, max-age=60, stale-while-revalidate=300"
    );
    let etag = storefront.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_owned();
    let html = body(storefront).await;
    let (_, data) = html.split_once(r#"id="qr-storefront-data">"#).unwrap();
    let (data, _) = data.split_once("</script>").unwrap();
    let embedded: Value = serde_json::from_str(data).unwrap();
    assert_eq!(embedded["slug"], alpha.slug);
    let (_, translations) = html.split_once(r#"id="qr-storefront-i18n">"#).unwrap();
    let (translations, _) = translations.split_once("</script>").unwrap();
    let translations: Value = serde_json::from_str(translations).unwrap();
    for attribute in html.split(" data-i18n=\"").skip(1) {
        let (key, _) = attribute.split_once('"').unwrap();
        for locale in ["pt-BR", "en", "es"] {
            assert!(
                translations[locale][key]
                    .as_str()
                    .is_some_and(|text| !text.is_empty()),
                "storefront label {key} is missing from the {locale} bundle"
            );
        }
    }
    assert_eq!(
        get_public(&preview, &alpha.storefront(), Some(&etag))
            .await
            .status(),
        StatusCode::NOT_MODIFIED
    );
    let json_response = get_public(&preview, &alpha_catalog, None).await;
    assert_eq!(
        json_response.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
    let catalog = expect_json(json_response, StatusCode::OK).await;
    assert_eq!(catalog, embedded);
    let catalog = catalog.to_string();
    assert!(catalog.contains(&alpha.item.product_id));
    assert!(!catalog.contains(&beta.item.product_id));

    let manifest: Value = expect_json(
        get_public(
            &preview,
            &format!("{}manifest.webmanifest", alpha.storefront()),
            None,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(manifest["scope"], alpha.storefront());
    assert_eq!(manifest["start_url"], alpha.storefront());

    let service_worker = get_public(&preview, &format!("{}sw.js", alpha.storefront()), None).await;
    assert_eq!(service_worker.status(), StatusCode::OK);
    assert_eq!(
        service_worker.headers()["service-worker-allowed"]
            .to_str()
            .unwrap(),
        alpha.storefront()
    );
    assert_eq!(service_worker.headers()[header::CACHE_CONTROL], "no-cache");
    let script = body(service_worker).await;
    assert!(script.contains(&alpha.storefront()));
    assert!(!script.contains(&beta.storefront()));

    assert_eq!(
        get_public(&preview, &beta.storefront(), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    for shop in [&alpha, &beta] {
        let response = get_public(&preview, &shop.menu(), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let html = body(response).await;
        assert!(html.contains("data-menu-item"));
        assert!(!html.contains("qr-storefront-data"));
    }
}

#[tokio::test]
async fn republish_all_recovers_the_published_presentation_without_exposing_draft_edits() {
    use qrcardapio::application::delivery::publishing::republish_all;
    use qrcardapio::application::menus::{ProductInput, update_product};
    use qrcardapio::infrastructure::assets::AssetProcessor;
    use qrcardapio::infrastructure::database::queries::find_product_for_est;

    let h = harness(false).await;
    let preview = preview_router(&h).await;
    let shop = Shop::create(&h, "delivery-recovery", 0).await;
    grant_preview(&h, &shop).await;
    shop.publish(&preview).await;
    let version = shop.catalog_version(&h).await;
    let catalog_name = format!("catalog.v{version}.json");
    let catalog_path = format!("{}{catalog_name}", shop.storefront());
    let html_before = body(get_public(&preview, &shop.storefront(), None).await).await;
    let catalog_before = expect_json(
        get_public(&preview, &catalog_path, None).await,
        StatusCode::OK,
    )
    .await;

    let cfg = config(&h.dir, false);
    let product = find_product_for_est(&h.pool, &shop.id, &shop.item.product_id)
        .await
        .unwrap()
        .unwrap();
    let private_name = "Mudança ainda no rascunho";
    update_product(
        &h.pool,
        &AssetProcessor::new(cfg.media_dir()),
        &shop.id,
        &product.id,
        ProductInput {
            category_id: &product.category_id,
            name: private_name,
            name_en: "",
            name_es: "",
            description: "",
            description_en: "",
            description_es: "",
            price_raw: "99,00",
            is_available: true,
            is_visible: true,
        },
        None,
        false,
    )
    .await
    .unwrap();
    let directory = cfg.storefronts_dir().join(shop.slug);
    std::fs::remove_file(directory.join("index.html")).unwrap();
    std::fs::remove_file(directory.join(catalog_name)).unwrap();
    assert_eq!(
        get_public(&preview, &shop.storefront(), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        republish_all(
            &h.pool,
            &cfg,
            &crate::fixture::preview_catalog(),
            &PublishLocks::default(),
        )
        .await
        .unwrap(),
        1
    );
    assert_eq!(shop.catalog_version(&h).await, version);
    let restored = get_public(&preview, &shop.storefront(), None).await;
    assert_eq!(restored.status(), StatusCode::OK);
    assert_eq!(body(restored).await, html_before);
    assert_eq!(
        expect_json(
            get_public(&preview, &catalog_path, None).await,
            StatusCode::OK,
        )
        .await,
        catalog_before
    );
    let draft = find_product_for_est(&h.pool, &shop.id, &product.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(draft.name, private_name);
    assert_eq!(draft.price_centavos, 9_900);
}
