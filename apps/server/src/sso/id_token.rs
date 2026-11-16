//! ID tokens (JSON Web Tokens signed as JWS, RFC 7515 and 7519), checked
//! with ring, which rustls already brings: the signature with the
//! provider's key, then the registered claims, read into strict types.
//!
//! pley reads these itself rather than through a JWT crate. Times (`exp`,
//! `nbf`, `iat`) must be JSON numbers: a token whose `exp` is a string, say,
//! is refused outright instead of being read as one without an expiry,
//! which let tokens past their time through in `jsonwebtoken` before 10.3
//! (CVE-2026-25537).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::signature::{self, RsaPublicKeyComponents, UnparsedPublicKey};
use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Deserializer};

/// Tokens longer than this are refused before anything is decoded. Generous:
/// with the `groups` scope, some providers (Keycloak, Authentik) put every
/// group and role of the person in the token, and someone in hundreds of
/// groups passes 16 KiB. The token comes in the body of pley's own request
/// to the provider, never in a header, so no header limit applies.
const MAX_TOKEN: usize = 256 * 1024;

/// The signing algorithms pley checks (RFC 7518 §3.1, RFC 8037 §3.1);
/// `none` and the others are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alg {
    HS256,
    HS384,
    HS512,
    RS256,
    RS384,
    RS512,
    PS256,
    PS384,
    PS512,
    ES256,
    ES384,
    EdDSA,
}

impl Alg {
    const ALL: [Alg; 12] = [
        Alg::HS256,
        Alg::HS384,
        Alg::HS512,
        Alg::RS256,
        Alg::RS384,
        Alg::RS512,
        Alg::PS256,
        Alg::PS384,
        Alg::PS512,
        Alg::ES256,
        Alg::ES384,
        Alg::EdDSA,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Alg::HS256 => "HS256",
            Alg::HS384 => "HS384",
            Alg::HS512 => "HS512",
            Alg::RS256 => "RS256",
            Alg::RS384 => "RS384",
            Alg::RS512 => "RS512",
            Alg::PS256 => "PS256",
            Alg::PS384 => "PS384",
            Alg::PS512 => "PS512",
            Alg::ES256 => "ES256",
            Alg::ES384 => "ES384",
            Alg::EdDSA => "EdDSA",
        }
    }

    fn parse(name: &str) -> Option<Alg> {
        Alg::ALL.into_iter().find(|alg| alg.name() == name)
    }

    /// Keyed with the client secret (OpenID Connect Core §10.1).
    pub fn is_shared_secret(self) -> bool {
        matches!(self, Alg::HS256 | Alg::HS384 | Alg::HS512)
    }
}

/// A public key from the provider's key set (RFC 7517), as much of it as
/// pley reads.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Jwk {
    pub kty: String,
    #[serde(default)]
    pub kid: Option<String>,
    #[serde(default, rename = "use")]
    pub use_: Option<String>,
    #[serde(default)]
    pub alg: Option<String>,
    #[serde(default)]
    pub crv: Option<String>,
    #[serde(default)]
    pub n: Option<String>,
    #[serde(default)]
    pub e: Option<String>,
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
}

impl Jwk {
    /// Whether this key may have signed with `alg`: its type and curve fit,
    /// and a key that names its algorithm or use is good for those only.
    pub fn fits(&self, alg: Alg) -> bool {
        let crv = self.crv.as_deref();
        let family = match self.kty.as_str() {
            "RSA" => matches!(alg, Alg::RS256 | Alg::RS384 | Alg::RS512 | Alg::PS256 | Alg::PS384 | Alg::PS512),
            "EC" => matches!((alg, crv), (Alg::ES256, Some("P-256")) | (Alg::ES384, Some("P-384"))),
            "OKP" => alg == Alg::EdDSA && crv == Some("Ed25519"),
            _ => false,
        };
        let named = self.alg.as_deref().is_none_or(|a| a == alg.name());
        let signs = self.use_.as_deref().is_none_or(|u| u == "sig");
        family && named && signs
    }
}

/// What a token's header says about how it was signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub alg: Alg,
    pub kid: Option<String>,
}

