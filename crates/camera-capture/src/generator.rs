//! Dynamic real-time frame generator for camera simulation and test patterns.
//! Generates real, live 30/60 FPS frames with timestamps, human subject simulation,
//! face tracking with distance-based auto-framing, and applied video adjustments.

use crate::face_tracking::HeadTracker;
use core_protocol::{FaceMetrics, FaceTrackingConfig, Orientation};
use std::time::Instant;

pub struct DynamicFrameGenerator {
    start_time: Instant,
    last_frame_instant: Instant,
    frame_counter: u64,
    pub tracker: HeadTracker,
}

impl Default for DynamicFrameGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicFrameGenerator {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            last_frame_instant: Instant::now(),
            frame_counter: 0,
            tracker: HeadTracker::default(),
        }
    }

    /// Sets face tracking and auto-framing configuration.
    pub fn set_face_tracking(&mut self, config: FaceTrackingConfig) {
        self.tracker.config = config;
    }

    /// Queries latest face tracking metrics.
    pub fn face_metrics(&self) -> FaceMetrics {
        self.tracker.latest_metrics
    }

    /// Generates a live RGBA frame with animated elements and applied video adjustments.
    pub fn generate_rgba_frame(
        &mut self,
        width: u32,
        height: u32,
        manual_zoom: f32,
        mirror_h: bool,
        mirror_v: bool,
        orientation: Orientation,
        torch_enabled: bool,
        camera_name: &str,
    ) -> Vec<u8> {
        self.frame_counter += 1;
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_instant).as_secs_f32().clamp(0.001, 0.1);
        self.last_frame_instant = now;

        let elapsed = self.start_time.elapsed().as_secs_f32();

        // Simulate natural human movement (subject subtly shifting left/right and moving closer/farther)
        let simulated_head_x = 0.5 + (elapsed * 0.7).sin() * 0.14;
        let simulated_head_y = 0.42 + (elapsed * 0.5).cos() * 0.05;
        // Distance oscillation: face height between 0.18 (far away) and 0.38 (close up)
        let simulated_face_h = 0.26 + (elapsed * 0.4).sin() * 0.08;
        let simulated_face_w = simulated_face_h * 0.75;

        // Update head tracker
        let (crop_cx, crop_cy, auto_zoom) = self.tracker.update(
            Some((simulated_head_x, simulated_head_y, simulated_face_w, simulated_face_h)),
            dt,
        );

        let active_zoom = if self.tracker.config.enabled {
            auto_zoom
        } else {
            manual_zoom
        };

        let w = width as usize;
        let h = height as usize;
        let mut buffer = vec![0u8; w * h * 4];

        // Torch multiplier
        let torch_boost = if torch_enabled { 60.0 } else { 0.0 };

        // Draw refined studio background with subtle vignette
        let cx = (w / 2) as f32;
        let cy = (h / 2) as f32;

        for y in 0..h {
            let dy = y as f32 - cy;
            let norm_y = y as f32 / h as f32;

            for x in 0..w {
                let dx = x as f32 - cx;
                let norm_x = x as f32 / w as f32;
                let idx = (y * w + x) * 4;

                // Subtle studio lighting falloff
                let dist_sq = (norm_x - 0.5).powi(2) + (norm_y - 0.5).powi(2);
                let vignette = (1.0 - dist_sq * 1.2).clamp(0.2, 1.0);

                let torch_dist = (dx.powi(2) + dy.powi(2)).sqrt();
                let torch_light = if torch_enabled {
                    (torch_boost * (1.0 - (torch_dist / (w as f32 * 0.65)).min(1.0))).max(0.0)
                } else {
                    0.0
                };

                let base_lum = 22.0 * vignette + torch_light;
                let r = (base_lum + 3.0) as u8;
                let g = (base_lum + 4.0) as u8;
                let b = (base_lum + 8.0) as u8;

                buffer[idx] = r;
                buffer[idx + 1] = g;
                buffer[idx + 2] = b;
                buffer[idx + 3] = 255;
            }
        }

        // Draw simulated subject (head, shoulders, eyes, mouth)
        let subject_px_x = (simulated_head_x * w as f32) as i32;
        let subject_px_y = (simulated_head_y * h as f32) as i32;
        let head_radius = (simulated_face_h * h as f32 * 0.45) as i32;

        draw_simulated_portrait(&mut buffer, w, h, subject_px_x, subject_px_y, head_radius);

        // Draw refined Center Stage tracking brackets if active
        if self.tracker.config.enabled {
            draw_tracking_brackets(
                &mut buffer,
                w,
                h,
                (simulated_head_x * w as f32) as i32,
                (simulated_head_y * h as f32) as i32,
                (simulated_face_w * w as f32) as i32,
                (simulated_face_h * h as f32) as i32,
            );
        }

        // Top HUD bar
        draw_hud_bar(&mut buffer, w, h, camera_name, self.frame_counter, elapsed, active_zoom, torch_enabled);

        // 2. Apply digital zoom with dynamic center point (Center Stage cropping)
        let zoomed_buffer = if active_zoom > 1.01 {
            apply_dynamic_zoom(&buffer, w, h, active_zoom, crop_cx, crop_cy)
        } else {
            buffer
        };

        // 3. Apply mirror & orientation
        apply_transforms(zoomed_buffer, w, h, mirror_h, mirror_v, orientation)
    }
}

