use std::collections::HashMap;
use std::env;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::http::HeaderMap;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::Sha256;

pub(super) fn verify_slack(headers: &HeaderMap, body: &[u8]) -> Result<(), &'static str> {
    let secret = env::var("SLACK_SIGNING_SECRET").ok();
    let Some(secret) = secret.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let signature = headers
        .get("x-slack-signature")
        .and_then(|value| value.to_str().ok())
        .ok_or("missing_signature")?;
    let timestamp = headers
        .get("x-slack-request-timestamp")
        .and_then(|value| value.to_str().ok())
        .ok_or("missing_timestamp")?;
    let timestamp_value: i64 = timestamp.parse().map_err(|_| "invalid_timestamp")?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs() as i64;
    if (now - timestamp_value).abs() > 60 * 5 {
        return Err("stale_timestamp");
    }

    let base = format!("v0:{}:{}", timestamp, String::from_utf8_lossy(body));
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| "bad_secret")?;
    mac.update(base.as_bytes());
    let expected = format!("v0={}", hex::encode(mac.finalize().into_bytes()));
    if expected != signature {
        return Err("invalid_signature");
    }
    Ok(())
}

pub(super) fn verify_postmark(headers: &HeaderMap) -> Result<(), &'static str> {
    let token = env::var("POSTMARK_INBOUND_TOKEN").ok();
    let Some(token) = token.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let header = headers
        .get("x-postmark-token")
        .and_then(|value| value.to_str().ok())
        .ok_or("missing_token")?;
    if header != token {
        return Err("invalid_token");
    }
    Ok(())
}

pub(super) fn verify_bluebubbles(headers: &HeaderMap) -> Result<(), &'static str> {
    let token = env::var("BLUEBUBBLES_WEBHOOK_TOKEN").ok();
    let Some(token) = token.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let header = headers
        .get("x-bluebubbles-token")
        .and_then(|value| value.to_str().ok())
        .ok_or("missing_token")?;
    if header != token {
        return Err("invalid_token");
    }
    Ok(())
}

pub(super) fn verify_twilio(headers: &HeaderMap, body: &[u8]) -> Result<(), &'static str> {
    let token = env::var("TWILIO_AUTH_TOKEN").ok();
    let url = env::var("TWILIO_WEBHOOK_URL").ok();
    let (Some(token), Some(url)) = (token, url) else {
        return Ok(());
    };
    if token.trim().is_empty() || url.trim().is_empty() {
        return Ok(());
    }
    let signature = headers
        .get("x-twilio-signature")
        .and_then(|value| value.to_str().ok())
        .ok_or("missing_signature")?;

    let params: HashMap<String, String> =
        serde_urlencoded::from_bytes(body).map_err(|_| "bad_form")?;
    let mut keys: Vec<_> = params.keys().cloned().collect();
    keys.sort();
    let mut data = url.clone();
    for key in keys {
        if let Some(value) = params.get(&key) {
            data.push_str(&key);
            data.push_str(value);
        }
    }

    let mut mac = Hmac::<Sha1>::new_from_slice(token.as_bytes()).map_err(|_| "bad_secret")?;
    mac.update(data.as_bytes());
    let expected = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    if expected != signature {
        return Err("invalid_signature");
    }
    Ok(())
}

/// Verify WhatsApp webhook subscription request.
/// Returns the challenge token if verification succeeds.
/// Verify Notion webhook signature using HMAC-SHA256.
///
/// Notion sends the signature in the `X-Notion-Signature` header.
/// The signature is computed as: HMAC-SHA256(verification_token, request_body)
pub(super) fn verify_notion(headers: &HeaderMap, body: &[u8]) -> Result<(), &'static str> {
    let secret = env::var("NOTION_WEBHOOK_SECRET").ok();
    let Some(secret) = secret.filter(|value| !value.trim().is_empty()) else {
        // If secret not configured, skip verification (allows testing)
        return Ok(());
    };

    let signature = headers
        .get("x-notion-signature")
        .and_then(|value| value.to_str().ok())
        .ok_or("missing_signature")?;

    // Notion signature format: "v0=<hex_digest>"
    let expected_prefix = "v0=";
    if !signature.starts_with(expected_prefix) {
        return Err("invalid_signature_format");
    }

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| "bad_secret")?;
    mac.update(body);
    let expected = format!("v0={}", hex::encode(mac.finalize().into_bytes()));

    if expected != signature {
        return Err("invalid_signature");
    }

    Ok(())
}

