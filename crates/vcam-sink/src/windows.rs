//! Windows Media Foundation Virtual Camera Sink implementation skeleton.
//!
//! On Windows, virtual camera streams are injected via the Media Foundation
//! Virtual Camera API (MFCreateVirtualCamera, introduced in Windows 11 Build 22000).

use crate::{VcamConfig, VcamError, VirtualCameraSink};
use tracing::info;

/// Windows Media Foundation Virtual Camera Sink.
pub struct WindowsMfVirtualCameraSink {
    config: Option<VcamConfig>,
    is_active: bool,
}

impl Default for WindowsMfVirtualCameraSink {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsMfVirtualCameraSink {
    pub fn new() -> Self {
        Self {
            config: None,
            is_active: false,
        }
    }
}

impl VirtualCameraSink for WindowsMfVirtualCameraSink {
    fn open(&mut self, config: VcamConfig) -> Result<(), VcamError> {
        info!(
            "Windows MF Virtual Camera Sink initializing: {}x{} @ {}fps",
            config.width, config.height, config.fps
        );
        // On Windows 11:
        // 1. MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET)
        // 2. MFCreateVirtualCamera(MFVirtualCameraType_SoftwareCameraSource, MFVirtualCameraLifetime_Session, ...)
        // 3. Register stream descriptors with NV12 / RGB24 formats
        self.config = Some(config);
        self.is_active = true;
        Ok(())
    }

    fn write_frame(&mut self, _frame: &[u8]) -> Result<(), VcamError> {
        if !self.is_active {
            return Err(VcamError::DeviceNotOpen);
        }
        // Injects buffer into IMFMediaEvent / IMFStreamDescriptor media sample queue
        Ok(())
    }

    fn close(&mut self) -> Result<(), VcamError> {
        self.is_active = false;
        self.config = None;
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.is_active
    }

    fn config(&self) -> Option<&VcamConfig> {
        self.config.as_ref()
    }

    fn device_path(&self) -> &str {
        "OpenCamLink Virtual Camera (Windows MF)"
    }
}