fn draw_simulated_portrait(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    head_x: i32,
    head_y: i32,
    radius: i32,
) {
    let r_sq = radius * radius;
    let r_neck = radius * 3 / 4;

    for dy in -radius..=(radius * 2) {
        let py = head_y + dy;
        if py < 0 || py >= height as i32 {
            continue;
        }

        let max_dx = if dy <= radius {
            let rem = (r_sq - dy * dy).max(0);
            (rem as f32).sqrt() as i32
        } else {
            // Torso / Shoulders widening
            r_neck + ((dy - radius) * 2)
        };

        for dx in -max_dx..=max_dx {
            let px = head_x + dx;
            if px < 0 || px >= width as i32 {
                continue;
            }

            let idx = (py as usize * width + px as usize) * 4;

            if dy > radius {
                // Shoulders (dark charcoal navy fabric)
                buffer[idx] = 34;
                buffer[idx + 1] = 40;
                buffer[idx + 2] = 52;
            } else {
                // Skin tone (warm soft tone)
                let dist_from_center = ((dx * dx + dy * dy) as f32).sqrt() / radius as f32;
                let shade = 1.0 - dist_from_center * 0.25;

                let mut r = (215.0 * shade) as u8;
                let mut g = (175.0 * shade) as u8;
                let mut b = (150.0 * shade) as u8;

                // Eyes
                let eye_y = -radius / 6;
                let eye_dx = radius / 3;
                if (dy - eye_y).abs() <= 3 && ((dx - eye_dx).abs() <= 4 || (dx + eye_dx).abs() <= 4) {
                    r = 30;
                    g = 30;
                    b = 30;
                }

                buffer[idx] = r;
                buffer[idx + 1] = g;
                buffer[idx + 2] = b;
            }
        }
    }
}

fn draw_tracking_brackets(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    cx: i32,
    cy: i32,
    box_w: i32,
    box_h: i32,
) {
    let x0 = (cx - box_w / 2 - 8).clamp(0, width as i32 - 1);
    let x1 = (cx + box_w / 2 + 8).clamp(0, width as i32 - 1);
    let y0 = (cy - box_h / 2 - 8).clamp(0, height as i32 - 1);
    let y1 = (cy + box_h / 2 + 8).clamp(0, height as i32 - 1);
    let corner_len = 16.min(box_w / 4);

    let draw_pixel = |buf: &mut [u8], x: i32, y: i32| {
        if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
            let idx = (y as usize * width + x as usize) * 4;
            // Refined Apple-style yellow/gold focus color
            buf[idx] = 250;
            buf[idx + 1] = 204;
            buf[idx + 2] = 21;
        }
    };

    // 4 Corner brackets
    for d in 0..corner_len {
        // Top-left
        draw_pixel(buffer, x0 + d, y0);
        draw_pixel(buffer, x0, y0 + d);
        // Top-right
        draw_pixel(buffer, x1 - d, y0);
        draw_pixel(buffer, x1, y0 + d);
        // Bottom-left
        draw_pixel(buffer, x0 + d, y1);
        draw_pixel(buffer, x0, y1 - d);
        // Bottom-right
        draw_pixel(buffer, x1 - d, y1);
        draw_pixel(buffer, x1, y1 - d);
    }
}

