mod transmitter;

use egui::{Color32, ColorImage, ImageData, TextureHandle, TextureOptions, Vec2};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use transmitter::TransmitterEngine;

struct MobileApp {
    engine: Arc<TransmitterEngine>,
    manual_ip: String,
    manual_port: String,
    texture: Option<TextureHandle>,
}

impl MobileApp {
    fn new(engine: Arc<TransmitterEngine>) -> Self {
        Self {
            engine,
            manual_ip: "127.0.0.1".to_string(),
            manual_port: core_protocol::DEFAULT_CONTROL_PORT.to_string(),
            texture: None,
        }
    }
}

impl eframe::App for MobileApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint();

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.heading(
                    egui::RichText::new("📱 OpenCamLink Mobile")
                        .color(Color32::from_rgb(0, 220, 255))
                        .strong(),
                );
            });
            ui.add_space(8.0);

            let is_conn = self.engine.is_connected.load(Ordering::Relaxed);
            let is_stream = self.engine.is_streaming.load(Ordering::Relaxed);

            // Connection Status Banner
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if is_conn {
                        ui.label(
                            egui::RichText::new("● CONNECTÉ AU PC")
                                .color(Color32::from_rgb(50, 240, 100))
                                .strong(),
                        );
                        if is_stream {
                            ui.label(
                                egui::RichText::new("(DIFFUSION ACTIVE)")
                                    .color(Color32::from_rgb(0, 200, 255)),
                            );
                        }
                    } else {
                        ui.label(
                            egui::RichText::new("○ DÉCONNECTÉ")
                                .color(Color32::from_rgb(220, 80, 80))
                                .strong(),
                        );
                    }
                });

                if !is_conn {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Récepteurs détectés sur le Wi-Fi (mDNS) :").strong());

                    let desktops = self.engine.discovered_desktops.lock().unwrap().clone();
                    if desktops.is_empty() {
                        ui.label(egui::RichText::new("Recherche en cours...").italics());
                    } else {
                        for d in desktops {
                            ui.horizontal(|ui| {
                                ui.label(format!("🖥 {}", d.name));
                                ui.label(format!("({}:{})", d.ip, d.port));
                                if ui.button("Connecter en 1 tap").clicked() {
                                    let engine = self.engine.clone();
                                    if let Ok(addr) = format!("{}:{}", d.ip, d.port).parse::<SocketAddr>() {
                                        tokio::spawn(async move {
                                            let _ = engine.connect_and_run(addr).await;
                                        });
                                    }
                                }
                            });
                        }
                    }

                    ui.separator();
                    ui.label("Ou connexion directe par adresse IP :");
                    ui.horizontal(|ui| {
                        ui.label("IP :");
                        ui.text_edit_singleline(&mut self.manual_ip);
                        ui.label("Port :");
                        ui.text_edit_singleline(&mut self.manual_port);
                        if ui.button("Connexion").clicked() {
                            let engine = self.engine.clone();
                            let ip = self.manual_ip.clone();
                            let port = self.manual_port.clone();
                            if let Ok(addr) = format!("{}:{}", ip, port).parse::<SocketAddr>() {
                                tokio::spawn(async move {
                                    let _ = engine.connect_and_run(addr).await;
                                });
                            }
                        }
                    });
                }
            });

            ui.add_space(10.0);

            // Viewfinder Camera Preview
            ui.group(|ui| {
                ui.label(egui::RichText::new("Retour Caméra (Viewfinder)").strong());
                let maybe_frame = self.engine.latest_preview_frame.lock().unwrap().clone();
                let w = *self.engine.frame_width.lock().unwrap();
                let h = *self.engine.frame_height.lock().unwrap();

                let available_w = ui.available_width().max(320.0);
                let preview_h = available_w * 9.0 / 16.0;

                if let Some(frame_data) = maybe_frame {
                    let color_image = ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        &frame_data,
                    );

                    let ctx = ui.ctx();
                    let tex = self.texture.get_or_insert_with(|| {
                        ctx.load_texture(
                            "viewfinder",
                            ImageData::Color(Arc::new(color_image.clone())),
                            TextureOptions::LINEAR,
                        )
                    });
                    tex.set(ImageData::Color(Arc::new(color_image)), TextureOptions::LINEAR);

                    ui.add(egui::Image::from_texture(&*tex).fit_to_exact_size(Vec2::new(available_w, preview_h)));
                } else {
                    ui.allocate_ui(Vec2::new(available_w, preview_h), |ui| {
                        ui.centered_and_justified(|ui| {
                            ui.label("Caméra en veille");
                        });
                    });
                }
            });

            ui.add_space(10.0);

            // Live telemetry
            ui.horizontal(|ui| {
                let fps = *self.engine.fps_counter.lock().unwrap();
                let bitrate = *self.engine.bitrate_kbps.lock().unwrap();
                ui.label(format!("FPS : {:.1}", fps));
                ui.separator();
                ui.label(format!("Débit : {} kbps", bitrate));
            });
        });
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let engine = Arc::new(TransmitterEngine::new());
    let _ = engine.start_mdns_discovery();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 720.0])
            .with_title("OpenCamLink Mobile"),
        ..Default::default()
    };

    eframe::run_native(
        "OpenCamLink Mobile",
        native_options,
        Box::new(|_cc| Ok(Box::new(MobileApp::new(engine)))),
    )
    .map_err(|e| anyhow::anyhow!("Mobile GUI error: {:?}", e))?;

    Ok(())
}
