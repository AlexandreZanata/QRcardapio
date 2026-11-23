use tempfile::TempDir;

use qrcardapio::application::auth::{
    authenticate_session, create_password_reset_token, login, logout, redeem_password_reset,
    register_owner,
};
use qrcardapio::application::establishments::{
    CreateEstablishmentInput, UpdateEstablishmentInput, create_establishment, update_profile,
};
use qrcardapio::application::menus::{
    CategoryInput, ProductInput, create_category, create_product, delete_category, delete_product,
    move_category, move_product, toggle_product_availability, update_category, update_product,
};
use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::error::AppError;
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::assets::AssetProcessor;
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::database::queries::{
    find_establishment_for_account, find_media_asset_for_est, record_publication_success,
};
use qrcardapio::infrastructure::security::PasswordService;

fn test_config(dir: &TempDir) -> Config {
    let static_dir = dir.path().join("static");
    std::fs::create_dir_all(&static_dir).unwrap();
    Config {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "http://127.0.0.1:8080".to_string(),
        data_dir: dir.path().join("data"),
        static_dir,
        db_max_connections: 2,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: false,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".to_string(),
    }
}

#[tokio::test]
async fn registration_login_rotation_logout_and_password_reset() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let pw = PasswordService::new();

    let reg = register_owner(
        &pool,
        &pw,
        "owner@cafe.com.br",
        "SenhaForte123",
        "SenhaForte123",
        Locale::PtBr,
    )
    .await
    .unwrap();

    // Duplicate email rejected.
    let dup = register_owner(
        &pool,
        &pw,
        "OWNER@CAFE.COM.BR",
        "SenhaForte123",
        "SenhaForte123",
        Locale::PtBr,
    )
    .await;
    assert!(matches!(dup, Err(AppError::Conflict("error.email_exists"))));

    // Session authenticates properly.
    assert!(
        authenticate_session(&pool, &reg.raw_session_token)
            .await
            .unwrap()
            .is_some()
    );

    // Wrong password rejected.
    let bad_login = login(&pool, &pw, "owner@cafe.com.br", "WrongPass123").await;
    assert!(matches!(
        bad_login,
        Err(AppError::Unauthorized("error.invalid_credentials"))
    ));

    // Re-login rotates token.
    let login2 = login(&pool, &pw, "owner@cafe.com.br", "SenhaForte123")
        .await
        .unwrap();
    assert_ne!(reg.raw_session_token, login2.raw_session_token);
    assert_ne!(reg.csrf_token, login2.csrf_token);

    // Logout removes only that session.
    logout(&pool, &reg.raw_session_token).await.unwrap();
    assert!(
        authenticate_session(&pool, &reg.raw_session_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        authenticate_session(&pool, &login2.raw_session_token)
            .await
            .unwrap()
            .is_some()
    );

    // Password reset token redeems once and invalidates all active sessions.
    let reset_tok = create_password_reset_token(&pool, "owner@cafe.com.br")
        .await
        .unwrap();
    redeem_password_reset(&pool, &pw, &reset_tok, "NovaSenha456", "NovaSenha456")
        .await
        .unwrap();

    // Old session revoked!
    assert!(
        authenticate_session(&pool, &login2.raw_session_token)
            .await
            .unwrap()
            .is_none()
    );
    // Token cannot be reused.
    let reuse = redeem_password_reset(&pool, &pw, &reset_tok, "Outra7890", "Outra7890").await;
    assert!(reuse.is_err());

    // New password logs in.
    assert!(
        login(&pool, &pw, "owner@cafe.com.br", "NovaSenha456")
            .await
            .is_ok()
    );

    pool.close().await;
}