pub(super) fn verify_whatsapp_subscription(
    mode: Option<&str>,
    token: Option<&str>,
    challenge: Option<&str>,
) -> Result<String, &'static str> {
    let expected_token = env::var("WHATSAPP_VERIFY_TOKEN").ok();
    let Some(expected) = expected_token.filter(|value| !value.trim().is_empty()) else {
        return Err("verify_token_not_configured");
    };

    if mode != Some("subscribe") {
        return Err("invalid_mode");
    }

    let provided_token = token.ok_or("missing_token")?;
    if provided_token != expected {
        return Err("token_mismatch");
    }

    challenge.map(|c| c.to_string()).ok_or("missing_challenge")
}

/// Verify WeChat webhook callback URL.
/// Returns the decrypted echostr if verification succeeds.
/// WeChat sends: GET /wechat/webhook?msg_signature=xxx&timestamp=xxx&nonce=xxx&echostr=xxx
///
/// When EncodingAESKey is configured:
/// - echostr is encrypted and must be decrypted
/// - Signature = SHA1(sort([token, timestamp, nonce, echostr]))
/// - Decrypt echostr using AES-256-CBC
pub(super) fn verify_wechat(
    msg_signature: Option<&str>,
    timestamp: Option<&str>,
    nonce: Option<&str>,
    echostr: Option<&str>,
) -> Result<String, &'static str> {
    verify_wechat_with_env(
        "WECHAT_TOKEN",
        "WECHAT_ENCODING_AES_KEY",
        msg_signature,
        timestamp,
        nonce,
        echostr,
    )
}

/// Verify WeChat Official Account webhook callback URL.
/// Returns the echostr if verification succeeds.
pub(super) fn verify_wechat_mp(
    signature: Option<&str>,
    timestamp: Option<&str>,
    nonce: Option<&str>,
    echostr: Option<&str>,
) -> Result<String, &'static str> {
    let token = env::var("WECHAT_MP_TOKEN").ok();
    let Some(token) = token.filter(|value| !value.trim().is_empty()) else {
        // If token not configured, just return echostr (allows testing)
        return echostr.map(|e| e.to_string()).ok_or("missing_echostr");
    };

    let signature = signature.ok_or("missing_signature")?;
    let timestamp = timestamp.ok_or("missing_timestamp")?;
    let nonce = nonce.ok_or("missing_nonce")?;
    let echostr = echostr.ok_or("missing_echostr")?;

    // Official Account URL verification uses SHA1(sort([token, timestamp, nonce]))
    let expected = sha1_of_sorted_parts(vec![token.as_str(), timestamp, nonce]);
    if expected != signature {
        return Err("invalid_signature");
    }

    Ok(echostr.to_string())
}

/// Verify WeChat Official Account webhook signature for POST messages.
///
/// - Plain mode: signature = SHA1(sort([token, timestamp, nonce]))
/// - Safe mode: msg_signature = SHA1(sort([token, timestamp, nonce, Encrypt]))
pub(super) fn verify_wechat_mp_message(
    signature: Option<&str>,
    msg_signature: Option<&str>,
    timestamp: Option<&str>,
    nonce: Option<&str>,
    body: &[u8],
) -> Result<(), &'static str> {
    let token = env::var("WECHAT_MP_TOKEN").ok();
    let Some(token) = token.filter(|value| !value.trim().is_empty()) else {
        // If token not configured, skip verification for local testing.
        return Ok(());
    };

    let timestamp = timestamp.ok_or("missing_timestamp")?;
    let nonce = nonce.ok_or("missing_nonce")?;
    let body_str = std::str::from_utf8(body).map_err(|_| "invalid_body_utf8")?;

    if let Some(encrypt) = extract_encrypt_field(body_str) {
        // Safe mode strictly uses msg_signature.
        let provided = msg_signature.ok_or("missing_msg_signature")?;
        let expected =
            sha1_of_sorted_parts(vec![token.as_str(), timestamp, nonce, encrypt.as_str()]);
        if expected != provided {
            return Err("invalid_signature");
        }
        return Ok(());
    }

    // Plain mode strictly uses signature.
    let provided = signature.ok_or("missing_signature")?;
    let expected = sha1_of_sorted_parts(vec![token.as_str(), timestamp, nonce]);
    if expected != provided {
        return Err("invalid_signature");
    }
    Ok(())
}

