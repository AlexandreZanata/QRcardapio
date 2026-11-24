//! Stripe webhook signature verification (`Stripe-Signature: t=<unix>,v1=<hex>[,v1=<hex>]`).
//! The signed payload is `"{t}.{raw body}"`, signed with HMAC-SHA256 using the endpoint
//! secret. Comparison is constant time (`aws_lc_rs::hmac::verify`).

use aws_lc_rs::hmac;

/// Default tolerance recommended by Stripe against replays.
pub const TOLERANCE_SECONDS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureError {
    Malformed,
    Mismatch,
    Expired,
}

/// Verifies the raw request body against the header. `now` is the current Unix time.
pub fn verify(header: &str, body: &[u8], secret: &[u8], now: i64) -> Result<(), SignatureError> {
    let mut timestamp: Option<i64> = None;
    let mut signatures: Vec<Vec<u8>> = Vec::new();
    for part in header.split(',') {
        let Some((key, value)) = part.trim().split_once('=') else {
            continue;
        };
        match key {
            "t" => timestamp = value.parse().ok(),
            "v1" => {
                if let Ok(bytes) = hex::decode(value) {
                    signatures.push(bytes);
                }
            }
            _ => {}
        }
    }
    let timestamp = timestamp.ok_or(SignatureError::Malformed)?;
    if signatures.is_empty() {
        return Err(SignatureError::Malformed);
    }
    let mut signed = Vec::with_capacity(body.len() + 12);
    signed.extend_from_slice(timestamp.to_string().as_bytes());
    signed.push(b'.');
    signed.extend_from_slice(body);
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    if !signatures
        .iter()
        .any(|tag| hmac::verify(&key, &signed, tag).is_ok())
    {
        return Err(SignatureError::Mismatch);
    }
    if (now - timestamp).abs() > TOLERANCE_SECONDS {
        return Err(SignatureError::Expired);
    }
    Ok(())
}

/// Builds a valid header (tests and local tooling).
pub fn sign(body: &[u8], secret: &[u8], timestamp: i64) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let mut signed = timestamp.to_string().into_bytes();
    signed.push(b'.');
    signed.extend_from_slice(body);
    format!(
        "t={timestamp},v1={}",
        hex::encode(hmac::sign(&key, &signed).as_ref())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"whsec_test_secret_value_123456";

    #[test]
    fn accepts_valid_and_rotated_signatures() {
        let body = br#"{"id":"evt_1"}"#;
        let header = sign(body, SECRET, 1_700_000_000);
        assert_eq!(verify(&header, body, SECRET, 1_700_000_100), Ok(()));
        let rotated = format!("{},v1=00ff,v0=abc", header);
        assert_eq!(verify(&rotated, body, SECRET, 1_700_000_000), Ok(()));
        let old_first = format!(
            "t=1700000000,v1={},{}",
            "ab".repeat(32),
            header.split(',').nth(1).unwrap()
        );
        assert_eq!(verify(&old_first, body, SECRET, 1_700_000_000), Ok(()));
    }

    #[test]
    fn rejects_tampering_replay_and_garbage() {
        let body = br#"{"id":"evt_1"}"#;
        let header = sign(body, SECRET, 1_700_000_000);
        assert_eq!(
            verify(&header, br#"{"id":"evt_2"}"#, SECRET, 1_700_000_000),
            Err(SignatureError::Mismatch)
        );
        assert_eq!(
            verify(&header, body, b"whsec_other", 1_700_000_000),
            Err(SignatureError::Mismatch)
        );
        assert_eq!(
            verify(&header, body, SECRET, 1_700_000_301),
            Err(SignatureError::Expired)
        );
        assert_eq!(
            verify("garbage", body, SECRET, 0),
            Err(SignatureError::Malformed)
        );
        assert_eq!(
            verify("t=1", body, SECRET, 0),
            Err(SignatureError::Malformed)
        );
    }
}
