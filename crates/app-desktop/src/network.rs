use crate::state::{AppState, ConnectedCamera};
use core_protocol::commands::{ControlCommand, ControlResponse};
use core_protocol::framing::{read_response, write_command};
use core_protocol::packet::{FrameReassembler, DEFAULT_VIDEO_PORT, MDNS_SERVICE_TYPE};
use core_protocol::{CodecType, StreamConfig};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Starts advertising OpenCamLink Desktop service over mDNS (DNS-SD).
pub fn start_mdns_broadcaster(state: AppState, port: u16) -> Result<ServiceDaemon, anyhow::Error> {
    let mdns = ServiceDaemon::new()?;
    let host_name = std::fs::read_to_string("/etc/hostname")
        .map(|s| format!("{}.local.", s.trim()))
        .unwrap_or_else(|_| "opencam-desktop.local.".to_string());

    let service_type = MDNS_SERVICE_TYPE;
    let instance_name = "OpenCam-Desktop";

    let mut properties = HashMap::new();
    properties.insert("version".to_string(), "1.0".to_string());
    properties.insert("proto".to_string(), "ocl1".to_string());

    // Register service
    let my_service = ServiceInfo::new(
        service_type,
        instance_name,
        &host_name,
        "",
        port,
        properties,
    )?;

    mdns.register(my_service)?;
    *state.is_mdns_active.lock().unwrap() = true;
    info!("mDNS announcement started: {} on port {}", service_type, port);

    Ok(mdns)
}

/// Runs TCP RPC server for multi-phone control channels.
pub async fn run_control_server(state: AppState, addr: SocketAddr) -> Result<(), anyhow::Error> {
    let listener = TcpListener::bind(addr).await?;
    info!("Control RPC server listening on {}", addr);

    loop {
        let (mut socket, peer_addr) = listener.accept().await?;
        info!("Incoming connection from mobile device: {}", peer_addr);

        let state_clone = state.clone();

        tokio::spawn(async move {
            // 1. Wait for initial Handshake
            let handshake = match read_response(&mut socket).await {
                Ok(ControlResponse::Handshake {
                    device_info,
                    capabilities,
                }) => (device_info, capabilities),
                Ok(other) => {
                    warn!("Expected Handshake from {}, got {:?}", peer_addr, other);
                    return;
                }
                Err(e) => {
                    error!("Handshake read failed from {}: {}", peer_addr, e);
                    return;
                }
            };

            let device_id = handshake.0.device_id.clone();
            info!(
                "Device connected: {} ({}) with {} cameras",
                handshake.0.device_name, device_id, handshake.1.cameras.len()
            );

            let (cmd_tx, mut cmd_rx) = mpsc::channel::<ControlCommand>(32);
            let connected_cam = ConnectedCamera::new(handshake.0, handshake.1, cmd_tx);

            // Register in global state
            {
                let mut cams = state_clone.cameras.lock().unwrap();
                cams.insert(device_id.clone(), connected_cam);

                let mut sel = state_clone.selected_device_id.lock().unwrap();
                if sel.is_none() {
                    *sel = Some(device_id.clone());
                }
            }

            // Send initial stream configuration & start command
            let initial_config = StreamConfig {
                camera_id: "0".to_string(),
                width: 1280,
                height: 720,
                fps: 30,
                codec: CodecType::RGBA,
                bitrate_kbps: 4000,
                udp_port: DEFAULT_VIDEO_PORT,
            };

            let _ = write_command(&mut socket, &ControlCommand::ConfigureStream(initial_config)).await;
            let _ = write_command(&mut socket, &ControlCommand::StartStream).await;

            let (mut reader, mut writer) = socket.into_split();

            // Writer task: forward outgoing commands to phone
            let writer_task = tokio::spawn(async move {
                while let Some(cmd) = cmd_rx.recv().await {
                    if let Err(e) = write_command(&mut writer, &cmd).await {
                        error!("Failed to send command to phone: {}", e);
                        break;
                    }
                }
            });

            // Reader task: handle incoming status updates and replies
            let dev_id_clone = device_id.clone();
            let state_for_reader = state_clone.clone();
            let reader_task = tokio::spawn(async move {
                while let Ok(response) = read_response(&mut reader).await {
                    match response {
                        ControlResponse::StatusUpdate(stats) => {
                            let mut cams = state_for_reader.cameras.lock().unwrap();
                            if let Some(cam) = cams.get_mut(&dev_id_clone) {
                                cam.latest_stats = Some(stats);
                            }
                        }
                        ControlResponse::Pong {
                            ping_timestamp_ms,
                            pong_timestamp_ms: _,
                        } => {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64;
                            let rtt = now.saturating_sub(ping_timestamp_ms);
                            info!("RTT with {}: {}ms", dev_id_clone, rtt);
                        }
                        _ => {}
                    }
                }
            });

            // Wait for disconnect
            tokio::select! {
                _ = writer_task => {},
                _ = reader_task => {},
            };

            info!("Device disconnected: {}", device_id);
            let mut cams = state_clone.cameras.lock().unwrap();
            cams.remove(&device_id);

            let mut sel = state_clone.selected_device_id.lock().unwrap();
            if sel.as_deref() == Some(&device_id) {
                *sel = cams.keys().next().cloned();
            }
        });
    }
}

