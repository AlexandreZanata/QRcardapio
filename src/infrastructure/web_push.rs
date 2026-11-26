mod encryption;

use std::io::Read;
use std::time::Duration;

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use reqwest::{Client, Request, StatusCode, Url, header};
use serde::Serialize;
use sha2::{Digest, Sha256};

const INVALID_SUBSCRIPTION: &str = "commerce.invalid_push_subscription";

pub struct PushSubscription {
    endpoint: Url,
    p256dh: String,
    auth_secret: String,
    key: [u8; 65],
    auth: [u8; 16],
}

impl PushSubscription {
    pub fn parse(endpoint: &str, p256dh: &str, auth_secret: &str) -> crate::error::AppResult<Self> {
        let invalid = || crate::error::AppError::Validation(INVALID_SUBSCRIPTION);
        let endpoint = push_endpoint(endpoint).ok_or_else(invalid)?;
        if p256dh.len() != 87 || auth_secret.len() != 22 || endpoint.as_str().len() > 4096 {
            return Err(invalid());
        }
        let point = URL_SAFE_NO_PAD.decode(p256dh).map_err(|_| invalid())?;
        let key = encryption::parse_public_key(point).map_err(|()| invalid())?;
        let auth_bytes = URL_SAFE_NO_PAD.decode(auth_secret).map_err(|_| invalid())?;
        let auth: [u8; 16] = auth_bytes.try_into().map_err(|_| invalid())?;
        Ok(Self {
            endpoint,
            p256dh: p256dh.into(),
            auth_secret: auth_secret.into(),
            key,
            auth,
        })
    }

    pub(crate) fn endpoint(&self) -> &str {
        self.endpoint.as_str()
    }

    pub(crate) fn public_key(&self) -> &str {
        &self.p256dh
    }

    pub(crate) fn auth_secret(&self) -> &str {
        &self.auth_secret
    }
}

fn push_endpoint(raw: &str) -> Option<Url> {
    if raw.len() > 4096 || raw.chars().any(char::is_control) {
        return None;
    }
    let url = Url::parse(raw).ok()?;
    let host = url.domain()?;
    let provider = matches!(
        host,
        "fcm.googleapis.com" | "updates.push.services.mozilla.com"
    ) || host.ends_with(".push.apple.com")
        || host.ends_with(".notify.windows.com");
    (provider
        && url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && url.path().len() > 1)
        .then_some(url)
}

pub struct WebPushSender {
    client: Client,
    key: EcdsaKeyPair,
    contact: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeliveryResult {
    Delivered,
    Gone,
    Rejected,
    Retry { after_seconds: Option<i64> },
}

impl WebPushSender {
    /// Sends only when both the VAPID contact and path to a PKCS#8 DER key are set.
    pub fn from_env() -> Result<Option<Self>, String> {
        let file =
            std::env::var_os("COMMERCE_VAPID_PRIVATE_KEY_FILE").filter(|raw| !raw.is_empty());
        let contact = std::env::var_os("COMMERCE_VAPID_CONTACT").filter(|raw| !raw.is_empty());
        match (file, contact) {
            (None, None) => Ok(None),
            (Some(file), Some(contact)) => {
                let contact = contact.into_string().map_err(|_| "invalid VAPID contact")?;
                let mut key = Vec::new();
                std::fs::File::open(file)
                    .and_then(|file| file.take(4097).read_to_end(&mut key))
                    .map_err(|_| "could not read COMMERCE_VAPID_PRIVATE_KEY_FILE")?;
                if key.len() > 4096 {
                    return Err("invalid VAPID key file size".into());
                }
                Self::new(&key, &contact).map(Some).map_err(str::to_owned)
            }
            _ => Err("set both COMMERCE_VAPID_PRIVATE_KEY_FILE and COMMERCE_VAPID_CONTACT".into()),
        }
    }