#[derive(Deserialize)]
struct RawHeader {
    alg: String,
    #[serde(default)]
    kid: Option<String>,
    /// Extensions the token says must be understood (RFC 7515 §4.1.11):
    /// pley understands none, so any is refused.
    #[serde(default)]
    crit: Option<serde_json::Value>,
}

/// A token's three parts: header, payload and signature, still encoded.
fn parts(token: &str) -> Result<[&str; 3], String> {
    if token.len() > MAX_TOKEN {
        return Err("the ID token is too long".into());
    }
    let mut parts = token.split('.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(header), Some(payload), Some(signature), None) => Ok([header, payload, signature]),
        // Five parts would be an encrypted token (JWE).
        _ => Err("the ID token isn't a signed JWT".into()),
    }
}

fn decode(part: &str, what: &str) -> Result<Vec<u8>, String> {
    URL_SAFE_NO_PAD.decode(part).map_err(|_| format!("the ID token's {what} isn't base64url"))
}

/// The header of `token`, before anything is trusted: which algorithm and
/// key to check it with.
pub fn header(token: &str) -> Result<Header, String> {
    let [header, _, _] = parts(token)?;
    let raw: RawHeader =
        serde_json::from_slice(&decode(header, "header")?).map_err(|_| "the ID token's header can't be read")?;
    if raw.crit.is_some() {
        return Err("the ID token has critical extensions Pley doesn't know".into());
    }
    let alg = Alg::parse(&raw.alg).ok_or_else(|| {
        let alg: String = raw.alg.chars().filter(char::is_ascii_graphic).take(16).collect();
        format!("the ID token is signed with {alg}, which Pley doesn't take")
    })?;
    Ok(Header { alg, kid: raw.kid })
}

