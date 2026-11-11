use crate::state::AppState;
use core_protocol::commands::ControlCommand;
use core_protocol::Orientation;
use egui::{Color32, ColorImage, ImageData, Panel, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use std::sync::Arc;
use vcam_sink::{create_default_sink, VcamConfig};

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
        // Request immediate redraw for smooth 60fps video preview
        ui.ctx().request_repaint();

        // Top Navigation & Discovery Bar
        Panel::top("top_header").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new("📷 OpenCamLink Studio")
                        .color(Color32::from_rgb(0, 220, 255))
                        .strong(),
                );
                ui.separator();

                let is_mdns = *self.state.is_mdns_active.lock().unwrap();
                let server_ip = self.state.server_ip.lock().unwrap().clone();
                let port = self.state.server_control_port;

                if is_mdns {
                    ui.label(
                        egui::RichText::new("● mDNS Actif (_opencamlink._tcp)")
                            .color(Color32::from_rgb(50, 220, 80))
                            .small(),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("○ mDNS Inactif")
                            .color(Color32::from_rgb(200, 100, 100))
                            .small(),
                    );
                }

                ui.separator();
                ui.label(format!("Serveur : {}:{}", server_ip, port));

                let cam_count = self.state.cameras.lock().unwrap().len();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{} Appareil(s) connecté(s)", cam_count))
                            .strong()
                            .color(if cam_count > 0 {
                                Color32::from_rgb(80, 240, 120)
                            } else {
                                Color32::LIGHT_GRAY
                            }),
                    );
                });
            });
            ui.add_space(6.0);
        });

        // Bottom Telemetry Bar
        Panel::bottom("bottom_status").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let bandwidth = *self.state.total_bandwidth_kbps.lock().unwrap();
                ui.label(format!("Débit Réseau : {:.1} Mbps", bandwidth / 1024.0));
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
                        ui.label(format!("FPS Caméra : {:.1}", fps));
                        ui.separator();
                        ui.label(format!("Résolution : {}x{}", cam.frame_width, cam.frame_height));
                        ui.separator();
                        if let Some(ref stats) = cam.latest_stats {
                            ui.label(format!("Batterie : {}%", stats.battery_pct));
                            ui.separator();
                            ui.label(format!("Temp : {:.1}°C", stats.temperature_c));
                        }
                    }
                } else {
                    ui.label("En attente de connexion d'un smartphone...");
                }
            });
            ui.add_space(4.0);
        });

        // Right Inspector & Controls Sidebar (OBS/Camo Studio style)
        Panel::right("right_controls")
            .default_size(320.0)
            .show(ui, |ui| {
                ui.add_space(10.0);
                ui.heading("Réglages Caméra");
                ui.separator();

                let selected_id_opt = self.state.selected_device_id.lock().unwrap().clone();

                if let Some(dev_id) = selected_id_opt {
                    let mut cams = self.state.cameras.lock().unwrap();
                    if let Some(cam) = cams.get_mut(&dev_id) {
                        ui.label(
                            egui::RichText::new(&cam.device_info.device_name)
                                .heading()
                                .color(Color32::WHITE),
                        );
                        ui.label(format!("OS: {} | ID: {}", cam.device_info.os, dev_id));
                        ui.add_space(10.0);

                        // 1. Physical Sensor Selection
                        ui.group(|ui| {
                            ui.label(egui::RichText::new("Capteur Physique").strong());
                            for c in &cam.capabilities.cameras {
                                let is_selected = cam.selected_camera_id == c.id;
                                if ui
                                    .selectable_label(is_selected, format!("📷 {} ({})", c.name, c.facing))
                                    .clicked()
                                    && !is_selected
                                {
                                    cam.selected_camera_id = c.id.clone();
                                    cam.send_command(ControlCommand::SelectCamera {
                                        camera_id: c.id.clone(),
                                    });
                                }
                            }
                        });

                        ui.add_space(10.0);

                        // 2. Digital Zoom Slider
                        ui.group(|ui| {
                            ui.label(egui::RichText::new("Zoom Numérique").strong());
                            let mut zoom = cam.zoom;
                            if ui
                                .add(egui::Slider::new(&mut zoom, 1.0..=10.0).text("x").step_by(0.1))
                                .changed()
                            {
                                cam.zoom = zoom;
                                cam.send_command(ControlCommand::SetZoom { factor: zoom });
                            }
                        });

                        ui.add_space(10.0);

                        // 3. Mirror & Orientation
                        ui.group(|ui| {
                            ui.label(egui::RichText::new("Transformation d'Image").strong());
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
                                ui.label("Rotation :");
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
                        });

                        ui.add_space(10.0);

                        // 4. Torch / Flash
                        ui.group(|ui| {
                            ui.label(egui::RichText::new("Éclairage Smartphone").strong());
                            let mut torch = cam.torch_enabled;
                            let torch_label = if torch { "🔦 Torche ACTIVE" } else { "🔦 Torche Éteinte" };
                            if ui.toggle_value(&mut torch, torch_label).changed() {
                                cam.torch_enabled = torch;
                                cam.send_command(ControlCommand::SetTorch { enabled: torch });
                            }
                        });

                        ui.add_space(15.0);

                        // 5. Virtual Camera Output (v4l2loopback on Linux)
                        ui.group(|ui| {
                            ui.label(
                                egui::RichText::new("🎥 Sortie Webcam Virtuelle (Linux v4l2)")
                                    .strong()
                                    .color(Color32::from_rgb(0, 220, 255)),
                            );

                            ui.horizontal(|ui| {
                                ui.label("Device :");
                                ui.text_edit_singleline(&mut cam.selected_vcam_device);
                            });

                            let vcam_on = cam.vcam_enabled;
                            let btn_text = if vcam_on {
                                egui::RichText::new("DÉSACTIVER WEBCAM VIRTUELLE")
                                    .color(Color32::RED)
                                    .strong()
                            } else {
                                egui::RichText::new("ACTIVER WEBCAM VIRTUELLE")
                                    .color(Color32::from_rgb(0, 255, 120))
                                    .strong()
                            };

                            if ui.button(btn_text).clicked() {
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
                                            tracing::error!("Failed to open vcam: {}", e);
                                        }
                                    }
                                }
                            }

                            if cam.vcam_enabled {
                                ui.label(
                                    egui::RichText::new("● Flux actif vers Discord / OBS / Zoom")
                                        .color(Color32::from_rgb(50, 240, 100))
                                        .small(),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new("○ Non injecté dans /dev/videoX")
                                        .color(Color32::GRAY)
                                        .small(),
                                );
                            }
                        });
                    }
                } else {
                    ui.label("Aucune caméra sélectionnée.");
                    ui.add_space(20.0);
                    ui.label("Connectez un smartphone via le même Wi-Fi pour piloter ses caméras et diffuser son flux en direct.");
                }
            });

        // Center Dynamic Multi-Cam Grid
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
                        ui.heading("📡 En attente de connexion...");
                        ui.add_space(10.0);
                        ui.label("Ouvrez l'application mobile OpenCamLink sur votre smartphone.");
                        ui.label("Elle détectera automatiquement ce récepteur via mDNS sur le réseau Wi-Fi local.");
                    });
                });
            } else {
                // Multi-Cam dynamic tiles grid
                let available_size = ui.available_size();
                let tile_width = (available_size.x * 0.95).max(320.0);
                let tile_height = (tile_width * 9.0 / 16.0).min(available_size.y * 0.9);

                for (dev_id, dev_name, width, height, maybe_frame) in cams_snapshot {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&dev_name).strong().heading());
                            ui.label(format!("({}x{})", width, height));

                            let is_selected = self.state.selected_device_id.lock().unwrap().as_deref() == Some(&dev_id);
                            if !is_selected && ui.button("Sélectionner pour contrôle").clicked() {
                                *self.state.selected_device_id.lock().unwrap() = Some(dev_id.clone());
                            }
                        });

                        ui.add_space(4.0);

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
                                    ui.label("Chargement du flux vidéo...");
                                });
                            });
                        }
                    });
                }
            }
        });
    }
}
