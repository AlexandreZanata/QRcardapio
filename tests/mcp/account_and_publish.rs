use std::io::Cursor;

use axum::http::StatusCode;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use image::{DynamicImage, ImageEncoder, codecs::png::PngEncoder};
use reqwest::Url;
use serde_json::json;
use sha2::{Digest, Sha256};

use qrcardapio::application::menus::{
    CategoryInput, ProductInput, create_category, create_product,
};
use qrcardapio::infrastructure::assets::AssetProcessor;
use qrcardapio::infrastructure::database::queries::find_media_asset_for_est;

use super::support::*;

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = DynamicImage::new_rgba8(w, h);
    let rgba = img.to_rgba8();
    let mut out = Vec::new();
    PngEncoder::new(Cursor::new(&mut out))
        .write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
        .unwrap();
    out
}

#[tokio::test]
async fn ai_can_read_edit_account_settings_and_publish_with_slug_lock() {
    let (_dir, pool, app) = setup(true).await;
    let (owner_auth, _est_id) = owner(&pool, "cafe-inicial").await;
    let (_other_auth, _other_est) = owner(&pool, "slug-ocupado").await;

    let code = authorize(&app, CHATGPT, Some(&owner_auth), "menu.read menu.write").await;
    let tokens = json_body(code_exchange(&app, CHATGPT, &code, VERIFIER).await).await;
    let access = tokens["access_token"].as_str().unwrap();

    // 1. Read complete account & establishment settings.
    let initial = tool(&app, access, "get_account_settings", json!({})).await;
    let initial_sc = &initial["structuredContent"];
    assert_eq!(initial_sc["account"]["email"], "cafe-inicial@example.test");
    assert_eq!(initial_sc["account"]["preferred_locale"], "pt-BR");
    assert_eq!(initial_sc["establishment"]["slug"], "cafe-inicial");
    assert_eq!(initial_sc["establishment"]["slug_locked"], false);
    assert_eq!(initial_sc["publication"]["is_published"], false);
    assert_eq!(initial_sc["publication"]["can_publish"], true);
    let initial_updated_at = initial_sc["establishment"]["updated_at"]
        .as_str()
        .unwrap()
        .to_string();

    // 2. Reject duplicate slug taken by another tenant.
    let dup_slug = tool(
        &app,
        access,
        "update_account_settings",
        json!({
            "operation_key": "acc-dup-slug",
            "updated_at": initial_updated_at,
            "slug": "slug-ocupado"
        }),
    )
    .await;
    assert_eq!(dup_slug["isError"], true);
    assert_eq!(dup_slug["structuredContent"]["code"], "error.slug_taken");

    // 3. Reject invalid brand color & invalid Google Maps URL.
    let bad_color = tool(
        &app,
        access,
        "update_account_settings",
        json!({
            "operation_key": "acc-bad-color",
            "updated_at": initial_updated_at,
            "accent_color": "blue"
        }),
    )
    .await;
    assert_eq!(bad_color["isError"], true);
    assert_eq!(
        bad_color["structuredContent"]["code"],
        "error.invalid_color"
    );

    let bad_maps = tool(
        &app,
        access,
        "update_account_settings",
        json!({
            "operation_key": "acc-bad-maps",
            "updated_at": initial_updated_at,
            "google_maps_url": "https://evil.example/maps"
        }),
    )
    .await;
    assert_eq!(bad_maps["isError"], true);
    assert_eq!(
        bad_maps["structuredContent"]["code"],
        "error.invalid_google_maps_link"
    );

    // 4. Update all editable account & establishment fields before first publish.
    let update_args = json!({
        "operation_key": "acc-update-1",
        "updated_at": initial_updated_at,
        "name": "Bistrô Aurora",
        "slug": "bistro-aurora",
        "description": "Cozinha artesanal e café especial",
        "description_en": "Artisan kitchen and specialty coffee",
        "description_es": "Cocina artesanal y café de especialidad",
        "accent_color": "#263be8",
        "google_maps_url": "https://maps.app.goo.gl/aurora123",
        "google_review_url": "https://g.page/r/aurora123/review",
        "preferred_locale": "en"
    });
    let updated = tool(&app, access, "update_account_settings", update_args.clone()).await;
    assert!(updated.get("isError").is_none());
    let sc = &updated["structuredContent"];
    assert_eq!(sc["account"]["preferred_locale"], "en");
    assert_eq!(sc["establishment"]["name"], "Bistrô Aurora");
    assert_eq!(sc["establishment"]["slug"], "bistro-aurora");
    assert_eq!(sc["establishment"]["accent_color"], "#263BE8");
    assert_eq!(
        sc["establishment"]["google_maps_url"],
        "https://maps.app.goo.gl/aurora123"
    );
    assert_eq!(
        sc["establishment"]["google_review_url"],
        "https://g.page/r/aurora123/review"
    );
    assert_eq!(sc["applied"]["establishment_updated"], true);
    assert_eq!(sc["applied"]["locale_updated"], true);

    // 5. Idempotent replay with same operation_key succeeds; conflict with different payload fails.
    let replay = tool(&app, access, "update_account_settings", update_args).await;
    assert_eq!(replay, updated);

    let conflict = tool(
        &app,
        access,
        "update_account_settings",
        json!({
            "operation_key": "acc-update-1",
            "updated_at": initial_updated_at,
            "name": "Different Name"
        }),
    )
    .await;
    assert_eq!(conflict["isError"], true);
    assert_eq!(
        conflict["structuredContent"]["code"],
        "mcp.operation_conflict"
    );

    // 6. Stale updated_at returns mcp.account_changed localized in owner's new locale ("en").
    let stale = tool(
        &app,
        access,
        "update_account_settings",
        json!({
            "operation_key": "acc-stale-1",
            "updated_at": initial_updated_at,
            "name": "Should Fail"
        }),
    )
    .await;
    assert_eq!(stale["isError"], true);
    assert_eq!(stale["structuredContent"]["code"], "mcp.account_changed");
    assert!(
        stale["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("get_account_settings")
    );

    // 7. Clear Google Maps links via null and empty string.
    let after_ts = sc["establishment"]["updated_at"]
        .as_str()
        .unwrap()
        .to_string();
    let cleared = tool(
        &app,
        access,
        "update_account_settings",
        json!({
            "operation_key": "acc-clear-maps",
            "updated_at": after_ts,
            "google_maps_url": null,
            "google_review_url": ""
        }),
    )
    .await;
    assert!(cleared.get("isError").is_none());
    assert!(cleared["structuredContent"]["establishment"]["google_maps_url"].is_null());
    assert!(cleared["structuredContent"]["establishment"]["google_review_url"].is_null());

    // 8. Add a category & product and publish live via AI.
    let _ = tool(
        &app,
        access,
        "apply_menu_changes",
        json!({
            "operation_key": "menu-before-publish",
            "new_categories": [{
                "text": {"name": "Cafés"},
                "products": [{"text": {"name": "Espresso Duplo"}, "price_centavos": 1200}]
            }]
        }),
    )
    .await;
    let published = tool(&app, access, "publish_menu", json!({})).await;
    assert!(published.get("isError").is_none());
    assert_eq!(published["structuredContent"]["published"], true);
    assert_eq!(published["structuredContent"]["version"], 1);
    assert_eq!(
        published["structuredContent"]["public_url"],
        format!("{ORIGIN}/c/bistro-aurora")
    );

    let live_res = request(&app, "GET", "/c/bistro-aurora", &[], String::new()).await;
    assert_eq!(live_res.status(), StatusCode::OK);
    let live_html = body(live_res).await;
    assert!(live_html.contains("Bistrô Aurora"));
    assert!(live_html.contains("Espresso Duplo"));

    // 9. Slug is strictly locked after first publication.
    let post_pub = tool(&app, access, "get_account_settings", json!({})).await;
    assert_eq!(
        post_pub["structuredContent"]["establishment"]["slug_locked"],
        true
    );
    assert_eq!(
        post_pub["structuredContent"]["publication"]["is_published"],
        true
    );
    let locked_ts = post_pub["structuredContent"]["establishment"]["updated_at"]
        .as_str()
        .unwrap()
        .to_string();
    let slug_attempt = tool(
        &app,
        access,
        "update_account_settings",
        json!({
            "operation_key": "acc-slug-after-pub",
            "updated_at": locked_ts,
            "slug": "tentativa-novo-slug"
        }),
    )
    .await;
    assert_eq!(slug_attempt["isError"], true);
    assert_eq!(
        slug_attempt["structuredContent"]["code"],
        "error.slug_locked"
    );

    // 10. Unpublish takes /c/{slug} offline cleanly.
    let unpublished = tool(&app, access, "unpublish_menu", json!({})).await;
    assert!(unpublished.get("isError").is_none());
    assert_eq!(unpublished["structuredContent"]["published"], false);
    assert_eq!(
        request(&app, "GET", "/c/bistro-aurora", &[], String::new())
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn ai_can_move_reorder_and_delete_items_cleaning_orphaned_media_only_after_commit() {
    let (dir, pool, app) = setup(true).await;
    let (owner_auth, est_id) = owner(&pool, "cantina-org").await;
    let (_other_auth, other_est) = owner(&pool, "vizinho-org").await;

    let assets = AssetProcessor::new(dir.path().join("media"));
    let cat_a = create_category(
        &pool,
        &est_id,
        CategoryInput {
            name: "Entradas",
            name_en: "",
            name_es: "",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();
    let cat_b = create_category(
        &pool,
        &est_id,
        CategoryInput {
            name: "Pratos",
            name_en: "",
            name_es: "",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();
    let foreign_cat = create_category(
        &pool,
        &other_est,
        CategoryInput {
            name: "Alheia",
            name_en: "",
            name_es: "",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();

    let prod_photo = create_product(
        &pool,
        &assets,
        &est_id,
        ProductInput {
            category_id: &cat_a.id,
            name: "Bruschetta",
            name_en: "",
            name_es: "",
            description: "",
            description_en: "",
            description_es: "",
            price_raw: "24,90",
            is_available: true,
            is_visible: true,
        },
        Some(png_bytes(64, 64)),
    )
    .await
    .unwrap();
    let prod_b1 = create_product(
        &pool,
        &assets,
        &est_id,
        ProductInput {
            category_id: &cat_b.id,
            name: "Risoto",
            name_en: "",
            name_es: "",
            description: "",
            description_en: "",
            description_es: "",
            price_raw: "52,00",
            is_available: true,
            is_visible: true,
        },
        None,
    )
    .await
    .unwrap();
    let asset = find_media_asset_for_est(
        &pool,
        &est_id,
        prod_photo.image_asset_id.as_deref().unwrap(),
    )
    .await
    .unwrap()
    .unwrap();
    let large_path = assets
        .asset_file_path(&est_id, &asset.filename_large)
        .unwrap();
    let thumb_path = assets
        .asset_file_path(&est_id, &asset.filename_thumb)
        .unwrap();
    assert!(large_path.exists() && thumb_path.exists());

    let code = authorize(&app, CLAUDE, Some(&owner_auth), "menu.read menu.write").await;
    let tokens = json_body(code_exchange(&app, CLAUDE, &code, VERIFIER).await).await;
    let access = tokens["access_token"].as_str().unwrap();

    // Verify get_menu_draft returns position, product_count, and has_image.
    let cats_res = tool(&app, access, "get_menu_draft", json!({})).await;
    assert_eq!(
        cats_res["structuredContent"]["categories"][0]["position"],
        1
    );
    assert_eq!(
        cats_res["structuredContent"]["categories"][0]["product_count"],
        1
    );
    let prods_a = tool(
        &app,
        access,
        "get_menu_draft",
        json!({"category_id": cat_a.id}),
    )
    .await;
    assert_eq!(
        prods_a["structuredContent"]["products"][0]["has_image"],
        true
    );

    // Cross-tenant product move must be rejected.
    let bad_move = tool(
        &app,
        access,
        "apply_menu_changes",
        json!({
            "operation_key": "move-foreign-cat",
            "product_updates": [{
                "id": prod_photo.id,
                "updated_at": prod_photo.updated_at,
                "category_id": foreign_cat.id
            }]
        }),
    )
    .await;
    assert_eq!(bad_move["isError"], true);
    assert_eq!(bad_move["structuredContent"]["code"], "error.forbidden");

    // Move prod_photo to cat_b and reorder categories [cat_b, cat_a].
    let moved = tool(
        &app,
        access,
        "apply_menu_changes",
        json!({
            "operation_key": "move-and-reorder-1",
            "product_updates": [{
                "id": prod_photo.id,
                "updated_at": prod_photo.updated_at,
                "category_id": cat_b.id
            }],
            "category_order": [cat_b.id, cat_a.id]
        }),
    )
    .await;
    assert!(moved.get("isError").is_none());
    assert_eq!(moved["structuredContent"]["applied"]["products_updated"], 1);
    assert_eq!(
        moved["structuredContent"]["applied"]["categories_reordered"],
        true
    );

    // Incomplete category_order must fail cleanly with mcp.invalid_order.
    let bad_order = tool(
        &app,
        access,
        "apply_menu_changes",
        json!({
            "operation_key": "bad-cat-order",
            "category_order": [cat_b.id]
        }),
    )
    .await;
    assert_eq!(bad_order["isError"], true);
    assert_eq!(bad_order["structuredContent"]["code"], "mcp.invalid_order");

    // Reorder products inside cat_b: put moved prod_photo BEFORE prod_b1.
    let prods_b_before = tool(
        &app,
        access,
        "get_menu_draft",
        json!({"category_id": cat_b.id}),
    )
    .await;
    assert_eq!(
        prods_b_before["structuredContent"]["products"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let reorder_prods = tool(
        &app,
        access,
        "apply_menu_changes",
        json!({
            "operation_key": "reorder-prods-b",
            "product_orders": [{
                "category_id": cat_b.id,
                "product_ids": [prod_photo.id, prod_b1.id]
            }]
        }),
    )
    .await;
    assert!(reorder_prods.get("isError").is_none());
    assert_eq!(
        reorder_prods["structuredContent"]["applied"]["products_reordered"],
        2
    );

    let prods_b_after = tool(
        &app,
        access,
        "get_menu_draft",
        json!({"category_id": cat_b.id}),
    )
    .await;
    assert_eq!(
        prods_b_after["structuredContent"]["products"][0]["id"],
        prod_photo.id
    );
    let photo_latest_ts = prods_b_after["structuredContent"]["products"][0]["updated_at"]
        .as_str()
        .unwrap()
        .to_string();

    // Rolled-back batch containing a delete must NOT delete image files on disk!
    let rollback_del = tool(
        &app,
        access,
        "apply_menu_changes",
        json!({
            "operation_key": "delete-then-fail",
            "delete_products": [{"id": prod_photo.id, "updated_at": photo_latest_ts}],
            "category_updates": [{"id": cat_a.id, "updated_at": "2020-01-01T00:00:00Z", "visible": false}]
        }),
    )
    .await;
    assert_eq!(rollback_del["isError"], true);
    assert!(large_path.exists() && thumb_path.exists());

    // Valid delete removes product from DB AND cleans up orphaned media files on disk!
    let cats_fresh = tool(&app, access, "get_menu_draft", json!({})).await;
    let cat_a_ts = cats_fresh["structuredContent"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == cat_a.id)
        .unwrap()["updated_at"]
        .as_str()
        .unwrap()
        .to_string();
    let deleted = tool(
        &app,
        access,
        "apply_menu_changes",
        json!({
            "operation_key": "delete-photo-and-empty-cat",
            "delete_products": [{"id": prod_photo.id, "updated_at": photo_latest_ts}],
            "delete_categories": [{"id": cat_a.id, "updated_at": cat_a_ts}]
        }),
    )
    .await;
    assert!(deleted.get("isError").is_none());
    assert_eq!(
        deleted["structuredContent"]["applied"]["products_deleted"],
        1
    );
    assert_eq!(
        deleted["structuredContent"]["applied"]["categories_deleted"],
        1
    );
    assert!(!large_path.exists() && !thumb_path.exists());
}

#[tokio::test]
async fn publish_enforces_billing_and_readonly_scope_blocks_all_writes() {
    let (_dir, pool, app) = setup_with_billing(true, true).await;
    let (owner_auth, _est_id) = owner(&pool, "pago-cafe").await;

    // 1. Write token cannot publish when Asaas billing is active without a paid subscription.
    let write_code = authorize(&app, GOOGLE, Some(&owner_auth), "menu.read menu.write").await;
    let write_tokens = json_body(code_exchange(&app, GOOGLE, &write_code, VERIFIER).await).await;
    let write_access = write_tokens["access_token"].as_str().unwrap();

    let status = tool(&app, write_access, "get_account_settings", json!({})).await;
    assert_eq!(
        status["structuredContent"]["publication"]["billing_enabled"],
        true
    );
    assert_eq!(
        status["structuredContent"]["publication"]["can_publish"],
        false
    );

    let blocked_pub = tool(&app, write_access, "publish_menu", json!({})).await;
    assert_eq!(blocked_pub["isError"], true);
    assert_eq!(
        blocked_pub["structuredContent"]["code"],
        "error.subscription_required"
    );
    assert_eq!(
        blocked_pub["structuredContent"]["billing_url"],
        format!("{ORIGIN}/dashboard/billing")
    );
    assert_eq!(
        request(&app, "GET", "/c/pago-cafe", &[], String::new())
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    // 2. Read-only token can read account & menu, but receives 403 on every write tool.
    let read_code = authorize(&app, CHATGPT, Some(&owner_auth), "menu.read").await;
    let read_tokens = json_body(code_exchange(&app, CHATGPT, &read_code, VERIFIER).await).await;
    let read_access = read_tokens["access_token"].as_str().unwrap();

    assert!(
        tool(&app, read_access, "get_account_settings", json!({}))
            .await
            .get("isError")
            .is_none()
    );
    assert!(
        tool(&app, read_access, "get_menu_draft", json!({}))
            .await
            .get("isError")
            .is_none()
    );
    for write_tool in [
        "update_account_settings",
        "apply_menu_changes",
        "publish_menu",
        "unpublish_menu",
    ] {
        let res = rpc(
            &app,
            read_access,
            "tools/call",
            json!({"name": write_tool, "arguments": {}}),
        )
        .await;
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "{write_tool} must require menu.write scope"
        );
    }
}

#[tokio::test]
async fn oauth_accepts_blank_optional_params_and_rejects_malformed_queries_as_json() {
    let (_dir, _pool, app) = setup(true).await;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(VERIFIER));
    let blank_opt = Url::parse_with_params(
        &format!("{ORIGIN}/mcp/oauth/authorize"),
        [
            ("client_id", CHATGPT.0),
            ("redirect_uri", CHATGPT.1),
            ("response_type", "code"),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("scope", ""),
            ("resource", ""),
        ],
    )
    .unwrap();
    let ok = request(
        &app,
        "GET",
        &format!("{}?{}", blank_opt.path(), blank_opt.query().unwrap()),
        &[],
        String::new(),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::SEE_OTHER);

    let malformed = request(
        &app,
        "GET",
        "/mcp/oauth/authorize?response_type=code",
        &[],
        String::new(),
    )
    .await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(malformed).await["error"], "invalid_request");
}
