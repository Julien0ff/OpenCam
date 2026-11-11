use camera_capture::create_default_capture_source;
use core_protocol::commands::{ControlCommand, ControlResponse};
use core_protocol::framing::{read_command, write_response};
use core_protocol::packet::{Packetizer, DEFAULT_VIDEO_PORT, MDNS_SERVICE_TYPE};
use core_protocol::{CodecType, DeviceInfo, StreamConfig};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct DiscoveredDesktop {
    pub name: String,
    pub ip: String,
    pub port: u16,
}

/// Transmitting engine running network discovery, capture pipeline, and UDP video streamer.
pub struct TransmitterEngine {
    pub is_connected: Arc<AtomicBool>,
    pub is_streaming: Arc<AtomicBool>,
    pub discovered_desktops: Arc<std::sync::Mutex<Vec<DiscoveredDesktop>>>,
    pub latest_preview_frame: Arc<std::sync::Mutex<Option<Arc<Vec<u8>>>>>,
    pub frame_width: Arc<std::sync::Mutex<u32>>,
    pub frame_height: Arc<std::sync::Mutex<u32>>,
    pub fps_counter: Arc<std::sync::Mutex<f32>>,
    pub bitrate_kbps: Arc<std::sync::Mutex<u32>>,
}

impl Default for TransmitterEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TransmitterEngine {
    pub fn new() -> Self {
        Self {
            is_connected: Arc::new(AtomicBool::new(false)),
            is_streaming: Arc::new(AtomicBool::new(false)),
            discovered_desktops: Arc::new(std::sync::Mutex::new(Vec::new())),
            latest_preview_frame: Arc::new(std::sync::Mutex::new(None)),
            frame_width: Arc::new(std::sync::Mutex::new(1280)),
            frame_height: Arc::new(std::sync::Mutex::new(720)),
            fps_counter: Arc::new(std::sync::Mutex::new(0.0)),
            bitrate_kbps: Arc::new(std::sync::Mutex::new(0)),
        }
    }

