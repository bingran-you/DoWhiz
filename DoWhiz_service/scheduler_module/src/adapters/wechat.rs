//! WeChat Work (企业微信) adapter for inbound and outbound messages via qyapi.
//!
//! This module provides adapters for handling WeChat Work messages:
//! - `WeChatInboundAdapter`: Parses WeChat webhook payloads (XML)
//! - `WeChatOutboundAdapter`: Sends messages via WeChat Work API

use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use tracing::info;

use crate::channel::{
    AdapterError, Channel, ChannelMetadata, InboundAdapter, InboundMessage, OutboundAdapter,
    OutboundMessage, SendResult,
};

/// Adapter for parsing WeChat Work webhook payloads.
#[derive(Debug, Clone, Default)]
pub struct WeChatInboundAdapter;

impl WeChatInboundAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl InboundAdapter for WeChatInboundAdapter {
    fn parse(&self, raw_payload: &[u8]) -> Result<InboundMessage, AdapterError> {
        let payload_str = std::str::from_utf8(raw_payload)
            .map_err(|e| AdapterError::ParseError(format!("invalid UTF-8: {}", e)))?;

        // Check if message is encrypted and decrypt if needed
        let xml_to_parse = if is_encrypted_wechat_message(payload_str) {
            decrypt_wechat_message(payload_str)?
        } else {
            payload_str.to_string()
        };

        // Parse XML payload
        let msg = parse_wechat_xml(&xml_to_parse)?;

        // Only handle text messages for now
        if msg.msg_type != "text" {
            return Err(AdapterError::ParseError(format!(
                "unsupported message type: {}",
                msg.msg_type
            )));
        }

        let sender = msg.from_user_name.clone();
        let thread_id = format!("wechat:{}:{}", msg.to_user_name, sender);

        Ok(InboundMessage {
            channel: Channel::WeChat,
            sender: sender.clone(),
            sender_name: None, // WeChat doesn't provide display name in webhook
            recipient: msg.to_user_name.clone(),
            subject: None,
            text_body: Some(msg.content.clone()),
            html_body: None,
            thread_id,
            message_id: Some(msg.msg_id.clone()),
            attachments: vec![],
            reply_to: vec![sender],
            raw_payload: raw_payload.to_vec(),
            metadata: ChannelMetadata {
                wechat_corp_id: Some(msg.to_user_name),
                wechat_user_id: Some(msg.from_user_name),
                wechat_agent_id: Some(msg.agent_id.to_string()),
                ..Default::default()
            },
        })
    }

    fn channel(&self) -> Channel {
        Channel::WeChat
    }
}

/// Adapter for sending messages via WeChat Work API.
#[derive(Debug)]
pub struct WeChatOutboundAdapter {
    pub corp_id: String,
    pub agent_id: String,
    pub secret: String,
    access_token_cache: RwLock<Option<CachedAccessToken>>,
}

#[derive(Debug, Clone)]
struct CachedAccessToken {
    token: String,
    expires_at: std::time::Instant,
}

impl WeChatOutboundAdapter {
    pub fn new(corp_id: String, agent_id: String, secret: String) -> Self {
        Self {
            corp_id,
            agent_id,
            secret,
            access_token_cache: RwLock::new(None),
        }
    }

    pub fn from_env() -> Result<Self, AdapterError> {
        let corp_id = std::env::var("WECHAT_CORP_ID")
            .map_err(|_| AdapterError::ConfigError("WECHAT_CORP_ID not set".to_string()))?;
        let agent_id = std::env::var("WECHAT_AGENT_ID")
            .map_err(|_| AdapterError::ConfigError("WECHAT_AGENT_ID not set".to_string()))?;
        let secret = std::env::var("WECHAT_SECRET")
            .map_err(|_| AdapterError::ConfigError("WECHAT_SECRET not set".to_string()))?;

        Ok(Self::new(corp_id, agent_id, secret))
    }

