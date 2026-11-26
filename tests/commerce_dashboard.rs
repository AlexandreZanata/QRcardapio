#[path = "commerce/order_fixtures.rs"]
mod fixture;
#[path = "commercial/support.rs"]
mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use qrcardapio::application::demo_menu::seed_public_demo;
use qrcardapio::domain::commerce::business_hours::ServiceMode;
use qrcardapio::infrastructure::database::queries::{
    addon_groups, addon_options, business_hours, product_variations,
};
use support::{config, get, harness, location, manager, owner, post_form};

#[tokio::test]
async fn owners_and_managers_edit_options_with_csrf_and_tenant_checks() {
    let h = harness(false).await;
    let (login, est) = owner(&h.pool, "catalog-editor-a").await;
    let (outsider, outsider_est) = owner(&h.pool, "catalog-editor-b").await;
    let manager = manager(&h.pool, &est, "catalog-editor-manager@example.test").await;
    let seeded = fixture::catalog(&h, &est, fixture::at("2026-11-25T15:00:00Z")).await;
    let product = &seeded.items[0].product_id;
    let path = format!("/dashboard/products/{product}/options");
    let fields = "name=Pequeno&name_en=Small&name_es=Pequeno&price=15%2C50&position=0&available=1";
    assert_eq!(get(&h.app, &login, &path).await.status(), StatusCode::OK);
    assert_eq!(
        get(&h.app, &outsider, &path).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post_form(&h.app, &manager, &path, format!("_csrf=wrong&{fields}"))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post_form(
            &h.app,
            &outsider,
            &path,
            format!("_csrf={}&{fields}", outsider.csrf_token)
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post_form(
            &h.app,
            &manager,
            &path,
            format!("_csrf={}&{fields}", manager.csrf_token)
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    let variations = product_variations::list_for_product(&h.pool, &est, product)
        .await
        .unwrap();
    let small = variations.iter().find(|v| v.name_en == "Small").unwrap();
    assert_eq!(small.price_centavos, 1550);
    let update = format!(
        "_csrf={}&id={}&name=Pequeno&price=16%2C00&position=9",
        login.csrf_token, small.id
    );
    assert_eq!(
        post_form(&h.app, &login, &path, update).await.status(),
        StatusCode::SEE_OTHER
    );
    let variations = product_variations::list_for_product(&h.pool, &est, product)
        .await
        .unwrap();
    assert!(variations.iter().any(|v| v.id == small.id
        && !v.is_available
        && v.price_centavos == 1600
        && v.position == 9));

    let response = post_form(
        &h.app,
        &login,
        "/dashboard/menu/addons",
        format!(
            "_csrf={}&name=Molhos&position=1&min_select=0&max_select=2",
            login.csrf_token
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let group_page = location(&response);
    let group = group_page.rsplit('/').next().unwrap();
    let option_path = format!("{group_page}/options");
    let fields = "name=Mostarda&price=0&position=0&max_quantity=1&available=1";
    assert_eq!(
        post_form(
            &h.app,
            &outsider,
            &option_path,
            format!("_csrf={}&{fields}", outsider.csrf_token)
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post_form(
            &h.app,
            &manager,
            &option_path,
            format!("_csrf={}&{fields}", manager.csrf_token)
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        get(&h.app, &manager, &group_page).await.status(),
        StatusCode::OK
    );
    let link_path = format!("/dashboard/products/{product}/addon-groups");
    assert_eq!(
        post_form(
            &h.app,
            &manager,
            &link_path,
            format!("_csrf={}&group_id={group}&position=1", manager.csrf_token)
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert!(
        addon_groups::linked_ids(&h.pool, &est, product)
            .await
            .unwrap()
            .contains(&group.to_owned())
    );
    assert!(
        addon_groups::linked_ids(&h.pool, &outsider_est, product)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        post_form(
            &h.app,
            &manager,
            &link_path,
            format!("_csrf={}&group_id={group}", manager.csrf_token)
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert!(
        !addon_groups::linked_ids(&h.pool, &est, product)
            .await
            .unwrap()
            .contains(&group.to_owned())
    );
    assert_eq!(
        addon_options::list_for_group(&h.pool, &est, group)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        post_form(
            &h.app,
            &login,
            &format!("{group_page}/delete"),
            format!("_csrf={}", login.csrf_token)
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert!(
        addon_options::list_for_group(&h.pool, &est, group)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn hours_dashboard_persists_modes_exceptions_and_tenant_pause() {
    let h = harness(false).await;
    let (login, est) = owner(&h.pool, "hours-owner-a").await;
    let (_, other) = owner(&h.pool, "hours-owner-b").await;
    let settings = format!(
        "_csrf={}&time_zone=America%2FCuiaba&pickup=1&minimum=10%2C00&acceptance_minutes=9",
        login.csrf_token
    );
    assert_eq!(
        post_form(&h.app, &login, "/dashboard/hours", settings)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let week = format!(
        "_csrf={}&mon=18%3A00-02%3A00&tue=&wed=&thu=&fri=&sat=&sun=",
        login.csrf_token
    );
    assert_eq!(
        post_form(&h.app, &login, "/dashboard/hours/week", week)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let response = get(&h.app, &login, "/dashboard/hours").await;
    assert_eq!(response.status(), StatusCode::OK);
    let schedule = business_hours::load(&h.pool, &est).await.unwrap();
    let instant = fixture::at("2026-11-24T05:00:00Z");
    assert_eq!(
        (
            schedule.settings.minimum_centavos,
            schedule.settings.acceptance_minutes
        ),
        (1000, 9)
    );
    assert!(schedule.service_at(instant, ServiceMode::Pickup).is_some());
    assert!(
        schedule
            .service_at(instant, ServiceMode::Delivery)
            .is_none()
    );
    assert!(
        business_hours::load(&h.pool, &other)
            .await
            .unwrap()
            .service_at(instant, ServiceMode::Pickup)
            .is_none()
    );
    let exception = format!("_csrf={}&date=2026-11-24&windows=", login.csrf_token);
    assert_eq!(
        post_form(&h.app, &login, "/dashboard/hours/exceptions", exception)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    assert!(
        business_hours::load(&h.pool, &est)
            .await
            .unwrap()
            .service_at(instant, ServiceMode::Pickup)
            .is_none()
    );
    let pause = format!("_csrf={}&duration=indefinite", login.csrf_token);
    assert_eq!(
        post_form(&h.app, &login, "/dashboard/hours/pause", pause)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    assert!(
        business_hours::load(&h.pool, &est)
            .await
            .unwrap()
            .settings
            .paused_at(instant)
    );
}

#[tokio::test]
async fn landing_button_keeps_demo_qr_and_demo_has_translated_read_only_customizations() {
    let h = harness(false).await;
    seed_public_demo(&h.pool, &config(&h.dir, false))
        .await
        .unwrap();
    async fn html(app: &axum::Router, path: &str) -> String {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("script-src 'self'")
        );
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
    for (path, label) in [
        ("/", "Explore o cardápio de demonstração"),
        ("/en", "Explore the sample menu"),
        ("/es", "Explora el menú de demostración"),
    ] {
        let landing = html(&h.app, path).await;
        assert!(landing.contains("landing-demo-qr.svg"));
        let (before_steps, _) = landing.split_once("id=\"how-it-works\"").unwrap();
        let (_, full_width_demo) = before_steps
            .split_once("class=\"landing-container demo-invitation-band\"")
            .unwrap();
        assert!(full_width_demo.contains("demo-invitation-action"));
        assert!(full_width_demo.contains("landing-demo-banner-mobile.webp"));
        assert!(full_width_demo.contains("landing-demo-banner-wide.webp"));
        assert!(landing.contains(label));
        assert!(landing.contains("href=\"/c/bistro-da-praca\""));
    }
    let menu = html(&h.app, "/c/bistro-da-praca").await;
    assert!(!menu.contains("data-price-centavos=\"350\""));
    let product_url = menu
        .split_once("id=\"item-classico\" href=\"")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap();
    let product = html(&h.app, product_url).await;
    assert!(product.contains("Ingredientes que podem ser retirados"));
    assert!(product.contains("Extras opcionales"));
    assert!(product.contains("Extra cheddar"));
    assert!(product.contains("data-price-centavos=\"350\""));
    assert!(product.contains("href=\"/c/bistro-da-praca#cat-"));
    assert!(!menu.contains("<form"));
    assert!(!product.contains("<form"));
}
