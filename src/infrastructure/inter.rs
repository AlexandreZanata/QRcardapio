//! Banco Inter adapter: Cobrança v3 (boleto with Pix, "BolePix") issued once per cycle.
//! Every call uses mTLS with the integration certificate and a cached OAuth2 token
//! (the token endpoint allows 5 calls per minute). See `docs/architecture/payments/INTER.md`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use chrono::NaiveDate;
use reqwest::header::ACCEPT;
use reqwest::{Client, Identity, Method, RequestBuilder, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::config::{AsaasEnvironment, InterConfig};
use crate::domain::payer::PayerProfile;
use crate::infrastructure::gateway::GatewayError;

mod parse;

pub use parse::{callback_request_codes, valid_request_code};
use parse::{charge_body, parse_snapshot};

const SCOPES: &str = "boleto-cobranca.read boleto-cobranca.write";
const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct InterClient {
    http: Client,
    config: InterConfig,
    origin: String,
    token: Arc<Mutex<Option<(String, Instant)>>>,
}

/// A charge as Inter reports it (authoritative state after any callback).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeSnapshot {
    pub request_code: String,
    pub our_number: Option<String>,
    pub status: String,
    pub due_date: Option<NaiveDate>,
    pub received_centavos: Option<i64>,
    pub status_date: Option<String>,
    pub pix_copy_paste: Option<String>,
    pub digitable_line: Option<String>,
}

pub struct NewCharge<'a> {
    pub our_number: &'a str,
    pub amount_centavos: i64,
    pub due_date: NaiveDate,
    pub payable_days_after_due: i64,
    pub payer: &'a PayerProfile,
    pub message: &'a str,
}

fn origin_for(environment: AsaasEnvironment) -> &'static str {
    match environment {
        AsaasEnvironment::Sandbox => "https://cdpj-sandbox.partners.uatinter.co",
        AsaasEnvironment::Production => "https://cdpj.partners.bancointer.com.br",
    }
}

