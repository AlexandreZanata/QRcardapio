use std::path::PathBuf;

use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::domain::commerce::catalog_options::{AddonGroup, AddonOption, ProductVariation};
use sqlx::SqlitePool;
use tempfile::TempDir;

pub(super) const NOW: &str = "2026-11-25T15:00:00Z";

pub(super) fn config(dir: &TempDir) -> Config {
    Config {
        payments: Default::default(),
        asaas: None,
        mcp_enabled: false,
        google: None,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "https://qrcardapio.com.br".into(),
        data_dir: dir.path().join("data"),
        static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
        db_max_connections: 4,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: false,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".into(),
    }
}

pub(super) async fn seed(pool: &SqlitePool) {
    sqlx::raw_sql(
        "INSERT INTO establishments (id, slug, name, created_at, updated_at)
         VALUES ('a', 'commerce-a', 'Tenant A', 'seed', 'seed'),
                ('b', 'commerce-b', 'Tenant B', 'seed', 'seed');
         INSERT INTO categories (id, establishment_id, name, position, is_visible, created_at, updated_at)
         VALUES ('a-cat', 'a', 'Combos', 0, 1, 'seed', 'seed'),
                ('a-secret', 'a', 'Categoria oculta', 1, 0, 'seed', 'seed'),
                ('a-empty', 'a', 'Categoria sem itens', 2, 1, 'seed', 'seed'),
                ('b-cat', 'b', 'Entradas', 0, 1, 'seed', 'seed');
         INSERT INTO products (id, establishment_id, category_id, name, price_centavos, position,
                               is_visible, is_available, created_at, updated_at)
         VALUES ('a-product', 'a', 'a-cat', 'Sanduíche', 1500, 0, 1, 1, 'seed', 'seed'),
                ('a-soldout', 'a', 'a-cat', 'Suco esgotado', 900, 1, 1, 0, 'seed', 'seed'),
                ('a-invisible', 'a', 'a-cat', 'Produto oculto', 800, 2, 0, 1, 'seed', 'seed'),
                ('a-hidden-cat', 'a', 'a-secret', 'Produto na categoria oculta', 500, 0, 1, 1, 'seed', 'seed'),
                ('b-product', 'b', 'b-cat', 'Prato do outro estabelecimento', 2000, 0, 1, 1, 'seed', 'seed');",
    ).execute(pool).await.unwrap();
}

pub(super) fn variation() -> ProductVariation {
    ProductVariation {
        id: "v".into(),
        name: "Grande".into(),
        name_en: "Large".into(),
        name_es: "Grande".into(),
        price_centavos: 1800,
        position: 0,
        is_available: true,
    }
}

pub(super) fn group(id: &str) -> AddonGroup {
    AddonGroup {
        id: id.into(),
        name: "Molhos".into(),
        name_en: "Sauces".into(),
        name_es: "Salsas".into(),
        min_select: 0,
        max_select: 2,
        position: 0,
    }
}

pub(super) fn option() -> AddonOption {
    AddonOption {
        id: "o".into(),
        name: "Mostarda".into(),
        name_en: "Mustard".into(),
        name_es: "Mostaza".into(),
        price_centavos: 100,
        position: 0,
        max_quantity: 1,
        is_available: true,
    }
}
