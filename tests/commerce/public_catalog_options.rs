use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sqlx::SqlitePool;
use tower::ServiceExt;

use qrcardapio::application::auth::AuthSuccess;
use qrcardapio::application::publishing::{PublishLocks, republish_all};
use qrcardapio::domain::commerce::catalog_options::{AddonGroup, AddonOption};
use qrcardapio::infrastructure::database::queries::{
    addon_groups, addon_options, product_variations,
};

use crate::{fixture, support};

const NOW: &str = "2026-11-26T15:00:00Z";
const GROUPS: &str = "/dashboard/menu/addons";
const PREFERENCE: &str = "/dashboard/menu/addons/public-options";
const PREVIEW: &str = "/dashboard/preview";
const PUBLIC: &str = "/c/public-options-a";
const CHOICES: &str = "<details class=\"item-choices\">";

async fn html(response: Response) -> String {
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("script-src 'self'")
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn public_html(app: &Router) -> String {
    html(
        app.clone()
            .oneshot(Request::get(PUBLIC).body(Body::empty()).unwrap())
            .await
            .unwrap(),
    )
    .await
}

async fn enabled(app: &Router, actor: &AuthSuccess) -> bool {
    let groups = html(support::get(app, actor, GROUPS).await).await;
    let (_, form) = groups
        .split_once("action=\"/dashboard/menu/addons/public-options\"")
        .unwrap();
    let (form, _) = form.split_once("</form>").unwrap();
    let checkbox = form
        .split('<')
        .find(|tag| tag.starts_with("input") && tag.contains("name=\"enabled\""))
        .unwrap();
    assert!(checkbox.contains("type=\"checkbox\"") && checkbox.contains("value=\"1\""));
    checkbox.contains(" checked")
}

async fn set_preference(app: &Router, actor: &AuthSuccess, fields: &str) {
    let response = support::post_form(
        app,
        actor,
        PREFERENCE,
        format!("_csrf={}{fields}", actor.csrf_token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(support::location(&response), GROUPS);
}

async fn publish(app: &Router, actor: &AuthSuccess) {
    let response = support::post_form(
        app,
        actor,
        "/dashboard/publish",
        format!("_csrf={}", actor.csrf_token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

async fn seed_addon(
    pool: &SqlitePool,
    establishment: &str,
    product: Option<&str>,
    id: &str,
    label: &str,
) {
    addon_groups::save(
        pool,
        establishment,
        &AddonGroup {
            id: id.into(),
            name: format!("Grupo {label}"),
            name_en: String::new(),
            name_es: String::new(),
            min_select: 0,
            max_select: 1,
            position: 0,
        },
        NOW,
    )
    .await
    .unwrap();
    addon_options::save(
        pool,
        establishment,
        id,
        &AddonOption {
            id: format!("{id}-option"),
            name: label.into(),
            name_en: String::new(),
            name_es: String::new(),
            price_centavos: 100,
            position: 0,
            max_quantity: 1,
            is_available: true,
        },
        NOW,
    )
    .await
    .unwrap();
    if let Some(product) = product {
        addon_groups::link_to_product(pool, establishment, product, id, 0, NOW)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn options_require_opt_in_and_publication_then_serve_without_database() {
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let app = &harness.app;
    let cfg = support::config(&harness.dir, false);
    let locks = PublishLocks::default();
    let (owner, establishment) = support::owner(pool, "public-options-a").await;
    let (other_owner, other_establishment) = support::owner(pool, "public-options-b").await;
    let manager =
        support::manager(pool, &establishment, "public-options-manager@example.test").await;
    let catalog = fixture::catalog(&harness, &establishment, fixture::at(NOW)).await;
    let product = &catalog.items[0].product_id;
    let other_catalog = fixture::catalog(&harness, &other_establishment, fixture::at(NOW)).await;
    let mut unavailable_variation = fixture::variation("public-option-unavailable-variation");
    unavailable_variation.name = "Variação indisponível".into();
    unavailable_variation.is_available = false;
    product_variations::save(pool, &establishment, product, &unavailable_variation, NOW)
        .await
        .unwrap();
    let group = addon_groups::list_for_establishment(pool, &establishment)
        .await
        .unwrap()
        .remove(0);
    let group = AddonGroup {
        name: "Complementos da casa".into(),
        name_en: "House extras".into(),
        name_es: "Extras de la casa".into(),
        ..group
    };
    addon_groups::save(pool, &establishment, &group, NOW)
        .await
        .unwrap();
    let mut option = addon_options::list_for_group(pool, &establishment, &group.id)
        .await
        .unwrap()
        .remove(0);
    addon_options::save(
        pool,
        &establishment,
        &group.id,
        &AddonOption {
            id: "public-option-unavailable".into(),
            name: "Adicional indisponível".into(),
            name_en: String::new(),
            name_es: String::new(),
            is_available: false,
            ..option.clone()
        },
        NOW,
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO products (id, establishment_id, category_id, name, price_centavos,
                               position, is_visible, created_at, updated_at)
         SELECT 'public-option-hidden-product', establishment_id, category_id, 'Produto oculto',
                price_centavos, 99, 0, created_at, updated_at
         FROM products WHERE id = ? AND establishment_id = ?",
    )
    .bind(product)
    .bind(&establishment)
    .execute(pool)
    .await
    .unwrap();
    for (est, linked_product, id, label) in [
        (
            establishment.as_str(),
            Some("public-option-hidden-product"),
            "public-option-hidden",
            "Adicional do produto oculto",
        ),
        (
            establishment.as_str(),
            None,
            "public-option-unlinked",
            "Adicional sem vínculo",
        ),
        (
            other_establishment.as_str(),
            Some(other_catalog.items[0].product_id.as_str()),
            "public-option-foreign",
            "Adicional de outro estabelecimento",
        ),
    ] {
        seed_addon(pool, est, linked_product, id, label).await;
    }

    assert!(!enabled(app, &owner).await);
    let unauthorized = app
        .clone()
        .oneshot(
            Request::post(PREFERENCE)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("_csrf=wrong&enabled=1"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(support::location(&unauthorized), "/login");
    assert_eq!(
        support::post_form(app, &manager, PREFERENCE, "_csrf=wrong&enabled=1".into())
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert!(
        !html(support::get(app, &owner, PREVIEW).await)
            .await
            .contains(CHOICES)
    );
    publish(app, &owner).await;
    let plain = public_html(app).await;
    assert!(!plain.contains(CHOICES));

    set_preference(app, &manager, "&enabled=1").await;
    assert!(enabled(app, &owner).await);
    assert!(!enabled(app, &other_owner).await);
    let preview = html(support::get(app, &owner, PREVIEW).await).await;
    assert!(preview.contains(CHOICES));
    assert!(preview.contains("data-pt=\"Queijo\""));
    assert!(!preview.contains("Adicional de outro estabelecimento"));
    assert!(!preview.contains("Adicional sem vínculo"));
    assert_eq!(republish_all(pool, &cfg, &locks).await.unwrap(), 0);
    assert_eq!(public_html(app).await, plain);

    publish(app, &owner).await;
    let published = public_html(app).await;
    for expected in [
        CHOICES,
        "data-i18n=\"commerce.catalog.variations\"",
        "data-en=\"Large\"",
        "data-price-centavos=\"2000\"",
        "data-en=\"House extras\"",
        "data-es=\"Extras de la casa\"",
        "data-en=\"Cheese\"",
        "data-es=\"Queso\"",
        "data-price-centavos=\"125\"",
    ] {
        assert!(published.contains(expected), "missing {expected}");
    }
    for excluded in [
        "Adicional de outro estabelecimento",
        "Adicional do produto oculto",
        "Adicional sem vínculo",
        "Adicional indisponível",
        "Variação indisponível",
        "<form",
        " style=",
    ] {
        assert!(!published.contains(excluded), "unexpected {excluded}");
    }
    assert_eq!(republish_all(pool, &cfg, &locks).await.unwrap(), 1);
    assert_eq!(public_html(app).await, published);

    option.name = "Queijo curado".into();
    addon_options::save(pool, &establishment, &group.id, &option, NOW)
        .await
        .unwrap();
    let preview = html(support::get(app, &owner, PREVIEW).await).await;
    assert!(preview.contains("data-pt=\"Queijo curado\""));
    assert_eq!(republish_all(pool, &cfg, &locks).await.unwrap(), 0);
    assert_eq!(public_html(app).await, published);
    publish(app, &owner).await;
    let updated = public_html(app).await;
    assert!(updated.contains("data-pt=\"Queijo curado\""));
    assert!(!updated.contains("data-pt=\"Queijo\""));

    set_preference(app, &owner, "").await;
    assert!(!enabled(app, &owner).await);
    let preview = html(support::get(app, &owner, PREVIEW).await).await;
    assert!(!preview.contains(CHOICES));
    assert_eq!(republish_all(pool, &cfg, &locks).await.unwrap(), 0);
    assert_eq!(public_html(app).await, updated);
    publish(app, &owner).await;
    assert_eq!(public_html(app).await, plain);

    set_preference(app, &owner, "&enabled=1").await;
    publish(app, &owner).await;
    assert_eq!(public_html(app).await, updated);
    pool.close().await;
    assert_eq!(public_html(app).await, updated);
}