    /// Get access token, refreshing if expired.
    fn get_access_token(&self) -> Result<String, AdapterError> {
        // Check cache first
        {
            let cache = self.access_token_cache.read().unwrap();
            if let Some(ref cached) = *cache {
                if cached.expires_at > std::time::Instant::now() {
                    return Ok(cached.token.clone());
                }
            }
        }

        // Fetch new token with secrets
        let url = format!(
            "https://qyapi.weixin.qq.com/cgi-bin/gettoken?corpid={}&corpsecret={}",
            self.corp_id, self.secret
        );

        let client = reqwest::blocking::Client::new();
        let response: WeChatAccessTokenResponse = client
            .get(&url)
            .send()
            .map_err(|e| AdapterError::SendError(format!("token request failed: {}", e)))?
            .json()
            .map_err(|e| AdapterError::SendError(format!("token parse failed: {}", e)))?;

        if response.errcode != 0 {
            return Err(AdapterError::SendError(format!(
                "WeChat token error {}: {}",
                response.errcode,
                response.errmsg.unwrap_or_default()
            )));
        }

        let token = response
            .access_token
            .ok_or_else(|| AdapterError::SendError("no access_token in response".to_string()))?;

        // Cache with 110 minute expiry (tokens last 2 hours, refresh early)
        let expires_at = std::time::Instant::now() + std::time::Duration::from_secs(110 * 60);
        {
            let mut cache = self.access_token_cache.write().unwrap();
            *cache = Some(CachedAccessToken {
                token: token.clone(),
                expires_at,
            });
        }

        Ok(token)
    }
}

impl OutboundAdapter for WeChatOutboundAdapter {
    fn send(&self, message: &OutboundMessage) -> Result<SendResult, AdapterError> {
        let access_token = self.get_access_token()?;

        let user_id = message
            .to
            .first()
            .ok_or_else(|| AdapterError::ConfigError("no recipient specified".to_string()))?;

        let text = if message.text_body.is_empty() {
            message.html_body.clone()
        } else {
            message.text_body.clone()
        };

        let request = WeChatSendMessageRequest {
            touser: user_id.clone(),
            msgtype: "text".to_string(),
            agentid: self.agent_id.parse().unwrap_or(1),
            text: WeChatTextContent { content: text },
        };

        let url = format!(
            "https://qyapi.weixin.qq.com/cgi-bin/message/send?access_token={}",
            access_token
        );

        let client = reqwest::blocking::Client::new();
        let response: WeChatSendResponse = client
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
                    "WeChat error {}: {}",
                    response.errcode,
                    response.errmsg.unwrap_or_default()
                )),
            });
        }

        info!("sent WeChat message to user {}", user_id);

        Ok(SendResult {
            success: true,
            message_id: response.msgid.unwrap_or_default(),
            submitted_at: chrono::Utc::now().to_rfc3339(),
            error: None,
        })
    }

    fn channel(&self) -> Channel {
        Channel::WeChat
    }
}

// ============================================================================
// XML Parsing
// ============================================================================

/// Parsed WeChat message from XML.
#[derive(Debug, Clone)]
pub struct WeChatMessage {
    pub to_user_name: String,
    pub from_user_name: String,
    pub create_time: i64,
    pub msg_type: String,
    pub content: String,
    pub msg_id: String,
    pub agent_id: i64,
}