/// What checks a token's signature.
pub enum Key<'a> {
    /// The provider's public key.
    Public(&'a Jwk),
    /// The client secret, for HS256, HS384 and HS512.
    Secret(&'a [u8]),
}

/// A big-endian number from a key, without the zeros some providers put
/// in front, which ring doesn't take.
fn number(value: Option<&str>, what: &str) -> Result<Vec<u8>, String> {
    let bytes = decode(value.ok_or_else(|| format!("the provider's key has no {what}"))?, "key")?;
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
    Ok(bytes[start..].to_vec())
}

/// Checks the signature of `token` with `key` for `alg`.
fn check_signature(token: &str, alg: Alg, key: Key<'_>) -> Result<(), String> {
    let [header, payload, signature] = parts(token)?;
    let signature = decode(signature, "signature")?;
    // The signing input: the first two parts as they came, with their dot.
    let signed = &token.as_bytes()[..header.len() + 1 + payload.len()];
    let refused = || "the ID token's signature doesn't match".to_owned();
    match (alg, key) {
        (Alg::HS256 | Alg::HS384 | Alg::HS512, Key::Secret(secret)) => {
            if secret.is_empty() {
                return Err("the ID token is signed with a shared secret, and Pley has none".into());
            }
            let algorithm = match alg {
                Alg::HS256 => ring::hmac::HMAC_SHA256,
                Alg::HS384 => ring::hmac::HMAC_SHA384,
                _ => ring::hmac::HMAC_SHA512,
            };
            // Compares in constant time.
            ring::hmac::verify(&ring::hmac::Key::new(algorithm, secret), signed, &signature).map_err(|_| refused())
        }
        (_, Key::Secret(_)) => Err(format!("{} is checked with the provider's key, not a secret", alg.name())),
        (_, Key::Public(jwk)) => {
            if !jwk.fits(alg) {
                return Err(format!("the provider's key isn't one for {}", alg.name()));
            }
            match alg {
                Alg::RS256 | Alg::RS384 | Alg::RS512 | Alg::PS256 | Alg::PS384 | Alg::PS512 => {
                    // Keys of 2048 to 8192 bits only.
                    let params: &signature::RsaParameters = match alg {
                        Alg::RS256 => &signature::RSA_PKCS1_2048_8192_SHA256,
                        Alg::RS384 => &signature::RSA_PKCS1_2048_8192_SHA384,
                        Alg::RS512 => &signature::RSA_PKCS1_2048_8192_SHA512,
                        Alg::PS256 => &signature::RSA_PSS_2048_8192_SHA256,
                        Alg::PS384 => &signature::RSA_PSS_2048_8192_SHA384,
                        _ => &signature::RSA_PSS_2048_8192_SHA512,
                    };
                    let (n, e) = (number(jwk.n.as_deref(), "modulus")?, number(jwk.e.as_deref(), "exponent")?);
                    RsaPublicKeyComponents { n: &n, e: &e }.verify(params, signed, &signature).map_err(|_| refused())
                }
                Alg::ES256 | Alg::ES384 => {
                    // JWS signatures are R || S of fixed length (RFC 7518 §3.4).
                    let (algorithm, size): (&'static signature::EcdsaVerificationAlgorithm, usize) =
                        if alg == Alg::ES256 {
                            (&signature::ECDSA_P256_SHA256_FIXED, 32)
                        } else {
                            (&signature::ECDSA_P384_SHA384_FIXED, 48)
                        };
                    let x = decode(jwk.x.as_deref().ok_or("the provider's key has no x")?, "key")?;
                    let y = decode(jwk.y.as_deref().ok_or("the provider's key has no y")?, "key")?;
                    if x.len() != size || y.len() != size {
                        return Err("the provider's key isn't on its curve".into());
                    }
                    // An uncompressed point (SEC 1 §2.3.3).
                    let mut point = Vec::with_capacity(1 + 2 * size);
                    point.push(4);
                    point.extend_from_slice(&x);
                    point.extend_from_slice(&y);
                    UnparsedPublicKey::new(algorithm, &point).verify(signed, &signature).map_err(|_| refused())
                }
                _ => {
                    let x = decode(jwk.x.as_deref().ok_or("the provider's key has no x")?, "key")?;
                    UnparsedPublicKey::new(&signature::ED25519, &x).verify(signed, &signature).map_err(|_| refused())
                }
            }
        }
    }
}

/// A time in a token: seconds since 1970 (RFC 7519 §2), as a JSON number,
/// whole or not. Anything else (a string, above all) is refused rather than
/// taken for a claim that isn't there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NumericDate(i64);

impl<'de> Deserialize<'de> for NumericDate {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(de)? {
            serde_json::Value::Number(n) => n
                .as_i64()
                .or_else(|| n.as_f64().filter(|f| f.is_finite() && f.abs() < 1e15).map(|f| f.floor() as i64))
                .map(NumericDate)
                .ok_or_else(|| D::Error::custom("a time out of range")),
            _ => Err(D::Error::custom("a time that isn't a number")),
        }
    }
}

/// A claim that is one string or a list of them (`aud`, `groups`).
pub fn strings<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(de)? {
        OneOrMany::One(one) => vec![one],
        OneOrMany::Many(many) => many,
    })
}

/// The registered claims every ID token must have (OpenID Connect Core
/// §2), and the times it may have.
#[derive(Deserialize)]
struct Registered {
    iss: String,
    #[serde(deserialize_with = "strings")]
    aud: Vec<String>,
    exp: NumericDate,
    #[serde(default)]
    nbf: Option<NumericDate>,
    #[serde(default)]
    iat: Option<NumericDate>,
}

/// Who a token must come from and be for, and when it is checked.
pub struct Expected<'a> {
    pub issuer: &'a str,
    pub audience: &'a str,
    /// Seconds since 1970.
    pub now: i64,
    /// How far the provider's clock may be from pley's, in seconds.
    pub leeway: i64,
}