fn draw_hud_bar(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    _camera_name: &str,
    _frame: u64,
    elapsed: f32,
    _zoom: f32,
    _torch: bool,
) {
    let hud_height = 32.min(height);
    for y in 0..hud_height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            buffer[idx] = (buffer[idx] as u32 * 30 / 100) as u8;
            buffer[idx + 1] = (buffer[idx + 1] as u32 * 30 / 100) as u8;
            buffer[idx + 2] = (buffer[idx + 2] as u32 * 35 / 100) as u8;

            // Live indicator dot (smooth pulsing green/emerald)
            if x >= 14 && x <= 22 && y >= 11 && y <= 19 {
                let pulse = ((elapsed * 3.0).sin() * 0.4 + 0.6) * 255.0;
                buffer[idx] = 16;
                buffer[idx + 1] = pulse as u8;
                buffer[idx + 2] = 129;
            }
        }
    }
}

fn apply_dynamic_zoom(
    src: &[u8],
    width: usize,
    height: usize,
    zoom: f32,
    center_x: f32,
    center_y: f32,
) -> Vec<u8> {
    let z = zoom.clamp(1.0, 10.0);
    let mut out = vec![0u8; width * height * 4];

    let crop_w = ((width as f32) / z) as usize;
    let crop_h = ((height as f32) / z) as usize;

    let target_offset_x = (center_x * width as f32) as isize - (crop_w as isize / 2);
    let target_offset_y = (center_y * height as f32) as isize - (crop_h as isize / 2);

    let offset_x = target_offset_x.clamp(0, (width - crop_w) as isize) as usize;
    let offset_y = target_offset_y.clamp(0, (height - crop_h) as isize) as usize;

    for y in 0..height {
        let src_y = offset_y + (y * crop_h) / height;
        for x in 0..width {
            let src_x = offset_x + (x * crop_w) / width;
            let src_idx = (src_y * width + src_x) * 4;
            let dst_idx = (y * width + x) * 4;

            out[dst_idx..dst_idx + 4].copy_from_slice(&src[src_idx..src_idx + 4]);
        }
    }

    out
}

fn apply_transforms(
    src: Vec<u8>,
    width: usize,
    height: usize,
    mirror_h: bool,
    mirror_v: bool,
    orientation: Orientation,
) -> Vec<u8> {
    if !mirror_h && !mirror_v && orientation == Orientation::Deg0 {
        return src;
    }

    let mut out = vec![0u8; width * height * 4];

    for y in 0..height {
        for x in 0..width {
            let (tx, ty) = match orientation {
                Orientation::Deg0 => (x, y),
                Orientation::Deg180 => (width - 1 - x, height - 1 - y),
                Orientation::Deg90 => (width - 1 - (y * width / height), (x * height / width)),
                Orientation::Deg270 => (y * width / height, height - 1 - (x * height / width)),
            };

            let final_x = if mirror_h { width - 1 - tx.min(width - 1) } else { tx.min(width - 1) };
            let final_y = if mirror_v { height - 1 - ty.min(height - 1) } else { ty.min(height - 1) };

            let src_idx = (y * width + x) * 4;
            let dst_idx = (final_y * width + final_x) * 4;

            out[dst_idx..dst_idx + 4].copy_from_slice(&src[src_idx..src_idx + 4]);
        }
    }

    out
}