    pub(crate) fn new(pkcs8_der: &[u8], contact: &str) -> Result<Self, &'static str> {
        let contact_url = Url::parse(contact).map_err(|_| "invalid VAPID contact URI")?;
        let valid_contact = match contact_url.scheme() {
            "mailto" => contact_url.path().contains('@'),
            "https" => contact_url.domain().is_some(),
            _ => false,
        };
        if !valid_contact
            || contact_url.fragment().is_some()
            || contact_url.password().is_some()
            || !contact_url.username().is_empty()
            || contact.len() > 256
            || contact.chars().any(char::is_control)
        {
            return Err("invalid VAPID contact URI");
        }
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8_der)
            .map_err(|_| "VAPID key must be a P-256 PKCS#8 DER private key")?;
        let client = Client::builder()
            .https_only(true)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| "could not initialize Web Push HTTP client")?;
        Ok(Self {
            client,
            key,
            contact: contact_url.into(),
        })
    }

    pub fn public_key(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.key.public_key().as_ref())
    }

    pub(crate) async fn send(
        &self,
        subscription: &PushSubscription,
        event_id: &str,
        body: Vec<u8>,
        ttl_seconds: i64,
    ) -> DeliveryResult {
        let retry = DeliveryResult::Retry {
            after_seconds: None,
        };
        let Ok(request) = self.request(subscription, event_id, body, ttl_seconds, Utc::now())
        else {
            return DeliveryResult::Rejected;
        };
        let Ok(response) = self.client.execute(request).await else {
            return retry;
        };
        match response.status() {
            code if code.is_success() => DeliveryResult::Delivered,
            StatusCode::NOT_FOUND | StatusCode::GONE => DeliveryResult::Gone,
            code if code.is_server_error()
                || matches!(
                    code,
                    StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT
                ) =>
            {
                DeliveryResult::Retry {
                    after_seconds: response
                        .headers()
                        .get(header::RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| {
                            value.parse::<i64>().ok().or_else(|| {
                                DateTime::parse_from_rfc2822(value)
                                    .ok()
                                    .map(|date| date.timestamp() - Utc::now().timestamp())
                            })
                        }),
                }
            }
            _ => DeliveryResult::Rejected,
        }
    }

    pub(crate) fn request(
        &self,
        sub: &PushSubscription,
        event_id: &str,
        body: Vec<u8>,
        ttl_seconds: i64,
        now: DateTime<Utc>,
    ) -> Result<Request, ()> {
        let body = encryption::encrypt(&sub.key, &sub.auth, body)?;
        let mut request = self
            .client
            .post(sub.endpoint.clone())
            .header("ttl", ttl_seconds.clamp(0, 60))
            .header(header::CONTENT_ENCODING, "aes128gcm")
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(body)
            .build()
            .map_err(|_| ())?;
        let headers = request.headers_mut();
        let encoded = self.authorization(&sub.endpoint, now)?;
        let mut authorization = header::HeaderValue::from_str(&encoded).map_err(|_| ())?;
        authorization.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, authorization);
        let topic = URL_SAFE_NO_PAD.encode(&Sha256::digest(event_id.as_bytes())[..24]);
        headers.insert(
            "topic",
            header::HeaderValue::from_str(&topic).map_err(|_| ())?,
        );
        Ok(request)
    }

    fn authorization(&self, endpoint: &Url, now: DateTime<Utc>) -> Result<String, ()> {
        #[derive(Serialize)]
        struct Claims<'a> {
            aud: String,
            exp: i64,
            sub: &'a str,
        }
        let claims = Claims {
            aud: endpoint.origin().ascii_serialization(),
            exp: now.timestamp() + 3600,
            sub: &self.contact,
        };
        let header = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).map_err(|_| ())?);
        let jwt = format!("{header}.{claims}");
        let signature = self
            .key
            .sign(&SystemRandom::new(), jwt.as_bytes())
            .map_err(|_| ())?;
        let signature = URL_SAFE_NO_PAD.encode(signature.as_ref());
        let public_key = self.public_key();
        Ok(format!("vapid t={jwt}.{signature}, k={public_key}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED, UnparsedPublicKey};
    use serde_json::Value;

    #[test]
    fn builds_encrypted_request_with_verified_vapid_claims() {
        let key =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
                .unwrap();
        let sender = WebPushSender::new(key.as_ref(), "mailto:push@example.test").unwrap();
        let subscriber = PushSubscription::parse(
            "https://fcm.googleapis.com/fcm/send/demo-token",
            "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
            "BTBZMqHH6r4Tts7J_aSIgg",
        ).unwrap();
        let now = Utc::now();
        let payload = b"New order";
        let request = sender
            .request(&subscriber, "event-1", payload.to_vec(), 60, now)
            .unwrap();
        let headers = request.headers();
        assert_eq!(headers[header::CONTENT_ENCODING], "aes128gcm");
        assert_eq!(headers["ttl"], "60");
        assert_eq!(headers["topic"].as_bytes().len(), 32);
        let body = request.body().unwrap().as_bytes().unwrap();
        assert_eq!(body[20], 65);
        assert_ne!(&body[86..][..payload.len()], payload);

        assert!(headers[header::AUTHORIZATION].is_sensitive());
        let auth = headers[header::AUTHORIZATION].to_str().unwrap();
        let (token, public) = auth
            .strip_prefix("vapid t=")
            .unwrap()
            .split_once(", k=")
            .unwrap();
        let (signed, signature) = token.rsplit_once('.').unwrap();
        let decode = |part| URL_SAFE_NO_PAD.decode(part).unwrap();
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, decode(public))
            .verify(signed.as_bytes(), &decode(signature))
            .unwrap();
        let claims: Value =
            serde_json::from_slice(&decode(signed.split_once('.').unwrap().1)).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        assert_eq!(claims["sub"], "mailto:push@example.test");
        assert_eq!(claims["exp"], now.timestamp() + 3600);
    }

    #[test]
    fn limits_endpoints_to_supported_https_push_services() {
        for endpoint in [
            "http://fcm.googleapis.com/send/token",
            "https://fcm.googleapis.com.evil.example/send/token",
            "https://web.push.apple.com@127.0.0.1:443/send/token",
            "https://fcm.googleapis.com:9500/send/token",
            "https://127.0.0.1/send/token",
            "https://fcm.googleapis.com/send/token#fragment",
        ] {
            assert!(push_endpoint(endpoint).is_none(), "accepted {endpoint}");
        }
        assert!(push_endpoint("https://web.push.apple.com/token").is_some());
    }
}