/// Checks `token`: signed with `key` for `alg` (from [`header`]), issued
/// by and for whom `expected` says, and current. Then its claims, as `T`.
pub fn verify<T: DeserializeOwned>(token: &str, alg: Alg, key: Key<'_>, expected: &Expected<'_>) -> Result<T, String> {
    // The algorithm is the header's own: one signed otherwise is refused.
    if header(token)?.alg != alg {
        return Err("the ID token's header names another algorithm".into());
    }
    check_signature(token, alg, key)?;
    // Signed by the provider: now the payload can be read.
    let [_, payload, _] = parts(token)?;
    let payload = decode(payload, "payload")?;
    let registered: Registered = serde_json::from_slice(&payload).map_err(|e| {
        // serde's message names the claim, never its value.
        format!("the ID token's claims can't be read: {}", e.to_string().chars().take(120).collect::<String>())
    })?;
    if registered.iss != expected.issuer {
        return Err("the ID token comes from another issuer".into());
    }
    if !registered.aud.iter().any(|aud| aud == expected.audience) {
        return Err("the ID token was issued to another client".into());
    }
    // Saturating: times at the ends of i64 are signed values too.
    if registered.exp.0.saturating_add(expected.leeway) <= expected.now {
        return Err("the ID token has expired".into());
    }
    if registered.nbf.is_some_and(|nbf| nbf.0.saturating_sub(expected.leeway) > expected.now) {
        return Err("the ID token isn't valid yet".into());
    }
    if registered.iat.is_some_and(|iat| iat.0.saturating_sub(expected.leeway) > expected.now) {
        return Err("the ID token was issued in the future".into());
    }
    serde_json::from_slice(&payload).map_err(|_| "the ID token's claims can't be read".into())
}

#[cfg(test)]
mod tests {
    use ring::rand::SystemRandom;
    use ring::signature::{EcdsaKeyPair, Ed25519KeyPair, KeyPair};
    use serde_json::{Value, json};

    use super::*;

    /// RFC 7515 Appendix A.2: an RS256 JWS and the public part of its key.
    const RFC_RS256: &str = concat!(
        "eyJhbGciOiJSUzI1NiJ9",
        ".",
        "eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFt",
        "cGxlLmNvbS9pc19yb290Ijp0cnVlfQ",
        ".",
        "cC4hiUPoj9Eetdgtv3hF80EGrhuB__dzERat0XF9g2VtQgr9PJbu3XOiZj5RZmh7",
        "AAuHIm4Bh-0Qc_lF5YKt_O8W2Fp5jujGbds9uJdbF9CUAr7t1dnZcAcQjbKBYNX4",
        "BAynRFdiuB--f_nZLgrnbyTyWzO75vRK5h6xBArLIARNPvkSjtQBMHlb1L07Qe7K",
        "0GarZRmB_eSN9383LcOLn6_dO--xi12jzDwusC-eOkHWEsqtFZESc6BfI7noOPqv",
        "hJ1phCnvWh6IeYI2w9QOYEUipUTI8np6LbgGY9Fs98rqVt5AXLIhWkWywlVmtVrB",
        "p0igcN_IoypGlUPQGe77Rw",
    );
    const RFC_RSA_N: &str = concat!(
        "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddx",
        "HmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMs",
        "D1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSH",
        "SXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdV",
        "MTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8",
        "NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ",
    );
    /// RFC 7515 Appendix A.3: an ES256 JWS and its public key.
    const RFC_ES256: &str = concat!(
        "eyJhbGciOiJFUzI1NiJ9",
        ".",
        "eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFt",
        "cGxlLmNvbS9pc19yb290Ijp0cnVlfQ",
        ".",
        "DtEhU3ljbEg8L38VWAfUAqOyKAM6-Xx-F4GawxaepmXFCgfTjDxw5djxLa8ISlSA",
        "pmWQxfKTUJqPP3-Kg6NU1Q",
    );
    /// RFC 8037 Appendix A.4: an EdDSA (Ed25519) JWS and its public key.
    const RFC_EDDSA: &str = concat!(
        "eyJhbGciOiJFZERTQSJ9.RXhhbXBsZSBvZiBFZDI1NTE5IHNpZ25pbmc.hgyY0il_MGCj",
        "P0JzlnLWG1PPOt7-09PGcvMg3AIbQR6dWbhijcNR4ki4iylGjg5BhVsPt9g7sVvpAr_Mu",
        "M0KAg",
    );

    fn rsa_key() -> Jwk {
        Jwk { kty: "RSA".into(), n: Some(RFC_RSA_N.into()), e: Some("AQAB".into()), ..Default::default() }
    }

    fn ec_key() -> Jwk {
        Jwk {
            kty: "EC".into(),
            crv: Some("P-256".into()),
            x: Some("f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU".into()),
            y: Some("x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0".into()),
            ..Default::default()
        }
    }

    fn ed_key() -> Jwk {
        Jwk {
            kty: "OKP".into(),
            crv: Some("Ed25519".into()),
            x: Some("11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo".into()),
            ..Default::default()
        }
    }

