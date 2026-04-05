# Zoom RTMS Integration

## Overview

Zoom audio stream via **two websocket connections** (one for Zoom auth, one for audio stream)

```
Zoom audio stream via two websocket connection (one for Zoom auth, one for audio stream)
    ↓
STT running continuously (Whisper)
    ↓
Every K seconds: check for "Proto" / "Oliver" mention
    ↓
If mentioned → package transcript as task → queue to scheduler
    ↓
Upstream acks via Zoom chat: "Got it! Working on it..."
    ↓
Scheduler delegates worker, codex executes in background (channel: Zoom)
    ↓
Response via Zoom chat when done
```

- **STT** = Speech-to-Text (audio → text)

---

## Mock Implementation

### scheduler_module/src/zoom_rtms.rs

```rust
//! Zoom RTMS (Real-Time Media Streams) client for receiving audio via WebSocket.
//!
//! Connection flow:
//! 1. Receive webhook `meeting.rtms.started` with server_urls
//! 2. Connect to signaling WebSocket, send SIGNALING_HAND_SHAKE_REQ
//! 3. Receive SIGNALING_HAND_SHAKE_RESP with media_urls
//! 4. Connect to media WebSocket (audio), send DATA_HAND_SHAKE_REQ
//! 5. Receive MEDIA_DATA_AUDIO messages with base64-encoded PCM

use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

use crate::channel::Channel;
use crate::index_store::IndexStore;
use crate::user_store::UserStore;
use crate::{ModuleExecutor, RunTaskTask, Scheduler, ServiceConfig, TaskKind};

type HmacSha256 = Hmac<Sha256>;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct ZoomRtmsConfig {
    pub client_id: String,
    pub client_secret: String,
    pub deepgram_api_key: Option<String>,
}

impl ZoomRtmsConfig {
    pub fn from_env() -> Option<Self> {
        Some(Self {
            client_id: std::env::var("ZOOM_CLIENT_ID").ok()?,
            client_secret: std::env::var("ZOOM_CLIENT_SECRET").ok()?,
            deepgram_api_key: std::env::var("DEEPGRAM_API_KEY").ok(),
        })
    }
}

/// Webhook payload from Zoom when meeting starts RTMS
#[derive(Debug, serde::Deserialize)]
pub struct ZoomRtmsWebhookPayload {
    pub meeting_uuid: String,
    pub rtms_stream_id: String,
    pub server_urls: Vec<String>,
}

/// Signaling handshake request
#[derive(Debug, serde::Serialize)]
struct SignalingHandshakeReq {
    msg_type: &'static str,
    protocol_version: &'static str,
    meeting_uuid: String,
    rtms_stream_id: String,
    signature: String,
}

/// Signaling handshake response
#[derive(Debug, serde::Deserialize)]
struct SignalingHandshakeResp {
    msg_type: String,
    status: String,
    media_urls: Option<MediaUrls>,
}

#[derive(Debug, serde::Deserialize)]
struct MediaUrls {
    audio: Option<String>,
    video: Option<String>,
    transcript: Option<String>,
}

/// Data handshake request
#[derive(Debug, serde::Serialize)]
struct DataHandshakeReq {
    msg_type: &'static str,
    protocol_version: &'static str,
    meeting_uuid: String,
    rtms_stream_id: String,
    signature: String,
    payload_encryption: bool,
}

/// Media data message
#[derive(Debug, serde::Deserialize)]
struct MediaDataAudio {
    msg_type: String,
    user_id: Option<String>,
    data: String, // base64 encoded
    timestamp: u64,
}

/// Shared handler state
pub struct ZoomRtmsHandler {
    pub config: Arc<ServiceConfig>,
    pub rtms_config: ZoomRtmsConfig,
    pub index_store: Arc<IndexStore>,
    pub user_store: Arc<UserStore>,
}

// ============================================================================
// Main handler
// ============================================================================

pub async fn handle_zoom_rtms_stream(
    payload: ZoomRtmsWebhookPayload,
    handler: Arc<ZoomRtmsHandler>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let meeting_uuid = payload.meeting_uuid;
    let rtms_stream_id = payload.rtms_stream_id;

    info!(
        "Starting Zoom RTMS handler for meeting={} stream={}",
        meeting_uuid, rtms_stream_id
    );

    // Step 1: Connect to signaling server
    let server_url = payload
        .server_urls
        .first()
        .ok_or("No server URLs in webhook payload")?;

    let audio_url = signaling_handshake(
        server_url, // Zoom's server url
        &meeting_uuid,
        &rtms_stream_id,
        &handler.rtms_config,
    )
    .await?;
    // signaling_handshake sets up the first WS connection for Zoom auth, gets audio_url

    // Step 2: Connect to audio media stream
    let audio_url = audio_url.ok_or("No audio URL in signaling response")?;

    media_stream_loop(
        &audio_url,
        &meeting_uuid,
        &rtms_stream_id,
        handler,
    )
    .await
}

// ============================================================================
// Signaling handshake
// ============================================================================

async fn signaling_handshake(
    server_url: &str,
    meeting_uuid: &str,
    rtms_stream_id: &str,
    config: &ZoomRtmsConfig,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    info!("Connecting to signaling server: {}", server_url);

    let (ws_stream, _) = connect_async(server_url).await?;
    // signaling_handshake sets up WebSocket connection to Zoom's signaling server,
    // authenticates, and returns the audio URL
    let (mut write, mut read) = ws_stream.split();

    // Generate signature: HMAC-SHA256(client_secret, client_id + meeting_uuid + rtms_stream_id)
    let signature = generate_signature(
        &config.client_secret,
        &format!("{}{}{}", config.client_id, meeting_uuid, rtms_stream_id),
    );

    let handshake_req = SignalingHandshakeReq {
        msg_type: "SIGNALING_HAND_SHAKE_REQ",
        protocol_version: "1.0",
        meeting_uuid: meeting_uuid.to_string(),
        rtms_stream_id: rtms_stream_id.to_string(),
        signature,
    };

    let req_json = serde_json::to_string(&handshake_req)?;
    write.send(Message::Text(req_json)).await?;
    // sends JSON over write channel for Websocket to Zoom with signature for auth

    // Wait for response
    while let Some(msg) = read.next().await {
        match msg? {
            Message::Text(text) => {
                let resp: SignalingHandshakeResp = serde_json::from_str(&text)?;

                if resp.status != "STATUS_OK" {
                    return Err(format!("Signaling handshake failed: {}", resp.status).into());
                }

                info!("Signaling handshake successful");
                return Ok(resp.media_urls.and_then(|m| m.audio));
            }
            Message::Close(_) => {
                return Err("Signaling WebSocket closed unexpectedly".into());
            }
            _ => {}
        }
    }

    Err("No signaling response received".into())
}

// ============================================================================
// Media stream loop
// ============================================================================

async fn media_stream_loop(
    audio_url: &str,
    meeting_uuid: &str,
    rtms_stream_id: &str,
    handler: Arc<ZoomRtmsHandler>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Connecting to audio media stream: {}", audio_url);

    let (ws_stream, _) = connect_async(audio_url).await?;
    // from the audio url from passing Zoom authentication (server WS),
    // setup WS connection to audio stream
    let (mut write, mut read) = ws_stream.split();

    // Data handshake
    let signature = generate_signature(
        &handler.rtms_config.client_secret,
        &format!(
            "{}{}{}",
            handler.rtms_config.client_id, meeting_uuid, rtms_stream_id
        ),
    );

    let handshake_req = DataHandshakeReq {
        msg_type: "DATA_HAND_SHAKE_REQ",
        protocol_version: "1.0",
        meeting_uuid: meeting_uuid.to_string(),
        rtms_stream_id: rtms_stream_id.to_string(),
        signature,
        payload_encryption: false,
    };

    let req_json = serde_json::to_string(&handshake_req)?;
    write.send(Message::Text(req_json)).await?;
    // sends JSON over write channel for Websocket to Zoom with signature for auth

    // Audio processing state
    let mut audio_buffer: Vec<u8> = Vec::new(); // u8 = bytes
    let mut transcript_buffer: Vec<String> = Vec::new();
    let check_interval = Duration::from_secs(5);
    let mut last_check = Instant::now();
    let mut handshake_complete = false;

    while let Some(msg) = read.next().await {
        match msg? {
            // message by type (text, audio, binary, etc.)
            Message::Text(text) => {
                // Could be handshake response or media data
                if !handshake_complete {
                    if text.contains("DATA_HAND_SHAKE_RESP") {
                        if text.contains("STATUS_OK") {
                            info!("Data handshake successful, receiving audio...");
                            handshake_complete = true;
                        } else {
                            return Err(format!("Data handshake failed: {}", text).into());
                        }
                    }
                    continue;
                }

                // Parse audio data
                if let Ok(audio_msg) = serde_json::from_str::<MediaDataAudio>(&text) {
                    if audio_msg.msg_type == "MEDIA_DATA_AUDIO" {
                        // Decode base64 audio
                        if let Ok(audio_bytes) = BASE64.decode(&audio_msg.data) {
                            audio_buffer.extend_from_slice(&audio_bytes);
                        }
                    }
                }
            }
            Message::Binary(data) => {
                // Some implementations send raw binary
                audio_buffer.extend_from_slice(&data);
            }
            Message::Close(_) => {
                info!("Audio stream closed for meeting {}", meeting_uuid);
                break;
            }
            _ => {}
        }

        // Periodic transcription and wake word check
        if handshake_complete && last_check.elapsed() >= check_interval {
            if !audio_buffer.is_empty() {
                match transcribe_audio(&audio_buffer).await {
                    Ok(text) if !text.trim().is_empty() => {
                        info!("Transcribed: {}", text);
                        transcript_buffer.push(text);
                    }
                    Err(e) => warn!("Transcription failed: {}", e),
                    _ => {}
                }

                // Check for wake words
                let full_transcript = transcript_buffer.join(" ");
                if contains_wake_word(&full_transcript) {
                    info!("Wake word detected in meeting {}", meeting_uuid);

                    let task_text = full_transcript.trim().to_string();

                    // Send acknowledgment
                    if let Err(e) = send_zoom_chat(meeting_uuid, "Got it! Working on it...").await {
                        warn!("Failed to send Zoom chat ack: {}", e);
                    }

                    // Queue task
                    if let Err(e) = enqueue_zoom_task(&handler, meeting_uuid, &task_text).await {
                        error!("Failed to enqueue Zoom task: {}", e);
                    }

                    transcript_buffer.clear();
                }

                // Keep rolling window of transcript (last ~30 seconds)
                // 6 transcript chunks * 5 sec per transcript chunk = 30 sec
                // only keep last 6 transcript chunks
                if transcript_buffer.len() > 6 {
                    transcript_buffer.drain(0..transcript_buffer.len() - 6);
                }

                audio_buffer.clear();
            }
            last_check = Instant::now();
        }
    }

    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

fn generate_signature(secret: &str, message: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(message.as_bytes());
    let result = mac.finalize();
    hex::encode(result.into_bytes()) // returns hex-encoded signature
}

fn contains_wake_word(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("hey proto")
        || lower.contains("proto")
        || lower.contains("oliver")
}

async fn transcribe_audio(
    audio: &[u8],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = std::env::var("AZURE_OPENAI_API_KEY")?;
    let endpoint = std::env::var("AZURE_OPENAI_ENDPOINT")?;

    // Whisper deployment name - you'd create this in Azure portal
    let deployment = "whisper-1";

    let client = reqwest::Client::new();

    // Multi-part request
    let part = reqwest::multipart::Part::bytes(audio.to_vec())
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("response_format", "json");

    let resp = client
        .post(format!(
            "{}openai/deployments/{}/audio/transcriptions?api-version=2024-02-01",
            endpoint, deployment
        ))
        .header("api-key", api_key)
        .multipart(form)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(format!("Azure Whisper error: {}", resp.status()).into());
    }

    let result: serde_json::Value = resp.json().await?;
    Ok(result["text"].as_str().unwrap_or("").to_string())
}

async fn enqueue_zoom_task(
    handler: &ZoomRtmsHandler,
    meeting_id: &str,
    task_text: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Similar to discord_gateway.rs enqueue pattern
    let run_task = RunTaskTask {
        channel: Channel::Zoom,
        thread_id: meeting_id.to_string(),
        message_id: Some(format!("zoom_{}", chrono::Utc::now().timestamp())),
        text_body: Some(task_text.to_string()),
        // ... other fields
    };

    let mut scheduler = Scheduler::load(&tasks_db_path, ModuleExecutor::default())?;
    let task_id = scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))?;

    info!("Enqueued Zoom task {} for meeting {}", task_id, meeting_id);
    Ok(())
}

async fn send_zoom_chat(meeting_id: &str, message: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Use Zoom Chat API to send message to meeting
    // POST https://api.zoom.us/v2/chat/users/me/messages
    info!("Would send to Zoom chat [{}]: {}", meeting_id, message);
    Ok(())
}
```

