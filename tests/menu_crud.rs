use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageEncoder};
use std::io::Cursor;
use tempfile::TempDir;

use qrcardapio::application::auth::register_owner;
use qrcardapio::application::establishments::{
    CreateEstablishmentInput, create_establishment, remove_logo, upload_logo,
};
use qrcardapio::application::menus::{
    CategoryInput, ProductInput, create_category, create_product, delete_category, move_category,
    move_product, toggle_product_availability, update_category, update_product,
};
use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::assets::AssetProcessor;
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::database::queries::{
    find_establishment_for_account, list_categories_for_est, list_products_for_est,
};
use qrcardapio::infrastructure::security::PasswordService;

fn test_config(dir: &TempDir) -> Config {
    let static_dir = dir.path().join("static");
    std::fs::create_dir_all(&static_dir).unwrap();
    Config {
        payments: Default::default(),
        asaas: None,
        mcp_enabled: false,
        google: None,
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
async fn category_product_ordering_images_and_logo_lifecycle() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let pw = PasswordService::new();
    let assets = AssetProcessor::new(cfg.media_dir());

    let auth = register_owner(
        &pool,
        &pw,
        "menu@padaria.com.br",
        "SenhaForte123",
        "SenhaForte123",
        Locale::PtBr,
    )
    .await
    .unwrap();

    let est = create_establishment(
        &pool,
        &auth.account.id,
        CreateEstablishmentInput {
            name: "Padaria Pão Quente",
            slug: "",
            description: "Pães artesanais",
            accent_color: "#D9532F",
        },
    )
    .await
    .unwrap();

    // Upload and replace logo.
    let logo1 = upload_logo(&pool, &assets, &est, png_bytes(256, 256))
        .await
        .unwrap();
    assert!(
        assets
            .asset_file_path(&est.id, &logo1.filename_large)
            .unwrap()
            .exists()
    );
    let est_with_logo = find_establishment_for_account(&pool, &auth.account.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        est_with_logo.logo_asset_id.as_deref(),
        Some(logo1.id.as_str())
    );

    remove_logo(&pool, &assets, &est_with_logo).await.unwrap();
    assert!(
        !assets
            .asset_file_path(&est.id, &logo1.filename_large)
            .unwrap()
            .exists()
    );

    // Create 2 categories and reorder.
    let c1 = create_category(
        &pool,
        &est.id,
        CategoryInput {
            name: "Pães",
            name_en: "Breads",
            name_es: "Panes",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();
    let c2 = create_category(
        &pool,
        &est.id,
        CategoryInput {
            name: "Cafés",
            name_en: "Coffees",
            name_es: "Cafés",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();

    move_category(&pool, &est.id, &c2.id, true).await.unwrap();
    let cats = list_categories_for_est(&pool, &est.id).await.unwrap();
    assert_eq!(cats[0].id, c2.id);
    assert_eq!(cats[1].id, c1.id);

    update_category(
        &pool,
        &est.id,
        &c1.id,
        CategoryInput {
            name: "Pães Artesanais",
            name_en: "Artisan Breads",
            name_es: "Panes Artesanales",
            description: "Fermentação natural",
            description_en: "Sourdough",
            description_es: "Masa madre",
            is_visible: true,
        },
    )
    .await
    .unwrap();

    // Products with image upload, editing, availability toggle, and reordering.
    let p1 = create_product(
        &pool,
        &assets,
        &est.id,
        ProductInput {
            category_id: &c1.id,
            name: "Sourdough",
            name_en: "Sourdough Loaf",
            name_es: "Hogaza de Masa Madre",
            description: "700g",
            description_en: "700g",
            description_es: "700g",
            price_raw: "28,50",
            is_available: true,
            is_visible: true,
        },
        Some(png_bytes(1000, 800)),
    )
    .await
    .unwrap();
    assert_eq!(p1.price_centavos, 2850);
    assert!(p1.image_asset_id.is_some());

    let p2 = create_product(
        &pool,
        &assets,
        &est.id,
        ProductInput {
            category_id: &c1.id,
            name: "Brioche",
            name_en: "Brioche",
            name_es: "Brioche",
            description: "Unidade",
            description_en: "Single",
            description_es: "Unidad",
            price_raw: "12.90",
            is_available: true,
            is_visible: true,
        },
        None,
    )
    .await
    .unwrap();

    move_product(&pool, &est.id, &p2.id, true).await.unwrap();
    let prods = list_products_for_est(&pool, &est.id).await.unwrap();
    assert_eq!(prods[0].id, p2.id);
    assert_eq!(prods[1].id, p1.id);

    let toggled = toggle_product_availability(&pool, &est.id, &p2.id)
        .await
        .unwrap();
    assert!(!toggled);

    update_product(
        &pool,
        &assets,
        &est.id,
        &p1.id,
        ProductInput {
            category_id: &c1.id,
            name: "Sourdough Especial",
            name_en: "Special Sourdough",
            name_es: "Masa Madre Especial",
            description: "Atualizado",
            description_en: "Updated",
            description_es: "Actualizado",
            price_raw: "32,00",
            is_available: true,
            is_visible: true,
        },
        None,
        true, // remove image
    )
    .await
    .unwrap();

    // Deleting category removes contained products and cleans up.
    delete_category(&pool, &assets, &est.id, &c1.id)
        .await
        .unwrap();
    assert_eq!(
        list_products_for_est(&pool, &est.id).await.unwrap().len(),
        0
    );

    pool.close().await;
}

#[tokio::test]
async fn concurrent_owner_edits_succeed_without_sqlite_busy() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let pw = PasswordService::new();
    let assets = AssetProcessor::new(cfg.media_dir());

    let mut est_ids = Vec::new();
    for i in 0..8 {
        let auth = register_owner(
            &pool,
            &pw,
            &format!("conc{i}@cafe.com.br"),
            "SenhaForte123",
            "SenhaForte123",
            Locale::PtBr,
        )
        .await
        .unwrap();
        let est = create_establishment(
            &pool,
            &auth.account.id,
            CreateEstablishmentInput {
                name: &format!("Cafe {i}"),
                slug: &format!("cafe-{i}"),
                description: "",
                accent_color: "#D9532F",
            },
        )
        .await
        .unwrap();
        est_ids.push(est.id);
    }

    let mut handles = Vec::new();
    for est_id in est_ids {
        let p = pool.clone();
        let a = assets.clone();
        handles.push(tokio::spawn(async move {
            let cat = create_category(
                &p,
                &est_id,
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
            for j in 0..3 {
                create_product(
                    &p,
                    &a,
                    &est_id,
                    ProductInput {
                        category_id: &cat.id,
                        name: &format!("Item {j}"),
                        name_en: "",
                        name_es: "",
                        description: "",
                        description_en: "",
                        description_es: "",
                        price_raw: "15,00",
                        is_available: true,
                        is_visible: true,
                    },
                    None,
                )
                .await
                .unwrap();
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    pool.close().await;
}

#[tokio::test]
async fn editor_paginates_products_in_category_order_and_keeps_empty_categories() {
    use qrcardapio::infrastructure::database::queries::menu_editor::list_page;

    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let assets = AssetProcessor::new(cfg.media_dir());
    let owner = register_owner(
        &pool,
        &PasswordService::new(),
        "editor-pages@example.invalid",
        "SenhaForte123",
        "SenhaForte123",
        Locale::PtBr,
    )
    .await
    .unwrap();
    let est = create_establishment(
        &pool,
        &owner.account.id,
        CreateEstablishmentInput {
            name: "Padaria das Páginas",
            slug: "",
            description: "",
            accent_color: "#D9532F",
        },
    )
    .await
    .unwrap();

    let empty_page = list_page(&pool, &est.id, 2).await.unwrap();
    assert_eq!(empty_page.current, 1);
    assert!(empty_page.rows.is_empty());

    let mut category_ids = Vec::new();
    for name in ["Lanches", "Bebidas"] {
        let category = create_category(
            &pool,
            &est.id,
            CategoryInput {
                name,
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
        category_ids.push(category.id);
    }

    for index in 1..=13 {
        create_product(
            &pool,
            &assets,
            &est.id,
            ProductInput {
                category_id: &category_ids[0],
                name: &format!("Lanche {index}"),
                name_en: "",
                name_es: "",
                description: "",
                description_en: "",
                description_es: "",
                price_raw: "12,00",
                is_available: index != 13,
                is_visible: index != 13,
            },
            None,
        )
        .await
        .unwrap();
    }

    let first = list_page(&pool, &est.id, 0).await.unwrap();
    assert_eq!((first.current, first.pages, first.rows.len()), (1, 2, 12));
    assert_eq!(first.rows[0].product_name, "Lanche 1");
    assert_eq!(first.rows[11].product_name, "Lanche 12");

    let last = list_page(&pool, &est.id, i64::MAX).await.unwrap();
    assert_eq!((last.current, last.pages, last.rows.len()), (2, 2, 2));
    assert_eq!(last.rows[0].category_id, category_ids[0]);
    assert_eq!(last.rows[0].product_name, "Lanche 13");
    assert!(!last.rows[0].is_available);
    assert!(!last.rows[0].is_visible);
    assert_eq!(last.rows[1].category_id, category_ids[1]);
    assert!(last.rows[1].product_id.is_none());

    let outside_tenant = list_page(&pool, "other-establishment", 1).await.unwrap();
    assert!(outside_tenant.rows.is_empty());
}
