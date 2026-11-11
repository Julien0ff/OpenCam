//! # core-protocol
//!
//! Enums, structures, packetization, and framing for the OpenCamLink protocol.
//! Supports bi-directional TCP RPC for camera control and low-latency UDP video transmission.

pub mod commands;
pub mod packet;
pub mod types;

#[cfg(feature = "async-framing")]
pub mod framing;

pub use commands::{ControlCommand, ControlResponse};
pub use packet::{
    FrameReassembler, Packetizer, ReassembledFrame, VideoPacketHeader,
    DEFAULT_CONTROL_PORT, DEFAULT_VIDEO_PORT, MAGIC_CONTROL, MAGIC_VIDEO,
    MAX_UDP_PAYLOAD_SIZE, MDNS_SERVICE_TYPE, PROTOCOL_VERSION,
};
pub use types::{
    CameraCapabilities, CameraFacing, CameraInfo, CodecType, DeviceInfo,
    Orientation, PixelFormat, Resolution, StreamConfig, StreamStats,
};

#[derive(thiserror::Error, Debug)]
pub enum ProtocolError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),

    #[error("Invalid magic bytes: expected {expected:?}, got {actual:?}")]
    InvalidMagic { expected: [u8; 4], actual: [u8; 4] },

    #[error("Packet buffer too short: need at least {needed} bytes, got {actual}")]
    BufferTooShort { needed: usize, actual: usize },

    #[error("Payload length mismatch: header specifies {expected}, got {actual}")]
    PayloadMismatch { expected: usize, actual: usize },

    #[error("Invalid codec identifier: {0}")]
    InvalidCodec(u8),

    #[error("Invalid orientation: {0}")]
    InvalidOrientation(u16),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
