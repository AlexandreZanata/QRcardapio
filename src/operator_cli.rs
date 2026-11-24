//! `qrcardapio operator ...`: audited platform-operator commands for explicit entitlement
//! grants and establishment suspension. Each command requires an existing admin account.

use chrono::Duration;
use clap::Subcommand;

use qrcardapio::application::operator::{self, GrantRequest};
use qrcardapio::config::Config;
use qrcardapio::domain::billing::billing_now;
use qrcardapio::domain::entitlements::{GrantKind, OperationalStatus};
use qrcardapio::infrastructure::database::init_pool;

#[derive(Debug, Subcommand)]
pub enum OperatorCommands {
    /// Grant a module or recurring add-on for a period (trial, beta, internal_preview, courtesy)
    Grant {
        #[arg(long)]
        operator: String,
        #[arg(long)]
        slug: String,
        #[arg(long)]
        offer: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        days: i64,
        #[arg(long)]
        reason: String,
    },
    /// Revoke an explicit grant (other sources of the same capability stay active)
    Revoke {
        #[arg(long)]
        operator: String,
        #[arg(long)]
        slug: String,
        #[arg(long)]
        grant_id: String,
    },
    /// Suspend new protected commercial operations (published menus and data are preserved)
    Suspend {
        #[arg(long)]
        operator: String,
        #[arg(long)]
        slug: String,
        /// `admin` or `security`
        #[arg(long)]
        kind: String,
        #[arg(long)]
        reason: String,
    },
    /// Lift an administrative or security suspension
    Reinstate {
        #[arg(long)]
        operator: String,
        #[arg(long)]
        slug: String,
        #[arg(long)]
        reason: String,
    },
}

pub async fn run(
    config: &Config,
    action: OperatorCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = init_pool(config).await?;
    let outcome = match action {
        OperatorCommands::Grant {
            operator: email,
            slug,
            offer,
            kind,
            days,
            reason,
        } => {
            let kind = GrantKind::parse(&kind)
                .ok_or("kind must be trial, beta, internal_preview or courtesy")?;
            let starts_on = billing_now().date_naive();
            let id = operator::grant(
                &pool,
                GrantRequest {
                    operator_email: &email,
                    establishment_slug: &slug,
                    offer_code: &offer,
                    kind,
                    starts_on,
                    ends_on: starts_on + Duration::days(days),
                    reason: &reason,
                },
            )
            .await;
            id.map(|id| format!("Grant {id} created for {slug}."))
        }
        OperatorCommands::Revoke {
            operator: email,
            slug,
            grant_id,
        } => operator::revoke(&pool, &email, &slug, &grant_id)
            .await
            .map(|()| format!("Grant {grant_id} revoked.")),
        OperatorCommands::Suspend {
            operator: email,
            slug,
            kind,
            reason,
        } => {
            let status = match kind.as_str() {
                "admin" => OperationalStatus::SuspendedAdmin,
                "security" => OperationalStatus::SuspendedSecurity,
                _ => return Err("kind must be admin or security".into()),
            };
            operator::set_operational_status(&pool, &email, &slug, status, &reason)
                .await
                .map(|()| format!("{slug} is now {}.", status.as_str()))
        }
        OperatorCommands::Reinstate {
            operator: email,
            slug,
            reason,
        } => operator::set_operational_status(
            &pool,
            &email,
            &slug,
            OperationalStatus::Active,
            &reason,
        )
        .await
        .map(|()| format!("{slug} is active again.")),
    };
    pool.close().await;
    println!("{}", outcome?);
    Ok(())
}
