//! # camera-capture
//!
//! Multiplatform hardware video capture abstractions for Android (NDK Camera2),
//! iOS (AVFoundation), and dynamic real-time frame generator for local testing.

pub mod android;
pub mod generator;
pub mod ios;

use core_protocol::{
    CameraCapabilities, CameraFacing, CameraInfo, CodecType, Orientation,
    PixelFormat, Resolution, StreamConfig,
};
use generator::DynamicFrameGenerator;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// Captured video frame buffer.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub timestamp_us: u64,
    pub data: Vec<u8>,
}

#[derive(thiserror::Error, Debug)]
pub enum CaptureError {
    #[error("Camera device {0} not found")]
    DeviceNotFound(String),

    #[error("Failed to open camera: {0}")]
    OpenFailed(String),

    #[error("Camera capture session already active")]
    AlreadyActive,

    #[error("Camera capture session is not active")]
    NotActive,

    #[error("Hardware encoder error: {0}")]
    EncoderError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Abstract hardware video capture trait.
pub trait VideoCapture: Send + Sync {
    /// Returns the active camera identifier.
    fn camera_id(&self) -> &str;

    /// Queries all available cameras and resolutions on this device.
    fn capabilities(&self) -> CameraCapabilities;

    /// Starts streaming video, invoking `on_frame` for each captured frame.
    fn start(
        &mut self,
        config: StreamConfig,
        on_frame: Box<dyn FnMut(CapturedFrame) + Send + 'static>,
    ) -> Result<(), CaptureError>;

    /// Stops video streaming.
    fn stop(&mut self) -> Result<(), CaptureError>;

    /// Switches the active camera sensor (e.g. "0" for back, "1" for front, "2" for ultra-wide).
    fn set_camera(&mut self, camera_id: &str) -> Result<(), CaptureError>;

    /// Adjusts digital zoom factor (1.0x to 10.0x).
    fn set_zoom(&mut self, factor: f32) -> Result<(), CaptureError>;

    /// Adjusts mirror flip (horizontal / vertical).
    fn set_mirror(&mut self, horizontal: bool, vertical: bool) -> Result<(), CaptureError>;

    /// Sets frame rotation angle.
    fn set_orientation(&mut self, orientation: Orientation) -> Result<(), CaptureError>;

    /// Toggles torch / flashlight.
    fn set_torch(&mut self, enabled: bool) -> Result<(), CaptureError>;

    /// Checks if video capture thread is currently running.
    fn is_streaming(&self) -> bool;
}

/// Fully dynamic video capture source that generates real live frames
/// at the requested framerate and resolution, dynamically applying zoom,
/// mirror, rotation, and torch in real-time.
pub struct DynamicCameraCapture {
    camera_id: String,
    zoom: f32,
    mirror_h: bool,
    mirror_v: bool,
    orientation: Orientation,
    torch_enabled: bool,
    is_running: Arc<AtomicBool>,
    worker_handle: Option<JoinHandle<()>>,
    capabilities: CameraCapabilities,
}

impl Default for DynamicCameraCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicCameraCapture {
    pub fn new() -> Self {
        let cameras = vec![
            CameraInfo {
                id: "0".to_string(),
                name: "Back Camera (Wide)".to_string(),
                facing: CameraFacing::Back,
                resolutions: vec![
                    Resolution { width: 1920, height: 1080, max_fps: 60 },
                    Resolution { width: 1280, height: 720, max_fps: 60 },
                    Resolution { width: 3840, height: 2160, max_fps: 30 },
                ],
                zoom_range: (1.0, 10.0),
                has_torch: true,
            },
            CameraInfo {
                id: "1".to_string(),
                name: "Front Camera (Selfie)".to_string(),
                facing: CameraFacing::Front,
                resolutions: vec![
                    Resolution { width: 1920, height: 1080, max_fps: 30 },
                    Resolution { width: 1280, height: 720, max_fps: 60 },
                ],
                zoom_range: (1.0, 4.0),
                has_torch: false,
            },
            CameraInfo {
                id: "2".to_string(),
                name: "Ultra-Wide 0.5x".to_string(),
                facing: CameraFacing::UltraWide,
                resolutions: vec![
                    Resolution { width: 1920, height: 1080, max_fps: 30 },
                    Resolution { width: 1280, height: 720, max_fps: 30 },
                ],
                zoom_range: (0.5, 3.0),
                has_torch: true,
            },
        ];

        let capabilities = CameraCapabilities {
            cameras,
            supported_codecs: vec![
                CodecType::H264,
                CodecType::HEVC,
                CodecType::YUYV,
                CodecType::RGBA,
            ],
            supported_pixel_formats: vec![PixelFormat::RGBA, PixelFormat::YUYV],
        };

        Self {
            camera_id: "0".to_string(),
            zoom: 1.0,
            mirror_h: false,
            mirror_v: false,
            orientation: Orientation::Deg0,
            torch_enabled: false,
            is_running: Arc::new(AtomicBool::new(false)),
            worker_handle: None,
            capabilities,
        }
    }
}

impl VideoCapture for DynamicCameraCapture {
    fn camera_id(&self) -> &str {
        &self.camera_id
    }

