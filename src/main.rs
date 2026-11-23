use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::application::admin::bootstrap_admin;
use qrcardapio::application::auth::{cleanup_expired_sessions, create_password_reset_token};
use qrcardapio::application::bench_gen::generate_benchmark_dataset;
use qrcardapio::application::publishing::{PublishLocks, republish_all};
use qrcardapio::application::seed::seed_demo_establishment;
use qrcardapio::config::{Config, LogFormat};
use qrcardapio::infrastructure::database::{backup_sqlite_into, init_pool, restore_sqlite_from};
use qrcardapio::infrastructure::security::PasswordService;

#[derive(Debug, Parser)]
#[command(
    name = "qrcardapio",
    about = "QR Cardapio SaaS server and operations CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start the HTTP web server (default if no subcommand is given)
    Serve,
    /// Database maintenance commands
    Db {
        #[command(subcommand)]
        action: DbCommands,
    },
    /// Operational administrator commands
    Admin {
        #[command(subcommand)]
        action: AdminCommands,
    },
    /// Seed a deterministic demo restaurant (generates random password)
    Seed {
        #[arg(long)]
        password: Option<String>,
    },
    /// Republish all currently published menus (useful after template upgrades)
    RepublishAll,
    /// Generate multi-tenant synthetic data for reproducible benchmarks
    BenchGen {
        #[arg(long, default_value_t = 100)]
        establishments: usize,
        #[arg(long, default_value_t = 12)]
        items_per_establishment: usize,
        #[arg(long, default_value_t = 5)]
        publish_sample: usize,
    },
}

#[derive(Debug, Subcommand)]
enum DbCommands {
    /// Run pending SQLite migrations
    Migrate,
    /// Create a consistent online SQLite backup via VACUUM INTO
    Backup {
        #[arg(long)]
        output: PathBuf,
    },
    /// Restore a verified SQLite backup file (run while server is stopped)
    Restore {
        #[arg(long)]
        input: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum AdminCommands {
    /// Bootstrap an operational administrator account
    Create {
        #[arg(long)]
        email: String,
        #[arg(long)]
        password: String,
    },
    /// Issue a one-time 1-hour password recovery token for an account
    ResetToken {
        #[arg(long)]
        email: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    init_tracing(&config);

    let cli = Cli::parse();
    match cli.command.unwrap_or(Commands::Serve) {
        Commands::Serve => run_server(config).await?,
        Commands::Db { action } => match action {
            DbCommands::Migrate => {
                let pool = init_pool(&config).await?;
                pool.close().await;
                tracing::info!("database migrations applied successfully");
            }
            DbCommands::Backup { output } => {
                let pool = init_pool(&config).await?;
                backup_sqlite_into(&pool, &output).await?;
                pool.close().await;
                tracing::info!(output = %output.display(), "database backup completed and verified");
            }
            DbCommands::Restore { input } => {
                restore_sqlite_from(&input, &config.db_path()).await?;
                tracing::info!(input = %input.display(), "database restore completed");
            }
        },
        Commands::Admin { action } => match action {
            AdminCommands::Create { email, password } => {
                let pool = init_pool(&config).await?;
                let passwords = PasswordService::new();
                let acc = bootstrap_admin(&pool, &passwords, &email, &password).await?;
                pool.close().await;
                println!("Admin account created: {} ({})", acc.email, acc.id);
            }
            AdminCommands::ResetToken { email } => {
                let pool = init_pool(&config).await?;
                let token = create_password_reset_token(&pool, &email).await?;
                pool.close().await;
                println!("One-time recovery token (valid 1 hour): {token}");
            }
        },
        Commands::Seed { password } => {
            let pool = init_pool(&config).await?;
            let passwords = PasswordService::new();
            let locks = PublishLocks::default();
            let out = seed_demo_establishment(&pool, &config, &passwords, &locks, password).await?;
            pool.close().await;
            println!(
                "Seeded '{}' -> {} (owner: {}, password: {})",
                out.slug, out.public_url, out.email, out.password
            );
        }
        Commands::RepublishAll => {
            let pool = init_pool(&config).await?;
            let locks = PublishLocks::default();
            let count = republish_all(&pool, &config, &locks).await?;
            pool.close().await;
            println!("Republished {count} menu snapshots.");
        }
        Commands::BenchGen {
            establishments,
            items_per_establishment,
            publish_sample,
        } => {
            let pool = init_pool(&config).await?;
            let locks = PublishLocks::default();
            let stats = generate_benchmark_dataset(
                &pool,
                &config,
                &locks,
                establishments,
                items_per_establishment,
                publish_sample,
            )
            .await?;
            pool.close().await;
            println!(
                "Generated {} establishments ({} categories, {} products), published {} snapshots in {} ms. First slug: {}. SQLite size: {} bytes",
                stats.establishments_created,
                stats.categories_created,
                stats.products_created,
                stats.menus_published,
                stats.elapsed_ms,
                stats.first_slug.as_deref().unwrap_or("none"),
                stats.db_bytes
            );
        }
    }

    Ok(())
}

async fn run_server(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let pool = init_pool(&config).await?;
    let bind_addr = config.bind_addr;
    let state = AppState::new(config, pool.clone());

    let maint_pool = pool.clone();
    let mcp_enabled = state.config.mcp_enabled;
    let google_enabled = state.google.is_some();
    let maint_limiters = state.rate_limiters.clone();
    let maintenance = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let _ = cleanup_expired_sessions(&maint_pool).await;
            if mcp_enabled
                && let Err(error) =
                    qrcardapio::infrastructure::database::queries::mcp_oauth::cleanup(
                        &maint_pool,
                        chrono::Utc::now().timestamp(),
                    )
                    .await
            {
                tracing::warn!(%error, "MCP cleanup failed");
            }
            if google_enabled
                && let Err(error) =
                    qrcardapio::infrastructure::database::queries::google::prune_flows(
                        &maint_pool,
                        chrono::Utc::now().timestamp(),
                    )
                    .await
            {
                tracing::warn!(%error, "Google OIDC cleanup failed");
            }
            maint_limiters.prune_stale();
        }
    });

    let router = build_router(state);
    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!(addr = %bind_addr, "QR Cardapio server listening");

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    maintenance.abort();
    pool.close().await;
    tracing::info!("server shut down gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

fn init_tracing(config: &Config) {
    let filter = EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    match config.log_format {
        LogFormat::Json => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .json()
                .init();
        }
        LogFormat::Pretty => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .compact()
                .init();
        }
    }
}
