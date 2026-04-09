//! WeChat Official Account (微信公众号) adapter.
//!
//! This adapter is intentionally separate from enterprise WeCom (`wechat`) to avoid
//! mixing identity/routing semantics (`openid` vs corp user id).

use serde::Deserialize;
use std::sync::RwLock;
use tracing::info;

use crate::channel::{
    AdapterError, Channel, ChannelMetadata, InboundAdapter, InboundMessage, OutboundAdapter,
    OutboundMessage, SendResult,
};

#[derive(Debug, Clone, Default)]
pub struct WeChatMpInboundAdapter;

impl WeChatMpInboundAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl InboundAdapter for WeChatMpInboundAdapter {
    fn parse(&self, raw_payload: &[u8]) -> Result<InboundMessage, AdapterError> {
        let payload_str = std::str::from_utf8(raw_payload)
            .map_err(|e| AdapterError::ParseError(format!("invalid UTF-8: {}", e)))?;

        let xml_to_parse = if is_encrypted_wechat_mp_message(payload_str) {
            decrypt_wechat_mp_message(payload_str)?
        } else {
            payload_str.to_string()
        };

        let msg = parse_wechat_mp_xml(&xml_to_parse)?;
        if msg.msg_type != "text" {
            return Err(AdapterError::ParseError(format!(
                "unsupported message type: {}",
                msg.msg_type
            )));
        }

        let sender = msg.from_user_name.clone();
        let recipient = msg.to_user_name.clone();
        let thread_id = format!("wechat_mp:{}:{}", recipient, sender);

        Ok(InboundMessage {
            channel: Channel::WeChatMp,
            sender: sender.clone(),
            sender_name: None,
            recipient,
            subject: None,
            text_body: Some(msg.content.clone()),
            html_body: None,
            thread_id,
            message_id: msg.msg_id.clone(),
            attachments: vec![],
            reply_to: vec![sender],
            raw_payload: raw_payload.to_vec(),
            metadata: ChannelMetadata {
                wechat_mp_app_id: Some(msg.to_user_name),
                wechat_mp_open_id: Some(msg.from_user_name),
                wechat_mp_msg_type: Some(msg.msg_type),
                ..Default::default()
            },
        })
    }

    fn channel(&self) -> Channel {
        Channel::WeChatMp
    }
}

#[derive(Debug)]
pub struct WeChatMpOutboundAdapter {
    pub app_id: String,
    pub app_secret: String,
    access_token_cache: RwLock<Option<CachedAccessToken>>,
}

#[derive(Debug, Clone)]
struct CachedAccessToken {
    token: String,
    expires_at: std::time::Instant,
}

impl WeChatMpOutboundAdapter {
    pub fn new(app_id: String, app_secret: String) -> Self {
        Self {
            app_id,
            app_secret,
            access_token_cache: RwLock::new(None),
        }
    }

    pub fn from_env() -> Result<Self, AdapterError> {
        let app_id = std::env::var("WECHAT_MP_APP_ID")
            .map_err(|_| AdapterError::ConfigError("WECHAT_MP_APP_ID not set".to_string()))?;
        let app_secret = std::env::var("WECHAT_MP_APP_SECRET")
            .map_err(|_| AdapterError::ConfigError("WECHAT_MP_APP_SECRET not set".to_string()))?;
        Ok(Self::new(app_id, app_secret))
    }

    fn get_access_token(&self) -> Result<String, AdapterError> {
        {
            let cache = self.access_token_cache.read().unwrap();
            if let Some(cached) = cache.as_ref() {
                if cached.expires_at > std::time::Instant::now() {
                    return Ok(cached.token.clone());
                }
            }
        }

        let url = format!(
            "https://api.weixin.qq.com/cgi-bin/token?grant_type=client_credential&appid={}&secret={}",
            self.app_id, self.app_secret
        );

        let client = reqwest::blocking::Client::new();
        let response: WeChatMpAccessTokenResponse = client
            .get(&url)
            .send()
            .map_err(|e| AdapterError::SendError(format!("token request failed: {}", e)))?
            .json()
            .map_err(|e| AdapterError::SendError(format!("token parse failed: {}", e)))?;

        if response.errcode.unwrap_or(0) != 0 {
            return Err(AdapterError::SendError(format!(
                "WeChat MP token error {}: {}",
                response.errcode.unwrap_or_default(),
                response.errmsg.unwrap_or_default()
            )));
        }

        let token = response
            .access_token
            .ok_or_else(|| AdapterError::SendError("no access_token in response".to_string()))?;
        let expires_in = response.expires_in.unwrap_or(7200).max(300);
        let safety_window = 120u64;
        let ttl = (expires_in as u64).saturating_sub(safety_window).max(60);

        {
            let mut cache = self.access_token_cache.write().unwrap();
            *cache = Some(CachedAccessToken {
                token: token.clone(),
                expires_at: std::time::Instant::now() + std::time::Duration::from_secs(ttl),
            });
        }

        Ok(token)
    }
}

