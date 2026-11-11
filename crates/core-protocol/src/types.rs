use serde::{Deserialize, Serialize};

/// Supported video codecs for mobile-to-desktop transmission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum CodecType {
    /// H.264 / AVC (Hardware encoded on Android MediaCodec & iOS VideoToolbox)
    H264 = 1,
    /// HEVC / H.265 (High-efficiency hardware encoding)
    HEVC = 2,
    /// YUYV 4:2:2 uncompressed (Universal format for Linux v4l2loopback)
    YUYV = 3,
    /// NV12 semi-planar (Standard Android camera buffer output)
    NV12 = 4,
    /// RGB 24-bit uncompressed
    RGB24 = 5,
    /// RGBA 32-bit uncompressed
    RGBA = 6,
    /// Motion JPEG (Fast software fallback)
    MJPEG = 7,
}

impl CodecType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::H264),
            2 => Some(Self::HEVC),
            3 => Some(Self::YUYV),
            4 => Some(Self::NV12),
            5 => Some(Self::RGB24),
            6 => Some(Self::RGBA),
            7 => Some(Self::MJPEG),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }

    pub fn is_hardware_accelerated(&self) -> bool {
        matches!(self, Self::H264 | Self::HEVC)
    }
}

/// Uncompressed pixel format for raw frames or display surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PixelFormat {
    YUYV,
    NV12,
    RGB24,
    RGBA,
}

/// Physical camera sensor placement on the mobile device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CameraFacing {
    Back,
    Front,
    UltraWide,
    Telephoto,
    External,
}

impl std::fmt::Display for CameraFacing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Back => write!(f, "Back (Main)"),
            Self::Front => write!(f, "Front (Selfie)"),
            Self::UltraWide => write!(f, "Ultra-Wide"),
            Self::Telephoto => write!(f, "Telephoto"),
            Self::External => write!(f, "External"),
        }
    }
}

/// Image rotation angle relative to natural sensor orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(u16)]
pub enum Orientation {
    #[default]
    Deg0 = 0,
    Deg90 = 90,
    Deg180 = 180,
    Deg270 = 270,
}

impl Orientation {
    pub fn as_degrees(&self) -> u16 {
        *self as u16
    }

    pub fn next_clockwise(&self) -> Self {
        match self {
            Self::Deg0 => Self::Deg90,
            Self::Deg90 => Self::Deg180,
            Self::Deg180 => Self::Deg270,
            Self::Deg270 => Self::Deg0,
        }
    }
}

/// Video resolution and framerate capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
    pub max_fps: u32,
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{} @ {}fps", self.width, self.height, self.max_fps)
    }
}

/// Physical or logical camera descriptor on the mobile device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraInfo {
    pub id: String,
    pub name: String,
    pub facing: CameraFacing,
    pub resolutions: Vec<Resolution>,
    pub zoom_range: (f32, f32),
    pub has_torch: bool,
}

/// Full camera capabilities reported by the mobile device upon connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraCapabilities {
    pub cameras: Vec<CameraInfo>,
    pub supported_codecs: Vec<CodecType>,
    pub supported_pixel_formats: Vec<PixelFormat>,
}

/// Device identification and hardware metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub os: String,
    pub protocol_version: u32,
    pub battery_level: Option<f32>,
    pub is_charging: Option<bool>,
}

/// Active stream configuration negotiated between Desktop and Mobile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamConfig {
    pub camera_id: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: CodecType,
    pub bitrate_kbps: u32,
    pub udp_port: u16,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            camera_id: "0".to_string(),
            width: 1920,
            height: 1080,
            fps: 30,
            codec: CodecType::H264,
            bitrate_kbps: 6000,
            udp_port: 52521,
        }
    }
}

/// Real-time streaming metrics emitted by the mobile device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamStats {
    pub fps: f32,
    pub bitrate_kbps: u32,
    pub frames_sent: u64,
    pub frames_dropped: u64,
    pub battery_pct: u8,
    pub temperature_c: f32,
    pub timestamp_ms: u64,
}
