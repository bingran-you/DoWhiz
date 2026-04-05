//! Zoom RTMS webhook handler.
//!
//! Receives webhook from Zoom when meeting.rtms.started event fires,
//! then spawns a task to handle the RTMS audio stream.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use tracing::{error, info, warn};

use scheduler_module::zoom_rtms::{
    handle_zoom_rtms_stream, ZoomRtmsConfig, ZoomRtmsHandler, ZoomRtmsWebhookPayload,
};

use super::state::GatewayState;

/// POST /webhooks/zoom-rtms
///
/// Zoom sends this webhook when a meeting starts with RTMS enabled.
/// Payload contains server_urls for connecting to the audio stream.
pub async fn handle_zoom_rtms_webhook(
    State(state): State<Arc<GatewayState>>,
    Json(payload): Json<ZoomRtmsWebhookPayload>,
) -> impl IntoResponse {
    info!(
        "Zoom RTMS webhook received: meeting={} stream={}",
        payload.meeting_uuid, payload.rtms_stream_id
    );

    let Some(rtms_config) = ZoomRtmsConfig::from_env() else {
        warn!("Zoom RTMS webhook received but ZOOM_CLIENT_ID/SECRET not configured");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "Zoom RTMS not configured"})),
        );
    };

    let handler = Arc::new(ZoomRtmsHandler {
        rtms_config,
        queue: state.queue.clone(),
    });

    let meeting_uuid = payload.meeting_uuid.clone();

    // Spawn task to handle this meeting's stream (runs for duration of meeting)
    tokio::spawn(async move {
        if let Err(e) = handle_zoom_rtms_stream(payload, handler).await {
            error!("Zoom RTMS handler error for {}: {}", meeting_uuid, e);
        }
        info!("Zoom RTMS handler finished for meeting {}", meeting_uuid);
    });

    (StatusCode::OK, Json(json!({"status": "ok"})))
}
