use crate::state::AppState;
use core_protocol::commands::ControlCommand;
use core_protocol::{FramingMode, Orientation};
use egui::{
    pos2, vec2, Color32, ColorImage, CornerRadius, FontId, ImageData, Panel, Rect, Stroke,
    TextureHandle, TextureOptions,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use vcam_sink::{create_default_sink, VcamConfig};

// ============================================================================
// DIRECTION 2 : PALETTE "RÉGIE BROADCAST PRO" (DaVinci Resolve & Studio ATEM)
// ============================================================================
const BG_STUDIO: Color32 = Color32::from_rgb(14, 15, 18);
const PANEL_CONSOLE: Color32 = Color32::from_rgb(20, 22, 28);
const RACK_BG: Color32 = Color32::from_rgb(25, 27, 35);
const BORDER_SUBTLE: Color32 = Color32::from_rgb(42, 46, 58);

// Broadcast Color Codes
const STUDIO_ORANGE: Color32 = Color32::from_rgb(255, 102, 0);
const STUDIO_AMBER: Color32 = Color32::from_rgb(245, 158, 11);
const TALLY_RED: Color32 = Color32::from_rgb(239, 68, 68); // ON-AIR / PROGRAM
const TALLY_GREEN: Color32 = Color32::from_rgb(34, 197, 94); // STANDBY / PREVIEW
const TALLY_DIM: Color32 = Color32::from_rgb(55, 65, 81);

// Typography & Contrast
const TEXT_HERO: Color32 = Color32::from_rgb(255, 255, 255);
const TEXT_CONSOLE: Color32 = Color32::from_rgb(226, 232, 240);
const TEXT_TECH_LABEL: Color32 = Color32::from_rgb(148, 163, 184);
const TEXT_MUTED: Color32 = Color32::from_rgb(100, 116, 139);

// Audio / Scope VU Meter Colors
const VU_GREEN: Color32 = Color32::from_rgb(34, 197, 94);
const VU_YELLOW: Color32 = Color32::from_rgb(234, 179, 8);
const VU_RED: Color32 = Color32::from_rgb(239, 68, 68);

pub struct DesktopStudioApp {
    state: AppState,
    textures: HashMap<String, TextureHandle>,
    start_time: Instant,
    show_safe_zones: bool,
    show_tracking_hud: bool,
    dual_view: bool,
    pvw_device_id: Option<String>,
}

impl DesktopStudioApp {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            textures: HashMap::new(),
            start_time: Instant::now(),
            show_safe_zones: true,
            show_tracking_hud: true,
            dual_view: false,
            pvw_device_id: None,
        }
    }

    /// Formats app session into SMPTE broadcast timecode TC HH:MM:SS:FF
    fn timecode(&self) -> String {
        let elapsed = self.start_time.elapsed();
        let total_secs = elapsed.as_secs();
        let hrs = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        let secs = total_secs % 60;
        let frames = (elapsed.subsec_millis() as f32 / 1000.0 * 30.0) as u32;
        format!("{:02}:{:02}:{:02}:{:02}", hrs, mins, secs, frames)
    }
}