impl InterClient {
    pub fn new(config: InterConfig) -> Result<Self, String> {
        let builder = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("QRCardapio/", env!("CARGO_PKG_VERSION")));
        let (builder, origin) = match &config.api_origin {
            // Loopback mock (tests): plain HTTP, no client certificate.
            Some(origin) => (builder, origin.clone()),
            None => {
                let identity = Identity::from_pem(&config.identity_pem)
                    .map_err(|_| "INTER_CERT_PATH/INTER_KEY_PATH are not a valid PEM identity")?;
                // Inter's API hosts negotiate TLS 1.2 only.
                let builder = builder
                    .https_only(true)
                    .identity(identity)
                    .min_tls_version(reqwest::tls::Version::TLS_1_2);
                (builder, origin_for(config.environment).to_owned())
            }
        };
        let http = builder
            .build()
            .map_err(|_| "Inter HTTP client configuration failed")?;
        Ok(Self {
            http,
            config,
            origin,
            token: Arc::new(Mutex::new(None)),
        })
    }

    pub fn config(&self) -> &InterConfig {
        &self.config
    }

    async fn bearer(&self) -> Result<String, GatewayError> {
        let mut cached = self.token.lock().await;
        if let Some((token, valid_until)) = cached.as_ref()
            && Instant::now() < *valid_until
        {
            return Ok(token.clone());
        }
        #[derive(Deserialize)]
        struct Token {
            access_token: String,
            expires_in: u64,
        }
        let response = self
            .http
            .post(format!("{}/oauth/v2/token", self.origin))
            .form(&[
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret()),
                ("grant_type", "client_credentials"),
                ("scope", SCOPES),
            ])
            .send()
            .await
            .map_err(|_| GatewayError::Transport)?;
        if !response.status().is_success() {
            return Err(GatewayError::Status(response.status()));
        }
        let token: Token = response.json().await.map_err(|_| GatewayError::Response)?;
        let lifetime = Duration::from_secs(token.expires_in.max(60));
        let valid_until = Instant::now() + lifetime.saturating_sub(TOKEN_REFRESH_MARGIN);
        *cached = Some((token.access_token.clone(), valid_until));
        Ok(token.access_token)
    }

    async fn request(&self, method: Method, path: &str) -> Result<RequestBuilder, GatewayError> {
        let token = self.bearer().await?;
        let mut request = self
            .http
            .request(method, format!("{}/cobranca/v3{path}", self.origin))
            .bearer_auth(token)
            .header(ACCEPT, "application/json");
        if let Some(account) = &self.config.account {
            request = request.header("x-conta-corrente", account);
        }
        Ok(request)
    }

    async fn send(request: RequestBuilder) -> Result<reqwest::Response, GatewayError> {
        let response = request.send().await.map_err(|_| GatewayError::Transport)?;
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(GatewayError::Status(response.status()))
        }
    }

    /// Issues a charge. Emission is asynchronous: only the request code is returned.
    pub async fn create_charge(&self, charge: &NewCharge<'_>) -> Result<String, GatewayError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Created {
            codigo_solicitacao: String,
        }
        let body = charge_body(charge);
        let response =
            Self::send(self.request(Method::POST, "/cobrancas").await?.json(&body)).await?;
        let created: Created = response.json().await.map_err(|_| GatewayError::Response)?;
        valid_request_code(&created.codigo_solicitacao)
            .then_some(created.codigo_solicitacao)
            .ok_or(GatewayError::Response)
    }

    /// Finds a charge by our `seuNumero` (recovery after a timeout on creation).
    pub async fn find_by_our_number(
        &self,
        our_number: &str,
        issued_from: NaiveDate,
        issued_to: NaiveDate,
    ) -> Result<Option<String>, GatewayError> {
        let from = issued_from.to_string();
        let to = issued_to.to_string();
        let response = Self::send(self.request(Method::GET, "/cobrancas").await?.query(&[
            ("dataInicial", from.as_str()),
            ("dataFinal", to.as_str()),
            ("filtrarDataPor", "EMISSAO"),
            ("seuNumero", our_number),
        ]))
        .await?;
        let list: Value = response.json().await.map_err(|_| GatewayError::Response)?;
        Ok(list
            .get("cobrancas")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("cobranca"))
            .filter(|c| c.get("seuNumero").and_then(Value::as_str) == Some(our_number))
            .filter_map(|c| c.get("codigoSolicitacao").and_then(Value::as_str))
            .find(|code| valid_request_code(code))
            .map(str::to_owned))
    }

    pub async fn get_charge(&self, request_code: &str) -> Result<ChargeSnapshot, GatewayError> {
        if !valid_request_code(request_code) {
            return Err(GatewayError::Response);
        }
        let response = Self::send(
            self.request(Method::GET, &format!("/cobrancas/{request_code}"))
                .await?,
        )
        .await?;
        let body: Value = response.json().await.map_err(|_| GatewayError::Response)?;
        parse_snapshot(&body).ok_or(GatewayError::Response)
    }

    /// Requests cancellation (asynchronous at Inter; `CANCELADO` arrives later).
    pub async fn cancel_charge(
        &self,
        request_code: &str,
        reason: &str,
    ) -> Result<(), GatewayError> {
        if !valid_request_code(request_code) {
            return Err(GatewayError::Response);
        }
        let reason: String = reason.chars().take(50).collect();
        let result = Self::send(
            self.request(Method::POST, &format!("/cobrancas/{request_code}/cancelar"))
                .await?
                .json(&json!({ "motivoCancelamento": reason })),
        )
        .await;
        match result {
            Ok(_) | Err(GatewayError::Status(StatusCode::NOT_FOUND)) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Boleto PDF (decoded from Inter's base64 response).
    pub async fn charge_pdf(&self, request_code: &str) -> Result<Vec<u8>, GatewayError> {
        if !valid_request_code(request_code) {
            return Err(GatewayError::Response);
        }
        #[derive(Deserialize)]
        struct Pdf {
            pdf: String,
        }
        let response = Self::send(
            self.request(Method::GET, &format!("/cobrancas/{request_code}/pdf"))
                .await?,
        )
        .await?;
        let pdf: Pdf = response.json().await.map_err(|_| GatewayError::Response)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(pdf.pdf.as_bytes())
            .map_err(|_| GatewayError::Response)?;
        bytes
            .starts_with(b"%PDF")
            .then_some(bytes)
            .ok_or(GatewayError::Response)
    }

    /// Registers (or replaces) the Cobrança callback URL.
    pub async fn register_webhook(&self, url: &str) -> Result<(), GatewayError> {
        if !url.starts_with("https://") {
            return Err(GatewayError::Response);
        }
        Self::send(
            self.request(Method::PUT, "/cobrancas/webhook")
                .await?
                .json(&json!({ "webhookUrl": url })),
        )
        .await
        .map(|_| ())
    }
}
