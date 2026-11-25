use aws_lc_rs::agreement::{ECDH_P256, ParsedPublicKey, PrivateKey, UnparsedPublicKey, agree};
use aws_lc_rs::{hkdf, rand};

pub(super) fn parse_public_key(point: Vec<u8>) -> Result<[u8; 65], ()> {
    let point: [u8; 65] = point.try_into().map_err(|_| ())?;
    if point[0] != 4 {
        return Err(());
    }
    ParsedPublicKey::try_from(UnparsedPublicKey::new(&ECDH_P256, &point)).map_err(|_| ())?;
    Ok(point)
}

pub(super) fn encrypt(peer: &[u8; 65], auth: &[u8; 16], body: Vec<u8>) -> Result<Vec<u8>, ()> {
    let key = PrivateKey::generate(&ECDH_P256).map_err(|_| ())?;
    let mut salt = [0; 16];
    rand::fill(&mut salt).map_err(|_| ())?;
    seal(peer, auth, body, &key, salt)
}

fn seal(
    peer: &[u8; 65],
    auth: &[u8; 16],
    body: Vec<u8>,
    key: &PrivateKey,
    salt: [u8; 16],
) -> Result<Vec<u8>, ()> {
    // RFC 8291 requires one record and allows 3993 plaintext bytes in a 4096-byte body.
    if body.len() > 3993 {
        return Err(());
    }
    let public = key.compute_public_key().map_err(|_| ())?;
    let ikm = agree(
        key,
        UnparsedPublicKey::new(&ECDH_P256, peer),
        (),
        |shared| {
            let info: [&[u8]; 3] = [b"WebPush: info\0", peer, public.as_ref()];
            let mut ikm = [0; 32];
            hkdf::Salt::new(hkdf::HKDF_SHA256, auth)
                .extract(shared)
                .expand(&info, hkdf::HKDF_SHA256)
                .map_err(|_| ())?
                .fill(&mut ikm)
                .map_err(|_| ())?;
            Ok(ikm)
        },
    )?;
    ece_native::encrypt(ikm, salt, public.as_ref(), std::iter::once(body), 4096).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

    #[test]
    fn encrypts_the_rfc8291_reference_message() {
        let decode = |value| URL_SAFE_NO_PAD.decode(value).unwrap();
        let client = parse_public_key(decode(concat!(
            "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcx",
            "aOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4"
        )))
        .unwrap();
        let secret = decode("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw");
        let key = PrivateKey::from_private_key(&ECDH_P256, &secret).unwrap();
        let encrypted = seal(
            &client,
            &decode("BTBZMqHH6r4Tts7J_aSIgg").try_into().unwrap(),
            b"When I grow up, I want to be a watermelon".to_vec(),
            &key,
            decode("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap(),
        )
        .unwrap();
        assert_eq!(
            URL_SAFE_NO_PAD.encode(encrypted),
            concat!(
                "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27ml",
                "mlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPT",
                "pK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
            )
        );
    }
}