    /// The token with one character of its signature changed.
    fn tampered(token: &str) -> String {
        let mut bytes = token.as_bytes().to_vec();
        let last = bytes.len() - 2;
        bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn the_rfcs_examples_check_out() {
        for (token, alg, key) in
            [(RFC_RS256, Alg::RS256, rsa_key()), (RFC_ES256, Alg::ES256, ec_key()), (RFC_EDDSA, Alg::EdDSA, ed_key())]
        {
            assert_eq!(header(token).unwrap().alg, alg);
            assert_eq!(check_signature(token, alg, Key::Public(&key)), Ok(()), "{}", alg.name());
            assert!(check_signature(&tampered(token), alg, Key::Public(&key)).is_err(), "{}", alg.name());
        }
        // A key of another kind, or for another algorithm, checks nothing.
        assert!(check_signature(RFC_RS256, Alg::RS256, Key::Public(&ec_key())).is_err());
        let named = Jwk { alg: Some("RS512".into()), ..rsa_key() };
        assert!(check_signature(RFC_RS256, Alg::RS256, Key::Public(&named)).is_err());
        let encrypts = Jwk { use_: Some("enc".into()), ..rsa_key() };
        assert!(check_signature(RFC_RS256, Alg::RS256, Key::Public(&encrypts)).is_err());
        // Zeros in front of the modulus are read past.
        let mut padded = vec![0u8];
        padded.extend(URL_SAFE_NO_PAD.decode(RFC_RSA_N).unwrap());
        let padded = Jwk { n: Some(URL_SAFE_NO_PAD.encode(padded)), ..rsa_key() };
        assert_eq!(check_signature(RFC_RS256, Alg::RS256, Key::Public(&padded)), Ok(()));
        // A public key never checks a shared-secret signature, nor the reverse.
        assert!(check_signature(RFC_RS256, Alg::RS256, Key::Secret(b"secret")).is_err());
    }

    #[test]
    fn headers_name_an_algorithm_pley_takes() {
        let token = |header: Value| format!("{}.e30.c2ln", URL_SAFE_NO_PAD.encode(header.to_string()));
        assert_eq!(
            header(&token(json!({"alg": "RS256", "kid": "k1", "typ": "JWT"}))),
            Ok(Header { alg: Alg::RS256, kid: Some("k1".into()) })
        );
        for refused in [
            json!({"alg": "none"}),
            json!({"alg": "RS1"}),
            json!({"alg": "rs256"}),
            json!({"typ": "JWT"}),
            json!({"alg": "RS256", "crit": ["b64"], "b64": false}),
        ] {
            assert!(header(&token(refused.clone())).is_err(), "{refused}");
        }
        assert!(header("a.b").is_err());
        assert!(header("a.b.c.d.e").is_err(), "an encrypted token");
        assert!(header("!!.e30.c2ln").is_err());
        assert!(header(&"a".repeat(MAX_TOKEN + 1)).is_err());
    }

    /// Signs `claims` as a token for `alg` with ring.
    fn signed(alg: Alg, kid: &str, claims: &Value, sign: impl Fn(&[u8]) -> Vec<u8>) -> String {
        let header = URL_SAFE_NO_PAD.encode(json!({"alg": alg.name(), "kid": kid}).to_string());
        let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
        let input = format!("{header}.{payload}");
        let signature = URL_SAFE_NO_PAD.encode(sign(input.as_bytes()));
        format!("{input}.{signature}")
    }

    fn expected() -> Expected<'static> {
        Expected { issuer: "https://auth.example.com", audience: "pley", now: 1_800_000_000, leeway: 60 }
    }

    fn claims(extra: Value) -> Value {
        let mut claims = json!({
            "iss": "https://auth.example.com", "aud": "pley", "sub": "ann", "exp": 1_800_000_300, "iat": 1_799_999_990
        });
        for (key, value) in extra.as_object().unwrap() {
            if value.is_null() {
                claims.as_object_mut().unwrap().remove(key);
            } else {
                claims[key] = value.clone();
            }
        }
        claims
    }

    #[derive(Deserialize, Debug)]
    struct Sub {
        sub: String,
    }

