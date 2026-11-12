#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod network;
mod state;
mod ui;
mod web_server;

use state::AppState;
use std::net::SocketAddr;
use tracing::info;
use ui::DesktopStudioApp;

#[cfg(target_os = "windows")]
fn show_native_error(title: &str, message: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    let wide_msg: Vec<u16> = OsStr::new(message).encode_wide().chain(std::iter::once(0)).collect();
    let wide_title: Vec<u16> = OsStr::new(title).encode_wide().chain(std::iter::once(0)).collect();
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            std::ptr::null_mut(),
            wide_msg.as_ptr(),
            wide_title.as_ptr(),
            0x00000010, // MB_ICONERROR
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn show_native_error(_title: &str, message: &str) {
    eprintln!("{}", message);
}

fn main() -> Result<(), anyhow::Error> {
    // 0. Install crash hook so errors are never silently swallowed on Windows
    std::panic::set_hook(Box::new(|info| {
        let msg = format!(
            "OpenCam Studio a rencontré une erreur inattendue :\n\n{}\n\nUn rapport d'erreur a été sauvegardé dans opencam-crash.log",
            info
        );
        let _ = std::fs::write("opencam-crash.log", &msg);
        show_native_error("OpenCam Studio - Erreur fatale", &msg);
    }));

    // 1. Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,app_desktop=debug".into()),
        )
        .init();

    info!("Starting OpenCam Studio...");

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

    // 3. Start Tokio runtime for background network tasks
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("Impossible d'initialiser le runtime Tokio : {:?}", e))?;

    let _guard = rt.enter();

    // 4. Start mDNS broadcaster
    let _mdns = network::start_mdns_broadcaster(state.clone(), state.server_control_port);

    // 5. Start TCP Control RPC Server
    let control_addr: SocketAddr = format!("0.0.0.0:{}", state.server_control_port).parse()?;
    let control_state = state.clone();
    rt.spawn(async move {
        if let Err(e) = network::run_control_server(control_state, control_addr).await {
            tracing::error!("Control server error: {}", e);
        }
    });

    // 6. Start UDP Video Stream Receiver
    let video_state = state.clone();
    let video_port = state.server_video_port;
    rt.spawn(async move {
        if let Err(e) = network::run_video_receiver(video_state, video_port).await {
            tracing::error!("Video receiver error: {}", e);
        }
    });

    // 7. Start HTML5 Mobile Web Streaming Server (No-App mode)
    let web_state = state.clone();
    let web_port = state.server_web_port;
    rt.spawn(async move {
        web_server::start_web_server(web_state, web_port).await;
    });

    // 8. Launch egui Studio UI on the genuine OS main thread
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("OpenCam Studio"),
        wgpu_options: {
            let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
            setup.instance_descriptor.backends = eframe::wgpu::Backends::all();
            eframe::egui_wgpu::WgpuConfiguration {
                wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(setup),
                ..Default::default()
            }
        },
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "OpenCam Studio",
        native_options,
        Box::new(|_cc| Ok(Box::new(DesktopStudioApp::new(state)))),
    ) {
        let msg = format!("Erreur lors du lancement de l'interface graphique :\n\n{:?}", e);
        let _ = std::fs::write("opencam-crash.log", &msg);
        show_native_error("OpenCam Studio - Erreur d'affichage", &msg);
        return Err(anyhow::anyhow!("Eframe GUI error: {:?}", e));
    }

    Ok(())
}