fn sha1_of_sorted_parts(mut parts: Vec<&str>) -> String {
    use sha1::{Digest, Sha1};
    parts.sort();
    let data = parts.join("");
    let mut hasher = Sha1::new();
    hasher.update(data.as_bytes());
    hex::encode(hasher.finalize())
}

fn extract_encrypt_field(xml: &str) -> Option<String> {
    let cdata_start = "<Encrypt><![CDATA[";
    let cdata_end = "]]></Encrypt>";
    if let Some(start) = xml.find(cdata_start) {
        let value_start = start + cdata_start.len();
        if let Some(end) = xml[value_start..].find(cdata_end) {
            return Some(xml[value_start..value_start + end].to_string());
        }
    }

    let start_tag = "<Encrypt>";
    let end_tag = "</Encrypt>";
    if let Some(start) = xml.find(start_tag) {
        let value_start = start + start_tag.len();
        if let Some(end) = xml[value_start..].find(end_tag) {
            return Some(xml[value_start..value_start + end].trim().to_string());
        }
    }

    None
}

fn verify_wechat_with_env(
    token_env_var: &str,
    encoding_key_env_var: &str,
    msg_signature: Option<&str>,
    timestamp: Option<&str>,
    nonce: Option<&str>,
    echostr: Option<&str>,
) -> Result<String, &'static str> {
    let token = env::var(token_env_var).ok();
    let encoding_aes_key = env::var(encoding_key_env_var).ok();

    let Some(token) = token.filter(|value| !value.trim().is_empty()) else {
        // If token not configured, just return echostr (allows testing)
        return echostr.map(|e| e.to_string()).ok_or("missing_echostr");
    };

    let signature = msg_signature.ok_or("missing_signature")?;
    let timestamp = timestamp.ok_or("missing_timestamp")?;
    let nonce = nonce.ok_or("missing_nonce")?;
    let echostr = echostr.ok_or("missing_echostr")?;

    // WeChat signature: SHA1(sort([token, timestamp, nonce, echostr]))
    let mut parts = vec![token.as_str(), timestamp, nonce, echostr];
    parts.sort();
    let data = parts.join("");

    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    let expected = hex::encode(result);

    if expected != signature {
        return Err("invalid_signature");
    }

    // If EncodingAESKey is configured, decrypt the echostr
    if let Some(aes_key_str) = encoding_aes_key.filter(|v| !v.trim().is_empty()) {
        return decrypt_wechat_echostr(echostr, &aes_key_str);
    }

    Ok(echostr.to_string())
}

/// Decrypt WeChat echostr using AES-256-CBC.
/// AESKey = Base64_Decode(EncodingAESKey + "=")
/// IV = first 16 bytes of AESKey
/// Message format after decryption: random(16B) + msg_len(4B, big endian) + msg + receiveid
fn decrypt_wechat_echostr(echostr: &str, encoding_aes_key: &str) -> Result<String, &'static str> {
    use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

    // Derive AESKey: Base64_Decode(EncodingAESKey + "=")
    // Use lenient decoder because WeChat's EncodingAESKey may have non-zero trailing bits
    let trimmed_key = encoding_aes_key.trim();
    let aes_key_b64 = format!("{}=", trimmed_key);

    let lenient_engine = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    let aes_key = lenient_engine.decode(&aes_key_b64).map_err(|e| {
        tracing::error!("base64 decode error: {:?}", e);
        "invalid_encoding_aes_key"
    })?;

    if aes_key.len() != 32 {
        return Err("invalid_aes_key_length");
    }

    // Decode the encrypted echostr from Base64
    let encrypted = base64::engine::general_purpose::STANDARD
        .decode(echostr)
        .map_err(|_| "invalid_echostr_base64")?;

    // IV is first 16 bytes of AESKey
    let iv: [u8; 16] = aes_key[..16].try_into().map_err(|_| "iv_error")?;
    let key: [u8; 32] = aes_key.try_into().map_err(|_| "key_error")?;

    // Decrypt using AES-256-CBC
    let mut buf = encrypted.clone();
    let decryptor = Aes256CbcDec::new(&key.into(), &iv.into());
    let decrypted = decryptor
        .decrypt_padded_mut::<NoPadding>(&mut buf)
        .map_err(|_| "decryption_failed")?;

    // Remove PKCS#7 padding
    let decrypted = remove_pkcs7_padding(decrypted)?;

    // Message format: random(16B) + msg_len(4B) + msg + receiveid
    if decrypted.len() < 20 {
        return Err("decrypted_too_short");
    }

    // Skip 16 random bytes
    let content = &decrypted[16..];

    // Read msg_len (4 bytes, big endian / network byte order)
    let msg_len = u32::from_be_bytes(
        content[0..4]
            .try_into()
            .map_err(|_| "msg_len_parse_error")?,
    ) as usize;

    if content.len() < 4 + msg_len {
        return Err("msg_length_mismatch");
    }

    // Extract the message
    let msg = &content[4..4 + msg_len];

    String::from_utf8(msg.to_vec()).map_err(|_| "invalid_utf8")
}

