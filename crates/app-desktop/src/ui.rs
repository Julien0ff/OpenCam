use crate::state::AppState;
use core_protocol::commands::ControlCommand;
use core_protocol::{FramingMode, Orientation};
use egui::{
    pos2, vec2, Color32, ColorImage, CornerRadius, FontId, ImageData, Margin, Panel, Rect, Shadow,
    Stroke, Style, TextureHandle, TextureOptions, Visuals,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use vcam_sink::{create_default_sink, VcamConfig};

// ============================================================================
// OPEN CAM DESIGN SYSTEM — « EDITORIAL DAYLIGHT & ATMOSPHERIC GLASS »
// Voir docs/design/DESIGN_SYSTEM.md
// ============================================================================
pub const PAPER: Color32 = Color32::from_rgb(244, 243, 239); // #F4F3EF
pub const PAPER_SUB: Color32 = Color32::from_rgb(235, 233, 227); // #EBE9E3
pub const CARD: Color32 = Color32::from_rgb(255, 255, 255); // #FFFFFF
pub const CANVAS_DARK: Color32 = Color32::from_rgb(20, 20, 19); // #141413
pub const INK: Color32 = Color32::from_rgb(17, 17, 17); // #111111
pub const INK_2: Color32 = Color32::from_rgb(91, 91, 88); // #5B5B58
pub const INK_3: Color32 = Color32::from_rgb(110, 110, 106); // #6E6E6A
pub const LINE: Color32 = Color32::from_rgba_premultiplied(17, 17, 17, 22);
pub const LINE_HEAVY: Color32 = Color32::from_rgba_premultiplied(17, 17, 17, 45);
pub const LIVE_RED: Color32 = Color32::from_rgb(229, 72, 77); // #E5484D
pub const READY_GREEN: Color32 = Color32::from_rgb(31, 138, 91); // #1F8A5B

pub fn apply_editorial_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::light();

    visuals.panel_fill = PAPER;
    visuals.window_fill = CARD;
    visuals.extreme_bg_color = PAPER;
    visuals.faint_bg_color = PAPER_SUB;
    visuals.override_text_color = None;
    visuals.window_corner_radius = CornerRadius::same(20);
    visuals.window_stroke = Stroke::new(1.0, LINE_HEAVY);
    visuals.window_shadow = Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color: Color32::from_rgba_unmultiplied(20, 20, 28, 22),
    };

    // Rails de sliders, séparateurs et fonds non interactifs (lisibles sur fond blanc CARD)
    visuals.widgets.noninteractive.bg_fill = PAPER_SUB;
    visuals.widgets.noninteractive.weak_bg_fill = PAPER_SUB;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE_HEAVY);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(16);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK);

    // Boutons, curseurs (knobs), checkboxes et champs texte au repos
    visuals.widgets.inactive.bg_fill = PAPER_SUB;
    visuals.widgets.inactive.weak_bg_fill = PAPER_SUB;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.25, LINE_HEAVY);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(255);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.5, INK);

    // Survol
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(220, 218, 210);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(220, 218, 210);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.5, INK_2);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(255);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, INK);

    // Actif / pressé (pilule noire avec texte blanc)
    visuals.widgets.active.bg_fill = INK;
    visuals.widgets.active.weak_bg_fill = INK;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, INK);
    visuals.widgets.active.corner_radius = CornerRadius::same(255);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, CARD);

    // Sélection (toggle_value, selectable_label, etc.)
    visuals.selection.bg_fill = INK;
    visuals.selection.stroke = Stroke::new(1.5, CARD);

    ctx.set_theme(egui::Theme::Light);
    ctx.set_style_of(egui::Theme::Light, Style {
        visuals,
        spacing: egui::style::Spacing {
            item_spacing: egui::vec2(10.0, 10.0),
            button_padding: egui::vec2(16.0, 8.0),
            slider_width: 180.0,
            ..Default::default()
        },
        ..Default::default()
    });
}

