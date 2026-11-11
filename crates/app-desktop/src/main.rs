mod network;
mod state;
mod ui;

use state::AppState;
use std::net::SocketAddr;
use tracing::info;
use ui::DesktopStudioApp;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 1. Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,app_desktop=debug".into()),
        )
        .init();

    info!("Starting OpenCamLink Desktop Multi-Camera Studio...");

    let state = AppState::default();

    // 2. Discover available loopback devices
    #[cfg(target_os = "linux")]
    {
        let loopback_devs = vcam_sink::linux::scan_loopback_devices();
        info!("Found {} V4L2 video devices on host", loopback_devs.len());
        for dev in &loopback_devs {
            info!(
                "  -> {} [{}]: is_loopback={}, writable={}",
                dev.path, dev.card_label, dev.is_loopback, dev.is_available
            );
        }
        *state.available_vcam_devices.lock().unwrap() = loopback_devs;
    }

    // 3. Start mDNS broadcaster
    let _mdns = network::start_mdns_broadcaster(state.clone(), state.server_control_port);

    // 4. Start TCP Control RPC Server
    let control_addr: SocketAddr = format!("0.0.0.0:{}", state.server_control_port).parse()?;
    let control_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = network::run_control_server(control_state, control_addr).await {
            tracing::error!("Control server error: {}", e);
        }
    });

    // 5. Start UDP Video Stream Receiver
    let video_state = state.clone();
    let video_port = state.server_video_port;
    tokio::spawn(async move {
        if let Err(e) = network::run_video_receiver(video_state, video_port).await {
            tracing::error!("Video receiver error: {}", e);
        }
    });

    // 6. Launch egui Studio UI on main thread
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("OpenCamLink - Studio Régie Multi-Caméras"),
        ..Default::default()
    };

    eframe::run_native(
        "OpenCamLink Studio",
        native_options,
        Box::new(|_cc| Ok(Box::new(DesktopStudioApp::new(state)))),
    )
    .map_err(|e| anyhow::anyhow!("Eframe GUI error: {:?}", e))?;

    Ok(())
}