---

## Webhook Endpoint

### bin/inbound_gateway/zoom_rtms.rs

```rust
use std::sync::Arc;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use tracing::{error, info};

use scheduler_module::zoom_rtms::{handle_zoom_rtms_stream, ZoomRtmsConfig, ZoomRtmsWebhookPayload};

use super::state::GatewayState;

/// POST /webhooks/zoom/rtms
pub async fn handle_zoom_rtms_webhook(
    State(state): State<Arc<GatewayState>>,
    Json(payload): Json<ZoomRtmsWebhookPayload>,
) -> impl IntoResponse {
    info!("Zoom RTMS webhook: meeting={}", payload.meeting_uuid);

    let Some(rtms_config) = ZoomRtmsConfig::from_env() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Zoom RTMS not configured"})),
        );
    };

    let meeting_uuid = payload.meeting_uuid.clone();

    // Spawn task to handle this meeting's stream
    tokio::spawn(async move {
        if let Err(e) = handle_zoom_rtms_stream(payload, rtms_config).await {
            error!("Zoom RTMS handler error for {}: {}", meeting_uuid, e);
        }
    });

    (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
}
```

---

## Router Integration

Add to `bin/inbound_gateway.rs`:

```rust
// At the top, add module declaration
#[path = "inbound_gateway/zoom_rtms.rs"]
mod zoom_rtms;

// Import the handler
use zoom_rtms::handle_zoom_rtms_webhook;

// Add to router (around line 240)
.route("/webhooks/zoom-rtms", post(handle_zoom_rtms_webhook))
```

---

## Channel Enum

Add to `channel.rs`:

```rust
pub enum Channel {
    Discord,
    Slack,
    Email,
    Zoom,  // Add this
    // ...
}
```

---

## Environment Variables

Required in `.env`:

```bash
ZOOM_CLIENT_ID=your_zoom_client_id
ZOOM_CLIENT_SECRET=your_zoom_client_secret

# For transcription (Azure OpenAI Whisper)
AZURE_OPENAI_API_KEY=your_key
AZURE_OPENAI_ENDPOINT=https://your-resource.openai.azure.com/
```

---

## References

- [Zoom RTMS Docs](https://developers.zoom.us/docs/rtms/)
- [RTMS GitHub Samples](https://github.com/zoom/rtms-samples)
- [RTMS SDK](https://github.com/zoom/rtms)