/// Parse WeChat XML payload into structured message.
fn parse_wechat_xml(xml: &str) -> Result<WeChatMessage, AdapterError> {
    // Simple XML parsing without external crate
    // WeChat XML format:
    // <xml>
    //   <ToUserName><![CDATA[corp_id]]></ToUserName>
    //   <FromUserName><![CDATA[user_id]]></FromUserName>
    //   <CreateTime>1348831860</CreateTime>
    //   <MsgType><![CDATA[text]]></MsgType>
    //   <Content><![CDATA[message content]]></Content>
    //   <MsgId>1234567890123456</MsgId>
    //   <AgentID>1</AgentID>
    // </xml>

    fn extract_cdata(xml: &str, tag: &str) -> Option<String> {
        let start_tag = format!("<{}>", tag);
        let end_tag = format!("</{}>", tag);

        let start = xml.find(&start_tag)? + start_tag.len();
        let end = xml.find(&end_tag)?;
        let content = &xml[start..end];

        // Handle CDATA
        if content.starts_with("<![CDATA[") && content.ends_with("]]>") {
            Some(content[9..content.len() - 3].to_string())
        } else {
            Some(content.trim().to_string())
        }
    }

    fn extract_value(xml: &str, tag: &str) -> Option<String> {
        let start_tag = format!("<{}>", tag);
        let end_tag = format!("</{}>", tag);

        let start = xml.find(&start_tag)? + start_tag.len();
        let end = xml.find(&end_tag)?;
        Some(xml[start..end].trim().to_string())
    }

    let to_user_name =
        extract_cdata(xml, "ToUserName").ok_or_else(|| AdapterError::MissingField("ToUserName"))?;
    let from_user_name = extract_cdata(xml, "FromUserName")
        .ok_or_else(|| AdapterError::MissingField("FromUserName"))?;
    let create_time = extract_value(xml, "CreateTime")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let msg_type =
        extract_cdata(xml, "MsgType").ok_or_else(|| AdapterError::MissingField("MsgType"))?;
    let content = extract_cdata(xml, "Content").unwrap_or_default();
    let msg_id = extract_value(xml, "MsgId").unwrap_or_default();
    let agent_id = extract_value(xml, "AgentID")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    Ok(WeChatMessage {
        to_user_name,
        from_user_name,
        create_time,
        msg_type,
        content,
        msg_id,
        agent_id,
    })
}

// ============================================================================
// WeChat Message Decryption
// ============================================================================

/// Decrypt an encrypted WeChat message payload.
///
/// WeChat Work sends encrypted messages in this format:
/// ```xml
/// <xml>
///   <ToUserName><![CDATA[corp_id]]></ToUserName>
///   <Encrypt><![CDATA[base64_encrypted_content]]></Encrypt>
/// </xml>
/// ```
///
/// This function extracts the Encrypt content, decrypts it using AES-256-CBC,
/// and returns the decrypted XML containing the actual message fields.
pub fn decrypt_wechat_message(xml: &str) -> Result<String, AdapterError> {
    use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    use base64::Engine;
    type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

    // Extract the encrypted content from <Encrypt> tag
    let encrypt_start = xml.find("<Encrypt><![CDATA[")
        .ok_or_else(|| AdapterError::ParseError("missing Encrypt tag".to_string()))?;
    let content_start = encrypt_start + "<Encrypt><![CDATA[".len();
    let content_end = xml[content_start..].find("]]></Encrypt>")
        .ok_or_else(|| AdapterError::ParseError("malformed Encrypt tag".to_string()))?;
    let encrypted_base64 = &xml[content_start..content_start + content_end];

    // Get the EncodingAESKey from environment
    let encoding_aes_key = std::env::var("WECHAT_ENCODING_AES_KEY")
        .map_err(|_| AdapterError::ConfigError("WECHAT_ENCODING_AES_KEY not set".to_string()))?;

    // Derive AESKey: Base64_Decode(EncodingAESKey + "=")
    let trimmed_key = encoding_aes_key.trim();
    let aes_key_b64 = format!("{}=", trimmed_key);

    let lenient_engine = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    let aes_key = lenient_engine.decode(&aes_key_b64)
        .map_err(|e| AdapterError::ParseError(format!("invalid encoding_aes_key: {}", e)))?;

    if aes_key.len() != 32 {
        return Err(AdapterError::ParseError(format!(
            "invalid aes key length: {} (expected 32)",
            aes_key.len()
        )));
    }

    // Decode the encrypted content from Base64
    let encrypted = base64::engine::general_purpose::STANDARD
        .decode(encrypted_base64)
        .map_err(|e| AdapterError::ParseError(format!("invalid base64: {}", e)))?;

    // IV is first 16 bytes of AESKey
    let iv: [u8; 16] = aes_key[..16].try_into()
        .map_err(|_| AdapterError::ParseError("iv error".to_string()))?;
    let key: [u8; 32] = aes_key.try_into()
        .map_err(|_| AdapterError::ParseError("key error".to_string()))?;

    // Decrypt using AES-256-CBC
    let mut buf = encrypted.clone();
    let decryptor = Aes256CbcDec::new(&key.into(), &iv.into());
    let decrypted = decryptor
        .decrypt_padded_mut::<NoPadding>(&mut buf)
        .map_err(|_| AdapterError::ParseError("decryption failed".to_string()))?;

    // Remove PKCS#7 padding
    let decrypted = remove_pkcs7_padding(decrypted)
        .map_err(|e| AdapterError::ParseError(format!("padding error: {}", e)))?;

    // Message format: random(16B) + msg_len(4B big-endian) + msg + receiveid
    if decrypted.len() < 20 {
        return Err(AdapterError::ParseError("decrypted content too short".to_string()));
    }

    // Skip 16 random bytes
    let content = &decrypted[16..];

    // Read msg_len (4 bytes, big endian)
    let msg_len = u32::from_be_bytes(
        content[0..4].try_into()
            .map_err(|_| AdapterError::ParseError("msg_len parse error".to_string()))?
    ) as usize;

    if content.len() < 4 + msg_len {
        return Err(AdapterError::ParseError(format!(
            "msg length mismatch: expected {}, have {}",
            msg_len,
            content.len() - 4
        )));
    }

    // Extract the message (the decrypted XML)
    let msg = &content[4..4 + msg_len];

    String::from_utf8(msg.to_vec())
        .map_err(|_| AdapterError::ParseError("invalid utf8 in decrypted message".to_string()))
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

/// Check if a WeChat XML payload is encrypted (contains <Encrypt> tag)
pub fn is_encrypted_wechat_message(xml: &str) -> bool {
    xml.contains("<Encrypt>")
}

// ============================================================================
// WeChat API Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct WeChatAccessTokenResponse {
    #[serde(default)]
    errcode: i32,
    errmsg: Option<String>,
    access_token: Option<String>,
    expires_in: Option<i64>,
}

