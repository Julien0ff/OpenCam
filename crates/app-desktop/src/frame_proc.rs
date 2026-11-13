use core_protocol::Orientation;

/// Fast skin-chrominance face bounding-box estimator on a coarse 64x36 grid (< 0.1ms).
/// Uses a 16x9 density histogram to lock onto the dominant face cluster rather than
/// averaging background skin-toned objects.
/// Returns `(center_x, center_y, width, height)` normalized to `0.0..=1.0`.
pub fn detect_face_bbox(rgba: &[u8], w: usize, h: usize) -> Option<(f32, f32, f32, f32)> {
    if w < 16 || h < 16 || rgba.len() < w * h * 4 {
        return None;
    }

    const GRID_W: usize = 64;
    const GRID_H: usize = 36;
    const BINS_X: usize = 16;
    const BINS_Y: usize = 9;

    let mut bins = [0u16; BINS_X * BINS_Y];
    let mut bin_sum_x = [0.0f32; BINS_X * BINS_Y];
    let mut bin_sum_y = [0.0f32; BINS_X * BINS_Y];

    for gy in 1..(GRID_H - 1) {
        let ny = (gy as f32 + 0.5) / GRID_H as f32;
        let sy = (ny * h as f32) as usize;
        let row_off = sy * w * 4;
        let by = (gy * BINS_Y) / GRID_H;
        for gx in 2..(GRID_W - 2) {
            let nx = (gx as f32 + 0.5) / GRID_W as f32;
            let sx = (nx * w as f32) as usize;
            let idx = row_off + sx * 4;

            let r = rgba[idx] as i32;
            let g = rgba[idx + 1] as i32;
            let b = rgba[idx + 2] as i32;

            // BT.601 YCbCr skin-tone chrominance heuristic
            let y = (77 * r + 150 * g + 29 * b) >> 8;
            if y < 45 || y > 240 {
                continue;
            }
            let cb = ((-43 * r - 85 * g + 128 * b) >> 8) + 128;
            let cr = ((128 * r - 107 * g - 21 * b) >> 8) + 128;

            if (77..=127).contains(&cb) && (133..=173).contains(&cr) {
                let bx = (gx * BINS_X) / GRID_W;
                let bi = by * BINS_X + bx;
                bins[bi] += 1;
                bin_sum_x[bi] += nx;
                bin_sum_y[bi] += ny;
            }
        }
    }

    // Find peak 3x3 cluster in the 16x9 grid
    let mut best_score = 0u32;
    let mut best_bx = 0usize;
    let mut best_by = 0usize;
    for by in 0..BINS_Y {
        for bx in 0..BINS_X {
            let mut score = 0u32;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let ny = by as i32 + dy;
                    let nx = bx as i32 + dx;
                    if ny >= 0 && ny < BINS_Y as i32 && nx >= 0 && nx < BINS_X as i32 {
                        score += bins[ny as usize * BINS_X + nx as usize] as u32;
                    }
                }
            }
            if score > best_score {
                best_score = score;
                best_bx = bx;
                best_by = by;
            }
        }
    }

    if best_score < 8 {
        return None;
    }

    // Compute centroid and bounding box over the 5x5 neighborhood around the peak
    let mut total_count = 0u32;
    let mut sum_x = 0.0f32;
    let mut sum_y = 0.0f32;
    let mut min_bx = BINS_X;
    let mut max_bx = 0usize;
    let mut min_by = BINS_Y;
    let mut max_by = 0usize;

    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            let ny = best_by as i32 + dy;
            let nx = best_bx as i32 + dx;
            if ny >= 0 && ny < BINS_Y as i32 && nx >= 0 && nx < BINS_X as i32 {
                let ux = nx as usize;
                let uy = ny as usize;
                let bi = uy * BINS_X + ux;
                let c = bins[bi];
                if c > 0 {
                    total_count += c as u32;
                    sum_x += bin_sum_x[bi];
                    sum_y += bin_sum_y[bi];
                    min_bx = min_bx.min(ux);
                    max_bx = max_bx.max(ux);
                    min_by = min_by.min(uy);
                    max_by = max_by.max(uy);
                }
            }
        }
    }

    if total_count < 8 {
        return None;
    }

    let cx = (sum_x / total_count as f32).clamp(0.08, 0.92);
    let cy = (sum_y / total_count as f32).clamp(0.08, 0.92);
    let bw = (((max_bx - min_bx + 1) as f32) / BINS_X as f32).clamp(0.12, 0.65);
    let bh = (((max_by - min_by + 1) as f32) / BINS_Y as f32).clamp(0.15, 0.75);
    Some((cx, cy, bw, bh))
}

/// Applies rotation (`0°/90°/180°/270°`), horizontal/vertical mirror, and zoom/crop
/// in a single pass over the RGBA buffer.
pub fn process_frame(
    rgba: &[u8],
    w: usize,
    h: usize,
    orientation: Orientation,
    mirror_h: bool,
    mirror_v: bool,
    zoom: f32,
    center_x: f32,
    center_y: f32,
) -> (Vec<u8>, usize, usize) {
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return (rgba.to_vec(), w, h);
    }

    let zoom = zoom.clamp(1.0, 8.0);
    if orientation == Orientation::Deg0 && !mirror_h && !mirror_v && zoom <= 1.001 {
        return (rgba.to_vec(), w, h);
    }

    let (out_w, out_h) = match orientation {
        Orientation::Deg0 | Orientation::Deg180 => (w, h),
        Orientation::Deg90 | Orientation::Deg270 => (h, w),
    };

    let inv_zoom = 1.0 / zoom;
    let half = 0.5 * inv_zoom;
    let cx = center_x.clamp(half, 1.0 - half);
    let cy = center_y.clamp(half, 1.0 - half);
    let x_start = cx - half;
    let y_start = cy - half;

    let mut out = vec![0u8; out_w * out_h * 4];
    let out_w_f = out_w as f32;
    let out_h_f = out_h as f32;
    let w_f = w as f32;
    let h_f = h as f32;
    let max_sx = w - 1;
    let max_sy = h - 1;

    for oy in 0..out_h {
        let mut v = y_start + ((oy as f32 + 0.5) / out_h_f) * inv_zoom;
        if mirror_v {
            v = 1.0 - v;
        }
        let dst_row = oy * out_w * 4;

        for ox in 0..out_w {
            let mut u = x_start + ((ox as f32 + 0.5) / out_w_f) * inv_zoom;
            if mirror_h {
                u = 1.0 - u;
            }

            let (su, sv) = match orientation {
                Orientation::Deg0 => (u, v),
                Orientation::Deg90 => (v, 1.0 - u),
                Orientation::Deg180 => (1.0 - u, 1.0 - v),
                Orientation::Deg270 => (1.0 - v, u),
            };

            let sx = ((su * w_f) as isize).clamp(0, max_sx as isize) as usize;
            let sy = ((sv * h_f) as isize).clamp(0, max_sy as isize) as usize;
            let src_idx = (sy * w + sx) * 4;
            let dst_idx = dst_row + ox * 4;

            out[dst_idx..dst_idx + 4].copy_from_slice(&rgba[src_idx..src_idx + 4]);
        }
    }

    (out, out_w, out_h)
}