/// Runs high-throughput UDP receiver for incoming video streams.
pub async fn run_video_receiver(state: AppState, port: u16) -> Result<(), anyhow::Error> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", port)).await?;
    info!("UDP Video receiver listening on 0.0.0.0:{}", port);

    let mut buf = vec![0u8; 65536];
    let mut reassembler = FrameReassembler::default();
    let mut yuyv_scratch = Vec::new();

    let mut total_bytes_window = 0usize;
    let mut last_bandwidth_calc = std::time::Instant::now();

    loop {
        let (len, _src) = match socket.recv_from(&mut buf).await {
            Ok(res) => res,
            Err(e) => {
                error!("UDP recv error: {}", e);
                continue;
            }
        };

        total_bytes_window += len;
        if last_bandwidth_calc.elapsed().as_secs_f32() >= 1.0 {
            let kbps = (total_bytes_window as f32 * 8.0) / (1024.0 * last_bandwidth_calc.elapsed().as_secs_f32());
            *state.total_bandwidth_kbps.lock().unwrap() = kbps;
            total_bytes_window = 0;
            last_bandwidth_calc = std::time::Instant::now();
        }

        match reassembler.process_packet(&buf[..len]) {
            Ok(Some(frame)) => {
                let w = frame.width as usize;
                let h = frame.height as usize;

                let rgba_bytes = match frame.codec {
                    CodecType::RGBA => Arc::new(frame.payload),
                    CodecType::YUYV => {
                        let mut rgba = vec![0u8; w * h * 4];
                        vcam_sink::converter::yuyv_to_rgba(&frame.payload, w, h, &mut rgba);
                        Arc::new(rgba)
                    }
                    _ => {
                        // For other codecs or fallback
                        Arc::new(frame.payload)
                    }
                };

                // Dispatch to active connected camera in state
                let mut cams = state.cameras.lock().unwrap();
                // If stream_id is mapped or we use the selected device
                let selected_id = state.selected_device_id.lock().unwrap().clone();
                if let Some(dev_id) = selected_id {
                    if let Some(cam) = cams.get_mut(&dev_id) {
                        cam.frame_width = frame.width as u32;
                        cam.frame_height = frame.height as u32;
                        cam.frame_count += 1;
                        cam.last_frame_time = std::time::Instant::now();
                        cam.latest_rgba_frame = Some(rgba_bytes.clone());

                        // If Virtual Camera Sink is open and active, write frame!
                        if cam.vcam_enabled {
                            let dev_path = cam.selected_vcam_device.clone();
                            if let Some(ref mut sink) = cam.vcam_sink {
                                if sink.config().map(|c| (c.width, c.height)) != Some((w as u32, h as u32)) {
                                    let _ = sink.open(vcam_sink::VcamConfig {
                                        device_path: dev_path,
                                        width: w as u32,
                                        height: h as u32,
                                        fps: 30,
                                        format: core_protocol::PixelFormat::YUYV,
                                    });
                                }
                                if sink.is_open() {
                                    // Convert to YUYV if needed
                                    let needed_yuyv_size = w * h * 2;
                                    if yuyv_scratch.len() != needed_yuyv_size {
                                        yuyv_scratch.resize(needed_yuyv_size, 0);
                                    }
                                    vcam_sink::converter::rgba_to_yuyv(
                                        &rgba_bytes,
                                        w,
                                        h,
                                        &mut yuyv_scratch,
                                    );
                                    let _ = sink.write_frame(&yuyv_scratch);
                                }
                            }
                        }
                    }
                }
            }
            Ok(None) => {
                // Waiting for remaining frame chunks
            }
            Err(e) => {
                warn!("UDP packet reassembly error: {}", e);
            }
        }
    }
}