#[derive(Debug, Serialize)]
struct WeChatSendMessageRequest {
    touser: String,
    msgtype: String,
    agentid: i64,
    text: WeChatTextContent,
}

#[derive(Debug, Serialize)]
struct WeChatTextContent {
    content: String,
}

#[derive(Debug, Deserialize)]
struct WeChatSendResponse {
    #[serde(default)]
    errcode: i32,
    errmsg: Option<String>,
    msgid: Option<String>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_message() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[ww1234567890]]></ToUserName>
            <FromUserName><![CDATA[zhangsan]]></FromUserName>
            <CreateTime>1348831860</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[Hello from WeChat!]]></Content>
            <MsgId>1234567890123456</MsgId>
            <AgentID>1</AgentID>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let message = adapter.parse(xml.as_bytes()).unwrap();

        assert_eq!(message.channel, Channel::WeChat);
        assert_eq!(message.sender, "zhangsan");
        assert_eq!(message.text_body, Some("Hello from WeChat!".to_string()));
        assert_eq!(
            message.metadata.wechat_corp_id,
            Some("ww1234567890".to_string())
        );
        assert_eq!(
            message.metadata.wechat_user_id,
            Some("zhangsan".to_string())
        );
        assert_eq!(message.metadata.wechat_agent_id, Some("1".to_string()));
    }

    #[test]
    fn parse_xml_helper() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[corp123]]></ToUserName>
            <FromUserName><![CDATA[user456]]></FromUserName>
            <CreateTime>1600000000</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[Test message]]></Content>
            <MsgId>999</MsgId>
            <AgentID>2</AgentID>
        </xml>"#;

        let msg = parse_wechat_xml(xml).unwrap();
        assert_eq!(msg.to_user_name, "corp123");
        assert_eq!(msg.from_user_name, "user456");
        assert_eq!(msg.create_time, 1600000000);
        assert_eq!(msg.msg_type, "text");
        assert_eq!(msg.content, "Test message");
        assert_eq!(msg.msg_id, "999");
        assert_eq!(msg.agent_id, 2);
    }

    #[test]
    fn reject_non_text_message() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[corp123]]></ToUserName>
            <FromUserName><![CDATA[user456]]></FromUserName>
            <CreateTime>1600000000</CreateTime>
            <MsgType><![CDATA[image]]></MsgType>
            <MsgId>999</MsgId>
            <AgentID>2</AgentID>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let result = adapter.parse(xml.as_bytes());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unsupported message type"));
    }

    #[test]
    fn reject_voice_message() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[corp123]]></ToUserName>
            <FromUserName><![CDATA[user456]]></FromUserName>
            <CreateTime>1600000000</CreateTime>
            <MsgType><![CDATA[voice]]></MsgType>
            <MsgId>999</MsgId>
            <AgentID>2</AgentID>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let result = adapter.parse(xml.as_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn reject_event_message() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[corp123]]></ToUserName>
            <FromUserName><![CDATA[user456]]></FromUserName>
            <CreateTime>1600000000</CreateTime>
            <MsgType><![CDATA[event]]></MsgType>
            <Event><![CDATA[subscribe]]></Event>
            <AgentID>2</AgentID>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let result = adapter.parse(xml.as_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn parse_message_with_chinese_content() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[ww企业ID]]></ToUserName>
            <FromUserName><![CDATA[张三]]></FromUserName>
            <CreateTime>1600000000</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[你好，请帮我处理这个任务]]></Content>
            <MsgId>12345</MsgId>
            <AgentID>1000002</AgentID>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let message = adapter.parse(xml.as_bytes()).unwrap();
        assert_eq!(
            message.text_body,
            Some("你好，请帮我处理这个任务".to_string())
        );
        assert_eq!(message.sender, "张三");
    }

    #[test]
    fn parse_message_with_multiline_content() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[corp123]]></ToUserName>
            <FromUserName><![CDATA[user456]]></FromUserName>
            <CreateTime>1600000000</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[Line 1
Line 2
Line 3]]></Content>
            <MsgId>999</MsgId>
            <AgentID>2</AgentID>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let message = adapter.parse(xml.as_bytes()).unwrap();
        assert!(message.text_body.as_ref().unwrap().contains("Line 1"));
        assert!(message.text_body.as_ref().unwrap().contains("Line 2"));
        assert!(message.text_body.as_ref().unwrap().contains("Line 3"));
    }

    #[test]
    fn thread_id_format() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[ww12345]]></ToUserName>
            <FromUserName><![CDATA[zhangsan]]></FromUserName>
            <CreateTime>1600000000</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[Test]]></Content>
            <MsgId>999</MsgId>
            <AgentID>1</AgentID>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let message = adapter.parse(xml.as_bytes()).unwrap();
        assert_eq!(message.thread_id, "wechat:ww12345:zhangsan");
    }

    #[test]
    fn adapter_channel_is_wechat() {
        let adapter = WeChatInboundAdapter::new();
        assert_eq!(adapter.channel(), Channel::WeChat);
    }

    #[test]
    fn parse_invalid_utf8() {
        let invalid_bytes = vec![0xff, 0xfe, 0x00, 0x01];
        let adapter = WeChatInboundAdapter::new();
        let result = adapter.parse(&invalid_bytes);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("UTF-8"));
    }

    #[test]
    fn parse_invalid_xml() {
        let invalid_xml = b"<xml><broken";
        let adapter = WeChatInboundAdapter::new();
        let result = adapter.parse(invalid_xml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_required_fields() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[corp123]]></ToUserName>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let result = adapter.parse(xml.as_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_content() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[corp123]]></ToUserName>
            <FromUserName><![CDATA[user456]]></FromUserName>
            <CreateTime>1600000000</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[]]></Content>
            <MsgId>999</MsgId>
            <AgentID>2</AgentID>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let message = adapter.parse(xml.as_bytes()).unwrap();
        assert_eq!(message.text_body, Some("".to_string()));
    }

    #[test]
    fn metadata_fields_populated() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[ww9876543210]]></ToUserName>
            <FromUserName><![CDATA[lisi]]></FromUserName>
            <CreateTime>1600000000</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[Hello]]></Content>
            <MsgId>55555</MsgId>
            <AgentID>1000005</AgentID>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let message = adapter.parse(xml.as_bytes()).unwrap();

        assert_eq!(
            message.metadata.wechat_corp_id,
            Some("ww9876543210".to_string())
        );
        assert_eq!(message.metadata.wechat_user_id, Some("lisi".to_string()));
        assert_eq!(
            message.metadata.wechat_agent_id,
            Some("1000005".to_string())
        );
    }

    #[test]
    fn reply_to_is_sender() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[corp]]></ToUserName>
            <FromUserName><![CDATA[sender123]]></FromUserName>
            <CreateTime>1600000000</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[Hi]]></Content>
            <MsgId>1</MsgId>
            <AgentID>1</AgentID>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let message = adapter.parse(xml.as_bytes()).unwrap();
        assert_eq!(message.reply_to, vec!["sender123".to_string()]);
    }

    #[test]
    fn raw_payload_preserved() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[corp]]></ToUserName>
            <FromUserName><![CDATA[user]]></FromUserName>
            <CreateTime>1600000000</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[Test]]></Content>
            <MsgId>1</MsgId>
            <AgentID>1</AgentID>
        </xml>"#;

        let adapter = WeChatInboundAdapter::new();
        let message = adapter.parse(xml.as_bytes()).unwrap();
        assert_eq!(message.raw_payload, xml.as_bytes());
    }

    // ==================== Outbound Adapter Tests ====================

    #[test]
    fn outbound_adapter_channel_is_wechat() {
        let adapter = WeChatOutboundAdapter::new(
            "corp123".to_string(),
            "1000001".to_string(),
            "secret".to_string(),
        );
        assert_eq!(adapter.channel(), Channel::WeChat);
    }

    #[test]
    fn outbound_adapter_caches_token() {
        let adapter = WeChatOutboundAdapter::new(
            "corp123".to_string(),
            "1000001".to_string(),
            "secret".to_string(),
        );
        // Initially no cached token
        {
            let cache = adapter.access_token_cache.read().unwrap();
            assert!(cache.is_none());
        }
    }

    // ==================== XML Helper Tests ====================

    #[test]
    fn parse_wechat_xml_extracts_all_fields() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[to_corp]]></ToUserName>
            <FromUserName><![CDATA[from_user]]></FromUserName>
            <CreateTime>1234567890</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[Message content here]]></Content>
            <MsgId>9999999</MsgId>
            <AgentID>42</AgentID>
        </xml>"#;

        let msg = parse_wechat_xml(xml).unwrap();
        assert_eq!(msg.to_user_name, "to_corp");
        assert_eq!(msg.from_user_name, "from_user");
        assert_eq!(msg.create_time, 1234567890);
        assert_eq!(msg.msg_type, "text");
        assert_eq!(msg.content, "Message content here");
        assert_eq!(msg.msg_id, "9999999");
        assert_eq!(msg.agent_id, 42);
    }

    #[test]
    fn parse_wechat_xml_handles_special_characters() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[corp]]></ToUserName>
            <FromUserName><![CDATA[user]]></FromUserName>
            <CreateTime>1000</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[Test with <special> & "characters"]]></Content>
            <MsgId>1</MsgId>
            <AgentID>1</AgentID>
        </xml>"#;

        let msg = parse_wechat_xml(xml).unwrap();
        assert_eq!(msg.content, r#"Test with <special> & "characters""#);
    }

    #[test]
    fn parse_wechat_xml_zero_agent_id() {
        let xml = r#"<xml>
            <ToUserName><![CDATA[corp]]></ToUserName>
            <FromUserName><![CDATA[user]]></FromUserName>
            <CreateTime>1000</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[Test]]></Content>
            <MsgId>1</MsgId>
            <AgentID>0</AgentID>
        </xml>"#;

        let msg = parse_wechat_xml(xml).unwrap();
        assert_eq!(msg.agent_id, 0);
    }

    // ==================== Encryption Detection Tests ====================

    #[test]
    fn is_encrypted_detects_encrypt_tag() {
        let encrypted_xml = r#"<xml><ToUserName><![CDATA[ww33c299595bee0107]]></ToUserName><Encrypt><![CDATA[kvSw6XAlZ9q9T0Es4VRz...]]></Encrypt></xml>"#;
        assert!(is_encrypted_wechat_message(encrypted_xml));
    }

    #[test]
    fn is_encrypted_returns_false_for_plaintext() {
        let plaintext_xml = r#"<xml>
            <ToUserName><![CDATA[corp]]></ToUserName>
            <FromUserName><![CDATA[user]]></FromUserName>
            <CreateTime>1000</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[Hello]]></Content>
            <MsgId>1</MsgId>
            <AgentID>1</AgentID>
        </xml>"#;
        assert!(!is_encrypted_wechat_message(plaintext_xml));
    }

    // ==================== Decryption Tests ====================

    #[test]
    fn decrypt_wechat_message_extracts_and_decrypts() {
        use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        use base64::Engine;
        type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

        // Use a test EncodingAESKey (43 chars of 'A' = 32 zero bytes when decoded)
        let encoding_aes_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        std::env::set_var("WECHAT_ENCODING_AES_KEY", encoding_aes_key);

        // Derive the AES key
        let aes_key = base64::engine::general_purpose::STANDARD
            .decode(format!("{}=", encoding_aes_key))
            .unwrap();
        let iv: [u8; 16] = aes_key[..16].try_into().unwrap();
        let key: [u8; 32] = aes_key.clone().try_into().unwrap();

        // Build the inner XML message that will be encrypted
        let inner_xml = r#"<xml><ToUserName><![CDATA[ww33c299595bee0107]]></ToUserName><FromUserName><![CDATA[TestUser]]></FromUserName><CreateTime>1712345678</CreateTime><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[Hello Proto!]]></Content><MsgId>123456</MsgId><AgentID>1000002</AgentID></xml>"#;
        let receiveid = b"ww33c299595bee0107";

        // Build plaintext: random(16B) + msg_len(4B big-endian) + msg + receiveid
        let random_bytes: [u8; 16] = [0u8; 16]; // Use zeros for determinism
        let msg_len = (inner_xml.len() as u32).to_be_bytes();

        let mut plaintext = Vec::new();
        plaintext.extend_from_slice(&random_bytes);
        plaintext.extend_from_slice(&msg_len);
        plaintext.extend_from_slice(inner_xml.as_bytes());
        plaintext.extend_from_slice(receiveid);

        // Encrypt with PKCS7 padding
        let encryptor = Aes256CbcEnc::new(&key.into(), &iv.into());
        let encrypted = encryptor.encrypt_padded_vec_mut::<Pkcs7>(&plaintext);
        let encrypted_base64 = base64::engine::general_purpose::STANDARD.encode(&encrypted);

        // Build the encrypted XML payload (matching WeChat's format)
        let encrypted_xml = format!(
            r#"<xml><ToUserName><![CDATA[ww33c299595bee0107]]></ToUserName><Encrypt><![CDATA[{}]]></Encrypt></xml>"#,
            encrypted_base64
        );

        // Test decryption
        let decrypted = decrypt_wechat_message(&encrypted_xml).unwrap();

        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        assert_eq!(decrypted, inner_xml);
    }

    #[test]
    fn decrypt_wechat_message_fails_without_encrypt_tag() {
        std::env::set_var("WECHAT_ENCODING_AES_KEY", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");

        let plaintext_xml = r#"<xml><ToUserName><![CDATA[corp]]></ToUserName></xml>"#;
        let result = decrypt_wechat_message(plaintext_xml);

        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing Encrypt tag"));
    }

    #[test]
    fn decrypt_wechat_message_fails_without_aes_key() {
        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        let encrypted_xml = r#"<xml><ToUserName><![CDATA[corp]]></ToUserName><Encrypt><![CDATA[somebase64data]]></Encrypt></xml>"#;
        let result = decrypt_wechat_message(encrypted_xml);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("WECHAT_ENCODING_AES_KEY not set"));
    }

    #[test]
    fn full_encrypted_message_parsing() {
        use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        use base64::Engine;
        type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

        // Use a test EncodingAESKey
        let encoding_aes_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        std::env::set_var("WECHAT_ENCODING_AES_KEY", encoding_aes_key);

        // Derive the AES key
        let aes_key = base64::engine::general_purpose::STANDARD
            .decode(format!("{}=", encoding_aes_key))
            .unwrap();
        let iv: [u8; 16] = aes_key[..16].try_into().unwrap();
        let key: [u8; 32] = aes_key.clone().try_into().unwrap();

        // Build the inner XML message
        let inner_xml = r#"<xml><ToUserName><![CDATA[ww33c299595bee0107]]></ToUserName><FromUserName><![CDATA[TestUser]]></FromUserName><CreateTime>1712345678</CreateTime><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[Hello Proto!]]></Content><MsgId>123456</MsgId><AgentID>1000002</AgentID></xml>"#;
        let receiveid = b"ww33c299595bee0107";

        // Build plaintext
        let random_bytes: [u8; 16] = [0u8; 16];
        let msg_len = (inner_xml.len() as u32).to_be_bytes();

        let mut plaintext = Vec::new();
        plaintext.extend_from_slice(&random_bytes);
        plaintext.extend_from_slice(&msg_len);
        plaintext.extend_from_slice(inner_xml.as_bytes());
        plaintext.extend_from_slice(receiveid);

        // Encrypt
        let encryptor = Aes256CbcEnc::new(&key.into(), &iv.into());
        let encrypted = encryptor.encrypt_padded_vec_mut::<Pkcs7>(&plaintext);
        let encrypted_base64 = base64::engine::general_purpose::STANDARD.encode(&encrypted);

        // Build encrypted XML
        let encrypted_xml = format!(
            r#"<xml><ToUserName><![CDATA[ww33c299595bee0107]]></ToUserName><Encrypt><![CDATA[{}]]></Encrypt></xml>"#,
            encrypted_base64
        );

        // Test full parsing flow (adapter should detect encryption, decrypt, then parse)
        let adapter = WeChatInboundAdapter::new();
        let message = adapter.parse(encrypted_xml.as_bytes()).unwrap();

        std::env::remove_var("WECHAT_ENCODING_AES_KEY");

        assert_eq!(message.sender, "TestUser");
        assert_eq!(message.text_body, Some("Hello Proto!".to_string()));
        assert_eq!(message.metadata.wechat_corp_id, Some("ww33c299595bee0107".to_string()));
        assert_eq!(message.metadata.wechat_user_id, Some("TestUser".to_string()));
        assert_eq!(message.metadata.wechat_agent_id, Some("1000002".to_string()));
    }

    // ==================== PKCS7 Padding Tests ====================

    #[test]
    fn remove_pkcs7_padding_valid() {
        // Data with 5 bytes of padding (0x05)
        let data = vec![1, 2, 3, 4, 5, 5, 5, 5, 5, 5];
        let result = remove_pkcs7_padding(&data).unwrap();
        assert_eq!(result, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn remove_pkcs7_padding_single_byte() {
        // Data with 1 byte of padding (0x01)
        let data = vec![1, 2, 3, 1];
        let result = remove_pkcs7_padding(&data).unwrap();
        assert_eq!(result, &[1, 2, 3]);
    }

    #[test]
    fn remove_pkcs7_padding_full_block() {
        // Full block of padding (16 bytes of 0x10)
        let mut data = vec![1, 2, 3];
        data.extend(vec![16u8; 16]);
        let result = remove_pkcs7_padding(&data).unwrap();
        assert_eq!(result, &[1, 2, 3]);
    }

    #[test]
    fn remove_pkcs7_padding_invalid_zero() {
        let data = vec![1, 2, 3, 0];
        let result = remove_pkcs7_padding(&data);
        assert!(result.is_err());
    }

    #[test]
    fn remove_pkcs7_padding_invalid_mismatch() {
        // Padding bytes don't match
        let data = vec![1, 2, 3, 3, 3, 2]; // Last byte says 2 but third-to-last is 3
        let result = remove_pkcs7_padding(&data);
        assert!(result.is_err());
    }

    #[test]
    fn remove_pkcs7_padding_empty() {
        let data: Vec<u8> = vec![];
        let result = remove_pkcs7_padding(&data);
        assert!(result.is_err());
    }
}