#[tokio::test]
async fn strict_multi_tenant_isolation_and_slug_locking() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let pw = PasswordService::new();
    let assets = AssetProcessor::new(cfg.media_dir());

    let owner_a = register_owner(
        &pool,
        &pw,
        "a@rest.com.br",
        "SenhaForte123",
        "SenhaForte123",
        Locale::PtBr,
    )
    .await
    .unwrap();
    let owner_b = register_owner(
        &pool,
        &pw,
        "b@rest.com.br",
        "SenhaForte123",
        "SenhaForte123",
        Locale::En,
    )
    .await
    .unwrap();

    // Reserved slug rejected.
    let res_slug = create_establishment(
        &pool,
        &owner_a.account.id,
        CreateEstablishmentInput {
            name: "Admin Cafe",
            slug: "admin",
            description: "",
            accent_color: "#D9532F",
        },
    )
    .await;
    assert!(matches!(
        res_slug,
        Err(AppError::Validation("error.slug_reserved"))
    ));

    let est_a = create_establishment(
        &pool,
        &owner_a.account.id,
        CreateEstablishmentInput {
            name: "Café da Praça",
            slug: "",
            description: "Cafés especiais",
            accent_color: "#D9532F",
        },
    )
    .await
    .unwrap();
    assert_eq!(est_a.slug, "cafe-da-praca");

    // Duplicate slug rejected for Tenant B.
    let dup_slug = create_establishment(
        &pool,
        &owner_b.account.id,
        CreateEstablishmentInput {
            name: "Outro Café",
            slug: "cafe-da-praca",
            description: "",
            accent_color: "#D9532F",
        },
    )
    .await;
    assert!(matches!(
        dup_slug,
        Err(AppError::Conflict("error.slug_taken"))
    ));

    let est_b = create_establishment(
        &pool,
        &owner_b.account.id,
        CreateEstablishmentInput {
            name: "Burger House",
            slug: "burger-house",
            description: "Artisan burgers",
            accent_color: "#1E40AF",
        },
    )
    .await
    .unwrap();

    // Tenant A creates category + product.
    let cat_a = create_category(
        &pool,
        &est_a.id,
        CategoryInput {
            name: "Bebidas",
            name_en: "Drinks",
            name_es: "Bebidas",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();

    let prod_a = create_product(
        &pool,
        &assets,
        &est_a.id,
        ProductInput {
            category_id: &cat_a.id,
            name: "Espresso",
            name_en: "Espresso",
            name_es: "Espresso",
            description: "Duplo",
            description_en: "Double",
            description_es: "Doble",
            price_raw: "9,50",
            is_available: true,
            is_visible: true,
        },
        None,
    )
    .await
    .unwrap();

    // Cross-tenant isolation checks: Owner B tries every operation on Tenant A IDs!
    assert_eq!(
        find_establishment_for_account(&pool, &owner_b.account.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        est_b.id
    );

    assert!(
        update_category(
            &pool,
            &est_b.id,
            &cat_a.id,
            CategoryInput {
                name: "Hacked",
                name_en: "",
                name_es: "",
                description: "",
                description_en: "",
                description_es: "",
                is_visible: true,
            },
        )
        .await
        .is_err()
    );
    assert!(
        move_category(&pool, &est_b.id, &cat_a.id, true)
            .await
            .is_err()
    );
    assert!(
        delete_category(&pool, &assets, &est_b.id, &cat_a.id)
            .await
            .is_err()
    );

    // Owner B cannot create product inside Owner A's category!
    assert!(
        create_product(
            &pool,
            &assets,
            &est_b.id,
            ProductInput {
                category_id: &cat_a.id,
                name: "Injected",
                name_en: "",
                name_es: "",
                description: "",
                description_en: "",
                description_es: "",
                price_raw: "10,00",
                is_available: true,
                is_visible: true,
            },
            None,
        )
        .await
        .is_err()
    );

    // Owner B cannot update, toggle, move, or delete Owner A's product.
    assert!(
        update_product(
            &pool,
            &assets,
            &est_b.id,
            &prod_a.id,
            ProductInput {
                category_id: &cat_a.id,
                name: "Hacked",
                name_en: "",
                name_es: "",
                description: "",
                description_en: "",
                description_es: "",
                price_raw: "1,00",
                is_available: true,
                is_visible: true,
            },
            None,
            false,
        )
        .await
        .is_err()
    );
    assert!(
        toggle_product_availability(&pool, &est_b.id, &prod_a.id)
            .await
            .is_err()
    );
    assert!(
        move_product(&pool, &est_b.id, &prod_a.id, true)
            .await
            .is_err()
    );
    assert!(
        delete_product(&pool, &assets, &est_b.id, &prod_a.id)
            .await
            .is_err()
    );
    assert!(
        find_media_asset_for_est(&pool, &est_b.id, "nonexistent")
            .await
            .unwrap()
            .is_none()
    );

    // Slug can be updated before first publication, then locked after publication.
    let est_a_updated = update_profile(
        &pool,
        &est_a,
        UpdateEstablishmentInput {
            name: "Café da Praça Central",
            slug: "cafe-central",
            description: "Nova desc",
            description_en: "New desc",
            description_es: "Nueva desc",
            accent_color: "#D9532F",
            google_maps_url: None,
            google_review_url: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(est_a_updated.slug, "cafe-central");

    record_publication_success(&pool, &est_a.id, 1, 2048, 1, 15, "2026-11-23T15:00:00Z")
        .await
        .unwrap();
    let est_a_pub = find_establishment_for_account(&pool, &owner_a.account.id)
        .await
        .unwrap()
        .unwrap();

    let locked = update_profile(
        &pool,
        &est_a_pub,
        UpdateEstablishmentInput {
            name: "Café da Praça Central",
            slug: "novo-slug-proibido",
            description: "Nova desc",
            description_en: "",
            description_es: "",
            accent_color: "#D9532F",
            google_maps_url: None,
            google_review_url: None,
        },
    )
    .await;
    assert!(matches!(
        locked,
        Err(AppError::Validation("error.slug_locked"))
    ));

    pool.close().await;
}
