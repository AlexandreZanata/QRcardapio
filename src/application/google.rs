use chrono::Utc;
use reqwest::Url;

use crate::domain::billing::BillingPlan;
use crate::error::AppResult;
use crate::infrastructure::database::queries::google::{self as queries, PendingGoogleFlow};
use crate::infrastructure::google::GoogleOidc;
use crate::infrastructure::security::{generate_token_32, sha256_hex};
use sqlx::SqlitePool;

pub enum Intent<'a> {
    SignIn,
    Plan(BillingPlan),
    Mcp(&'a str),
    Link(&'a str),
}

pub async fn begin(
    pool: &SqlitePool,
    provider: &GoogleOidc,
    browser: &str,
    intent: Intent<'_>,
) -> AppResult<Url> {
    let state = generate_token_32()?;
    let nonce = generate_token_32()?;
    let verifier = generate_token_32()?;
    let (purpose, plan_key, mcp_flow, session_id) = match intent {
        Intent::SignIn => ("signin", None, None, None),
        Intent::Plan(plan) => ("plan", Some(plan.key().to_owned()), None, None),
        Intent::Mcp(flow) => ("mcp", None, Some(flow.to_owned()), None),
        Intent::Link(session) => ("link", None, None, Some(session.to_owned())),
    };
    let config = provider.config();
    let flow = PendingGoogleFlow {
        purpose: purpose.into(),
        plan_key,
        mcp_flow_id: mcp_flow,
        local_session_id: session_id,
        nonce_hash: sha256_hex(&nonce),
        code_verifier: verifier.clone(),
    };
    queries::save_flow(
        pool,
        &sha256_hex(&state),
        &sha256_hex(browser),
        &flow,
        &config.client_id,
        &config.redirect_uri,
        Utc::now().timestamp() + 600,
    )
    .await?;
    Ok(provider.authorization_url(&state, &nonce, &verifier, purpose == "link"))
}

pub async fn take(
    pool: &SqlitePool,
    provider: &GoogleOidc,
    state: &str,
    browser: &str,
) -> AppResult<Option<PendingGoogleFlow>> {
    let config = provider.config();
    queries::take_flow(
        pool,
        &sha256_hex(state),
        &sha256_hex(browser),
        &config.client_id,
        &config.redirect_uri,
        Utc::now().timestamp(),
    )
    .await
}