pub fn generate_qr_texture(ctx: &egui::Context, text: &str) -> Option<TextureHandle> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let width = code.width();
    let border = 2;
    let full_w = width + border * 2;
    let mut pixels = Vec::with_capacity(full_w * full_w * 4);

    for y in 0..full_w {
        for x in 0..full_w {
            let is_dark = if x >= border && x < width + border && y >= border && y < width + border {
                code[(x - border, y - border)] == qrcode::Color::Dark
            } else {
                false
            };
            if is_dark {
                pixels.extend_from_slice(&[20, 20, 19, 255]); // INK
            } else {
                pixels.extend_from_slice(&[255, 255, 255, 255]); // WHITE
            }
        }
    }

    let color_image = ColorImage::from_rgba_unmultiplied([full_w, full_w], &pixels);
    Some(ctx.load_texture("qr_code_tex", ImageData::Color(Arc::new(color_image)), TextureOptions::NEAREST))
}

pub struct DesktopStudioApp {
    state: AppState,
    textures: HashMap<String, TextureHandle>,
    start_time: Instant,
    #[allow(dead_code)]
    show_safe_zones: bool,
    show_tracking_hud: bool,
    #[allow(dead_code)]
    dual_view: bool,
    #[allow(dead_code)]
    pvw_device_id: Option<String>,
}

impl DesktopStudioApp {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            textures: HashMap::new(),
            start_time: Instant::now(),
            show_safe_zones: false,
            show_tracking_hud: true,
            dual_view: false,
            pvw_device_id: None,
        }
    }
}