    /// Starts mDNS background discovery of OpenCamLink desktop receivers.
    pub fn start_mdns_discovery(&self) -> Result<(), anyhow::Error> {
        let mdns = ServiceDaemon::new()?;
        let receiver = mdns.browse(MDNS_SERVICE_TYPE)?;
        let desktops_list = self.discovered_desktops.clone();

        tokio::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                if let ServiceEvent::ServiceResolved(info) = event {
                    let name = info.get_fullname().to_string();
                    let port = info.get_port();
                    for ip in info.get_addresses() {
                        let ip_str = ip.to_string();
                        let mut list = desktops_list.lock().unwrap();
                        if !list.iter().any(|d| d.ip == ip_str && d.port == port) {
                            info!("mDNS discovered desktop receiver: {} at {}:{}", name, ip_str, port);
                            list.push(DiscoveredDesktop {
                                name: name.clone(),
                                ip: ip_str,
                                port,
                            });
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Connects to a desktop receiver, executes handshake, and begins streaming when requested.
    pub async fn connect_and_run(
        &self,
        desktop_addr: SocketAddr,
    ) -> Result<(), anyhow::Error> {
        info!("Connecting to OpenCam Desktop at {}", desktop_addr);
        let mut stream = TcpStream::connect(desktop_addr).await?;
        self.is_connected.store(true, Ordering::SeqCst);

        let mut capture_source = create_default_capture_source();
        let capabilities = capture_source.capabilities();

        let device_info = DeviceInfo {
            device_id: "phone-rust-01".to_string(),
            device_name: "Smartphone (OpenCam)".to_string(),
            os: std::env::consts::OS.to_string(),
            protocol_version: 1,
            battery_level: Some(0.88),
            is_charging: Some(true),
        };

        // 1. Send initial Handshake
        let handshake = ControlResponse::Handshake {
            device_info,
            capabilities,
        };
        write_response(&mut stream, &handshake).await?;
        info!("Handshake sent successfully");

        // 2. Prepare UDP video sender socket
        let udp_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
        let target_udp_addr = SocketAddr::new(desktop_addr.ip(), DEFAULT_VIDEO_PORT);

        let (frame_tx, mut frame_rx) = mpsc::channel::<camera_capture::CapturedFrame>(16);

        // Frame transmission background task
        let udp_clone = udp_socket.clone();
        let is_streaming_flag = self.is_streaming.clone();
        let preview_frame = self.latest_preview_frame.clone();
        let fps_counter = self.fps_counter.clone();
        let bitrate_kbps = self.bitrate_kbps.clone();

        tokio::spawn(async move {
            let packetizer = Packetizer::default();
            let mut frame_id = 0u32;
            let mut frames_window = 0usize;
            let mut bytes_window = 0usize;
            let mut last_metric_time = std::time::Instant::now();

            while let Some(frame) = frame_rx.recv().await {
                if !is_streaming_flag.load(Ordering::Relaxed) {
                    continue;
                }

                frame_id += 1;
                let payload_len = frame.data.len();
                bytes_window += payload_len;
                frames_window += 1;

                // Update local preview
                *preview_frame.lock().unwrap() = Some(Arc::new(frame.data.clone()));

                // Calculate real-time metrics
                if last_metric_time.elapsed().as_secs_f32() >= 1.0 {
                    let elapsed = last_metric_time.elapsed().as_secs_f32();
                    *fps_counter.lock().unwrap() = frames_window as f32 / elapsed;
                    *bitrate_kbps.lock().unwrap() = ((bytes_window as f32 * 8.0) / (1024.0 * elapsed)) as u32;
                    frames_window = 0;
                    bytes_window = 0;
                    last_metric_time = std::time::Instant::now();
                }

                // Packetize into UDP datagrams (< 1400 bytes each)
                let packets = packetizer.packetize(
                    1,
                    frame_id,
                    CodecType::RGBA,
                    frame_id % 30 == 1,
                    frame.width as u16,
                    frame.height as u16,
                    frame.timestamp_us,
                    &frame.data,
                );

                for packet in packets {
                    let _ = udp_clone.send_to(&packet, target_udp_addr).await;
                }
            }
        });

        // 3. Command loop reading RPC instructions from Desktop
        loop {
            match read_command(&mut stream).await {
                Ok(cmd) => {
                    info!("Received command from Desktop: {:?}", cmd);
                    match cmd {
                        ControlCommand::ConfigureStream(config) => {
                            *self.frame_width.lock().unwrap() = config.width;
                            *self.frame_height.lock().unwrap() = config.height;
                        }
                        ControlCommand::StartStream => {
                            self.is_streaming.store(true, Ordering::SeqCst);
                            let f_tx = frame_tx.clone();
                            let w = *self.frame_width.lock().unwrap();
                            let h = *self.frame_height.lock().unwrap();

                            let stream_cfg = StreamConfig {
                                camera_id: capture_source.camera_id().to_string(),
                                width: w,
                                height: h,
                                fps: 30,
                                codec: CodecType::RGBA,
                                bitrate_kbps: 4000,
                                udp_port: DEFAULT_VIDEO_PORT,
                            };

                            let _ = capture_source.start(
                                stream_cfg,
                                Box::new(move |captured| {
                                    let _ = f_tx.try_send(captured);
                                }),
                            );
                        }
                        ControlCommand::StopStream => {
                            self.is_streaming.store(false, Ordering::SeqCst);
                            let _ = capture_source.stop();
                        }
                        ControlCommand::SelectCamera { camera_id } => {
                            let _ = capture_source.set_camera(&camera_id);
                        }
                        ControlCommand::SetZoom { factor } => {
                            let _ = capture_source.set_zoom(factor);
                        }
                        ControlCommand::SetMirror { horizontal, vertical } => {
                            let _ = capture_source.set_mirror(horizontal, vertical);
                        }
                        ControlCommand::SetOrientation { orientation } => {
                            let _ = capture_source.set_orientation(orientation);
                        }
                        ControlCommand::SetTorch { enabled } => {
                            let _ = capture_source.set_torch(enabled);
                        }
                        ControlCommand::SetFaceTracking(config) => {
                            let _ = capture_source.set_face_tracking(config);
                        }
                        ControlCommand::Ping { timestamp_ms } => {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64;
                            let _ = write_response(
                                &mut stream,
                                &ControlResponse::Pong {
                                    ping_timestamp_ms: timestamp_ms,
                                    pong_timestamp_ms: now,
                                },
                            )
                            .await;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    warn!("Connection lost with desktop: {}", e);
                    break;
                }
            }
        }

        self.is_connected.store(false, Ordering::SeqCst);
        self.is_streaming.store(false, Ordering::SeqCst);
        let _ = capture_source.stop();

        Ok(())
    }
}