impl OutboundAdapter for WeChatMpOutboundAdapter {
    fn send(&self, message: &OutboundMessage) -> Result<SendResult, AdapterError> {
        let open_id = message
            .to
            .first()
            .ok_or_else(|| AdapterError::ConfigError("no recipient specified".to_string()))?;

        let access_token = self.get_access_token()?;
        let text = if message.text_body.is_empty() {
            message.html_body.clone()
        } else {
            message.text_body.clone()
        };

        let request = WeChatMpSendMessageRequest {
            touser: open_id.clone(),
            msgtype: "text".to_string(),
            text: WeChatMpTextContent { content: text },
        };

        let url = format!(
            "https://api.weixin.qq.com/cgi-bin/message/custom/send?access_token={}",
            access_token
        );

        let client = reqwest::blocking::Client::new();
        let response: WeChatMpSendResponse = client
            .post(&url)
            .json(&request)
            .send()
            .map_err(|e| AdapterError::SendError(format!("send request failed: {}", e)))?
            .json()
            .map_err(|e| AdapterError::SendError(format!("response parse failed: {}", e)))?;

        if response.errcode != 0 {
            return Ok(SendResult {
                success: false,
                message_id: String::new(),
                submitted_at: String::new(),
                error: Some(format!(
                    "WeChat MP error {}: {}",
                    response.errcode,
                    response.errmsg.unwrap_or_default()
                )),
            });
        }

        info!("sent WeChat MP message to openid {}", open_id);

        Ok(SendResult {
            success: true,
            message_id: response.msgid.unwrap_or_default(),
            submitted_at: chrono::Utc::now().to_rfc3339(),
            error: None,
        })
    }

    fn channel(&self) -> Channel {
        Channel::WeChatMp
    }
}

#[derive(Debug, Clone)]
struct WeChatMpMessage {
    to_user_name: String,
    from_user_name: String,
    msg_type: String,
    content: String,
    msg_id: Option<String>,
}

fn parse_wechat_mp_xml(xml: &str) -> Result<WeChatMpMessage, AdapterError> {
    let to_user_name = extract_xml_tag(xml, "ToUserName")
        .ok_or_else(|| AdapterError::MissingField("ToUserName"))?;
    let from_user_name = extract_xml_tag(xml, "FromUserName")
        .ok_or_else(|| AdapterError::MissingField("FromUserName"))?;
    let msg_type = extract_xml_tag(xml, "MsgType")
        .ok_or_else(|| AdapterError::MissingField("MsgType"))?
        .to_lowercase();

    let content = extract_xml_tag(xml, "Content").unwrap_or_default();
    let msg_id = extract_xml_tag(xml, "MsgId");

    Ok(WeChatMpMessage {
        to_user_name,
        from_user_name,
        msg_type,
        content,
        msg_id,
    })
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let cdata_start = format!("<{}><![CDATA[", tag);
    let cdata_end = format!("]]></{}>", tag);
    if let Some(start) = xml.find(&cdata_start) {
        let content_start = start + cdata_start.len();
        if let Some(end) = xml[content_start..].find(&cdata_end) {
            return Some(xml[content_start..content_start + end].to_string());
        }
    }

    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    if let Some(start) = xml.find(&start_tag) {
        let content_start = start + start_tag.len();
        if let Some(end) = xml[content_start..].find(&end_tag) {
            return Some(xml[content_start..content_start + end].trim().to_string());
        }
    }

    None
}

fn is_encrypted_wechat_mp_message(xml: &str) -> bool {
    xml.contains("<Encrypt>")
}

