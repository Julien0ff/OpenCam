use crate::state::{AppState, ConnectedCamera};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Html,
    routing::get,
    Router,
};
use core_protocol::{CameraCapabilities, CameraInfo, ControlCommand, DeviceInfo, StreamStats};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

static CLIENT_COUNTER: AtomicU64 = AtomicU64::new(1);

pub async fn start_web_server(state: AppState, port: u16) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let local_ip = state.server_ip.lock().unwrap().clone();
    let app = Router::new()
        .route("/", get(serve_camera_page))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    // Generate self-signed TLS certificate so mobile browsers allow getUserMedia()
    let mut san = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    if local_ip != "0.0.0.0" && local_ip != "127.0.0.1" {
        san.push(local_ip.clone());
    }

    match rcgen::generate_simple_self_signed(san) {
        Ok(certified_key) => {
            let cert_der = certified_key.cert.der().to_vec();
            let key_der = certified_key.key_pair.serialize_der();
            match axum_server::tls_rustls::RustlsConfig::from_der(vec![cert_der], key_der).await {
                Ok(tls_config) => {
                    info!(
                        "OpenCam HTTPS Web Camera Server starting on https://{}:{}",
                        local_ip, port
                    );
                    if let Err(e) = axum_server::bind_rustls(addr, tls_config)
                        .serve(app.into_make_service())
                        .await
                    {
                        error!("HTTPS web server error: {}", e);
                    }
                    return;
                }
                Err(e) => {
                    warn!("Failed to create RustlsConfig ({}), falling back to HTTP", e);
                }
            }
        }
        Err(e) => {
            warn!("Failed to generate TLS certificate ({}), falling back to HTTP", e);
        }
    }

    info!("OpenCam HTTP Web Camera Server starting on http://0.0.0.0:{}", port);
    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Web server error: {}", e);
            }
        }
        Err(e) => {
            warn!("Failed to bind web server to {}: {}", addr, e);
        }
    }
}

async fn serve_camera_page() -> Html<&'static str> {
    Html(CAMERA_PAGE_HTML)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(app_state): State<AppState>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, app_state))
}