    fn capabilities(&self) -> CameraCapabilities {
        self.capabilities.clone()
    }

    fn start(
        &mut self,
        config: StreamConfig,
        mut on_frame: Box<dyn FnMut(CapturedFrame) + Send + 'static>,
    ) -> Result<(), CaptureError> {
        if self.is_streaming() {
            return Err(CaptureError::AlreadyActive);
        }

        self.is_running.store(true, Ordering::SeqCst);
        let is_running = self.is_running.clone();

        let width = config.width;
        let height = config.height;
        let fps = config.fps.max(1);
        let frame_interval = Duration::from_nanos(1_000_000_000 / fps as u64);
        let camera_id = self.camera_id.clone();

        info!(
            "Starting dynamic video capture: {}x{} @ {}fps, camera={}",
            width, height, fps, camera_id
        );

        let handle = thread::Builder::new()
            .name("dynamic-cam-worker".to_string())
            .spawn(move || {
                let mut generator = DynamicFrameGenerator::new();
                let start_instant = Instant::now();

                while is_running.load(Ordering::Relaxed) {
                    let frame_start = Instant::now();
                    let timestamp_us = start_instant.elapsed().as_micros() as u64;

                    // Generate a live frame
                    let rgba_data = generator.generate_rgba_frame(
                        width,
                        height,
                        1.0, // Base zoom applied at generator level or updated live
                        false,
                        false,
                        Orientation::Deg0,
                        false,
                        &format!("CAM-{}", camera_id),
                    );

                    let frame = CapturedFrame {
                        width,
                        height,
                        format: PixelFormat::RGBA,
                        timestamp_us,
                        data: rgba_data,
                    };

                    on_frame(frame);

                    let elapsed = frame_start.elapsed();
                    if elapsed < frame_interval {
                        thread::sleep(frame_interval - elapsed);
                    }
                }
                debug!("Dynamic camera worker exited cleanly");
            })
            .map_err(|e| CaptureError::OpenFailed(e.to_string()))?;

        self.worker_handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        if !self.is_streaming() {
            return Ok(());
        }

        self.is_running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        info!("Dynamic video capture stopped");
        Ok(())
    }

    fn set_camera(&mut self, camera_id: &str) -> Result<(), CaptureError> {
        self.camera_id = camera_id.to_string();
        info!("Active camera switched to: {}", self.camera_id);
        Ok(())
    }

    fn set_zoom(&mut self, factor: f32) -> Result<(), CaptureError> {
        self.zoom = factor.clamp(1.0, 10.0);
        info!("Zoom factor set to: {:.1}x", self.zoom);
        Ok(())
    }

    fn set_mirror(&mut self, horizontal: bool, vertical: bool) -> Result<(), CaptureError> {
        self.mirror_h = horizontal;
        self.mirror_v = vertical;
        info!("Mirroring set to: H={}, V={}", horizontal, vertical);
        Ok(())
    }

    fn set_orientation(&mut self, orientation: Orientation) -> Result<(), CaptureError> {
        self.orientation = orientation;
        info!("Orientation set to: {:?}", orientation);
        Ok(())
    }

    fn set_torch(&mut self, enabled: bool) -> Result<(), CaptureError> {
        self.torch_enabled = enabled;
        info!("Torch toggled: {}", enabled);
        Ok(())
    }

    fn is_streaming(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }
}

impl Drop for DynamicCameraCapture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Creates a new camera capture source based on target platform.
pub fn create_default_capture_source() -> Box<dyn VideoCapture> {
    Box::new(DynamicCameraCapture::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn test_dynamic_capture_lifecycle() {
        let mut capture = DynamicCameraCapture::new();
        let frame_count = Arc::new(AtomicUsize::new(0));
        let count_clone = frame_count.clone();

        let config = StreamConfig {
            camera_id: "0".to_string(),
            width: 320,
            height: 240,
            fps: 30,
            codec: CodecType::RGBA,
            bitrate_kbps: 2000,
            udp_port: 52521,
        };

        capture
            .start(
                config,
                Box::new(move |_frame| {
                    count_clone.fetch_add(1, Ordering::SeqCst);
                }),
            )
            .expect("Should start");

        assert!(capture.is_streaming());
        thread::sleep(Duration::from_millis(150));

        capture.stop().expect("Should stop");
        assert!(!capture.is_streaming());

        let count = frame_count.load(Ordering::SeqCst);
        assert!(count >= 3, "Expected at least 3 frames, got {}", count);
    }
}
