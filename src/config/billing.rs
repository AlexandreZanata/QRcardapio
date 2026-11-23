use reqwest::header::HeaderValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsaasEnvironment {
    Sandbox,
    Production,
}

impl AsaasEnvironment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sandbox => "sandbox",
            Self::Production => "production",
        }
    }

    pub fn api_base(self) -> &'static str {
        match self {
            Self::Sandbox => "https://api-sandbox.asaas.com/v3",
            Self::Production => "https://api.asaas.com/v3",
        }
    }

    pub fn checkout_origin(self) -> &'static str {
        match self {
            Self::Sandbox => "https://sandbox.asaas.com",
            Self::Production => "https://asaas.com",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AsaasConfig {
    pub environment: AsaasEnvironment,
    api_key: HeaderValue,
    webhook_token: HeaderValue,
}

impl AsaasConfig {
    pub fn new(
        environment: AsaasEnvironment,
        api_key: &str,
        webhook_token: &str,
    ) -> Result<Self, String> {
        let api_key = secret_header(api_key, "ASAAS_API_KEY")?;
        let webhook_header = secret_header(webhook_token, "ASAAS_WEBHOOK_TOKEN")?;
        if webhook_token.len() < 32 || api_key == webhook_header {
            return Err("ASAAS_WEBHOOK_TOKEN must have at least 32 characters and differ from ASAAS_API_KEY".into());
        }
        Ok(Self {
            environment,
            api_key,
            webhook_token: webhook_header,
        })
    }

    pub(super) fn from_env(public_base: &str, cookie_secure: bool) -> Result<Option<Self>, String> {
        let env = std::env::var("ASAAS_ENV").unwrap_or_default();
        let api_key = std::env::var("ASAAS_API_KEY").unwrap_or_default();
        let webhook_token = std::env::var("ASAAS_WEBHOOK_TOKEN").unwrap_or_default();
        let environment = match env.as_str() {
            "" if api_key.is_empty() && webhook_token.is_empty() => return Ok(None),
            "sandbox" => AsaasEnvironment::Sandbox,
            "production" => AsaasEnvironment::Production,
            _ => {
                return Err(
                    "ASAAS_ENV must be sandbox or production when billing is configured".into(),
                );
            }
        };
        let base = reqwest::Url::parse(public_base)
            .map_err(|_| "PUBLIC_BASE_URL must be an HTTPS origin for Asaas callbacks")?;
        if base.scheme() != "https"
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.path() != "/"
            || base.query().is_some()
            || base.fragment().is_some()
            || !cookie_secure
        {
            return Err(
                "Asaas requires an HTTPS PUBLIC_BASE_URL origin and COOKIE_SECURE=true".into(),
            );
        }
        Self::new(environment, &api_key, &webhook_token).map(Some)
    }

    pub fn api_key(&self) -> HeaderValue {
        self.api_key.clone()
    }

    pub fn webhook_token(&self) -> &str {
        self.webhook_token
            .to_str()
            .expect("validated Asaas webhook token")
    }
}

fn secret_header(raw: &str, name: &str) -> Result<HeaderValue, String> {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(format!("{name} must be a nonempty ASCII token"));
    }
    let mut value = HeaderValue::from_str(raw).map_err(|_| format!("invalid {name}"))?;
    value.set_sensitive(true);
    Ok(value)
}