async fn handle_socket(mut socket: WebSocket, app_state: AppState) {
    let client_id = format!("web_cam_{}", CLIENT_COUNTER.fetch_add(1, Ordering::Relaxed));
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ControlCommand>(32);

    info!("New mobile web client connected: {}", client_id);

    // Initialise la caméra dans l'état de l'application
    let device_info = DeviceInfo {
        device_id: client_id.clone(),
        device_name: "Navigateur Mobile (Web)".to_string(),
        os: "HTML5 / WebRTC".to_string(),
        protocol_version: 1,
        battery_level: None,
        is_charging: None,
    };

    let resolutions = vec![
        core_protocol::Resolution { width: 1920, height: 1080, max_fps: 60 },
        core_protocol::Resolution { width: 1280, height: 720, max_fps: 60 },
    ];

    let capabilities = CameraCapabilities {
        cameras: vec![
            CameraInfo {
                id: "back".to_string(),
                name: "Caméra Arrière".to_string(),
                facing: core_protocol::CameraFacing::Back,
                resolutions: resolutions.clone(),
                zoom_range: (1.0, 5.0),
                has_torch: true,
            },
            CameraInfo {
                id: "front".to_string(),
                name: "Caméra Avant".to_string(),
                facing: core_protocol::CameraFacing::Front,
                resolutions,
                zoom_range: (1.0, 1.0),
                has_torch: false,
            },
        ],
        supported_codecs: vec![core_protocol::CodecType::MJPEG, core_protocol::CodecType::RGBA],
        supported_pixel_formats: vec![core_protocol::PixelFormat::RGBA],
    };

    let mut camera = ConnectedCamera::new(device_info, capabilities, cmd_tx);
    camera.frame_width = 1280;
    camera.frame_height = 720;

    {
        let mut cams = app_state.cameras.lock().unwrap();
        cams.insert(client_id.clone(), camera);
        let mut selected = app_state.selected_device_id.lock().unwrap();
        if selected.is_none() {
            *selected = Some(client_id.clone());
        }
    }

    let mut last_fps_check = Instant::now();
    let mut frames_since_check = 0u32;
    let mut total_bytes_since_check = 0u64;

    loop {
        tokio::select! {
            // Envoi de commandes reçues depuis l'UI desktop vers le navigateur smartphone
            Some(cmd) = cmd_rx.recv() => {
                let msg_str = match cmd {
                    ControlCommand::SelectCamera { camera_id } => {
                        format!("{{\"action\":\"switch_camera\",\"camera\":\"{}\"}}", camera_id)
                    }
                    ControlCommand::SetTorch { enabled } => {
                        format!("{{\"action\":\"torch\",\"enabled\":{}}}", enabled)
                    }
                    ControlCommand::SetZoom { factor } => {
                        format!("{{\"action\":\"zoom\",\"factor\":{}}}", factor)
                    }
                    _ => continue,
                };
                if socket.send(Message::Text(msg_str.into())).await.is_err() {
                    break;
                }
            }

            // Réception des frames vidéo du smartphone
            Some(msg_result) = socket.recv() => {
                match msg_result {
                    Ok(Message::Binary(bytes)) => {
                        let bytes_len = bytes.len() as u64;
                        total_bytes_since_check += bytes_len;
                        frames_since_check += 1;

                        // Décode l'image JPEG hors du thread async Tokio pour éviter toute latence
                        let decoded = tokio::task::spawn_blocking(move || {
                            image::load_from_memory(&bytes).ok().map(|dyn_img| {
                                let rgba_img = dyn_img.to_rgba8();
                                let (w, h) = rgba_img.dimensions();
                                (rgba_img.into_raw(), w, h)
                            })
                        })
                        .await
                        .ok()
                        .flatten();

                        if let Some((raw_rgba, w, h)) = decoded {
                            let mut cams = app_state.cameras.lock().unwrap();
                            if let Some(cam) = cams.get_mut(&client_id) {
                                let dt = cam.last_frame_time.elapsed().as_secs_f32().clamp(0.005, 0.2);
                                cam.head_tracker.config = cam.face_tracking.clone();
                                let detected = if cam.face_tracking.enabled {
                                    crate::frame_proc::detect_face_bbox(&raw_rgba, w as usize, h as usize)
                                } else {
                                    None
                                };
                                let (cx, cy, track_zoom) = cam.head_tracker.update(detected, dt);
                                let effective_zoom = (cam.zoom * track_zoom).clamp(1.0, 8.0);

                                let (proc_rgba, proc_w, proc_h) = crate::frame_proc::process_frame(
                                    &raw_rgba,
                                    w as usize,
                                    h as usize,
                                    cam.orientation,
                                    cam.mirror_h,
                                    cam.mirror_v,
                                    effective_zoom,
                                    cx,
                                    cy,
                                );

                                let pw = proc_w as u32;
                                let ph = proc_h as u32;
                                let proc_arc = Arc::new(proc_rgba);

                                cam.frame_width = pw;
                                cam.frame_height = ph;
                                cam.latest_rgba_frame = Some(proc_arc.clone());
                                cam.last_frame_time = Instant::now();
                                cam.frame_count += 1;

                                // Si la diffusion vers la webcam virtuelle est active, injecte la frame
                                if cam.vcam_enabled {
                                    let dev_path = cam.selected_vcam_device.clone();
                                    if let Some(ref mut sink) = cam.vcam_sink {
                                        if sink.config().map(|c| (c.width, c.height)) != Some((pw, ph)) {
                                            let _ = sink.open(vcam_sink::VcamConfig {
                                                device_path: dev_path,
                                                width: pw,
                                                height: ph,
                                                fps: 30,
                                                format: core_protocol::PixelFormat::YUYV,
                                            });
                                        }
                                        // Conversion RGBA vers YUYV pour vcam
                                        let yuyv = rgba_to_yuyv(&proc_arc, proc_w, proc_h);
                                        let _ = sink.write_frame(&yuyv);
                                    }
                                }
                            }
                        }

                        // Mise à jour périodique des statistiques (FPS / débit)
                        if last_fps_check.elapsed().as_secs_f32() >= 1.0 {
                            let elapsed = last_fps_check.elapsed().as_secs_f32();
                            let fps = frames_since_check as f32 / elapsed;
                            let kbps = (total_bytes_since_check as f32 * 8.0) / (elapsed * 1000.0);

                            *app_state.total_bandwidth_kbps.lock().unwrap() = kbps;

                            let mut cams = app_state.cameras.lock().unwrap();
                            if let Some(cam) = cams.get_mut(&client_id) {
                                cam.latest_stats = Some(StreamStats {
                                    fps,
                                    bitrate_kbps: kbps as u32,
                                    frames_sent: cam.frame_count,
                                    frames_dropped: 0,
                                    battery_pct: 100,
                                    temperature_c: 32.0,
                                    timestamp_ms: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0),
                                });
                            }

                            frames_since_check = 0;
                            total_bytes_since_check = 0;
                            last_fps_check = Instant::now();
                        }
                    }
                    Ok(Message::Text(txt)) => {
                        // Réception de métadonnées de l'appareil (ex: nom du smartphone)
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                            if let Some(dev_name) = v.get("device_name").and_then(|s| s.as_str()) {
                                let mut cams = app_state.cameras.lock().unwrap();
                                if let Some(cam) = cams.get_mut(&client_id) {
                                    cam.device_info.device_name = dev_name.to_string();
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => {
                        break;
                    }
                    _ => {}
                }
            }
            else => break,
        }
    }

    info!("Mobile web client disconnected: {}", client_id);
    let mut cams = app_state.cameras.lock().unwrap();
    cams.remove(&client_id);
    let mut selected = app_state.selected_device_id.lock().unwrap();
    if selected.as_deref() == Some(&client_id) {
        *selected = cams.keys().next().cloned();
    }
}

/// Convertit un buffer RGBA en buffer YUYV (4:2:2) standard pour les webcams virtuelles.
fn rgba_to_yuyv(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut yuyv = vec![0u8; width * height * 2];
    let num_pairs = (width * height) / 2;

    for i in 0..num_pairs {
        let px1_idx = i * 2 * 4;
        let px2_idx = (i * 2 + 1) * 4;

        if px2_idx + 3 >= rgba.len() {
            break;
        }

        let r1 = rgba[px1_idx] as f32;
        let g1 = rgba[px1_idx + 1] as f32;
        let b1 = rgba[px1_idx + 2] as f32;

        let r2 = rgba[px2_idx] as f32;
        let g2 = rgba[px2_idx + 1] as f32;
        let b2 = rgba[px2_idx + 2] as f32;

        // RGB to YUV formula
        let y1 = (0.299 * r1 + 0.587 * g1 + 0.114 * b1).clamp(0.0, 255.0) as u8;
        let y2 = (0.299 * r2 + 0.587 * g2 + 0.114 * b2).clamp(0.0, 255.0) as u8;

        let r_avg = (r1 + r2) * 0.5;
        let g_avg = (g1 + g2) * 0.5;
        let b_avg = (b1 + b2) * 0.5;

        let u = (-0.1687 * r_avg - 0.3313 * g_avg + 0.5 * b_avg + 128.0).clamp(0.0, 255.0) as u8;
        let v = (0.5 * r_avg - 0.4187 * g_avg - 0.0813 * b_avg + 128.0).clamp(0.0, 255.0) as u8;

        let out_idx = i * 4;
        yuyv[out_idx] = y1;
        yuyv[out_idx + 1] = u;
        yuyv[out_idx + 2] = y2;
        yuyv[out_idx + 3] = v;
    }

    yuyv
}

/// Page Web servie aux smartphones scannant le QR code.
/// Conçue spécifiquement pour iOS Safari & Android Chrome (HTML5 getUserMedia + WebSocket).
const CAMERA_PAGE_HTML: &str = r##"<!DOCTYPE html>
<html lang="fr">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover">
    <title>OpenCam — Caméra Direct</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; user-select: none; -webkit-user-select: none; }
        body, html { width: 100%; height: 100%; background: #0c0d10; color: #fff; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; overflow: hidden; }
        
        #video-container { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; }
        video { width: 100%; height: 100%; object-fit: cover; }
        
        .hud { position: absolute; inset: 0; pointer-events: none; display: flex; flex-direction: column; justify-content: space-between; padding: env(safe-area-inset-top, 20px) 20px env(safe-area-inset-bottom, 24px); }
        .pill { background: rgba(32, 34, 42, 0.88); color: #ffffff; border: 1px solid rgba(255, 255, 255, 0.30); backdrop-filter: blur(20px); -webkit-backdrop-filter: blur(20px); border-radius: 999px; padding: 8px 16px; font-size: 0.8125rem; font-weight: 500; display: inline-flex; align-items: center; gap: 8px; }
        .dot { width: 8px; height: 8px; border-radius: 50%; background: #e5484d; box-shadow: 0 0 10px #e5484d; }
        .dot.connecting { background: #f59e0b; box-shadow: 0 0 10px #f59e0b; }
        
        .controls { display: flex; justify-content: center; gap: 14px; pointer-events: auto; }
        .btn { background: rgba(255, 255, 255, 0.95); color: #111; border: none; border-radius: 999px; padding: 14px 24px; font-size: 0.9375rem; font-weight: 600; cursor: pointer; display: flex; align-items: center; gap: 8px; box-shadow: 0 10px 30px rgba(0,0,0,0.4); }
        .btn:active { transform: scale(0.96); opacity: 0.9; }
        .btn-icon { width: 52px; height: 52px; padding: 0; justify-content: center; border-radius: 50%; background: rgba(34, 36, 44, 0.90); border: 1px solid rgba(255,255,255,0.32); color: #fff; }
        
        #start-overlay { position: fixed; inset: 0; background: #0c0d10; z-index: 100; display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 32px; text-align: center; }
        #start-overlay h1 { font-size: 1.875rem; font-weight: 700; margin-bottom: 12px; letter-spacing: -0.02em; }
        #start-overlay p { color: rgba(255,255,255,0.65); font-size: 0.9375rem; margin-bottom: 32px; line-height: 1.5; max-width: 320px; }
        #start-btn { background: #fff; color: #000; border: none; border-radius: 999px; padding: 16px 36px; font-size: 1.0625rem; font-weight: 600; cursor: pointer; }
    </style>
</head>
<body>
    <div id="start-overlay">
        <h1>OpenCam Direct</h1>
        <p>Touchez le bouton pour démarrer la diffusion de votre caméra vers l’ordinateur.</p>
        <button id="start-btn">Démarrer la caméra</button>
    </div>

    <div id="video-container">
        <video id="preview" playsinline autoplay muted></video>
    </div>

    <div class="hud">
        <div style="display:flex; justify-content:space-between; align-items:center;">
            <div class="pill">
                <span class="dot connecting" id="status-dot"></span>
                <span id="status-text">Connexion...</span>
            </div>
            <div class="pill" id="fps-badge">0 fps</div>
        </div>

        <div class="controls">
            <button class="btn btn-icon" id="flip-btn" aria-label="Changer de caméra">
                <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 19H4a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h5"/><path d="M13 5h7a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-5"/><circle cx="12" cy="12" r="3"/><path d="m18 22-3-3 3-3"/><path d="m6 2 3 3-3 3"/></svg>
            </button>
            <button class="btn btn-icon" id="torch-btn" aria-label="Torche">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2 3 14h9l-1 8 10-12h-9l1-8z"/></svg>
            </button>
        </div>
    </div>

    <canvas id="offscreen-canvas" style="display:none;"></canvas>

    <script>
        (() => {
            const video = document.getElementById("preview");
            const startOverlay = document.getElementById("start-overlay");
            const startBtn = document.getElementById("start-btn");
            const statusDot = document.getElementById("status-dot");
            const statusText = document.getElementById("status-text");
            const fpsBadge = document.getElementById("fps-badge");
            const flipBtn = document.getElementById("flip-btn");
            const torchBtn = document.getElementById("torch-btn");
            const canvas = document.getElementById("offscreen-canvas");
            const ctx = canvas.getContext("2d", { alpha: false, willReadFrequently: true });

            let currentFacing = "environment";
            let activeStream = null;
            let currentTrack = null;
            let ws = null;
            let torchActive = false;
            let frameCount = 0;
            let lastFpsTime = performance.now();

            // Nom de l'appareil (ex: iPhone, Android)
            const ua = navigator.userAgent;
            const deviceName = /iPhone/.test(ua) ? "iPhone (Web)" : /iPad/.test(ua) ? "iPad (Web)" : /Android/.test(ua) ? "Android (Web)" : "Smartphone (Web)";

            startBtn.addEventListener("click", () => {
                startOverlay.style.display = "none";
                initCamera();
                connectWebSocket();
            });

            async function initCamera() {
                if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
                    if (location.protocol !== "https:") {
                        location.replace("https://" + location.host + location.pathname);
                        return;
                    }
                    alert("Votre navigateur bloque l'accès à la caméra. Vérifiez les permissions de la page et assurez-vous d'utiliser HTTPS.");
                    return;
                }
                if (activeStream) {
                    activeStream.getTracks().forEach(t => t.stop());
                }
                try {
                    const constraints = {
                        audio: false,
                        video: {
                            facingMode: { ideal: currentFacing },
                            width: { ideal: 3840 },
                            height: { ideal: 2160 },
                            frameRate: { ideal: 60 }
                        }
                    };
                    const stream = await navigator.mediaDevices.getUserMedia(constraints);
                    activeStream = stream;
                    currentTrack = stream.getVideoTracks()[0];
                    video.srcObject = stream;
                    await video.play();

                    updateCanvasSize();
                } catch (e) {
                    alert("Impossible d'accéder à la caméra : " + e.message);
                }
            }

            function updateCanvasSize() {
                const vw = video.videoWidth || 1920;
                const vh = video.videoHeight || 1080;
                const cw = vw & ~1;
                const ch = vh & ~1;
                if (canvas.width !== cw || canvas.height !== ch) {
                    canvas.width = cw || 1920;
                    canvas.height = ch || 1080;
                }
            }

            flipBtn.addEventListener("click", () => {
                currentFacing = currentFacing === "environment" ? "user" : "environment";
                initCamera();
            });

            torchBtn.addEventListener("click", async () => {
                if (!currentTrack) return;
                try {
                    torchActive = !torchActive;
                    await currentTrack.applyConstraints({ advanced: [{ torch: torchActive }] });
                    torchBtn.style.color = torchActive ? "#f59e0b" : "#fff";
                } catch (_) {}
            });

            let streamTimer = null;
            function connectWebSocket() {
                const proto = location.protocol === "https:" ? "wss:" : "ws:";
                ws = new WebSocket(`${proto}//${location.host}/ws`);
                ws.binaryType = "arraybuffer";

                ws.onopen = () => {
                    statusDot.className = "dot";
                    statusText.textContent = "En direct";
                    ws.send(JSON.stringify({ device_name: deviceName }));
                    startStreamingLoop();
                };

                ws.onclose = () => {
                    if (streamTimer) {
                        clearTimeout(streamTimer);
                        streamTimer = null;
                    }
                    statusDot.className = "dot connecting";
                    statusText.textContent = "Reconnexion...";
                    setTimeout(connectWebSocket, 1500);
                };

                ws.onmessage = (e) => {
                    try {
                        const data = JSON.parse(e.data);
                        if (data.action === "switch_camera") {
                            flipBtn.click();
                        } else if (data.action === "torch") {
                            torchBtn.click();
                        }
                    } catch (_) {}
                };
            }

            function startStreamingLoop() {
                if (streamTimer) {
                    clearTimeout(streamTimer);
                    streamTimer = null;
                }
                const targetMs = 16; // jusqu'à 60 fps selon les capacités de la caméra

                function tick() {
                    if (!ws || ws.readyState !== WebSocket.OPEN) return;
                    const t0 = performance.now();

                    if (!video.videoWidth || ws.bufferedAmount > 524288) {
                        streamTimer = setTimeout(tick, 10);
                        return;
                    }

                    updateCanvasSize();
                    ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
                    canvas.toBlob((blob) => {
                        if (blob && ws && ws.readyState === WebSocket.OPEN) {
                            blob.arrayBuffer().then((buf) => {
                                if (ws && ws.readyState === WebSocket.OPEN) {
                                    ws.send(buf);
                                    frameCount++;
                                }
                                const now = performance.now();
                                if (now - lastFpsTime >= 1000) {
                                    const currentFps = Math.round((frameCount * 1000) / (now - lastFpsTime));
                                    fpsBadge.textContent = `${currentFps} fps`;
                                    frameCount = 0;
                                    lastFpsTime = now;
                                }
                                const elapsed = now - t0;
                                streamTimer = setTimeout(tick, Math.max(1, targetMs - elapsed));
                            });
                        } else {
                            streamTimer = setTimeout(tick, targetMs);
                        }
                    }, "image/jpeg", 0.65);
                }

                tick();
            }
        })();
    </script>
</body>
</html>
"##;
