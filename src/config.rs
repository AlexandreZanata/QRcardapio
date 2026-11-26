use std::net::SocketAddr;
use std::path::PathBuf;

mod billing;
mod google;
mod mcp;
mod payments;
pub use billing::{AsaasConfig, AsaasEnvironment};
pub use payments::{InterConfig, PaymentsConfig, StripeConfig};

/// Billing environment shared by every payment provider (`sandbox` or `production`).
pub type BillingEnvironment = AsaasEnvironment;
pub use google::{CALLBACK_PATH as GOOGLE_CALLBACK_PATH, GoogleConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbSynchronous {
    Full,
    Normal,
}

impl DbSynchronous {
    pub fn parse(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("NORMAL") {
            Self::Normal
        } else {
            Self::Full
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
}

impl LogFormat {
    pub fn parse(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("json") {
            Self::Json
        } else {
            Self::Pretty
        }
    }
}

/// Centralized application configuration loaded from environment variables or injected in tests.
#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub public_base_url: String,
    pub data_dir: PathBuf,
    pub static_dir: PathBuf,
    pub db_max_connections: u32,
    pub db_synchronous: DbSynchronous,
    pub cookie_secure: bool,
    pub trust_proxy: bool,
    pub log_format: LogFormat,
    pub log_level: String,
    pub asaas: Option<AsaasConfig>,
    /// Stripe, Inter and payment-method routing (`Default` = not configured).
    pub payments: PaymentsConfig,
    pub mcp_enabled: bool,
    pub google: Option<GoogleConfig>,
}

impl Config {
    /// Load configuration from environment variables (with `.env` fallback if present).
    pub fn from_env() -> Result<Self, String> {
        let _ = dotenvy::dotenv();

        let bind_raw = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let bind_addr: SocketAddr = bind_raw
            .parse()
            .map_err(|e| format!("invalid BIND_ADDR '{bind_raw}': {e}"))?;

        let public_base_url = std::env::var("PUBLIC_BASE_URL")
            .unwrap_or_else(|_| format!("http://{bind_addr}"))
            .trim_end_matches('/')
            .to_string();

        let data_dir = PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into()));
        let static_dir =
            PathBuf::from(std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into()));

        let db_max_connections = std::env::var("DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .map(|v| v.clamp(1, 32))
            .unwrap_or(5);

        let db_synchronous = std::env::var("DB_SYNCHRONOUS")
            .map(|v| DbSynchronous::parse(&v))
            .unwrap_or(DbSynchronous::Full);

        let cookie_secure = parse_bool_env("COOKIE_SECURE", false);
        let trust_proxy = parse_bool_env("TRUST_PROXY", false);
        let log_format = std::env::var("LOG_FORMAT")
            .map(|v| LogFormat::parse(&v))
            .unwrap_or(LogFormat::Pretty);
        let log_level =
            std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info,qrcardapio=debug".to_string());
        let asaas = AsaasConfig::from_env(&public_base_url, cookie_secure)?;
        let payments = PaymentsConfig::from_env(asaas.as_ref(), cookie_secure)?;
        let google = GoogleConfig::from_env(&public_base_url, cookie_secure)?;
        let mcp_enabled = parse_bool_env("MCP_ENABLED", false);
        if mcp_enabled {
            mcp::validate_origin(&public_base_url, cookie_secure)?;
        }

        Ok(Self {
            bind_addr,
            public_base_url,
            data_dir,
            static_dir,
            db_max_connections,
            db_synchronous,
            cookie_secure,
            trust_proxy,
            log_format,
            log_level,
            asaas,
            payments,
            mcp_enabled,
            google,
        })
    }

    pub fn db_dir(&self) -> PathBuf {
        self.data_dir.join("db")
    }

    pub fn db_path(&self) -> PathBuf {
        self.db_dir().join("qrcardapio.sqlite")
    }

    pub fn media_dir(&self) -> PathBuf {
        self.data_dir.join("media")
    }

    pub fn published_dir(&self) -> PathBuf {
        self.data_dir.join("published")
    }

    pub fn published_tmp_dir(&self) -> PathBuf {
        self.published_dir().join(".tmp")
    }

    pub fn storefronts_dir(&self) -> PathBuf {
        self.data_dir.join("storefronts")
    }

    pub fn storefronts_tmp_dir(&self) -> PathBuf {
        self.storefronts_dir().join(".tmp")
    }

    pub fn public_menu_url(&self, slug: &str) -> String {
        format!("{}/c/{slug}", self.public_base_url.trim_end_matches('/'))
    }

    pub fn public_storefront_url(&self, slug: &str) -> String {
        format!("{}/s/{slug}/", self.public_base_url.trim_end_matches('/'))
    }
}

fn parse_bool_env(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"),
        Err(_) => default,
    }
}
