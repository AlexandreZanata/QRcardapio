use sqlx::SqlitePool;

use crate::application::auth::register_owner;
use crate::application::establishments::{CreateEstablishmentInput, create_establishment};
use crate::application::menus::{CategoryInput, ProductInput, create_category, create_product};
use crate::application::publishing::{PublishLocks, publish_establishment};
use crate::config::Config;
use crate::error::AppResult;
use crate::i18n::Locale;
use crate::infrastructure::assets::AssetProcessor;
use crate::infrastructure::database::queries::find_establishment_for_account;
use crate::infrastructure::security::{PasswordService, generate_token_32};

pub struct SeedOutcome {
    pub email: String,
    pub password: String,
    pub slug: String,
    pub public_url: String,
}

/// Seeds a deterministic pilot-style demo establishment (`cafe-da-praca`) with 3 categories
/// and localized products in PT-BR, EN, and ES, generating a random password on stdout.
pub async fn seed_demo_establishment(
    pool: &SqlitePool,
    config: &Config,
    passwords: &PasswordService,
    locks: &PublishLocks,
    custom_password: Option<String>,
) -> AppResult<SeedOutcome> {
    let email = "demo@qrcardapio.com.br".to_string();
    let password = match custom_password {
        Some(p) if !p.is_empty() => p,
        _ => generate_token_32()?.chars().take(16).collect(),
    };

    let auth = register_owner(pool, passwords, &email, &password, &password, Locale::PtBr).await?;
    let est = create_establishment(
        pool,
        &auth.account.id,
        CreateEstablishmentInput {
            name: "Café da Praça",
            slug: "cafe-da-praca",
            description: "Cafés especiais, pães artesanais e doces feitos hoje.",
            accent_color: "#D9532F",
        },
    )
    .await?;

    let assets = AssetProcessor::new(config.media_dir());

    let cat1 = create_category(
        pool,
        &est.id,
        CategoryInput {
            name: "Cafés Especiais",
            name_en: "Specialty Coffee",
            name_es: "Cafés de Especialidad",
            description: "Grãos selecionados da Serra da Mantiqueira",
            description_en: "Single-origin beans from Mantiqueira Highlands",
            description_es: "Granos seleccionados de la Sierra de Mantiqueira",
            is_visible: true,
        },
    )
    .await?;

    let cat2 = create_category(
        pool,
        &est.id,
        CategoryInput {
            name: "Salgados & Pães",
            name_en: "Savory & Breads",
            name_es: "Salados y Panes",
            description: "Assados frescos ao longo do dia",
            description_en: "Baked fresh throughout the day",
            description_es: "Horneados frescos durante todo el día",
            is_visible: true,
        },
    )
    .await?;

    let cat3 = create_category(
        pool,
        &est.id,
        CategoryInput {
            name: "Sobremesas",
            name_en: "Desserts",
            name_es: "Postres",
            description: "Receitas da casa sem conservantes",
            description_en: "House-made desserts with zero preservatives",
            description_es: "Recetas de la casa sin conservantes",
            is_visible: true,
        },
    )
    .await?;

    let items = [
        (
            &cat1.id,
            "Espresso Duplo",
            "Double Espresso",
            "Espresso Doble",
            "Extração encorpada com crema persistente.",
            "Rich double shot with silky crema.",
            "Extracción intensa con crema sedosa.",
            "9,50",
            true,
        ),
        (
            &cat1.id,
            "Cappuccino Cremoso",
            "Velvet Cappuccino",
            "Cappuccino Cremoso",
            "Espresso, leite vaporizado e cacau 70%.",
            "Espresso, steamed milk, and 70% cocoa.",
            "Espresso, leche vaporizada y cacao 70%.",
            "14,90",
            true,
        ),
        (
            &cat1.id,
            "Cold Brew Cítrico",
            "Citrus Cold Brew",
            "Cold Brew Cítrico",
            "Infusão a frio de 16 horas com toque de laranja.",
            "16-hour cold extraction with fresh orange zest.",
            "Infusión en frío de 16 horas con toque de naranja.",
            "16,50",
            false,
        ),
        (
            &cat2.id,
            "Pão de Queijo Artesanal",
            "Artisan Cheese Bread",
            "Pan de Queso Artesanal",
            "Queijo Canastra curado e casquinha crocante.",
            "Made with aged Canastra cheese, crisp outside.",
            "Queso Canastra curado y corteza crujiente.",
            "9,00",
            true,
        ),
        (
            &cat2.id,
            "Tostex de Queijo e Tomate",
            "Grilled Cheese & Tomato Toast",
            "Tostado de Queso y Tomate",
            "Pão sourdough na chapa com manteiga de ervas.",
            "Sourdough grilled with herb butter.",
            "Pan de masa madre con mantequilla de hierbas.",
            "24,90",
            true,
        ),
        (
            &cat3.id,
            "Bolo de Cenoura com Brigadeiro",
            "Carrot Cake with Brigadeiro",
            "Pastel de Zanahoria con Brigadeiro",
            "Fatia generosa servida morna.",
            "Generous slice topped with rich chocolate fudge.",
            "Porción generosa con cobertura de brigadeiro.",
            "17,00",
            true,
        ),
    ];

    for (cat_id, n, nen, nes, d, den, des, price, avail) in items {
        create_product(
            pool,
            &assets,
            &est.id,
            ProductInput {
                category_id: cat_id,
                name: n,
                name_en: nen,
                name_es: nes,
                description: d,
                description_en: den,
                description_es: des,
                price_raw: price,
                is_available: avail,
                is_visible: true,
            },
            None,
        )
        .await?;
    }

    let _ = find_establishment_for_account(pool, &auth.account.id).await?;
    publish_establishment(pool, config, locks, &est.id).await?;

    Ok(SeedOutcome {
        email,
        password,
        public_url: config.public_menu_url(&est.slug),
        slug: est.slug,
    })
}
