use core_protocol::{
    CameraCapabilities, ControlCommand, DeviceInfo, FaceTrackingConfig, Orientation, StreamConfig,
    StreamStats,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use vcam_sink::{LoopbackDeviceInfo, VirtualCameraSink};

/// State of a single connected mobile smartphone.
pub struct ConnectedCamera {
    pub device_info: DeviceInfo,
    pub capabilities: CameraCapabilities,
    #[allow(dead_code)]
    pub stream_config: StreamConfig,
    pub latest_stats: Option<StreamStats>,
    pub command_tx: tokio::sync::mpsc::Sender<ControlCommand>,

    // Current adjustments
    pub selected_camera_id: String,
    pub zoom: f32,
    pub mirror_h: bool,
    pub mirror_v: bool,
    pub orientation: Orientation,
    pub torch_enabled: bool,
    pub face_tracking: FaceTrackingConfig,

    // Live video frame texture
    pub latest_rgba_frame: Option<Arc<Vec<u8>>>,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frame_count: u64,
    pub last_frame_time: std::time::Instant,

    // Assigned Virtual Camera Sink
    pub vcam_sink: Option<Box<dyn VirtualCameraSink>>,
    pub vcam_enabled: bool,
    pub selected_vcam_device: String,
}

impl ConnectedCamera {
    pub fn new(
        device_info: DeviceInfo,
        capabilities: CameraCapabilities,
        command_tx: tokio::sync::mpsc::Sender<ControlCommand>,
    ) -> Self {
        let initial_cam_id = capabilities
            .cameras
            .first()
            .map(|c| c.id.clone())
            .unwrap_or_else(|| "0".to_string());

        Self {
            device_info,
            capabilities,
            stream_config: StreamConfig::default(),
            latest_stats: None,
            command_tx,
            selected_camera_id: initial_cam_id,
            zoom: 1.0,
            mirror_h: false,
            mirror_v: false,
            orientation: Orientation::Deg0,
            torch_enabled: false,
            face_tracking: FaceTrackingConfig::default(),
            latest_rgba_frame: None,
            frame_width: 1280,
            frame_height: 720,
            frame_count: 0,
            last_frame_time: std::time::Instant::now(),
            vcam_sink: None,
            vcam_enabled: false,
            #[cfg(target_os = "windows")]
            selected_vcam_device: "OpenCam Virtual Camera".to_string(),
            #[cfg(not(target_os = "windows"))]
            selected_vcam_device: "/dev/video10".to_string(),
        }
    }

    pub fn send_command(&self, cmd: ControlCommand) {
        let _ = self.command_tx.try_send(cmd);
    }
}

/// Global shared state across networking threads and egui UI.
#[derive(Clone)]
pub struct AppState {
    pub cameras: Arc<Mutex<HashMap<String, ConnectedCamera>>>,
    pub selected_device_id: Arc<Mutex<Option<String>>>,
    pub available_vcam_devices: Arc<Mutex<Vec<LoopbackDeviceInfo>>>,
    pub server_ip: Arc<Mutex<String>>,
    pub server_control_port: u16,
    pub server_video_port: u16,
    pub server_web_port: u16,
    pub show_qr_modal: Arc<Mutex<bool>>,
    pub is_mdns_active: Arc<Mutex<bool>>,
    pub total_bandwidth_kbps: Arc<Mutex<f32>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            cameras: Arc::new(Mutex::new(HashMap::new())),
            selected_device_id: Arc::new(Mutex::new(None)),
            available_vcam_devices: Arc::new(Mutex::new(Vec::new())),
            server_ip: Arc::new(Mutex::new(Self::detect_local_ip())),
            server_control_port: core_protocol::DEFAULT_CONTROL_PORT,
            server_video_port: core_protocol::DEFAULT_VIDEO_PORT,
            server_web_port: 8766,
            show_qr_modal: Arc::new(Mutex::new(false)),
            is_mdns_active: Arc::new(Mutex::new(false)),
            total_bandwidth_kbps: Arc::new(Mutex::new(0.0)),
        }
    }
}

impl AppState {
    pub fn detect_local_ip() -> String {
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if socket.connect("8.8.8.8:80").is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    return addr.ip().to_string();
                }
            }
        }
        "127.0.0.1".to_string()
    }
}
