mod transmitter;

use egui::{Color32, ColorImage, ImageData, Stroke, TextureHandle, TextureOptions, Vec2};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use transmitter::TransmitterEngine;

// Refined Palette
const CARD_BORDER: Color32 = Color32::from_rgb(38, 43, 56);
const ACCENT_BLUE: Color32 = Color32::from_rgb(37, 99, 235);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(241, 245, 249);
const TEXT_SECONDARY: Color32 = Color32::from_rgb(148, 163, 184);
const TEXT_MUTED: Color32 = Color32::from_rgb(100, 116, 139);
const STATUS_GREEN: Color32 = Color32::from_rgb(16, 185, 129);
const STATUS_OFF: Color32 = Color32::from_rgb(71, 85, 105);

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
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("OpenCam")
                        .size(16.0)
                        .strong()
                        .color(TEXT_PRIMARY),
                );
                ui.label(
                    egui::RichText::new("Mobile")
                        .size(12.0)
                        .strong()
                        .color(ACCENT_BLUE),
                );
            });
            ui.add_space(8.0);

            let is_conn = self.engine.is_connected.load(Ordering::Relaxed);
            let is_stream = self.engine.is_streaming.load(Ordering::Relaxed);

            // Connection Status Banner
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    let dot_color = if is_conn { STATUS_GREEN } else { STATUS_OFF };
                    let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot_rect.center(), 3.5, dot_color);

                    let status_text = if is_conn {
                        if is_stream {
                            "Connecté • Diffusion en direct"
                        } else {
                            "Connecté au Studio (En attente)"
                        }
                    } else {
                        "Prêt à connecter"
                    };

                    ui.label(
                        egui::RichText::new(status_text)
                            .size(12.5)
                            .strong()
                            .color(if is_conn { STATUS_GREEN } else { TEXT_SECONDARY }),
                    );
                });

                if !is_conn {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("RÉCEPTEURS SUR LE RÉSEAU (mDNS)")
                            .size(11.0)
                            .color(TEXT_MUTED)
                            .strong(),
                    );
                    ui.add_space(4.0);

                    let desktops = self.engine.discovered_desktops.lock().unwrap().clone();
                    if desktops.is_empty() {
                        ui.label(
                            egui::RichText::new("Recherche du Studio sur le Wi-Fi local...")
                                .size(12.0)
                                .color(TEXT_MUTED),
                        );
                    } else {
                        for d in desktops {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&d.name)
                                        .size(12.5)
                                        .color(TEXT_PRIMARY),
                                );
                                ui.label(
                                    egui::RichText::new(format!("({}:{})", d.ip, d.port))
                                        .size(11.0)
                                        .color(TEXT_MUTED),
                                );

                                if ui.button("Connecter").clicked() {
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

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    ui.label(
                        egui::RichText::new("CONNEXION DIRECTE PAR ADRESSE IP")
                            .size(11.0)
                            .color(TEXT_MUTED)
                            .strong(),
                    );
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.manual_ip);
                        ui.label(":");
                        ui.text_edit_singleline(&mut self.manual_port);
                        if ui.button("Lancer").clicked() {
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
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("RETOUR VIDÉO (VISEUR)")
                        .size(11.0)
                        .color(TEXT_MUTED)
                        .strong(),
                );
                ui.add_space(4.0);

                let maybe_frame = self.engine.latest_preview_frame.lock().unwrap().clone();
                let w = *self.engine.frame_width.lock().unwrap();
                let h = *self.engine.frame_height.lock().unwrap();

                let available_w = ui.available_width().max(300.0);
                let preview_h = available_w * 9.0 / 16.0;

                egui::Frame::new()
                    .fill(Color32::BLACK)
                    .stroke(Stroke::new(1.0, CARD_BORDER))
                    .corner_radius(8.0)
                    .show(ui, |ui| {
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
                                    ui.label(
                                        egui::RichText::new("Caméra en attente d'activation")
                                            .size(12.0)
                                            .color(TEXT_MUTED),
                                    );
                                });
                            });
                        }
                    });
            });

            ui.add_space(10.0);

            // Live telemetry readout
            ui.horizontal(|ui| {
                let fps = *self.engine.fps_counter.lock().unwrap();
                let bitrate = *self.engine.bitrate_kbps.lock().unwrap();
                ui.label(
                    egui::RichText::new(format!("Cadence : {:.1} FPS", fps))
                        .size(11.5)
                        .color(TEXT_SECONDARY),
                );
                ui.separator();
                ui.label(
                    egui::RichText::new(format!("Débit : {} kbps", bitrate))
                        .size(11.5)
                        .color(TEXT_SECONDARY),
                );
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
            .with_title("OpenCam Mobile"),
        ..Default::default()
    };

    eframe::run_native(
        "OpenCam Mobile",
        native_options,
        Box::new(|_cc| Ok(Box::new(MobileApp::new(engine)))),
    )
    .map_err(|e| anyhow::anyhow!("Mobile GUI error: {:?}", e))?;

    Ok(())
}
