use crate::state::AppState;
use core_protocol::commands::ControlCommand;
use core_protocol::{FramingMode, Orientation};
use egui::{Color32, ColorImage, ImageData, Panel, Stroke, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use std::sync::Arc;
use vcam_sink::{create_default_sink, VcamConfig};

// Refined Apple / Pro Studio Dark Palette
const CARD_BORDER: Color32 = Color32::from_rgb(38, 43, 56);
const ACCENT_BLUE: Color32 = Color32::from_rgb(37, 99, 235);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(241, 245, 249);
const TEXT_SECONDARY: Color32 = Color32::from_rgb(148, 163, 184);
const TEXT_MUTED: Color32 = Color32::from_rgb(100, 116, 139);
const STATUS_GREEN: Color32 = Color32::from_rgb(16, 185, 129);
const STATUS_OFF: Color32 = Color32::from_rgb(71, 85, 105);

pub struct DesktopStudioApp {
    state: AppState,
    textures: HashMap<String, TextureHandle>,
}

impl DesktopStudioApp {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            textures: HashMap::new(),
        }
    }
}

impl eframe::App for DesktopStudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Request repaint for smooth 60fps video preview
        ui.ctx().request_repaint();

        // 1. Top Navigation Bar (Sleek Pro Bar)
        Panel::top("top_header").show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                // Wordmark & Version
                ui.label(
                    egui::RichText::new("OpenCam")
                        .size(16.0)
                        .strong()
                        .color(TEXT_PRIMARY),
                );
                ui.label(
                    egui::RichText::new("Studio")
                        .size(12.0)
                        .color(ACCENT_BLUE)
                        .strong(),
                );

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                // mDNS & Network indicator
                let is_mdns = *self.state.is_mdns_active.lock().unwrap();
                let server_ip = self.state.server_ip.lock().unwrap().clone();
                let port = self.state.server_control_port;

                ui.horizontal(|ui| {
                    let dot_color = if is_mdns { STATUS_GREEN } else { STATUS_OFF };
                    let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot_rect.center(), 3.5, dot_color);

                    ui.label(
                        egui::RichText::new(format!("Réseau : {}:{}", server_ip, port))
                            .size(12.0)
                            .color(TEXT_SECONDARY),
                    );
                });

                // Connected count on right
                let cam_count = self.state.cameras.lock().unwrap().len();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let badge_text = if cam_count == 0 {
                        "Aucun smartphone connecté".to_string()
                    } else {
                        format!("{} smartphone(s) en ligne", cam_count)
                    };

                    ui.label(
                        egui::RichText::new(badge_text)
                            .size(12.0)
                            .color(if cam_count > 0 { STATUS_GREEN } else { TEXT_MUTED }),
                    );
                });
            });
            ui.add_space(8.0);
        });

        // 2. Bottom Status Bar (Discrete Performance & Vcam Stats)
        Panel::bottom("bottom_status").show(ui, |ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                let bandwidth = *self.state.total_bandwidth_kbps.lock().unwrap();
                ui.label(
                    egui::RichText::new(format!("Débit : {:.1} Mbps", bandwidth / 1024.0))
                        .size(11.5)
                        .color(TEXT_MUTED),
                );
                ui.separator();

                let selected_id = self.state.selected_device_id.lock().unwrap().clone();
                if let Some(ref dev_id) = selected_id {
                    let cams = self.state.cameras.lock().unwrap();
                    if let Some(cam) = cams.get(dev_id) {
                        let fps = if cam.last_frame_time.elapsed().as_secs_f32() > 1.5 {
                            0.0
                        } else {
                            cam.latest_stats.as_ref().map(|s| s.fps).unwrap_or(30.0)
                        };

                        ui.label(
                            egui::RichText::new(format!("Cadence : {:.1} FPS", fps))
                                .size(11.5)
                                .color(TEXT_SECONDARY),
                        );
                        ui.separator();
                        ui.label(
                            egui::RichText::new(format!("Format : {}×{}", cam.frame_width, cam.frame_height))
                                .size(11.5)
                                .color(TEXT_SECONDARY),
                        );

                        if cam.face_tracking.enabled {
                            ui.separator();
                            ui.label(
                                egui::RichText::new("● Suivi du visage actif")
                                    .size(11.5)
                                    .color(Color32::from_rgb(250, 204, 21)),
                            );
                        }

                        if cam.vcam_enabled {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(format!("● Webcam Virtuelle ({})", cam.selected_vcam_device))
                                    .size(11.5)
                                    .color(STATUS_GREEN),
                            );
                        }
                    }
                } else {
                    ui.label(
                        egui::RichText::new("Prêt pour la diffusion.")
                            .size(11.5)
                            .color(TEXT_MUTED),
                    );
                }
            });
            ui.add_space(5.0);
        });

        // 3. Right Inspector & Controls Panel (Apple HIG Pro Layout)
        Panel::right("right_controls")
            .default_size(340.0)
            .show(ui, |ui| {
                ui.add_space(12.0);

                let selected_id_opt = self.state.selected_device_id.lock().unwrap().clone();

                if let Some(dev_id) = selected_id_opt {
                    let mut cams = self.state.cameras.lock().unwrap();
                    if let Some(cam) = cams.get_mut(&dev_id) {
                        // Device Header
                        ui.label(
                            egui::RichText::new(&cam.device_info.device_name)
                                .size(15.0)
                                .strong()
                                .color(TEXT_PRIMARY),
                        );
                        ui.label(
                            egui::RichText::new(format!("{} • Sans fil (Wi-Fi)", cam.device_info.os))
                                .size(12.0)
                                .color(TEXT_MUTED),
                        );
                        ui.add_space(12.0);

                        // SECTION 1: Suivi Intelligent & Cadreur (Center Stage)
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("CADRAGE INTELLIGENT")
                                        .size(11.0)
                                        .color(ACCENT_BLUE)
                                        .strong(),
                                );
                            });
                            ui.add_space(4.0);

                            // Face Tracking Toggle
                            let mut tracking_on = cam.face_tracking.enabled;
                            if ui.checkbox(&mut tracking_on, "Suivi de la tête (Center Stage)").changed() {
                                cam.face_tracking.enabled = tracking_on;
                                cam.send_command(ControlCommand::SetFaceTracking(cam.face_tracking.clone()));
                            }

                            if cam.face_tracking.enabled {
                                ui.add_space(6.0);

                                // Distance-based Auto Zoom Toggle
                                let mut auto_zoom = cam.face_tracking.auto_zoom;
                                if ui.checkbox(&mut auto_zoom, "Zoom automatique selon la distance").changed() {
                                    cam.face_tracking.auto_zoom = auto_zoom;
                                    cam.send_command(ControlCommand::SetFaceTracking(cam.face_tracking.clone()));
                                }

                                ui.add_space(6.0);
                                ui.label(egui::RichText::new("Cadrage de plan :").size(12.0).color(TEXT_SECONDARY));

                                // Framing Mode Segmented Control
                                ui.horizontal(|ui| {
                                    let modes = [
                                        (FramingMode::HeadAndShoulders, "Visage"),
                                        (FramingMode::Torso, "Buste"),
                                        (FramingMode::GroupWide, "Large"),
                                    ];

                                    for (mode, label) in modes {
                                        let is_sel = cam.face_tracking.framing_mode == mode;
                                        if ui.selectable_label(is_sel, label).clicked() && !is_sel {
                                            cam.face_tracking.framing_mode = mode;
                                            cam.send_command(ControlCommand::SetFaceTracking(cam.face_tracking.clone()));
                                        }
                                    }
                                });

                                ui.add_space(6.0);
                                ui.label(egui::RichText::new("Fluidité du mouvement :").size(12.0).color(TEXT_MUTED));
                                let mut smooth = cam.face_tracking.smoothness;
                                if ui.add(egui::Slider::new(&mut smooth, 0.05..=0.5).text("amorti").step_by(0.01)).changed() {
                                    cam.face_tracking.smoothness = smooth;
                                    cam.send_command(ControlCommand::SetFaceTracking(cam.face_tracking.clone()));
                                }
                            }
                        });

                        ui.add_space(10.0);

                        // SECTION 2: Optique & Caméra
                        ui.group(|ui| {
                            ui.label(
                                egui::RichText::new("OPTIQUE & CAPTEURS")
                                    .size(11.0)
                                    .color(TEXT_MUTED)
                                    .strong(),
                            );
                            ui.add_space(4.0);

                            // Physical Camera Selection
                            ui.label(egui::RichText::new("Objectif actif :").size(12.0).color(TEXT_SECONDARY));
                            ui.horizontal_wrapped(|ui| {
                                for c in &cam.capabilities.cameras {
                                    let is_selected = cam.selected_camera_id == c.id;
                                    let label = format!("{}", c.name);
                                    if ui.selectable_label(is_selected, label).clicked() && !is_selected {
                                        cam.selected_camera_id = c.id.clone();
                                        cam.send_command(ControlCommand::SelectCamera {
                                            camera_id: c.id.clone(),
                                        });
                                    }
                                }
                            });

                            ui.add_space(8.0);

                            // Zoom Slider (disabled or overriding auto-zoom)
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Zoom manuel :").size(12.0).color(TEXT_SECONDARY));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(egui::RichText::new(format!("{:.1}×", cam.zoom)).strong().color(TEXT_PRIMARY));
                                });
                            });

                            let mut zoom = cam.zoom;
                            if ui.add(egui::Slider::new(&mut zoom, 1.0..=10.0).show_value(false).step_by(0.1)).changed() {
                                cam.zoom = zoom;
                                cam.send_command(ControlCommand::SetZoom { factor: zoom });
                            }

                            ui.add_space(8.0);

                            // Mirror & Rotation
                            ui.label(egui::RichText::new("Orientation & Miroir :").size(12.0).color(TEXT_SECONDARY));
                            ui.horizontal(|ui| {
                                let mut mh = cam.mirror_h;
                                if ui.checkbox(&mut mh, "Miroir H").changed() {
                                    cam.mirror_h = mh;
                                    cam.send_command(ControlCommand::SetMirror {
                                        horizontal: cam.mirror_h,
                                        vertical: cam.mirror_v,
                                    });
                                }

                                let mut mv = cam.mirror_v;
                                if ui.checkbox(&mut mv, "Miroir V").changed() {
                                    cam.mirror_v = mv;
                                    cam.send_command(ControlCommand::SetMirror {
                                        horizontal: cam.mirror_h,
                                        vertical: cam.mirror_v,
                                    });
                                }
                            });

                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                for &orient in &[
                                    Orientation::Deg0,
                                    Orientation::Deg90,
                                    Orientation::Deg180,
                                    Orientation::Deg270,
                                ] {
                                    let is_active = cam.orientation == orient;
                                    if ui
                                        .selectable_label(is_active, format!("{}°", orient.as_degrees()))
                                        .clicked()
                                    {
                                        cam.orientation = orient;
                                        cam.send_command(ControlCommand::SetOrientation {
                                            orientation: orient,
                                        });
                                    }
                                }
                            });

                            ui.add_space(8.0);

                            // Flashlight / Torch
                            let mut torch = cam.torch_enabled;
                            let torch_label = if torch { "Éclairage activé" } else { "Éclairage smartphone" };
                            if ui.toggle_value(&mut torch, torch_label).changed() {
                                cam.torch_enabled = torch;
                                cam.send_command(ControlCommand::SetTorch { enabled: torch });
                            }
                        });

                        ui.add_space(10.0);

                        // SECTION 3: Sortie Caméra Virtuelle (v4l2loopback)
                        ui.group(|ui| {
                            ui.label(
                                egui::RichText::new("DIFFUSION WEBCAM VIRTUELLE")
                                    .size(11.0)
                                    .color(STATUS_GREEN)
                                    .strong(),
                            );
                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Périphérique :").size(12.0).color(TEXT_SECONDARY));
                                ui.text_edit_singleline(&mut cam.selected_vcam_device);
                            });

                            ui.add_space(6.0);

                            let vcam_on = cam.vcam_enabled;
                            let (btn_text, btn_color) = if vcam_on {
                                ("Arrêter la Webcam Virtuelle", Color32::from_rgb(220, 38, 38))
                            } else {
                                ("Diffuser vers Discord / OBS / Zoom", ACCENT_BLUE)
                            };

                            let btn = egui::Button::new(
                                egui::RichText::new(btn_text)
                                    .color(Color32::WHITE)
                                    .size(12.5)
                                    .strong(),
                            )
                            .fill(btn_color)
                            .min_size(Vec2::new(ui.available_width(), 32.0));

                            if ui.add(btn).clicked() {
                                if vcam_on {
                                    if let Some(ref mut sink) = cam.vcam_sink {
                                        let _ = sink.close();
                                    }
                                    cam.vcam_enabled = false;
                                } else {
                                    let mut sink = create_default_sink();
                                    let config = VcamConfig {
                                        device_path: cam.selected_vcam_device.clone(),
                                        width: cam.frame_width,
                                        height: cam.frame_height,
                                        fps: 30,
                                        format: core_protocol::PixelFormat::YUYV,
                                    };
                                    match sink.open(config) {
                                        Ok(_) => {
                                            cam.vcam_sink = Some(sink);
                                            cam.vcam_enabled = true;
                                        }
                                        Err(e) => {
                                            tracing::error!("Impossible d'ouvrir le périphérique virtuel: {}", e);
                                        }
                                    }
                                }
                            }

                            ui.add_space(4.0);
                            if cam.vcam_enabled {
                                ui.label(
                                    egui::RichText::new("Flux disponible dans vos applications d'appel vidéo.")
                                        .size(11.0)
                                        .color(STATUS_GREEN),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new("Le flux n'est pas encore injecté dans le système.")
                                        .size(11.0)
                                        .color(TEXT_MUTED),
                                );
                            }
                        });
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(
                            egui::RichText::new("Aucun smartphone sélectionné")
                                .size(14.0)
                                .color(TEXT_MUTED),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("Lancez l'application OpenCam sur votre smartphone connecté au même réseau Wi-Fi.")
                                .size(12.0)
                                .color(TEXT_MUTED),
                        );
                    });
                }
            });

        // 4. Center Viewport (Clean Studio Monitor)
        egui::CentralPanel::default().show(ui, |ui| {
            let cams_snapshot = {
                let cams = self.state.cameras.lock().unwrap();
                cams.iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            v.device_info.device_name.clone(),
                            v.frame_width,
                            v.frame_height,
                            v.latest_rgba_frame.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            };

            if cams_snapshot.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("En attente de connexion...")
                                .size(18.0)
                                .color(TEXT_PRIMARY)
                                .strong(),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("Ouvrez OpenCam sur votre smartphone Android ou iPhone.")
                                .size(13.0)
                                .color(TEXT_SECONDARY),
                        );
                        ui.label(
                            egui::RichText::new("La détection est automatique via mDNS sur le réseau local.")
                                .size(12.0)
                                .color(TEXT_MUTED),
                        );
                    });
                });
            } else {
                let available_size = ui.available_size();
                let tile_width = (available_size.x * 0.98).max(320.0);
                let tile_height = (tile_width * 9.0 / 16.0).min(available_size.y * 0.95);

                for (dev_id, dev_name, width, height, maybe_frame) in cams_snapshot {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&dev_name)
                                    .size(13.5)
                                    .strong()
                                    .color(TEXT_PRIMARY),
                            );

                            ui.label(
                                egui::RichText::new(format!("({}×{})", width, height))
                                    .size(12.0)
                                    .color(TEXT_MUTED),
                            );

                            let is_selected = self.state.selected_device_id.lock().unwrap().as_deref() == Some(&dev_id);
                            if !is_selected && ui.button("Sélectionner").clicked() {
                                *self.state.selected_device_id.lock().unwrap() = Some(dev_id.clone());
                            }
                        });

                        ui.add_space(6.0);

                        // Framed Canvas Container
                        egui::Frame::new()
                            .fill(Color32::BLACK)
                            .stroke(Stroke::new(1.0, CARD_BORDER))
                            .corner_radius(8.0)
                            .show(ui, |ui| {
                                if let Some(frame_data) = maybe_frame {
                                    let color_image = ColorImage::from_rgba_unmultiplied(
                                        [width as usize, height as usize],
                                        &frame_data,
                                    );

                                    let ctx = ui.ctx();
                                    let texture = self.textures.entry(dev_id.clone()).or_insert_with(|| {
                                        ctx.load_texture(
                                            &dev_id,
                                            ImageData::Color(Arc::new(color_image.clone())),
                                            TextureOptions::LINEAR,
                                        )
                                    });

                                    texture.set(ImageData::Color(Arc::new(color_image)), TextureOptions::LINEAR);

                                    ui.add(egui::Image::from_texture(&*texture).fit_to_exact_size(Vec2::new(tile_width, tile_height)));
                                } else {
                                    ui.allocate_ui(Vec2::new(tile_width, tile_height), |ui| {
                                        ui.centered_and_justified(|ui| {
                                            ui.label(
                                                egui::RichText::new("Connexion au flux vidéo...")
                                                    .size(12.0)
                                                    .color(TEXT_MUTED),
                                            );
                                        });
                                    });
                                }
                            });
                    });
                }
            }
        });
    }
}
