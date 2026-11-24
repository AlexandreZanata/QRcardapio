use aws_lc_rs::signature::{RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;

use crate::domain::account::Email;
use crate::error::{AppError, AppResult};
use crate::infrastructure::security::{constant_time_eq, sha256_hex};

#[derive(Deserialize)]
pub(super) struct KeyDocument {
    pub(super) keys: Vec<SigningKey>,
}

#[derive(Clone, Deserialize)]
pub(super) struct SigningKey {
    pub(super) kid: String,
    kty: String,
    alg: Option<String>,
    #[serde(rename = "use")]
    usage: Option<String>,
    n: String,
    e: String,
}

impl SigningKey {
    pub(super) fn signing_key(&self) -> bool {
        self.kty == "RSA"
            && self.alg.as_deref().is_none_or(|alg| alg == "RS256")
            && self.usage.as_deref().is_none_or(|usage| usage == "sig")
    }
}

#[derive(Deserialize)]
struct JwtHeader {
    kid: String,
    alg: String,
    crit: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}

impl Audience {
    fn matches(&self, client_id: &str) -> bool {
        match self {
            Self::One(id) => id == client_id,
            Self::Many(ids) => ids.as_slice() == [client_id],
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Verified {
    Boolean(bool),
    Text(String),
}

#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: Audience,
    azp: Option<String>,
    sub: String,
    email: String,
    email_verified: Verified,
    nonce: String,
    exp: i64,
    iat: i64,
}

pub struct GoogleIdentity {
    pub subject: String,
    pub email: Email,
}

pub(super) struct SignedToken<'a> {
    input: &'a str,
    claims: &'a str,
    signature: &'a str,
    pub(super) kid: String,
}

impl<'a> SignedToken<'a> {
    pub(super) fn parse(token: &'a str) -> AppResult<Self> {
        if token.len() > 16 * 1024 {
            return Err(invalid());
        }
        let (input, signature) = token.rsplit_once('.').ok_or_else(invalid)?;
        let (header, claims) = input.split_once('.').ok_or_else(invalid)?;
        let header: JwtHeader = decode_json(header)?;
        if header.alg != "RS256" || header.crit.is_some() || !printable(&header.kid, 128) {
            return Err(invalid());
        }
        Ok(Self {
            input,
            claims,
            signature,
            kid: header.kid,
        })
    }

    pub(super) fn verify(
        self,
        key: &SigningKey,
        client_id: &str,
        nonce_hash: &str,
        now: i64,
    ) -> AppResult<GoogleIdentity> {
        if !key.signing_key() || key.kid != self.kid || key.n.len() > 1400 || key.e.len() > 12 {
            return Err(invalid());
        }
        let components = RsaPublicKeyComponents {
            n: decode(&key.n)?,
            e: decode(&key.e)?,
        };
        components
            .verify(
                &RSA_PKCS1_2048_8192_SHA256,
                self.input.as_bytes(),
                &decode(self.signature)?,
            )
            .map_err(|_| invalid())?;
        let claims: Claims = decode_json(self.claims)?;
        let verified = match claims.email_verified {
            Verified::Boolean(value) => value,
            Verified::Text(value) => value == "true",
        };
        if !matches!(
            claims.iss.as_str(),
            "accounts.google.com" | "https://accounts.google.com"
        ) || !claims.aud.matches(client_id)
            || claims.azp.as_ref().is_some_and(|azp| azp != client_id)
            || claims.exp <= now
            || claims.iat < now - 660
            || claims.iat > now + 60
            || claims.exp <= claims.iat
            || !constant_time_eq(&sha256_hex(&claims.nonce), nonce_hash)
            || !printable(&claims.sub, 255)
        {
            return Err(invalid());
        }
        if !verified || claims.email.chars().any(char::is_control) {
            return Err(AppError::Unauthorized("google.email_unverified"));
        }
        let email = Email::parse(&claims.email).map_err(|_| invalid())?;
        Ok(GoogleIdentity {
            subject: claims.sub,
            email,
        })
    }
}

fn printable(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.bytes().all(|b| b.is_ascii_graphic())
}

fn decode(raw: &str) -> AppResult<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(raw).map_err(|_| invalid())
}

fn decode_json<T: serde::de::DeserializeOwned>(raw: &str) -> AppResult<T> {
    serde_json::from_slice(&decode(raw)?).map_err(|_| invalid())
}

pub(super) fn invalid() -> AppError {
    AppError::Unauthorized("google.invalid_identity")
}
