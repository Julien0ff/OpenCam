use crate::types::{
    CameraCapabilities, DeviceInfo, Orientation, StreamConfig, StreamStats,
};
use serde::{Deserialize, Serialize};

/// Remote procedure call commands sent from Desktop to Mobile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ControlCommand {
    /// Select physical camera sensor by ID (Front, Back, Ultra-Wide, etc.)
    SelectCamera { camera_id: String },

    /// Digital zoom factor (1.0x to 10.0x)
    SetZoom { factor: f32 },

    /// Mirror image horizontally and/or vertically
    SetMirror { horizontal: bool, vertical: bool },

    /// Image rotation (0°, 90°, 180°, 270°)
    SetOrientation { orientation: Orientation },

    /// Toggle hardware torch/flash on the mobile device
    SetTorch { enabled: bool },

    /// Configure active streaming parameters (resolution, fps, codec, target UDP port)
    ConfigureStream(StreamConfig),

    /// Start streaming video over UDP to the configured target
    StartStream,

    /// Stop streaming video
    StopStream,

    /// Request an immediate keyframe / IDR from the hardware encoder
    RequestKeyframe,

    /// Ping message for latency measurement and keepalive
    Ping { timestamp_ms: u64 },
}

/// Responses and telemetry events emitted by Mobile to Desktop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ControlResponse {
    /// Initial handshake containing hardware capabilities and camera devices
    Handshake {
        device_info: DeviceInfo,
        capabilities: CameraCapabilities,
    },

    /// Acknowledgment of a control command
    CommandAck {
        command_name: String,
        success: bool,
        error: Option<String>,
    },

    /// Confirmation that streaming has started
    StreamStarted { config: StreamConfig },

    /// Confirmation that streaming has stopped
    StreamStopped,

    /// Real-time streaming and device telemetry update
    StatusUpdate(StreamStats),

    /// Pong reply with original ping timestamp and receive timestamp
    Pong {
        ping_timestamp_ms: u64,
        pong_timestamp_ms: u64,
    },

    /// Critical error on mobile side (e.g. camera disconnected or overheated)
    Error { message: String },
}