fn decrypt_wechat_mp_message(xml: &str) -> Result<String, AdapterError> {
    use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    use base64::Engine;

    type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

    let encrypt_start = xml
        .find("<Encrypt><![CDATA[")
        .ok_or_else(|| AdapterError::ParseError("missing Encrypt tag".to_string()))?;
    let content_start = encrypt_start + "<Encrypt><![CDATA[".len();
    let content_end = xml[content_start..]
        .find("]]></Encrypt>")
        .ok_or_else(|| AdapterError::ParseError("malformed Encrypt tag".to_string()))?;
    let encrypted_base64 = &xml[content_start..content_start + content_end];

    let encoding_aes_key = std::env::var("WECHAT_MP_ENCODING_AES_KEY")
        .map_err(|_| AdapterError::ConfigError("WECHAT_MP_ENCODING_AES_KEY not set".to_string()))?;
    let aes_key_b64 = format!("{}=", encoding_aes_key.trim());

    let lenient_engine = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    let aes_key = lenient_engine
        .decode(&aes_key_b64)
        .map_err(|e| AdapterError::ParseError(format!("invalid encoding_aes_key: {}", e)))?;

    if aes_key.len() != 32 {
        return Err(AdapterError::ParseError(format!(
            "invalid aes key length: {} (expected 32)",
            aes_key.len()
        )));
    }

    let encrypted = base64::engine::general_purpose::STANDARD
        .decode(encrypted_base64)
        .map_err(|e| AdapterError::ParseError(format!("invalid base64: {}", e)))?;

    let iv: [u8; 16] = aes_key[..16]
        .try_into()
        .map_err(|_| AdapterError::ParseError("iv error".to_string()))?;
    let key: [u8; 32] = aes_key
        .try_into()
        .map_err(|_| AdapterError::ParseError("key error".to_string()))?;

    let mut buf = encrypted.clone();
    let decryptor = Aes256CbcDec::new(&key.into(), &iv.into());
    let decrypted = decryptor
        .decrypt_padded_mut::<NoPadding>(&mut buf)
        .map_err(|_| AdapterError::ParseError("decryption failed".to_string()))?;
    let decrypted = remove_pkcs7_padding(decrypted)
        .map_err(|e| AdapterError::ParseError(format!("padding error: {}", e)))?;

    if decrypted.len() < 20 {
        return Err(AdapterError::ParseError(
            "decrypted content too short".to_string(),
        ));
    }

    let content = &decrypted[16..];
    let msg_len = u32::from_be_bytes(
        content[0..4]
            .try_into()
            .map_err(|_| AdapterError::ParseError("msg_len parse error".to_string()))?,
    ) as usize;

    if content.len() < 4 + msg_len {
        return Err(AdapterError::ParseError(format!(
            "msg length mismatch: expected {}, have {}",
            msg_len,
            content.len() - 4
        )));
    }

    let msg = &content[4..4 + msg_len];
    String::from_utf8(msg.to_vec())
        .map_err(|_| AdapterError::ParseError("invalid utf8 in decrypted message".to_string()))
}

fn remove_pkcs7_padding(data: &[u8]) -> Result<&[u8], &'static str> {
    if data.is_empty() {
        return Err("empty_data");
    }
    let padding_len = data[data.len() - 1] as usize;
    if padding_len == 0 || padding_len > 32 || padding_len > data.len() {
        return Err("invalid_padding");
    }
    for &byte in &data[data.len() - padding_len..] {
        if byte as usize != padding_len {
            return Err("invalid_padding_bytes");
        }
    }
    Ok(&data[..data.len() - padding_len])
}

#[derive(Debug, Deserialize)]
struct WeChatMpAccessTokenResponse {
    access_token: Option<String>,
    expires_in: Option<i64>,
    errcode: Option<i64>,
    errmsg: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct WeChatMpSendMessageRequest {
    touser: String,
    msgtype: String,
    text: WeChatMpTextContent,
}

#[derive(Debug, serde::Serialize)]
struct WeChatMpTextContent {
    content: String,
}

#[derive(Debug, Deserialize)]
struct WeChatMpSendResponse {
    errcode: i64,
    errmsg: Option<String>,
    msgid: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_wechat_mp_text_xml() {
        let xml = r#"<xml>
<ToUserName><![CDATA[gh_xxx]]></ToUserName>
<FromUserName><![CDATA[oOpenId123]]></FromUserName>
<CreateTime>1712540000</CreateTime>
<MsgType><![CDATA[text]]></MsgType>
<Content><![CDATA[hello mp]]></Content>
<MsgId>1234567890123456</MsgId>
</xml>"#;

        let msg = parse_wechat_mp_xml(xml).unwrap();
        assert_eq!(msg.to_user_name, "gh_xxx");
        assert_eq!(msg.from_user_name, "oOpenId123");
        assert_eq!(msg.msg_type, "text");
        assert_eq!(msg.content, "hello mp");
        assert_eq!(msg.msg_id.as_deref(), Some("1234567890123456"));
    }

    #[test]
    fn extract_xml_tag_supports_plain_text_nodes() {
        let xml = "<xml><MsgType>text</MsgType><Content>hello</Content></xml>";
        assert_eq!(extract_xml_tag(xml, "MsgType").as_deref(), Some("text"));
        assert_eq!(extract_xml_tag(xml, "Content").as_deref(), Some("hello"));
    }

    #[test]
    fn adapter_channel_is_wechat_mp() {
        let adapter = WeChatMpInboundAdapter::new();
        assert_eq!(adapter.channel(), Channel::WeChatMp);
    }
}
