//! # vcam-sink
//!
//! Virtual Camera Sink abstraction and implementations for Linux (v4l2loopback)
//! and Windows (Media Foundation Virtual Camera).

pub mod converter;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

use core_protocol::PixelFormat;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcamConfig {
    pub device_path: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: PixelFormat,
}

impl Default for VcamConfig {
    fn default() -> Self {
        Self {
            device_path: "/dev/video10".to_string(),
            width: 1920,
            height: 1080,
            fps: 30,
            format: PixelFormat::YUYV,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopbackDeviceInfo {
    pub path: String,
    pub card_label: String,
    pub driver: String,
    pub bus_info: String,
    pub is_loopback: bool,
    pub is_available: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum VcamError {
    #[error("Failed to open virtual camera device {device}: {source}")]
    DeviceOpenFailed {
        device: String,
        source: std::io::Error,
    },

    #[error("Device {0} is not a valid video output / loopback device")]
    NotAnOutputDevice(String),

    #[error("V4L2 ioctl '{call}' failed: {source}")]
    IoctlFailed {
        call: &'static str,
        source: std::io::Error,
    },

    #[error("Failed to write video frame to sink: {0}")]
    WriteFailed(std::io::Error),

    #[error("Virtual camera device is not open")]
    DeviceNotOpen,

    #[error("Unsupported platform for virtual camera sink")]
    UnsupportedPlatform,
}

/// Abstract interface for a virtual camera output device.
pub trait VirtualCameraSink: Send + Sync {
    /// Opens and configures the virtual camera device.
    fn open(&mut self, config: VcamConfig) -> Result<(), VcamError>;

    /// Writes an uncompressed video frame (matching configured pixel format and dimensions).
    fn write_frame(&mut self, frame: &[u8]) -> Result<(), VcamError>;

    /// Closes the device.
    fn close(&mut self) -> Result<(), VcamError>;

    /// Checks if device is currently open and ready to receive frames.
    fn is_open(&self) -> bool;

    /// Returns current active configuration, if open.
    fn config(&self) -> Option<&VcamConfig>;

    /// Returns device name or path.
    fn device_path(&self) -> &str;
}

/// Creates a new virtual camera sink suited for the current host operating system.
pub fn create_default_sink() -> Box<dyn VirtualCameraSink> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::V4l2LoopbackSink::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsMfVirtualCameraSink::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        struct NullSink;
        impl VirtualCameraSink for NullSink {
            fn open(&mut self, _config: VcamConfig) -> Result<(), VcamError> {
                Err(VcamError::UnsupportedPlatform)
            }
            fn write_frame(&mut self, _frame: &[u8]) -> Result<(), VcamError> {
                Err(VcamError::UnsupportedPlatform)
            }
            fn close(&mut self) -> Result<(), VcamError> {
                Ok(())
            }
            fn is_open(&self) -> bool {
                false
            }
            fn config(&self) -> Option<&VcamConfig> {
                None
            }
            fn device_path(&self) -> &str {
                "null"
            }
        }
        Box::new(NullSink)
    }
}