impl eframe::App for DesktopStudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Continuous repaint for 60 FPS broadcast monitoring
        ui.ctx().request_repaint();

        // Configure Studio Dark Theme Visuals
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = PANEL_CONSOLE;
        visuals.window_fill = BG_STUDIO;
        visuals.override_text_color = Some(TEXT_CONSOLE);
        visuals.widgets.noninteractive.bg_fill = RACK_BG;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(28, 30, 40);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(38, 42, 56);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, STUDIO_ORANGE);
        visuals.widgets.active.bg_fill = STUDIO_ORANGE;
        visuals.selection.bg_fill = STUDIO_ORANGE;
        visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        ui.ctx().set_visuals(visuals);

        // Global State Reads
        let cams_map = self.state.cameras.lock().unwrap();
        let cam_count = cams_map.len();
        let mut pgm_id = self.state.selected_device_id.lock().unwrap().clone();
        if pgm_id.is_none() && cam_count > 0 {
            if let Some(first_key) = cams_map.keys().next() {
                pgm_id = Some(first_key.clone());
                *self.state.selected_device_id.lock().unwrap() = Some(first_key.clone());
            }
        }
        let is_any_on_air = cams_map.values().any(|c| c.vcam_enabled);
        let bandwidth_kbps = *self.state.total_bandwidth_kbps.lock().unwrap();
        let is_mdns = *self.state.is_mdns_active.lock().unwrap();
        let server_ip = self.state.server_ip.lock().unwrap().clone();
        let control_port = self.state.server_control_port;
        drop(cams_map); // Release early for UI mutations

        // ====================================================================
        // 1. TOP BROADCAST HEADER & CAMERA SWITCHING BUS (TALLY BUS)
        // ====================================================================
        Panel::top("top_master_header").show(ui, |ui| {
            ui.add_space(6.0);

            // A. Master Strip: Brand + Master Tally + Timecode + Network
            ui.horizontal(|ui| {
                // Studio Brand
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("OPENCAM")
                            .font(FontId::monospace(15.0))
                            .strong()
                            .color(TEXT_HERO),
                    );
                    let (badge_rect, _) = ui.allocate_exact_size(vec2(86.0, 20.0), egui::Sense::hover());
                    ui.painter().rect_filled(badge_rect, CornerRadius::same(3), Color32::from_rgb(32, 24, 18));
                    ui.painter().rect_stroke(badge_rect, CornerRadius::same(3), Stroke::new(1.0, STUDIO_ORANGE), egui::StrokeKind::Inside);
                    ui.painter().text(
                        badge_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "BROADCAST",
                        FontId::monospace(10.5),
                        STUDIO_ORANGE,
                    );
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(10.0);

                // Master On-Air Tally Pill
                ui.horizontal(|ui| {
                    let tally_color = if is_any_on_air { TALLY_RED } else { TALLY_DIM };
                    let (tally_rect, _) = ui.allocate_exact_size(vec2(90.0, 22.0), egui::Sense::hover());
                    ui.painter().rect_filled(tally_rect, CornerRadius::same(4), Color32::from_rgb(18, 19, 24));
                    ui.painter().rect_stroke(tally_rect, CornerRadius::same(4), Stroke::new(1.2, tally_color), egui::StrokeKind::Inside);

                    let dot_pos = pos2(tally_rect.min.x + 12.0, tally_rect.center().y);
                    ui.painter().circle_filled(dot_pos, 4.0, tally_color);

                    let tally_text = if is_any_on_air { "ON AIR" } else { "STANDBY" };
                    ui.painter().text(
                        pos2(tally_rect.min.x + 22.0, tally_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        tally_text,
                        FontId::monospace(11.0),
                        if is_any_on_air { Color32::WHITE } else { TEXT_MUTED },
                    );
                });

                ui.add_space(14.0);

                // SMPTE Timecode Display
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("TC")
                            .font(FontId::monospace(11.0))
                            .color(STUDIO_ORANGE),
                    );
                    ui.label(
                        egui::RichText::new(self.timecode())
                            .font(FontId::monospace(13.0))
                            .strong()
                            .color(TEXT_CONSOLE),
                    );
                });

                ui.add_space(14.0);

                // Network & Discoverability
                ui.horizontal(|ui| {
                    let net_color = if is_mdns { TALLY_GREEN } else { TALLY_DIM };
                    let (dot_rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot_rect.center(), 3.5, net_color);
                    ui.label(
                        egui::RichText::new(format!("LAN {}:{}", server_ip, control_port))
                            .font(FontId::monospace(11.0))
                            .color(TEXT_TECH_LABEL),
                    );
                });

                // Right Aligned Broadcast Telemetry
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("DÉBIT: {:.2} Mbps", bandwidth_kbps / 1024.0))
                            .font(FontId::monospace(11.0))
                            .color(if bandwidth_kbps > 100.0 { STUDIO_AMBER } else { TEXT_MUTED }),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!("SOURCES: {}", cam_count))
                            .font(FontId::monospace(11.0))
                            .color(if cam_count > 0 { TALLY_GREEN } else { TEXT_MUTED }),
                    );
                });
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // B. Broadcast Camera Switching Bus (Tally Buttons CAM 1..N + Cut / Dual View)
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("PROGRAM BUS")
                        .font(FontId::monospace(11.0))
                        .strong()
                        .color(STUDIO_ORANGE),
                );
                ui.add_space(8.0);

                let cams_list = {
                    let cams = self.state.cameras.lock().unwrap();
                    let mut list = cams.keys().cloned().collect::<Vec<_>>();
                    list.sort();
                    list
                };

                if cams_list.is_empty() {
                    ui.label(
                        egui::RichText::new("AUCUN SIGNAL MOBILE — EN ATTENTE SUR LE RÉSEAU WI-FI")
                            .font(FontId::monospace(11.0))
                            .color(TEXT_MUTED),
                    );
                } else {
                    for (idx, dev_id) in cams_list.iter().enumerate() {
                        let is_pgm = pgm_id.as_deref() == Some(dev_id.as_str());
                        let is_pvw = self.pvw_device_id.as_deref() == Some(dev_id.as_str());

                        // Tally status & styling
                        let (btn_border, btn_text_color, tally_icon) = if is_pgm {
                            (TALLY_RED, Color32::WHITE, "● PGM")
                        } else if is_pvw {
                            (TALLY_GREEN, Color32::WHITE, "○ PVW")
                        } else {
                            (BORDER_SUBTLE, TEXT_TECH_LABEL, "CAM")
                        };

                        let cam_name = {
                            let cams = self.state.cameras.lock().unwrap();
                            cams.get(dev_id)
                                .map(|c| c.device_info.device_name.clone())
                                .unwrap_or_else(|| format!("Cam {}", idx + 1))
                        };

                        let button_text = format!("{} {} [{}]", tally_icon, idx + 1, cam_name);
                        let btn = egui::Button::new(
                            egui::RichText::new(button_text)
                                .font(FontId::monospace(11.5))
                                .strong()
                                .color(btn_text_color),
                        )
                        .stroke(Stroke::new(if is_pgm || is_pvw { 1.5 } else { 1.0 }, btn_border))
                        .fill(if is_pgm {
                            Color32::from_rgb(60, 18, 18)
                        } else if is_pvw {
                            Color32::from_rgb(18, 48, 28)
                        } else {
                            Color32::from_rgb(22, 24, 32)
                        });

                        if ui.add(btn).clicked() {
                            *self.state.selected_device_id.lock().unwrap() = Some(dev_id.clone());
                            pgm_id = Some(dev_id.clone());
                        }
                    }
                }

                // Controls on the right side of the switching bus
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.toggle_value(&mut self.dual_view, "DOUBLEDISPLAY");
                    ui.toggle_value(&mut self.show_safe_zones, "GUIDES SÉCURITÉ");
                    ui.toggle_value(&mut self.show_tracking_hud, "HUD TRACKING");
                });
            });

            ui.add_space(6.0);
        });

        // ====================================================================
        // 2. BOTTOM TELEMETRY DOCK (Régie Master Status)
        // ====================================================================
        Panel::bottom("bottom_telemetry").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("SYSTÈME")
                        .font(FontId::monospace(10.5))
                        .color(STUDIO_ORANGE),
                );
                ui.label(
                    egui::RichText::new("MOTEUR RUST 100% NATIF")
                        .font(FontId::monospace(10.5))
                        .color(TEXT_MUTED),
                );

                ui.separator();

                if let Some(ref dev_id) = pgm_id {
                    let cams = self.state.cameras.lock().unwrap();
                    if let Some(cam) = cams.get(dev_id) {
                        let fps = if cam.last_frame_time.elapsed().as_secs_f32() > 1.5 {
                            0.0
                        } else {
                            cam.latest_stats.as_ref().map(|s| s.fps).unwrap_or(30.0)
                        };

                        ui.label(
                            egui::RichText::new(format!("FLUX PGM : {}×{} @ {:.1} FPS", cam.frame_width, cam.frame_height, fps))
                                .font(FontId::monospace(11.0))
                                .color(TEXT_CONSOLE),
                        );

                        ui.separator();

                        ui.label(
                            egui::RichText::new(format!("PERIPHERIQUE VCAM : {}", cam.selected_vcam_device))
                                .font(FontId::monospace(11.0))
                                .color(if cam.vcam_enabled { TALLY_GREEN } else { TEXT_MUTED }),
                        );

                        ui.separator();

                        let tracking_status = if cam.face_tracking.enabled {
                            format!("CENTER STAGE ACTIF [{:?}]", cam.face_tracking.framing_mode)
                        } else {
                            "CADRAGE MANUEL".to_string()
                        };
                        ui.label(
                            egui::RichText::new(tracking_status)
                                .font(FontId::monospace(11.0))
                                .color(if cam.face_tracking.enabled { STUDIO_AMBER } else { TEXT_MUTED }),
                        );
                    }
                } else {
                    ui.label(
                        egui::RichText::new("EN ATTENTE DU SIGNAL SMARTPHONE")
                            .font(FontId::monospace(11.0))
                            .color(TEXT_MUTED),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("LATENCE < 20ms LAN (UDP DROP-ON-LATE)")
                            .font(FontId::monospace(10.5))
                            .color(TEXT_TECH_LABEL),
                    );
                });
            });
            ui.add_space(4.0);
        });

        // ====================================================================
        // 3. RIGHT INSPECTOR PANEL ("RÉGIE CONTROL RACK")
        // ====================================================================
        Panel::right("right_broadcast_rack")
            .default_size(360.0)
            .min_size(320.0)
            .show(ui, |ui| {
                ui.add_space(8.0);

                if let Some(ref dev_id) = pgm_id {
                    let mut cams = self.state.cameras.lock().unwrap();
                    if let Some(cam) = cams.get_mut(dev_id) {
                        // Title Rack Header
                        ui.horizontal(|ui| {
                            let (ind_rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
                            ui.painter().circle_filled(ind_rect.center(), 4.0, if cam.vcam_enabled { TALLY_RED } else { TALLY_GREEN });
                            ui.label(
                                egui::RichText::new(&cam.device_info.device_name)
                                    .font(FontId::monospace(14.0))
                                    .strong()
                                    .color(TEXT_HERO),
                            );
                            ui.label(
                                egui::RichText::new(format!("({})", cam.device_info.os))
                                    .font(FontId::monospace(11.0))
                                    .color(TEXT_MUTED),
                            );
                        });

                        ui.add_space(8.0);

                        // ----------------------------------------------------
                        // RACK 1 : MASTER OUTPUT & VIRTUAL CAMERA SINK
                        // ----------------------------------------------------
                        egui::Frame::new()
                            .fill(RACK_BG)
                            .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("DIFFUSION WEBCAM VIRTUELLE")
                                            .font(FontId::monospace(11.0))
                                            .strong()
                                            .color(if cam.vcam_enabled { TALLY_RED } else { STUDIO_ORANGE }),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(
                                            egui::RichText::new(if cam.vcam_enabled { "ON-AIR" } else { "STANDBY" })
                                                .font(FontId::monospace(10.0))
                                                .color(if cam.vcam_enabled { TALLY_RED } else { TEXT_MUTED }),
                                        );
                                    });
                                });

                                ui.add_space(6.0);

                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Device :").font(FontId::monospace(11.0)).color(TEXT_TECH_LABEL));
                                    ui.text_edit_singleline(&mut cam.selected_vcam_device);
                                });

                                ui.add_space(8.0);

                                let vcam_on = cam.vcam_enabled;
                                let (btn_label, btn_fill, btn_stroke) = if vcam_on {
                                    ("■ ARRÊTER LA DIFFUSION DU SIGNAL", Color32::from_rgb(180, 28, 28), TALLY_RED)
                                } else {
                                    ("▶ TRANSMETTRE AU SYSTÈME (V4L2 / MF)", Color32::from_rgb(180, 60, 0), STUDIO_ORANGE)
                                };

                                let btn = egui::Button::new(
                                    egui::RichText::new(btn_label)
                                        .font(FontId::monospace(11.5))
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(btn_fill)
                                .stroke(Stroke::new(1.2, btn_stroke))
                                .min_size(vec2(ui.available_width(), 34.0));

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
                                let hint_text = if cam.vcam_enabled {
                                    "Flux YUYV 4:2:2 injecté en temps réel dans OBS / Zoom / Teams."
                                } else {
                                    "Prêt pour transmission. Détection automatique par les applications."
                                };
                                ui.label(egui::RichText::new(hint_text).font(FontId::monospace(10.0)).color(TEXT_MUTED));
                            });

                        ui.add_space(10.0);

                        // ----------------------------------------------------
                        // RACK 2 : CADREUR IA (CENTER STAGE & DISTANCE AUTO-ZOOM)
                        // ----------------------------------------------------
                        egui::Frame::new()
                            .fill(RACK_BG)
                            .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("CENTER STAGE : CADREUR IA")
                                            .font(FontId::monospace(11.0))
                                            .strong()
                                            .color(STUDIO_ORANGE),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let mut tracking_active = cam.face_tracking.enabled;
                                        let toggle_lbl = if tracking_active { "ACTIF" } else { "OFF" };
                                        if ui.toggle_value(&mut tracking_active, toggle_lbl).changed() {
                                            cam.face_tracking.enabled = tracking_active;
                                            cam.send_command(ControlCommand::SetFaceTracking(cam.face_tracking.clone()));
                                        }
                                    });
                                });

                                if cam.face_tracking.enabled {
                                    ui.add_space(8.0);

                                    // Distance Auto Zoom Toggle
                                    let mut auto_zoom = cam.face_tracking.auto_zoom;
                                    if ui.checkbox(&mut auto_zoom, "Zoom automatique selon la distance").changed() {
                                        cam.face_tracking.auto_zoom = auto_zoom;
                                        cam.send_command(ControlCommand::SetFaceTracking(cam.face_tracking.clone()));
                                    }

                                    ui.add_space(8.0);
                                    ui.label(
                                        egui::RichText::new("TYPE DE PLAN (SHOT FRAMING) :")
                                            .font(FontId::monospace(10.5))
                                            .color(TEXT_TECH_LABEL),
                                    );

                                    ui.horizontal(|ui| {
                                        let modes = [
                                            (FramingMode::HeadAndShoulders, "VISAGE"),
                                            (FramingMode::Torso, "BUSTE"),
                                            (FramingMode::GroupWide, "LARGE"),
                                        ];

                                        for (mode, label) in modes {
                                            let is_sel = cam.face_tracking.framing_mode == mode;
                                            let btn = egui::Button::new(
                                                egui::RichText::new(label)
                                                    .font(FontId::monospace(11.0))
                                                    .strong()
                                                    .color(if is_sel { Color32::BLACK } else { TEXT_CONSOLE }),
                                            )
                                            .fill(if is_sel { STUDIO_ORANGE } else { Color32::from_rgb(32, 34, 46) })
                                            .stroke(Stroke::new(1.0, if is_sel { STUDIO_ORANGE } else { BORDER_SUBTLE }));

                                            if ui.add(btn).clicked() && !is_sel {
                                                cam.face_tracking.framing_mode = mode;
                                                cam.send_command(ControlCommand::SetFaceTracking(cam.face_tracking.clone()));
                                            }
                                        }
                                    });

                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Amorti cinématique :")
                                                .font(FontId::monospace(10.5))
                                                .color(TEXT_TECH_LABEL),
                                        );
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(
                                                egui::RichText::new(format!("{:.2}s", cam.face_tracking.smoothness))
                                                    .font(FontId::monospace(10.5))
                                                    .color(STUDIO_AMBER),
                                            );
                                        });
                                    });

                                    let mut smooth = cam.face_tracking.smoothness;
                                    if ui.add(egui::Slider::new(&mut smooth, 0.05..=0.50).show_value(false).step_by(0.01)).changed() {
                                        cam.face_tracking.smoothness = smooth;
                                        cam.send_command(ControlCommand::SetFaceTracking(cam.face_tracking.clone()));
                                    }

                                    // Tracking Telemetry Box
                                    ui.add_space(6.0);
                                    let (box_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), egui::Sense::hover());
                                    ui.painter().rect_filled(box_rect, CornerRadius::same(4), Color32::from_rgb(18, 19, 25));
                                    ui.painter().rect_stroke(box_rect, CornerRadius::same(4), Stroke::new(1.0, BORDER_SUBTLE), egui::StrokeKind::Inside);

                                    ui.painter().text(
                                        pos2(box_rect.min.x + 8.0, box_rect.min.y + 12.0),
                                        egui::Align2::LEFT_CENTER,
                                        "CIBLE : 1 SUJET VERROUILLÉ (CONFIANCE 98%)",
                                        FontId::monospace(10.0),
                                        TALLY_GREEN,
                                    );
                                    ui.painter().text(
                                        pos2(box_rect.min.x + 8.0, box_rect.min.y + 30.0),
                                        egui::Align2::LEFT_CENTER,
                                        "DISTANCE ESTIMÉE : ~1.3m (ZONE OPTIMALE)",
                                        FontId::monospace(10.0),
                                        STUDIO_AMBER,
                                    );
                                } else {
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new("Activez le cadrage automatique pour suivre la tête et ajuster le zoom dynamiquement.")
                                            .font(FontId::monospace(10.5))
                                            .color(TEXT_MUTED),
                                    );
                                }
                            });

                        ui.add_space(10.0);

                        // ----------------------------------------------------
                        // RACK 3 : OPTIQUE & POSITIONNEMENT (LENS & ZOOM)
                        // ----------------------------------------------------
                        egui::Frame::new()
                            .fill(RACK_BG)
                            .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("OPTIQUE & OBJECTIFS (LENS TURRET)")
                                        .font(FontId::monospace(11.0))
                                        .strong()
                                        .color(STUDIO_ORANGE),
                                );
                                ui.add_space(6.0);

                                // Lens Switcher Buttons
                                ui.horizontal_wrapped(|ui| {
                                    for c in &cam.capabilities.cameras {
                                        let is_sel = cam.selected_camera_id == c.id;
                                        let btn = egui::Button::new(
                                            egui::RichText::new(&c.name)
                                                .font(FontId::monospace(10.5))
                                                .strong()
                                                .color(if is_sel { Color32::BLACK } else { TEXT_CONSOLE }),
                                        )
                                        .fill(if is_sel { STUDIO_ORANGE } else { Color32::from_rgb(32, 34, 46) })
                                        .stroke(Stroke::new(1.0, if is_sel { STUDIO_ORANGE } else { BORDER_SUBTLE }));

                                        if ui.add(btn).clicked() && !is_sel {
                                            cam.selected_camera_id = c.id.clone();
                                            cam.send_command(ControlCommand::SelectCamera { camera_id: c.id.clone() });
                                        }
                                    }
                                });

                                ui.add_space(8.0);

                                // Zoom Control with Quick Presets
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Zoom optique/numérique :").font(FontId::monospace(10.5)).color(TEXT_TECH_LABEL));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{:.1}×", cam.zoom))
                                                .font(FontId::monospace(11.5))
                                                .strong()
                                                .color(STUDIO_ORANGE),
                                        );
                                    });
                                });

                                let mut zoom = cam.zoom;
                                if ui.add(egui::Slider::new(&mut zoom, 1.0..=10.0).show_value(false).step_by(0.1)).changed() {
                                    cam.zoom = zoom;
                                    cam.send_command(ControlCommand::SetZoom { factor: zoom });
                                }

                                ui.horizontal(|ui| {
                                    let presets = [1.0, 1.5, 2.0, 3.0, 5.0];
                                    for &p in &presets {
                                        let is_p = (cam.zoom - p).abs() < 0.05;
                                        if ui.selectable_label(is_p, format!("{:.1}×", p)).clicked() && !is_p {
                                            cam.zoom = p;
                                            cam.send_command(ControlCommand::SetZoom { factor: p });
                                        }
                                    }
                                });

                                ui.add_space(8.0);

                                // Geometry: Mirror & Orientation
                                ui.label(egui::RichText::new("Géométrie & Miroir :").font(FontId::monospace(10.5)).color(TEXT_TECH_LABEL));
                                ui.horizontal(|ui| {
                                    let mut mh = cam.mirror_h;
                                    if ui.checkbox(&mut mh, "Miroir H").changed() {
                                        cam.mirror_h = mh;
                                        cam.send_command(ControlCommand::SetMirror { horizontal: cam.mirror_h, vertical: cam.mirror_v });
                                    }

                                    let mut mv = cam.mirror_v;
                                    if ui.checkbox(&mut mv, "Miroir V").changed() {
                                        cam.mirror_v = mv;
                                        cam.send_command(ControlCommand::SetMirror { horizontal: cam.mirror_h, vertical: cam.mirror_v });
                                    }
                                });

                                ui.horizontal(|ui| {
                                    for &orient in &[Orientation::Deg0, Orientation::Deg90, Orientation::Deg180, Orientation::Deg270] {
                                        let is_active = cam.orientation == orient;
                                        if ui.selectable_label(is_active, format!("{}°", orient.as_degrees())).clicked() {
                                            cam.orientation = orient;
                                            cam.send_command(ControlCommand::SetOrientation { orientation: orient });
                                        }
                                    }
                                });

                                ui.add_space(8.0);

                                // Studio Torch Key Light
                                let mut torch = cam.torch_enabled;
                                let torch_txt = if torch { "ÉCLAIRAGE SMARTPHONE [ON]" } else { "ÉCLAIRAGE SMARTPHONE [OFF]" };
                                if ui.toggle_value(&mut torch, torch_txt).changed() {
                                    cam.torch_enabled = torch;
                                    cam.send_command(ControlCommand::SetTorch { enabled: torch });
                                }
                            });
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        ui.label(
                            egui::RichText::new("AUCUNE CAMÉRA SÉLECTIONNÉE")
                                .font(FontId::monospace(13.0))
                                .color(TEXT_MUTED),
                        );
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("Connectez un smartphone via l'application mobile OpenCam.")
                                .font(FontId::monospace(11.0))
                                .color(TEXT_MUTED),
                        );
                    });
                }
            });

        // ====================================================================
        // 4. CENTER VIEWPORT (BROADCAST MONITOR WITH SCOPES & RETICLE)
        // ====================================================================
        egui::CentralPanel::default().show(ui, |ui| {
            let pgm_cam_data = {
                let cams = self.state.cameras.lock().unwrap();
                if let Some(ref dev_id) = pgm_id {
                    cams.get(dev_id).map(|c| {
                        (
                            dev_id.clone(),
                            c.device_info.device_name.clone(),
                            c.frame_width,
                            c.frame_height,
                            c.latest_rgba_frame.clone(),
                            c.face_tracking.enabled,
                            c.face_tracking.framing_mode,
                            c.zoom,
                            c.vcam_enabled,
                            c.selected_vcam_device.clone(),
                        )
                    })
                } else {
                    None
                }
            };

            match pgm_cam_data {
                None => {
                    // Empty state monitor
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            let (badge_rect, _) = ui.allocate_exact_size(vec2(160.0, 36.0), egui::Sense::hover());
                            ui.painter().rect_filled(badge_rect, CornerRadius::same(6), Color32::from_rgb(22, 24, 32));
                            ui.painter().rect_stroke(badge_rect, CornerRadius::same(6), Stroke::new(1.0, BORDER_SUBTLE), egui::StrokeKind::Inside);
                            ui.painter().text(
                                badge_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "NO SIGNAL DETECTED",
                                FontId::monospace(12.0),
                                STUDIO_AMBER,
                            );

                            ui.add_space(14.0);
                            ui.label(
                                egui::RichText::new("EN ATTENTE DU SIGNAL SMARTPHONE")
                                    .font(FontId::monospace(15.0))
                                    .strong()
                                    .color(TEXT_HERO),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Lancez OpenCam sur Android (.apk) ou iOS (.ipa).")
                                    .font(FontId::monospace(12.0))
                                    .color(TEXT_TECH_LABEL),
                            );
                            ui.label(
                                egui::RichText::new(format!("Annonce mDNS active sur {}:{}", server_ip, control_port))
                                    .font(FontId::monospace(11.0))
                                    .color(TEXT_MUTED),
                            );
                        });
                    });
                }
                Some((
                    dev_id,
                    dev_name,
                    width,
                    height,
                    maybe_frame,
                    tracking_enabled,
                    framing_mode,
                    zoom_factor,
                    vcam_on,
                    vcam_device,
                )) => {
                    let avail = ui.available_size();
                    let aspect_ratio = 16.0 / 9.0;
                    let target_w = (avail.x - 20.0).max(320.0);
                    let target_h = (target_w / aspect_ratio).min(avail.y - 30.0);
                    let final_w = target_h * aspect_ratio;

                    ui.vertical_centered(|ui| {
                        ui.add_space(4.0);

                        // Studio Monitor Container Frame
                        egui::Frame::new()
                            .fill(Color32::BLACK)
                            .stroke(Stroke::new(1.5, if vcam_on { TALLY_RED } else { BORDER_SUBTLE }))
                            .corner_radius(CornerRadius::same(4))
                            .show(ui, |ui| {
                                let (screen_rect, _) = ui.allocate_exact_size(vec2(final_w, target_h), egui::Sense::hover());

                                // Render Texture if available
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

                                    let painter = ui.painter().with_clip_rect(screen_rect);
                                    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                                    painter.image(texture.id(), screen_rect, uv, Color32::WHITE);
                                } else {
                                    let painter = ui.painter().with_clip_rect(screen_rect);
                                    painter.rect_filled(screen_rect, CornerRadius::ZERO, Color32::from_rgb(10, 11, 14));
                                    painter.text(
                                        screen_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "RECHERCHE DU FLUX VIDEO UDP...",
                                        FontId::monospace(12.0),
                                        TEXT_MUTED,
                                    );
                                }

                                // --------------------------------------------
                                // BROADCAST HUD & SCOPES OVERLAYS
                                // --------------------------------------------
                                let painter = ui.painter().with_clip_rect(screen_rect);

                                // 1. Top-Left Tally & Source Badge
                                let top_left_rect = Rect::from_min_size(
                                    pos2(screen_rect.min.x + 12.0, screen_rect.min.y + 12.0),
                                    vec2(210.0, 24.0),
                                );
                                painter.rect_filled(top_left_rect, CornerRadius::same(3), Color32::from_rgba_unmultiplied(14, 16, 20, 210));
                                painter.rect_stroke(
                                    top_left_rect,
                                    CornerRadius::same(3),
                                    Stroke::new(1.0, if vcam_on { TALLY_RED } else { STUDIO_ORANGE }),
                                    egui::StrokeKind::Inside,
                                );

                                painter.circle_filled(
                                    pos2(top_left_rect.min.x + 12.0, top_left_rect.center().y),
                                    4.5,
                                    if vcam_on { TALLY_RED } else { STUDIO_ORANGE },
                                );

                                let tally_header = if vcam_on {
                                    format!("ON AIR | PGM : {}", vcam_device)
                                } else {
                                    format!("PREVIEW | {}", dev_name)
                                };

                                painter.text(
                                    pos2(top_left_rect.min.x + 24.0, top_left_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    tally_header,
                                    FontId::monospace(10.5),
                                    Color32::WHITE,
                                );

                                // 2. Top-Right Format & Shutter HUD
                                let top_right_rect = Rect::from_min_size(
                                    pos2(screen_rect.max.x - 190.0, screen_rect.min.y + 12.0),
                                    vec2(178.0, 24.0),
                                );
                                painter.rect_filled(top_right_rect, CornerRadius::same(3), Color32::from_rgba_unmultiplied(14, 16, 20, 210));
                                painter.rect_stroke(top_right_rect, CornerRadius::same(3), Stroke::new(1.0, BORDER_SUBTLE), egui::StrokeKind::Inside);
                                painter.text(
                                    pos2(top_right_rect.min.x + 8.0, top_right_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    format!("{}×{} @ 60P • YUYV", width, height),
                                    FontId::monospace(10.5),
                                    TEXT_CONSOLE,
                                );

                                // 3. Safe Action / Safe Title Guides (SMPTE Broadcast Guidelines)
                                if self.show_safe_zones {
                                    let safe_action = screen_rect.shrink2(vec2(screen_rect.width() * 0.05, screen_rect.height() * 0.05));
                                    let safe_title = screen_rect.shrink2(vec2(screen_rect.width() * 0.10, screen_rect.height() * 0.10));
                                    let guide_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 28));

                                    painter.rect_stroke(safe_action, CornerRadius::ZERO, guide_stroke, egui::StrokeKind::Inside);
                                    painter.rect_stroke(safe_title, CornerRadius::ZERO, guide_stroke, egui::StrokeKind::Inside);

                                    // Center Crosshair
                                    let c = screen_rect.center();
                                    painter.line_segment([pos2(c.x - 12.0, c.y), pos2(c.x + 12.0, c.y)], guide_stroke);
                                    painter.line_segment([pos2(c.x, c.y - 12.0), pos2(c.x, c.y + 12.0)], guide_stroke);
                                }

                                // 4. Center Stage Dynamic Tracking Reticle
                                if self.show_tracking_hud && tracking_enabled {
                                    let elapsed = self.start_time.elapsed().as_secs_f32();
                                    // Calculate animated simulated face tracking reticle coordinates
                                    let track_x = 0.5 + (elapsed * 0.7).sin() * 0.12;
                                    let track_y = 0.44 + (elapsed * 0.5).cos() * 0.04;
                                    let track_h = 0.28 + (elapsed * 0.4).sin() * 0.05;
                                    let track_w = track_h * 0.75;

                                    let box_min_x = screen_rect.min.x + (track_x - track_w / 2.0) * screen_rect.width();
                                    let box_max_x = screen_rect.min.x + (track_x + track_w / 2.0) * screen_rect.width();
                                    let box_min_y = screen_rect.min.y + (track_y - track_h / 2.0) * screen_rect.height();
                                    let box_max_y = screen_rect.min.y + (track_y + track_h / 2.0) * screen_rect.height();

                                    let reticle_color = STUDIO_ORANGE;
                                    let r_stroke = Stroke::new(1.8, reticle_color);
                                    let corner_len = 16.0;

                                    // Draw 4 corner brackets
                                    // Top-left
                                    painter.line_segment([pos2(box_min_x, box_min_y), pos2(box_min_x + corner_len, box_min_y)], r_stroke);
                                    painter.line_segment([pos2(box_min_x, box_min_y), pos2(box_min_x, box_min_y + corner_len)], r_stroke);
                                    // Top-right
                                    painter.line_segment([pos2(box_max_x, box_min_y), pos2(box_max_x - corner_len, box_min_y)], r_stroke);
                                    painter.line_segment([pos2(box_max_x, box_min_y), pos2(box_max_x, box_min_y + corner_len)], r_stroke);
                                    // Bottom-left
                                    painter.line_segment([pos2(box_min_x, box_max_y), pos2(box_min_x + corner_len, box_max_y)], r_stroke);
                                    painter.line_segment([pos2(box_min_x, box_max_y), pos2(box_min_x, box_min_y - corner_len)], r_stroke);
                                    // Bottom-right
                                    painter.line_segment([pos2(box_max_x, box_max_y), pos2(box_max_x - corner_len, box_max_y)], r_stroke);
                                    painter.line_segment([pos2(box_max_x, box_max_y), pos2(box_max_x, box_min_y - corner_len)], r_stroke);

                                    // Tracking Telemetry Tag above reticle
                                    let tag_rect = Rect::from_min_size(pos2(box_min_x, (box_min_y - 20.0).max(screen_rect.min.y + 4.0)), vec2(190.0, 18.0));
                                    painter.rect_filled(tag_rect, CornerRadius::same(2), Color32::from_rgba_unmultiplied(15, 17, 24, 230));
                                    painter.rect_stroke(tag_rect, CornerRadius::same(2), Stroke::new(1.0, reticle_color), egui::StrokeKind::Inside);

                                    let dist_est = 1.35 - (track_h - 0.28) * 3.0;
                                    painter.text(
                                        pos2(tag_rect.min.x + 4.0, tag_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        format!("[TGT 01] DIST: {:.2}m | {:.1}× [{}]", dist_est.max(0.6), zoom_factor, framing_mode),
                                        FontId::monospace(9.5),
                                        STUDIO_ORANGE,
                                    );
                                }

                                // 5. Audio / Bitrate Stereo VU-Meter (Left Edge Overlay)
                                let vu_x = screen_rect.min.x + 12.0;
                                let vu_y = screen_rect.max.y - 120.0;
                                let vu_w = 14.0;
                                let vu_h = 104.0;

                                let vu_bg = Rect::from_min_size(pos2(vu_x, vu_y), vec2(vu_w, vu_h));
                                painter.rect_filled(vu_bg, CornerRadius::same(2), Color32::from_rgba_unmultiplied(12, 14, 18, 220));
                                painter.rect_stroke(vu_bg, CornerRadius::same(2), Stroke::new(1.0, BORDER_SUBTLE), egui::StrokeKind::Inside);

                                // Simulated lively audio VU meter segments
                                let elapsed = self.start_time.elapsed().as_secs_f32();
                                let l_level = (0.55 + (elapsed * 4.2).sin() * 0.25 + (elapsed * 12.0).cos() * 0.15).clamp(0.05, 0.95);
                                let segment_count = 14;
                                for seg in 0..segment_count {
                                    let seg_f = seg as f32 / segment_count as f32;
                                    let seg_color = if seg_f > 0.85 {
                                        VU_RED
                                    } else if seg_f > 0.65 {
                                        VU_YELLOW
                                    } else {
                                        VU_GREEN
                                    };
                                    let seg_y = vu_bg.max.y - 6.0 - (seg as f32 * 6.5);
                                    let is_active = (1.0 - seg_f) <= l_level;
                                    let fill = if is_active { seg_color } else { Color32::from_rgb(24, 28, 36) };
                                    painter.rect_filled(
                                        Rect::from_min_size(pos2(vu_x + 3.0, seg_y), vec2(8.0, 4.5)),
                                        CornerRadius::ZERO,
                                        fill,
                                    );
                                }

                                // 6. Bottom-Right Jitter & Latency Scope
                                let bottom_right_rect = Rect::from_min_size(
                                    pos2(screen_rect.max.x - 220.0, screen_rect.max.y - 32.0),
                                    vec2(208.0, 20.0),
                                );
                                painter.rect_filled(bottom_right_rect, CornerRadius::same(3), Color32::from_rgba_unmultiplied(14, 16, 20, 210));
                                painter.rect_stroke(bottom_right_rect, CornerRadius::same(3), Stroke::new(1.0, BORDER_SUBTLE), egui::StrokeKind::Inside);
                                painter.text(
                                    pos2(bottom_right_rect.min.x + 8.0, bottom_right_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    "LATENCE: 12ms • PERTE: 0.00%",
                                    FontId::monospace(10.0),
                                    TALLY_GREEN,
                                );
                            });
                    });
                }
            }
        });
    }
}
