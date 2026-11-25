//! `qrcardapio billing ...`: provider operations that need the server's credentials.

use clap::Subcommand;

use qrcardapio::application::billing_inter;
use qrcardapio::application::payments::BillingGateways;
use qrcardapio::config::Config;
use qrcardapio::infrastructure::database::init_pool;

#[derive(Debug, Subcommand)]
pub enum BillingCommands {
    /// Register the Inter Cobrança callback at <base>/webhooks/inter/<INTER_WEBHOOK_TOKEN>
    RegisterInterWebhook {
        /// HTTPS origin Inter calls (defaults to PUBLIC_BASE_URL; use a dedicated mTLS host)
        #[arg(long)]
        base: Option<String>,
    },
    /// Issue due Inter renewal charges and reconcile open charges now (also runs hourly)
    RunInterCycle,
}

pub async fn run(
    config: &Config,
    action: BillingCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let gateways = BillingGateways::from_config(config)?;
    let inter = gateways
        .inter
        .as_ref()
        .ok_or("Inter is not configured (INTER_ENV, INTER_CLIENT_ID, ...)")?;
    match action {
        BillingCommands::RegisterInterWebhook { base } => {
            let base = base
                .unwrap_or_else(|| config.public_base_url.clone())
                .trim_end_matches('/')
                .to_owned();
            if !base.starts_with("https://") {
                return Err("the callback origin must use HTTPS".into());
            }
            let url = format!("{base}/webhooks/inter/{}", inter.config().webhook_token());
            inter
                .register_webhook(&url)
                .await
                .map_err(|error| format!("Inter rejected the webhook: {error}"))?;
            println!("Inter Cobrança webhook registered for {base}/webhooks/inter/<token>.");
        }
        BillingCommands::RunInterCycle => {
            let pool = init_pool(config).await?;
            let (issued, reconciled) = billing_inter::run_cycle(&pool, inter).await?;
            pool.close().await;
            println!("Inter cycle: {issued} charge(s) issued, {reconciled} reconciled.");
        }
    }
    Ok(())
}