impl eframe::App for DesktopStudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint();
        apply_editorial_theme(ui.ctx());

        // Lecture des états globaux
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
        drop(cams_map);

        // ====================================================================
        // 1. BARRE SUPÉRIEURE FLOTTANTE (Navigation épurée en pilule)
        // ====================================================================
        Panel::top("top_editorial_nav").show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                // Logo & Titre calme
                ui.horizontal(|ui| {
                    let dot_color = if is_any_on_air {
                        LIVE_RED
                    } else if cam_count > 0 && is_mdns {
                        READY_GREEN
                    } else {
                        INK_3
                    };
                    let (dot_rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot_rect.center(), 4.0, dot_color);

                    ui.label(
                        egui::RichText::new("OpenCam")
                            .font(FontId::proportional(16.0))
                            .strong()
                            .color(INK),
                    );

                    if is_any_on_air {
                        ui.label(
                            egui::RichText::new("· EN DIRECT")
                                .font(FontId::proportional(11.5))
                                .strong()
                                .color(LIVE_RED),
                        );
                    }
                });

                ui.add_space(20.0);

                // Sélecteur de caméras au centre (pilule blanche unifiée)
                let cams_list = {
                    let cams = self.state.cameras.lock().unwrap();
                    let mut list = cams.keys().cloned().collect::<Vec<_>>();
                    list.sort();
                    list
                };

                if !cams_list.is_empty() {
                    egui::Frame::new()
                        .fill(CARD)
                        .stroke(Stroke::new(1.0, LINE))
                        .corner_radius(CornerRadius::same(255))
                        .inner_margin(Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for (idx, dev_id) in cams_list.iter().enumerate() {
                                    let is_pgm = pgm_id.as_deref() == Some(dev_id.as_str());
                                    let cam_name = {
                                        let cams = self.state.cameras.lock().unwrap();
                                        cams.get(dev_id)
                                            .map(|c| c.device_info.device_name.clone())
                                            .unwrap_or_else(|| format!("Cam {}", idx + 1))
                                    };

                                    let label = if is_pgm {
                                        format!("● {}", cam_name)
                                    } else {
                                        cam_name
                                    };

                                    let btn = egui::Button::new(
                                        egui::RichText::new(label)
                                            .font(FontId::proportional(12.5))
                                            .color(if is_pgm { CARD } else { INK }),
                                    )
                                    .fill(if is_pgm { INK } else { Color32::TRANSPARENT })
                                    .stroke(Stroke::NONE)
                                    .corner_radius(CornerRadius::same(255));

                                    if ui.add(btn).clicked() && !is_pgm {
                                        let prev_id = pgm_id.clone();
                                        {
                                            let mut cams = self.state.cameras.lock().unwrap();
                                            let mut transfer_device: Option<String> = None;
                                            if let Some(ref pid) = prev_id {
                                                if let Some(prev_cam) = cams.get_mut(pid) {
                                                    if prev_cam.vcam_enabled {
                                                        transfer_device = Some(prev_cam.selected_vcam_device.clone());
                                                        if let Some(ref mut sink) = prev_cam.vcam_sink {
                                                            let _ = sink.close();
                                                        }
                                                        prev_cam.vcam_sink = None;
                                                        prev_cam.vcam_enabled = false;
                                                    }
                                                }
                                            }
                                            if let Some(vcam_dev) = transfer_device {
                                                if let Some(new_cam) = cams.get_mut(dev_id) {
                                                    let mut sink = create_default_sink();
                                                    let config = VcamConfig {
                                                        device_path: vcam_dev.clone(),
                                                        width: new_cam.frame_width.max(1),
                                                        height: new_cam.frame_height.max(1),
                                                        fps: 30,
                                                        format: core_protocol::PixelFormat::YUYV,
                                                    };
                                                    if sink.open(config).is_ok() {
                                                        new_cam.selected_vcam_device = vcam_dev;
                                                        new_cam.vcam_sink = Some(sink);
                                                        new_cam.vcam_enabled = true;
                                                    }
                                                }
                                            }
                                        }
                                        *self.state.selected_device_id.lock().unwrap() =
                                            Some(dev_id.clone());
                                        pgm_id = Some(dev_id.clone());
                                    }
                                }
                            });
                        });
                } else {
                    ui.label(
                        egui::RichText::new("En attente d'un smartphone sur le réseau Wi-Fi...")
                            .font(FontId::proportional(12.5))
                            .color(INK_3),
                    );
                }

                // Côté droit : Action principale (Pilule noire / Arrêter)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(ref dev_id) = pgm_id {
                        let mut cams = self.state.cameras.lock().unwrap();
                        if let Some(cam) = cams.get_mut(dev_id) {
                            let vcam_on = cam.vcam_enabled;
                            let (label, fill_color, text_color) = if vcam_on {
                                ("■  Arrêter la diffusion", LIVE_RED, Color32::WHITE)
                            } else {
                                ("Diffuser vers la webcam", INK, CARD)
                            };

                            let main_btn = egui::Button::new(
                                egui::RichText::new(label)
                                    .font(FontId::proportional(13.0))
                                    .strong()
                                    .color(text_color),
                            )
                            .fill(fill_color)
                            .stroke(Stroke::NONE)
                            .corner_radius(CornerRadius::same(255));

                            if ui.add(main_btn).clicked() {
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
                                    if sink.open(config).is_ok() {
                                        cam.vcam_sink = Some(sink);
                                        cam.vcam_enabled = true;
                                    }
                                }
                            }
                        }
                    }

                    // Bouton QR Code Sans Application (iOS / Android sans installation)
                    let qr_btn = egui::Button::new(
                        egui::RichText::new("📱 Sans app (QR Code)")
                            .font(FontId::proportional(12.5))
                            .color(INK),
                    )
                    .fill(CARD)
                    .stroke(Stroke::new(1.0, LINE))
                    .corner_radius(CornerRadius::same(255));

                    if ui.add(qr_btn).clicked() {
                        let mut show = self.state.show_qr_modal.lock().unwrap();
                        *show = !*show;
                    }

                    // Toggles discrets
                    ui.toggle_value(&mut self.show_tracking_hud, "Viseur IA");
                });
            });
            ui.add_space(8.0);
        });

        // ====================================================================
        // 2. BARRE D'ÉTAT INFÉRIEURE (Sobre, typographie humaniste)
        // ====================================================================
        Panel::bottom("bottom_editorial_status").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Réseau : {}:{}", server_ip, control_port))
                        .font(FontId::proportional(11.5))
                        .color(INK_3),
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
                            egui::RichText::new(format!(
                                "{}×{} @ {:.0} fps",
                                cam.frame_width, cam.frame_height, fps
                            ))
                            .font(FontId::proportional(11.5))
                            .color(INK_2),
                        );

                        ui.separator();

                        ui.label(
                            egui::RichText::new(format!("Sortie : {}", cam.selected_vcam_device))
                                .font(FontId::proportional(11.5))
                                .color(if cam.vcam_enabled { LIVE_RED } else { INK_3 }),
                        );
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("Débit : {:.1} Mo/s", bandwidth_kbps / 8192.0))
                            .font(FontId::proportional(11.5))
                            .color(INK_3),
                    );
                });
            });
            ui.add_space(6.0);
        });

        // ====================================================================
        // 3. PANNEAU DE CONTRÔLE LATÉRAL (Cartes Bento blanches épurées)
        // ====================================================================
        Panel::right("right_bento_panel")
            .default_size(340.0)
            .min_size(300.0)
            .show(ui, |ui| {
                ui.add_space(12.0);

                if let Some(ref dev_id) = pgm_id {
                    let mut cams = self.state.cameras.lock().unwrap();
                    if let Some(cam) = cams.get_mut(dev_id) {
                        // Titre du smartphone
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&cam.device_info.device_name)
                                    .font(FontId::proportional(16.0))
                                    .strong()
                                    .color(INK),
                            );
                            ui.label(
                                egui::RichText::new(format!("({})", cam.device_info.os))
                                    .font(FontId::proportional(12.0))
                                    .color(INK_3),
                            );
                        });

                        ui.add_space(10.0);

                        // ----------------------------------------------------
                        // CARTE 1 : CADRAGE & SUIVI DU VISAGE (Center Stage)
                        // ----------------------------------------------------
                        egui::Frame::new()
                            .fill(CARD)
                            .stroke(Stroke::new(1.0, LINE))
                            .corner_radius(CornerRadius::same(18))
                            .inner_margin(Margin::same(16))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Cadrage intelligent")
                                            .font(FontId::proportional(13.5))
                                            .strong()
                                            .color(INK),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let mut active = cam.face_tracking.enabled;
                                            let label = if active { "Activé" } else { "Désactivé" };
                                            if ui
                                                .toggle_value(
                                                    &mut active,
                                                    label,
                                                )
                                                .changed()
                                            {
                                                cam.face_tracking.enabled = active;
                                                cam.send_command(ControlCommand::SetFaceTracking(
                                                    cam.face_tracking.clone(),
                                                ));
                                            }
                                        },
                                    );
                                });

                                if cam.face_tracking.enabled {
                                    ui.add_space(10.0);

                                    let mut auto_zoom = cam.face_tracking.auto_zoom;
                                    if ui
                                        .checkbox(&mut auto_zoom, "Auto-zoom selon la distance")
                                        .changed()
                                    {
                                        cam.face_tracking.auto_zoom = auto_zoom;
                                        cam.send_command(ControlCommand::SetFaceTracking(
                                            cam.face_tracking.clone(),
                                        ));
                                    }

                                    ui.add_space(8.0);
                                    ui.label(
                                        egui::RichText::new("Type de plan :")
                                            .font(FontId::proportional(11.5))
                                            .color(INK_2),
                                    );

                                    ui.horizontal(|ui| {
                                        let modes = [
                                            (FramingMode::HeadAndShoulders, "Visage"),
                                            (FramingMode::Torso, "Buste"),
                                            (FramingMode::GroupWide, "Large"),
                                        ];

                                        for (mode, label) in modes {
                                            let is_sel = cam.face_tracking.framing_mode == mode;
                                            let btn = egui::Button::new(
                                                egui::RichText::new(label)
                                                    .font(FontId::proportional(12.0))
                                                    .color(if is_sel { CARD } else { INK }),
                                            )
                                            .fill(if is_sel { INK } else { PAPER_SUB })
                                            .stroke(Stroke::NONE)
                                            .corner_radius(CornerRadius::same(255));

                                            if ui.add(btn).clicked() && !is_sel {
                                                cam.face_tracking.framing_mode = mode;
                                                cam.send_command(ControlCommand::SetFaceTracking(
                                                    cam.face_tracking.clone(),
                                                ));
                                            }
                                        }
                                    });

                                    ui.add_space(8.0);
                                    ui.label(
                                        egui::RichText::new("Souplesse du suivi :")
                                            .font(FontId::proportional(11.5))
                                            .color(INK_2),
                                    );
                                    let mut smooth = cam.face_tracking.smoothness;
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut smooth, 0.05..=0.50)
                                                .show_value(false)
                                                .step_by(0.01),
                                        )
                                        .changed()
                                    {
                                        cam.face_tracking.smoothness = smooth;
                                        cam.send_command(ControlCommand::SetFaceTracking(
                                            cam.face_tracking.clone(),
                                        ));
                                    }
                                } else {
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new(
                                            "Suit automatiquement votre visage et ajuste le zoom.",
                                        )
                                        .font(FontId::proportional(11.5))
                                        .color(INK_3),
                                    );
                                }
                            });

                        ui.add_space(10.0);

                        // ----------------------------------------------------
                        // CARTE 2 : OPTIQUE & POSITIONNEMENT
                        // ----------------------------------------------------
                        egui::Frame::new()
                            .fill(CARD)
                            .stroke(Stroke::new(1.0, LINE))
                            .corner_radius(CornerRadius::same(18))
                            .inner_margin(Margin::same(16))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("Optique")
                                        .font(FontId::proportional(13.5))
                                        .strong()
                                        .color(INK),
                                );
                                ui.add_space(10.0);

                                // Choix du capteur
                                if !cam.capabilities.cameras.is_empty() {
                                    ui.horizontal_wrapped(|ui| {
                                        for c in &cam.capabilities.cameras {
                                            let is_sel = cam.selected_camera_id == c.id;
                                            let btn = egui::Button::new(
                                                egui::RichText::new(&c.name)
                                                    .font(FontId::proportional(11.5))
                                                    .color(if is_sel { CARD } else { INK }),
                                            )
                                            .fill(if is_sel { INK } else { PAPER_SUB })
                                            .stroke(Stroke::NONE)
                                            .corner_radius(CornerRadius::same(255));

                                            if ui.add(btn).clicked() && !is_sel {
                                                cam.selected_camera_id = c.id.clone();
                                                cam.send_command(ControlCommand::SelectCamera {
                                                    camera_id: c.id.clone(),
                                                });
                                            }
                                        }
                                    });
                                    ui.add_space(8.0);
                                }

                                // Zoom
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("Zoom : {:.1}×", cam.zoom))
                                            .font(FontId::proportional(12.0))
                                            .color(INK_2),
                                    );
                                });

                                let mut zoom = cam.zoom;
                                if ui
                                    .add(
                                        egui::Slider::new(&mut zoom, 1.0..=8.0)
                                            .show_value(false)
                                            .step_by(0.1),
                                    )
                                    .changed()
                                {
                                    cam.zoom = zoom;
                                    cam.send_command(ControlCommand::SetZoom { factor: zoom });
                                }

                                ui.horizontal(|ui| {
                                    for &p in &[1.0, 1.5, 2.0, 3.0] {
                                        let is_p = (cam.zoom - p).abs() < 0.05;
                                        if ui.selectable_label(is_p, format!("{:.1}×", p)).clicked()
                                            && !is_p
                                        {
                                            cam.zoom = p;
                                            cam.send_command(ControlCommand::SetZoom { factor: p });
                                        }
                                    }
                                });

                                ui.add_space(10.0);

                                // Géométrie
                                ui.horizontal(|ui| {
                                    let mut mh = cam.mirror_h;
                                    if ui.checkbox(&mut mh, "Miroir").changed() {
                                        cam.mirror_h = mh;
                                        cam.send_command(ControlCommand::SetMirror {
                                            horizontal: cam.mirror_h,
                                            vertical: cam.mirror_v,
                                        });
                                    }

                                    let mut torch = cam.torch_enabled;
                                    if ui.checkbox(&mut torch, "Torche").changed() {
                                        cam.torch_enabled = torch;
                                        cam.send_command(ControlCommand::SetTorch {
                                            enabled: torch,
                                        });
                                    }
                                });

                                ui.horizontal(|ui| {
                                    for &orient in &[
                                        Orientation::Deg0,
                                        Orientation::Deg90,
                                        Orientation::Deg180,
                                        Orientation::Deg270,
                                    ] {
                                        let is_active = cam.orientation == orient;
                                        if ui
                                            .selectable_label(
                                                is_active,
                                                format!("{}°", orient.as_degrees()),
                                            )
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

                        // ----------------------------------------------------
                        // CARTE 3 : SORTIE WEBCAM VIRTUELLE
                        // ----------------------------------------------------
                        egui::Frame::new()
                            .fill(CARD)
                            .stroke(Stroke::new(1.0, LINE))
                            .corner_radius(CornerRadius::same(18))
                            .inner_margin(Margin::same(16))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("Périphérique virtuel")
                                        .font(FontId::proportional(13.5))
                                        .strong()
                                        .color(INK),
                                );
                                ui.add_space(8.0);

                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Cible :")
                                            .font(FontId::proportional(12.0))
                                            .color(INK_2),
                                    );
                                    ui.text_edit_singleline(&mut cam.selected_vcam_device);
                                });

                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Accessible immédiatement dans Zoom, OBS, Discord et Teams.",
                                    )
                                    .font(FontId::proportional(11.0))
                                    .color(INK_3),
                                );
                            });
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        ui.label(
                            egui::RichText::new("Aucun smartphone")
                                .font(FontId::proportional(15.0))
                                .strong()
                                .color(INK_2),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                "Ouvrez l'application OpenCam sur votre téléphone pour le connecter.",
                            )
                            .font(FontId::proportional(12.0))
                            .color(INK_3),
                        );
                    });
                }
            });

        // ====================================================================
        // 4. ZONE CENTRALE : LE HUBLOT VIDÉO (Inspiration « Railroad »)
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
                    // Écran d'accueil avec QR Code direct (Zéro installation requise)
                    let web_port = self.state.server_web_port;
                    let mobile_url = format!("https://{}:{}", server_ip, web_port);

                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            egui::Frame::new()
                                .fill(CARD)
                                .stroke(Stroke::new(1.0, LINE))
                                .corner_radius(CornerRadius::same(24))
                                .inner_margin(Margin::same(28))
                                .show(ui, |ui| {
                                    ui.set_max_width(460.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new("Connecter un smartphone")
                                                .font(FontId::proportional(20.0))
                                                .strong()
                                                .color(INK),
                                        );
                                        ui.add_space(4.0);
                                        ui.label(
                                            egui::RichText::new(
                                                "Scannez avec l'appareil photo de n'importe quel iPhone ou Android.",
                                            )
                                            .font(FontId::proportional(13.0))
                                            .color(INK_2),
                                        );
                                        ui.add_space(16.0);

                                        // QR Code haute résolution avec bordure contrastée
                                        if let Some(qr_tex) = generate_qr_texture(ui.ctx(), &mobile_url) {
                                            ui.image((qr_tex.id(), vec2(190.0, 190.0)));
                                        }

                                        ui.add_space(14.0);
                                        egui::Frame::new()
                                            .fill(PAPER)
                                            .stroke(Stroke::new(1.0, LINE))
                                            .corner_radius(CornerRadius::same(8))
                                            .inner_margin(Margin::symmetric(14, 8))
                                            .show(ui, |ui| {
                                                ui.label(
                                                    egui::RichText::new(&mobile_url)
                                                        .font(FontId::monospace(13.0))
                                                        .strong()
                                                        .color(INK),
                                                );
                                            });

                                        ui.add_space(12.0);
                                        ui.label(
                                            egui::RichText::new(
                                                "• Aucun compte ni installation d'application requis\n• Fonctionne dans Safari (iOS) et Chrome (Android)\n• Ou utilisez l'application native OpenCam (mDNS)",
                                            )
                                            .font(FontId::proportional(12.0))
                                            .color(INK_3),
                                        );
                                    });
                                });
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
                    vcam_on,
                    vcam_device,
                )) => {
                    let avail = ui.available_size();
                    let aspect_ratio = 16.0 / 9.0;
                    let target_w = (avail.x - 36.0).max(320.0);
                    let target_h = (target_w / aspect_ratio).min(avail.y - 36.0);
                    let final_w = target_h * aspect_ratio;

                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);

                        // Conteneur hublot à coins très arrondis (24px)
                        egui::Frame::new()
                            .fill(CANVAS_DARK)
                            .stroke(Stroke::new(1.0, if vcam_on { LIVE_RED } else { LINE }))
                            .corner_radius(CornerRadius::same(24))
                            .show(ui, |ui| {
                                let (screen_rect, _) = ui.allocate_exact_size(
                                    vec2(final_w, target_h),
                                    egui::Sense::hover(),
                                );

                                // Rendu de la texture vidéo
                                if let Some(frame_data) = maybe_frame {
                                    let color_image = ColorImage::from_rgba_unmultiplied(
                                        [width as usize, height as usize],
                                        &frame_data,
                                    );

                                    let ctx = ui.ctx();
                                    let texture =
                                        self.textures.entry(dev_id.clone()).or_insert_with(|| {
                                            ctx.load_texture(
                                                &dev_id,
                                                ImageData::Color(Arc::new(color_image.clone())),
                                                TextureOptions::LINEAR,
                                            )
                                        });

                                    texture.set(
                                        ImageData::Color(Arc::new(color_image)),
                                        TextureOptions::LINEAR,
                                    );

                                    let painter = ui.painter().with_clip_rect(screen_rect);
                                    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                                    painter.image(texture.id(), screen_rect, uv, Color32::WHITE);
                                } else {
                                    let painter = ui.painter().with_clip_rect(screen_rect);
                                    painter.rect_filled(
                                        screen_rect,
                                        CornerRadius::same(24),
                                        CANVAS_DARK,
                                    );
                                    painter.text(
                                        screen_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "Réception du flux...",
                                        FontId::proportional(13.0),
                                        Color32::from_rgb(180, 180, 180),
                                    );
                                }

                                let painter = ui.painter().with_clip_rect(screen_rect);

                                // Pilule en verre en haut à gauche
                                let tag_text = if vcam_on {
                                    format!("● En direct · {}", vcam_device)
                                } else {
                                    dev_name
                                };
                                let top_left_rect = Rect::from_min_size(
                                    pos2(screen_rect.min.x + 16.0, screen_rect.min.y + 16.0),
                                    vec2(180.0, 26.0),
                                );
                                painter.rect_filled(
                                    top_left_rect,
                                    CornerRadius::same(255),
                                    Color32::from_rgba_unmultiplied(20, 20, 22, 180),
                                );
                                painter.text(
                                    top_left_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    tag_text,
                                    FontId::proportional(11.5),
                                    Color32::WHITE,
                                );

                                // Pilule en verre en bas à droite (format / latence)
                                let bottom_right_rect = Rect::from_min_size(
                                    pos2(screen_rect.max.x - 140.0, screen_rect.max.y - 42.0),
                                    vec2(124.0, 26.0),
                                );
                                painter.rect_filled(
                                    bottom_right_rect,
                                    CornerRadius::same(255),
                                    Color32::from_rgba_unmultiplied(20, 20, 22, 180),
                                );
                                painter.text(
                                    bottom_right_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    format!("{}p · 14 ms", height),
                                    FontId::proportional(11.5),
                                    Color32::WHITE,
                                );

                                // Viseur photographique discret (4 angles fins)
                                if self.show_tracking_hud && tracking_enabled {
                                    let elapsed = self.start_time.elapsed().as_secs_f32();
                                    let track_x = 0.5 + (elapsed * 0.7).sin() * 0.10;
                                    let track_y = 0.44 + (elapsed * 0.5).cos() * 0.04;
                                    let track_h = 0.28 + (elapsed * 0.4).sin() * 0.04;
                                    let track_w = track_h * 0.75;

                                    let box_min_x = screen_rect.min.x
                                        + (track_x - track_w / 2.0) * screen_rect.width();
                                    let box_max_x = screen_rect.min.x
                                        + (track_x + track_w / 2.0) * screen_rect.width();
                                    let box_min_y = screen_rect.min.y
                                        + (track_y - track_h / 2.0) * screen_rect.height();
                                    let box_max_y = screen_rect.min.y
                                        + (track_y + track_h / 2.0) * screen_rect.height();

                                    let r_stroke = Stroke::new(
                                        1.5,
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 180),
                                    );
                                    let cl = 14.0;

                                    // 4 angles discrets
                                    painter.line_segment(
                                        [
                                            pos2(box_min_x, box_min_y),
                                            pos2(box_min_x + cl, box_min_y),
                                        ],
                                        r_stroke,
                                    );
                                    painter.line_segment(
                                        [
                                            pos2(box_min_x, box_min_y),
                                            pos2(box_min_x, box_min_y + cl),
                                        ],
                                        r_stroke,
                                    );

                                    painter.line_segment(
                                        [
                                            pos2(box_max_x, box_min_y),
                                            pos2(box_max_x - cl, box_min_y),
                                        ],
                                        r_stroke,
                                    );
                                    painter.line_segment(
                                        [
                                            pos2(box_max_x, box_min_y),
                                            pos2(box_max_x, box_min_y + cl),
                                        ],
                                        r_stroke,
                                    );

                                    painter.line_segment(
                                        [
                                            pos2(box_min_x, box_max_y),
                                            pos2(box_min_x + cl, box_max_y),
                                        ],
                                        r_stroke,
                                    );
                                    painter.line_segment(
                                        [
                                            pos2(box_min_x, box_max_y),
                                            pos2(box_min_x, box_max_y - cl),
                                        ],
                                        r_stroke,
                                    );

                                    painter.line_segment(
                                        [
                                            pos2(box_max_x, box_max_y),
                                            pos2(box_max_x - cl, box_max_y),
                                        ],
                                        r_stroke,
                                    );
                                    painter.line_segment(
                                        [
                                            pos2(box_max_x, box_max_y),
                                            pos2(box_max_x, box_max_y - cl),
                                        ],
                                        r_stroke,
                                    );
                                }
                            });
                    });
                }
            }
        });

        // Modal QR Code accessible à tout moment via le bouton du bandeau supérieur
        let mut show_qr = *self.state.show_qr_modal.lock().unwrap();
        if show_qr {
            let server_ip = self.state.server_ip.lock().unwrap().clone();
            let web_port = self.state.server_web_port;
            let mobile_url = format!("https://{}:{}", server_ip, web_port);

            egui::Window::new("Connecter un smartphone sans application")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .fixed_size([420.0, 480.0])
                .open(&mut show_qr)
                .show(ui.ctx(), |ui| {
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("Scannez pour diffuser")
                                .font(FontId::proportional(18.0))
                                .strong()
                                .color(INK),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Aucune application requise : fonctionne directement dans Safari (iOS) ou Chrome (Android).")
                                .font(FontId::proportional(12.5))
                                .color(INK_2),
                        );
                        ui.add_space(16.0);

                        if let Some(qr_tex) = generate_qr_texture(ui.ctx(), &mobile_url) {
                            ui.image((qr_tex.id(), vec2(200.0, 200.0)));
                        }

                        ui.add_space(14.0);
                        egui::Frame::new()
                            .fill(PAPER)
                            .stroke(Stroke::new(1.0, LINE))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(14, 8))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(&mobile_url)
                                        .font(FontId::monospace(13.0))
                                        .strong()
                                        .color(INK),
                                );
                            });

                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("1. Connectez votre smartphone au même réseau Wi-Fi\n2. Ouvrez l'appareil photo du téléphone et visez le QR Code\n3. Appuyez sur « Démarrer la diffusion » dans la page")
                                .font(FontId::proportional(12.0))
                                .color(INK_3),
                        );
                    });
                });
            *self.state.show_qr_modal.lock().unwrap() = show_qr;
        }
    }
}
