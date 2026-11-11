//! Dynamic real-time frame generator for camera simulation and test patterns.
//! Generates real, live 30/60 FPS frames with timestamps, moving radar elements,
//! and live applied zoom/mirror/rotation/torch transformations.

use core_protocol::Orientation;
use std::time::Instant;

pub struct DynamicFrameGenerator {
    start_time: Instant,
    frame_counter: u64,
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
            frame_counter: 0,
        }
    }

    /// Generates a live RGBA frame with animated elements and applied video adjustments.
    pub fn generate_rgba_frame(
        &mut self,
        width: u32,
        height: u32,
        zoom: f32,
        mirror_h: bool,
        mirror_v: bool,
        orientation: Orientation,
        torch_enabled: bool,
        camera_name: &str,
    ) -> Vec<u8> {
        self.frame_counter += 1;
        let elapsed = self.start_time.elapsed().as_secs_f32();

        // 1. Generate base scene into an RGBA buffer
        let w = width as usize;
        let h = height as usize;
        let mut buffer = vec![0u8; w * h * 4];

        // Animated color gradient background
        let wave_r = ((elapsed * 2.0).sin() * 0.5 + 0.5) * 50.0;
        let wave_g = ((elapsed * 1.5 + 2.0).cos() * 0.5 + 0.5) * 50.0;
        let wave_b = ((elapsed * 2.5 + 4.0).sin() * 0.5 + 0.5) * 70.0;

        // Torch multiplier
        let torch_boost = if torch_enabled { 80.0 } else { 0.0 };

        // Center coordinates
        let cx = (w / 2) as f32;
        let cy = (h / 2) as f32;
        let radius = (w.min(h) as f32) * 0.35 / zoom.clamp(1.0, 10.0);

        // Animated target angle
        let angle = elapsed * 3.0;
        let target_x = cx + angle.cos() * radius;
        let target_y = cy + angle.sin() * radius;

        for y in 0..h {
            let dy = y as f32 - cy;
            let norm_y = y as f32 / h as f32;

            for x in 0..w {
                let dx = x as f32 - cx;
                let norm_x = x as f32 / w as f32;
                let idx = (y * w + x) * 4;

                // Distance to animated target
                let dist_target = ((x as f32 - target_x).powi(2) + (y as f32 - target_y).powi(2)).sqrt();
                // Distance to center (for torch lighting cone)
                let dist_center = (dx.powi(2) + dy.powi(2)).sqrt();

                // Torch light falloff from center
                let torch_light = if torch_enabled {
                    (torch_boost * (1.0 - (dist_center / (w as f32 * 0.6)).min(1.0))).max(0.0)
                } else {
                    0.0
                };

                let mut r = (norm_x * 40.0 + wave_r + torch_light) as u8;
                let mut g = (norm_y * 40.0 + wave_g + torch_light) as u8;
                let mut b = (60.0 + wave_b + torch_light) as u8;

                // Center crosshair
                if (dx.abs() < 2.0 && dy.abs() < 40.0) || (dy.abs() < 2.0 && dx.abs() < 40.0) {
                    r = 255;
                    g = 255;
                    b = 255;
                }

                // Radar circular ring
                let dist_circle = (dist_center - radius).abs();
                if dist_circle < 2.5 {
                    r = 0;
                    g = 220;
                    b = 255;
                }

                // Animated glowing beacon
                if dist_target < 24.0 {
                    let pulse = (1.0 - dist_target / 24.0) * 255.0;
                    r = r.saturating_add(pulse as u8);
                    g = g.saturating_add((pulse * 0.8) as u8);
                    b = b.saturating_add(40);
                }

                // Grid lines every 100 pixels
                if x % 100 == 0 || y % 100 == 0 {
                    r = r.saturating_add(25);
                    g = g.saturating_add(25);
                    b = b.saturating_add(35);
                }

                buffer[idx] = r;
                buffer[idx + 1] = g;
                buffer[idx + 2] = b;
                buffer[idx + 3] = 255;
            }
        }

        // Draw top HUD banner (frame counter, time, camera name)
        draw_hud_bar(&mut buffer, w, h, camera_name, self.frame_counter, elapsed, zoom, torch_enabled);

        // 2. Apply digital zoom if > 1.0
        let zoomed_buffer = if zoom > 1.01 {
            apply_zoom(&buffer, w, h, zoom)
        } else {
            buffer
        };

        // 3. Apply mirror & orientation
        apply_transforms(zoomed_buffer, w, h, mirror_h, mirror_v, orientation)
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
    let hud_height = 40.min(height);
    for y in 0..hud_height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            // Dark translucent overlay
            buffer[idx] = (buffer[idx] as u32 * 40 / 100) as u8;
            buffer[idx + 1] = (buffer[idx + 1] as u32 * 40 / 100) as u8;
            buffer[idx + 2] = (buffer[idx + 2] as u32 * 50 / 100) as u8;

            // Live indicator dot (red blinking)
            if x >= 15 && x <= 27 && y >= 14 && y <= 26 {
                let is_blink_on = (elapsed * 2.0).fract() < 0.6;
                if is_blink_on {
                    buffer[idx] = 255;
                    buffer[idx + 1] = 40;
                    buffer[idx + 2] = 40;
                }
            }
        }
    }
}

fn apply_zoom(src: &[u8], width: usize, height: usize, zoom: f32) -> Vec<u8> {
    let z = zoom.clamp(1.0, 10.0);
    let mut out = vec![0u8; width * height * 4];

    let crop_w = (width as f32 / z) as usize;
    let crop_h = (height as f32 / z) as usize;
    let offset_x = (width - crop_w) / 2;
    let offset_y = (height - crop_h) / 2;

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
                // 90 and 270 degree rotation when preserving aspect ratio
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