    #[test]
    fn tokens_are_from_the_issuer_for_this_client_and_current() {
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let key = Jwk {
            kty: "OKP".into(),
            crv: Some("Ed25519".into()),
            x: Some(URL_SAFE_NO_PAD.encode(pair.public_key().as_ref())),
            ..Default::default()
        };
        let token = |extra: Value| signed(Alg::EdDSA, "k", &claims(extra), |input| pair.sign(input).as_ref().to_vec());
        let check = |extra: Value| verify::<Sub>(&token(extra), Alg::EdDSA, Key::Public(&key), &expected());
        assert_eq!(check(json!({})).unwrap().sub, "ann");
        assert!(check(json!({"aud": ["other", "pley"]})).is_ok(), "one of its audiences");
        assert!(check(json!({"exp": 1_799_999_950})).is_ok(), "within the leeway");
        assert!(check(json!({"exp": 1_800_000_300.5})).is_ok(), "a time that isn't whole");
        assert!(check(json!({"exp": i64::MAX, "nbf": i64::MIN})).is_ok(), "no overflow at the ends");
        for refused in [
            json!({"iss": "https://auth.example.com/"}),
            json!({"iss": null}),
            json!({"aud": "other"}),
            json!({"aud": []}),
            json!({"aud": null}),
            json!({"exp": 1_799_999_940}),
            json!({"exp": null}),
            json!({"nbf": 1_800_000_061}),
            json!({"iat": 1_800_000_061}),
            // At the ends of i64: refused, not wrapped around.
            json!({"nbf": i64::MAX}),
            json!({"iat": i64::MAX}),
            json!({"exp": i64::MIN}),
        ] {
            assert!(check(refused.clone()).is_err(), "{refused}");
        }
        // Checked with the algorithm the caller chose, which is the header's.
        assert!(verify::<Sub>(&token(json!({})), Alg::ES256, Key::Public(&key), &expected()).is_err());
    }

    /// CVE-2026-25537: times that aren't numbers are refused, never read as
    /// claims that aren't there.
    #[test]
    fn times_that_are_strings_are_refused() {
        let secret = b"a client secret of some length";
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret);
        let token = |extra: Value| {
            signed(Alg::HS256, "k", &claims(extra), |input| ring::hmac::sign(&key, input).as_ref().to_vec())
        };
        let check = |extra: Value| verify::<Sub>(&token(extra), Alg::HS256, Key::Secret(secret), &expected());
        assert!(check(json!({})).is_ok());
        for refused in [
            json!({"exp": "1800000300"}),
            json!({"exp": "1"}),
            json!({"nbf": "4000000000"}),
            json!({"iat": "4000000000"}),
            json!({"exp": true}),
            json!({"exp": {}}),
            json!({"exp": 1e300}),
        ] {
            assert!(check(refused.clone()).is_err(), "{refused}");
        }
        // A secret checks only its own signatures.
        assert!(verify::<Sub>(&token(json!({})), Alg::HS256, Key::Secret(b"another"), &expected()).is_err());
        assert!(verify::<Sub>(&token(json!({})), Alg::HS256, Key::Secret(b""), &expected()).is_err());
    }

    #[test]
    fn ecdsa_tokens_signed_with_ring_check_out() {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&signature::ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let pair = EcdsaKeyPair::from_pkcs8(&signature::ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng).unwrap();
        let point = pair.public_key().as_ref();
        let key = Jwk {
            kty: "EC".into(),
            crv: Some("P-256".into()),
            x: Some(URL_SAFE_NO_PAD.encode(&point[1..33])),
            y: Some(URL_SAFE_NO_PAD.encode(&point[33..])),
            ..Default::default()
        };
        let token =
            signed(Alg::ES256, "k", &claims(json!({})), |input| pair.sign(&rng, input).unwrap().as_ref().to_vec());
        assert_eq!(verify::<Sub>(&token, Alg::ES256, Key::Public(&key), &expected()).unwrap().sub, "ann");
        let other_curve = Jwk { crv: Some("P-384".into()), ..key };
        assert!(verify::<Sub>(&token, Alg::ES256, Key::Public(&other_curve), &expected()).is_err());
    }
}