/// Verify Lark webhook signature using SHA256.
/// Lark sends: X-Lark-Request-Timestamp, X-Lark-Request-Nonce, X-Lark-Signature
/// Signature = SHA256(timestamp + nonce + encrypt_key + body)
pub(super) fn verify_lark(headers: &HeaderMap, body: &[u8]) -> Result<(), &'static str> {
    let encrypt_key = env::var("LARK_ENCRYPT_KEY").ok();
    let Some(encrypt_key) = encrypt_key.filter(|v| !v.trim().is_empty()) else {
        // If encrypt_key not configured, skip verification
        return Ok(());
    };

    let timestamp = headers
        .get("X-Lark-Request-Timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or("missing_timestamp")?;

    let nonce = headers
        .get("X-Lark-Request-Nonce")
        .and_then(|v| v.to_str().ok())
        .ok_or("missing_nonce")?;

    let signature = headers
        .get("X-Lark-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or("missing_signature")?;

    // Signature = SHA256(timestamp + nonce + encrypt_key + body)
    use sha2::Digest;
    let body_str = std::str::from_utf8(body).unwrap_or("");
    let data = format!("{}{}{}{}", timestamp, nonce, encrypt_key, body_str);

    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let expected = hex::encode(hasher.finalize());

    if expected != signature {
        return Err("invalid_signature");
    }

    Ok(())
}

/// Check if this is a URL verification challenge from Lark.
/// Returns the challenge string if it is, None otherwise.
pub(super) fn verify_lark_challenge(body: &[u8]) -> Option<String> {
    let payload: serde_json::Value = serde_json::from_slice(body).ok()?;

    // Check for URL verification event (type = "url_verification")
    if payload.get("type").and_then(|v| v.as_str()) == Some("url_verification") {
        return payload
            .get("challenge")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }

    // Also check schema 2.0 format (challenge + token present)
    if let Some(challenge) = payload.get("challenge").and_then(|v| v.as_str()) {
        if payload.get("token").is_some() {
            return Some(challenge.to_string());
        }
    }

    None
}

/// Remove PKCS#7 padding from decrypted data
fn remove_pkcs7_padding(data: &[u8]) -> Result<&[u8], &'static str> {
    if data.is_empty() {
        return Err("empty_data");
    }
    let padding_len = data[data.len() - 1] as usize;
    if padding_len == 0 || padding_len > 32 || padding_len > data.len() {
        return Err("invalid_padding");
    }
    // Verify all padding bytes are correct
    for &byte in &data[data.len() - padding_len..] {
        if byte as usize != padding_len {
            return Err("invalid_padding_bytes");
        }
    }
    Ok(&data[..data.len() - padding_len])
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha1::{Digest, Sha1};

    // ==================== WeChat Verification Tests ====================

    #[test]
    fn verify_wechat_returns_echostr_when_no_token_configured() {
        // When WECHAT_TOKEN is not set, should just return echostr
        std::env::remove_var("WECHAT_TOKEN");
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        let result = verify_wechat(
            Some("signature"),
            Some("1234567890"),
            Some("nonce"),
            Some("test_echostr"),
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test_echostr");
    }

    #[test]
    fn verify_wechat_returns_error_when_missing_echostr_no_token() {
        std::env::remove_var("WECHAT_TOKEN");
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        let result = verify_wechat(Some("sig"), Some("ts"), Some("nonce"), None);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "missing_echostr");
    }

    #[test]
    fn verify_wechat_validates_signature_when_token_set() {
        let token = "test_token_12345";
        std::env::set_var("WECHAT_TOKEN", token);
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        let timestamp = "1609459200";
        let nonce = "random_nonce";
        let echostr = "challenge_string";

        // Calculate the expected signature: SHA1(sort([token, timestamp, nonce, echostr]))
        let mut parts = vec![token, timestamp, nonce, echostr];
        parts.sort();
        let data = parts.join("");
        let mut hasher = Sha1::new();
        hasher.update(data.as_bytes());
        let valid_signature = hex::encode(hasher.finalize());

        let result = verify_wechat(
            Some(&valid_signature),
            Some(timestamp),
            Some(nonce),
            Some(echostr),
        );

        // Clean up env var
        std::env::remove_var("WECHAT_TOKEN");

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "challenge_string");
    }

    #[test]
    fn verify_wechat_rejects_invalid_signature() {
        std::env::set_var("WECHAT_TOKEN", "secret_token");
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        let result = verify_wechat(
            Some("invalid_signature"),
            Some("1234567890"),
            Some("nonce123"),
            Some("echostr"),
        );

        std::env::remove_var("WECHAT_TOKEN");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "invalid_signature");
    }

    #[test]
    fn verify_wechat_requires_signature_when_token_set() {
        std::env::set_var("WECHAT_TOKEN", "token");
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        let result = verify_wechat(None, Some("ts"), Some("nonce"), Some("echo"));

        std::env::remove_var("WECHAT_TOKEN");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "missing_signature");
    }

    #[test]
    fn verify_wechat_requires_timestamp_when_token_set() {
        std::env::set_var("WECHAT_TOKEN", "token");
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        let result = verify_wechat(Some("sig"), None, Some("nonce"), Some("echo"));

        std::env::remove_var("WECHAT_TOKEN");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "missing_timestamp");
    }

    #[test]
    fn verify_wechat_requires_nonce_when_token_set() {
        std::env::set_var("WECHAT_TOKEN", "token");
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        let result = verify_wechat(Some("sig"), Some("ts"), None, Some("echo"));

        std::env::remove_var("WECHAT_TOKEN");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "missing_nonce");
    }

    #[test]
    fn verify_wechat_requires_echostr_when_token_set() {
        std::env::set_var("WECHAT_TOKEN", "token");
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        let result = verify_wechat(Some("sig"), Some("ts"), Some("nonce"), None);

        std::env::remove_var("WECHAT_TOKEN");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "missing_echostr");
    }

    #[test]
    fn verify_wechat_ignores_empty_token() {
        std::env::set_var("WECHAT_TOKEN", "   ");
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        let result = verify_wechat(
            Some("any_signature"),
            Some("ts"),
            Some("nonce"),
            Some("echostr"),
        );

        std::env::remove_var("WECHAT_TOKEN");

        // Empty token is treated as not configured, so should pass
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "echostr");
    }

    #[test]
    fn verify_wechat_signature_sort_order() {
        // Test that the signature algorithm sorts parts correctly
        // This is critical: WeChat sorts [token, timestamp, nonce, echostr] alphabetically
        let token = "zzz_token";
        let timestamp = "aaa_timestamp";
        let nonce = "mmm_nonce";
        let echostr = "bbb_echo";

        std::env::set_var("WECHAT_TOKEN", token);
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        // Sorted: ["aaa_timestamp", "bbb_echo", "mmm_nonce", "zzz_token"]
        let mut parts = vec![token, timestamp, nonce, echostr];
        parts.sort();
        assert_eq!(
            parts,
            vec!["aaa_timestamp", "bbb_echo", "mmm_nonce", "zzz_token"]
        );

        let data = parts.join("");
        let mut hasher = Sha1::new();
        hasher.update(data.as_bytes());
        let valid_signature = hex::encode(hasher.finalize());

        let result = verify_wechat(
            Some(&valid_signature),
            Some(timestamp),
            Some(nonce),
            Some(echostr),
        );

        std::env::remove_var("WECHAT_TOKEN");

        assert!(result.is_ok());
    }

    #[test]
    fn verify_wechat_with_encryption_decrypts_echostr() {
        // Test with WeChat encryption
        // EncodingAESKey is 43 chars, Base64 decode with "=" suffix gives 32 bytes
        // Using 43 'A's which decodes to 32 zero bytes
        let token = "test_token";
        let encoding_aes_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

        std::env::set_var("WECHAT_TOKEN", token);
        std::env::set_var("WECHAT_ENCODING_AES_KEY", encoding_aes_key);

        // Create a valid encrypted echostr for testing
        // The format after decryption: random(16B) + msg_len(4B) + msg + receiveid
        use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        use base64::Engine;
        type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

        let aes_key = base64::engine::general_purpose::STANDARD
            .decode(format!("{}=", encoding_aes_key))
            .unwrap();
        assert_eq!(aes_key.len(), 32, "AES key should be 32 bytes");
        let iv: [u8; 16] = aes_key[..16].try_into().unwrap();
        let key: [u8; 32] = aes_key.clone().try_into().unwrap();

        // Build plaintext: random(16B) + msg_len(4B) + msg + receiveid
        let random_bytes: [u8; 16] = [0u8; 16]; // Use zeros for determinism
        let msg = b"test_echo_response";
        let msg_len = (msg.len() as u32).to_be_bytes();
        let receiveid = b"wx5823bf96d3bd56c7";

        let mut plaintext = Vec::new();
        plaintext.extend_from_slice(&random_bytes);
        plaintext.extend_from_slice(&msg_len);
        plaintext.extend_from_slice(msg);
        plaintext.extend_from_slice(receiveid);

        // Encrypt with PKCS7 padding
        let encryptor = Aes256CbcEnc::new(&key.into(), &iv.into());
        let encrypted = encryptor.encrypt_padded_vec_mut::<Pkcs7>(&plaintext);
        let echostr = base64::engine::general_purpose::STANDARD.encode(&encrypted);

        let timestamp = "1409659813";
        let nonce = "1372623149";

        // Calculate signature: SHA1(sort([token, timestamp, nonce, echostr]))
        let mut parts = vec![token, timestamp, nonce, echostr.as_str()];
        parts.sort();
        let data = parts.join("");
        let mut hasher = Sha1::new();
        hasher.update(data.as_bytes());
        let signature = hex::encode(hasher.finalize());

        let result = verify_wechat(
            Some(&signature),
            Some(timestamp),
            Some(nonce),
            Some(&echostr),
        );

        std::env::remove_var("WECHAT_TOKEN");
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(result.unwrap(), "test_echo_response");
    }

    #[test]
    fn verify_wechat_decryption_invalid_aes_key() {
        // Clean up any existing env vars first
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        std::env::set_var("WECHAT_TOKEN", "token_for_invalid_test");
        std::env::set_var("WECHAT_ENCODING_AES_KEY", "short");

        let timestamp = "12345";
        let nonce = "nonce";
        let echostr = "c29tZWJhc2U2NGRhdGE="; // valid base64

        // Calculate signature
        let mut parts = vec!["token_for_invalid_test", timestamp, nonce, echostr];
        parts.sort();
        let data = parts.join("");
        let mut hasher = Sha1::new();
        hasher.update(data.as_bytes());
        let signature = hex::encode(hasher.finalize());

        let result = verify_wechat(
            Some(&signature),
            Some(timestamp),
            Some(nonce),
            Some(echostr),
        );

        std::env::remove_var("WECHAT_TOKEN");
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        // Should fail during decryption due to invalid key length
        assert!(result.is_err(), "Expected Err, got {:?}", result);
    }

    #[test]
    fn verify_wechat_mp_get_validates_standard_signature() {
        let token = "wechat_mp_token";
        let timestamp = "1712345678";
        let nonce = "nonce_123";
        let echostr = "challenge_echo";

        std::env::set_var("WECHAT_MP_TOKEN", token);

        let mut parts = vec![token, timestamp, nonce];
        parts.sort();
        let mut hasher = Sha1::new();
        hasher.update(parts.join("").as_bytes());
        let signature = hex::encode(hasher.finalize());

        let result = verify_wechat_mp(
            Some(&signature),
            Some(timestamp),
            Some(nonce),
            Some(echostr),
        );

        std::env::remove_var("WECHAT_MP_TOKEN");

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), echostr);
    }

    #[test]
    fn verify_wechat_mp_message_plain_mode_requires_signature_param() {
        let token = "wechat_mp_token";
        let timestamp = "1712345678";
        let nonce = "nonce_abc";
        let body = br#"<xml><ToUserName><![CDATA[gh_xxx]]></ToUserName><Content><![CDATA[hello]]></Content></xml>"#;

        std::env::set_var("WECHAT_MP_TOKEN", token);

        let mut parts = vec![token, timestamp, nonce];
        parts.sort();
        let mut hasher = Sha1::new();
        hasher.update(parts.join("").as_bytes());
        let msg_signature = hex::encode(hasher.finalize());

        let result = verify_wechat_mp_message(
            None,
            Some(&msg_signature),
            Some(timestamp),
            Some(nonce),
            body,
        );

        std::env::remove_var("WECHAT_MP_TOKEN");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "missing_signature");
    }

    #[test]
    fn verify_wechat_mp_message_safe_mode_requires_msg_signature_param() {
        let token = "wechat_mp_token";
        let timestamp = "1712345678";
        let nonce = "nonce_safe";
        let encrypt = "encrypted_payload_for_test";
        let body = format!("<xml><Encrypt><![CDATA[{encrypt}]]></Encrypt></xml>");

        std::env::set_var("WECHAT_MP_TOKEN", token);

        let mut parts = vec![token, timestamp, nonce, encrypt];
        parts.sort();
        let mut hasher = Sha1::new();
        hasher.update(parts.join("").as_bytes());
        let signature = hex::encode(hasher.finalize());

        let result = verify_wechat_mp_message(
            Some(&signature),
            None,
            Some(timestamp),
            Some(nonce),
            body.as_bytes(),
        );

        std::env::remove_var("WECHAT_MP_TOKEN");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "missing_msg_signature");
    }

    #[test]
    fn verify_wechat_mp_message_safe_mode_validates_msg_signature() {
        let token = "wechat_mp_token";
        let timestamp = "1712345678";
        let nonce = "nonce_safe_ok";
        let encrypt = "encrypt_blob_123";
        let body = format!("<xml><Encrypt><![CDATA[{encrypt}]]></Encrypt></xml>");

        std::env::set_var("WECHAT_MP_TOKEN", token);

        let mut parts = vec![token, timestamp, nonce, encrypt];
        parts.sort();
        let mut hasher = Sha1::new();
        hasher.update(parts.join("").as_bytes());
        let msg_signature = hex::encode(hasher.finalize());

        let result = verify_wechat_mp_message(
            Some("ignored_for_safe_mode"),
            Some(&msg_signature),
            Some(timestamp),
            Some(nonce),
            body.as_bytes(),
        );

        std::env::remove_var("WECHAT_MP_TOKEN");

        assert!(result.is_ok());
    }

    // ==================== WhatsApp Verification Tests ====================

    #[test]
    fn verify_whatsapp_subscription_requires_subscribe_mode() {
        std::env::set_var("WHATSAPP_VERIFY_TOKEN", "my_token");

        let result = verify_whatsapp_subscription(
            Some("webhook"), // wrong mode
            Some("my_token"),
            Some("challenge123"),
        );

        std::env::remove_var("WHATSAPP_VERIFY_TOKEN");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "invalid_mode");
    }

    #[test]
    fn verify_whatsapp_subscription_validates_token() {
        std::env::set_var("WHATSAPP_VERIFY_TOKEN", "correct_token");

        let result =
            verify_whatsapp_subscription(Some("subscribe"), Some("wrong_token"), Some("challenge"));

        std::env::remove_var("WHATSAPP_VERIFY_TOKEN");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "token_mismatch");
    }

    #[test]
    fn verify_whatsapp_subscription_success() {
        std::env::set_var("WHATSAPP_VERIFY_TOKEN", "my_secret_token");

        let result = verify_whatsapp_subscription(
            Some("subscribe"),
            Some("my_secret_token"),
            Some("hub.challenge.12345"),
        );

        std::env::remove_var("WHATSAPP_VERIFY_TOKEN");

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hub.challenge.12345");
    }

    // ==================== Lark Verification Tests ====================

    #[test]
    fn verify_lark_skips_when_no_encrypt_key() {
        std::env::remove_var("LARK_ENCRYPT_KEY");

        let headers = HeaderMap::new();
        let body = b"{}";

        let result = verify_lark(&headers, body);
        assert!(result.is_ok());
    }

    #[test]
    fn verify_lark_skips_when_empty_encrypt_key() {
        std::env::set_var("LARK_ENCRYPT_KEY", "   ");

        let headers = HeaderMap::new();
        let body = b"{}";

        let result = verify_lark(&headers, body);

        std::env::remove_var("LARK_ENCRYPT_KEY");

        assert!(result.is_ok());
    }

    #[test]
    fn verify_lark_requires_timestamp_when_key_set() {
        std::env::set_var("LARK_ENCRYPT_KEY", "test_encrypt_key");

        let headers = HeaderMap::new();
        let body = b"{}";

        let result = verify_lark(&headers, body);

        std::env::remove_var("LARK_ENCRYPT_KEY");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "missing_timestamp");
    }

    #[test]
    fn verify_lark_requires_nonce_when_key_set() {
        std::env::set_var("LARK_ENCRYPT_KEY", "test_encrypt_key");

        let mut headers = HeaderMap::new();
        headers.insert("X-Lark-Request-Timestamp", "1234567890".parse().unwrap());

        let body = b"{}";
        let result = verify_lark(&headers, body);

        std::env::remove_var("LARK_ENCRYPT_KEY");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "missing_nonce");
    }

    #[test]
    fn verify_lark_requires_signature_when_key_set() {
        std::env::set_var("LARK_ENCRYPT_KEY", "test_encrypt_key");

        let mut headers = HeaderMap::new();
        headers.insert("X-Lark-Request-Timestamp", "1234567890".parse().unwrap());
        headers.insert("X-Lark-Request-Nonce", "nonce123".parse().unwrap());

        let body = b"{}";
        let result = verify_lark(&headers, body);

        std::env::remove_var("LARK_ENCRYPT_KEY");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "missing_signature");
    }

    #[test]
    fn verify_lark_validates_signature() {
        use sha2::{Digest, Sha256};

        let encrypt_key = "my_secret_encrypt_key";
        let timestamp = "1234567890";
        let nonce = "abc123nonce";
        let body = r#"{"event": "test"}"#;

        // Compute expected signature: SHA256(timestamp + nonce + encrypt_key + body)
        let data = format!("{}{}{}{}", timestamp, nonce, encrypt_key, body);
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        let expected_signature = hex::encode(hasher.finalize());

        std::env::set_var("LARK_ENCRYPT_KEY", encrypt_key);

        let mut headers = HeaderMap::new();
        headers.insert("X-Lark-Request-Timestamp", timestamp.parse().unwrap());
        headers.insert("X-Lark-Request-Nonce", nonce.parse().unwrap());
        headers.insert("X-Lark-Signature", expected_signature.parse().unwrap());

        let result = verify_lark(&headers, body.as_bytes());

        std::env::remove_var("LARK_ENCRYPT_KEY");

        assert!(result.is_ok());
    }

    #[test]
    fn verify_lark_rejects_invalid_signature() {
        std::env::set_var("LARK_ENCRYPT_KEY", "secret_key");

        let mut headers = HeaderMap::new();
        headers.insert("X-Lark-Request-Timestamp", "1234567890".parse().unwrap());
        headers.insert("X-Lark-Request-Nonce", "nonce".parse().unwrap());
        headers.insert("X-Lark-Signature", "wrong_signature".parse().unwrap());

        let body = b"{}";
        let result = verify_lark(&headers, body);

        std::env::remove_var("LARK_ENCRYPT_KEY");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "invalid_signature");
    }

    #[test]
    fn verify_lark_challenge_detects_url_verification() {
        let body = r#"{"type": "url_verification", "challenge": "test_challenge_string", "token": "verification_token"}"#;

        let result = verify_lark_challenge(body.as_bytes());

        assert!(result.is_some());
        assert_eq!(result.unwrap(), "test_challenge_string");
    }

    #[test]
    fn verify_lark_challenge_detects_schema_2_format() {
        let body = r#"{"challenge": "challenge_value", "token": "some_token"}"#;

        let result = verify_lark_challenge(body.as_bytes());

        assert!(result.is_some());
        assert_eq!(result.unwrap(), "challenge_value");
    }

    #[test]
    fn verify_lark_challenge_ignores_regular_events() {
        let body = r#"{"schema": "2.0", "event": {"message": {}}}"#;

        let result = verify_lark_challenge(body.as_bytes());

        assert!(result.is_none());
    }

    #[test]
    fn verify_lark_challenge_ignores_invalid_json() {
        let body = b"not valid json";

        let result = verify_lark_challenge(body);

        assert!(result.is_none());
    }

    #[test]
    fn verify_lark_challenge_requires_token_for_schema_2() {
        // challenge without token should not be treated as URL verification
        let body = r#"{"challenge": "challenge_value"}"#;

        let result = verify_lark_challenge(body.as_bytes());

        assert!(result.is_none());
    }
}
