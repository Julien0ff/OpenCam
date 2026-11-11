//! Fast integer pixel conversions between RGB/RGBA and YUYV 4:2:2.
//! Optimized for real-time webcam video pipelines.

#[inline(always)]
fn clamp_u8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// Converts an RGBA buffer (4 bytes per pixel) to YUYV 4:2:2 (2 bytes per pixel).
/// Destination buffer must be at least `width * height * 2` bytes.
pub fn rgba_to_yuyv(rgba: &[u8], width: usize, height: usize, yuyv: &mut [u8]) {
    let pixel_count = width * height;
    assert!(rgba.len() >= pixel_count * 4, "RGBA buffer too small");
    assert!(yuyv.len() >= pixel_count * 2, "YUYV destination buffer too small");

    let num_pairs = pixel_count / 2;
    for i in 0..num_pairs {
        let rgba_idx = i * 8;
        let yuyv_idx = i * 4;

        let r0 = rgba[rgba_idx] as i32;
        let g0 = rgba[rgba_idx + 1] as i32;
        let b0 = rgba[rgba_idx + 2] as i32;

        let r1 = rgba[rgba_idx + 4] as i32;
        let g1 = rgba[rgba_idx + 5] as i32;
        let b1 = rgba[rgba_idx + 6] as i32;

        // Average colors for shared chroma
        let r_avg = (r0 + r1) >> 1;
        let g_avg = (g0 + g1) >> 1;
        let b_avg = (b0 + b1) >> 1;

        // Full-range BT.601 conversion
        let y0 = clamp_u8((77 * r0 + 150 * g0 + 29 * b0) >> 8);
        let y1 = clamp_u8((77 * r1 + 150 * g1 + 29 * b1) >> 8);
        let u = clamp_u8(((-43 * r_avg - 85 * g_avg + 128 * b_avg) >> 8) + 128);
        let v = clamp_u8(((128 * r_avg - 107 * g_avg - 21 * b_avg) >> 8) + 128);

        yuyv[yuyv_idx] = y0;
        yuyv[yuyv_idx + 1] = u;
        yuyv[yuyv_idx + 2] = y1;
        yuyv[yuyv_idx + 3] = v;
    }
}

/// Converts an RGB24 buffer (3 bytes per pixel) to YUYV 4:2:2 (2 bytes per pixel).
pub fn rgb24_to_yuyv(rgb: &[u8], width: usize, height: usize, yuyv: &mut [u8]) {
    let pixel_count = width * height;
    assert!(rgb.len() >= pixel_count * 3, "RGB buffer too small");
    assert!(yuyv.len() >= pixel_count * 2, "YUYV destination buffer too small");

    let num_pairs = pixel_count / 2;
    for i in 0..num_pairs {
        let rgb_idx = i * 6;
        let yuyv_idx = i * 4;

        let r0 = rgb[rgb_idx] as i32;
        let g0 = rgb[rgb_idx + 1] as i32;
        let b0 = rgb[rgb_idx + 2] as i32;

        let r1 = rgb[rgb_idx + 3] as i32;
        let g1 = rgb[rgb_idx + 4] as i32;
        let b1 = rgb[rgb_idx + 5] as i32;

        let r_avg = (r0 + r1) >> 1;
        let g_avg = (g0 + g1) >> 1;
        let b_avg = (b0 + b1) >> 1;

        let y0 = clamp_u8((77 * r0 + 150 * g0 + 29 * b0) >> 8);
        let y1 = clamp_u8((77 * r1 + 150 * g1 + 29 * b1) >> 8);
        let u = clamp_u8(((-43 * r_avg - 85 * g_avg + 128 * b_avg) >> 8) + 128);
        let v = clamp_u8(((128 * r_avg - 107 * g_avg - 21 * b_avg) >> 8) + 128);

        yuyv[yuyv_idx] = y0;
        yuyv[yuyv_idx + 1] = u;
        yuyv[yuyv_idx + 2] = y1;
        yuyv[yuyv_idx + 3] = v;
    }
}

/// Converts a YUYV 4:2:2 buffer to RGBA (for live egui texture rendering).
pub fn yuyv_to_rgba(yuyv: &[u8], width: usize, height: usize, rgba: &mut [u8]) {
    let pixel_count = width * height;
    assert!(yuyv.len() >= pixel_count * 2, "YUYV buffer too small");
    assert!(rgba.len() >= pixel_count * 4, "RGBA destination buffer too small");

    let num_pairs = pixel_count / 2;
    for i in 0..num_pairs {
        let yuyv_idx = i * 4;
        let rgba_idx = i * 8;

        let y0 = yuyv[yuyv_idx] as i32;
        let u = yuyv[yuyv_idx + 1] as i32 - 128;
        let y1 = yuyv[yuyv_idx + 2] as i32;
        let v = yuyv[yuyv_idx + 3] as i32 - 128;

        // Pixel 0
        let r0 = clamp_u8(y0 + ((359 * v) >> 8));
        let g0 = clamp_u8(y0 - ((88 * u + 183 * v) >> 8));
        let b0 = clamp_u8(y0 + ((454 * u) >> 8));

        rgba[rgba_idx] = r0;
        rgba[rgba_idx + 1] = g0;
        rgba[rgba_idx + 2] = b0;
        rgba[rgba_idx + 3] = 255;

        // Pixel 1
        let r1 = clamp_u8(y1 + ((359 * v) >> 8));
        let g1 = clamp_u8(y1 - ((88 * u + 183 * v) >> 8));
        let b1 = clamp_u8(y1 + ((454 * u) >> 8));

        rgba[rgba_idx + 4] = r1;
        rgba[rgba_idx + 5] = g1;
        rgba[rgba_idx + 6] = b1;
        rgba[rgba_idx + 7] = 255;
    }
}

/// Converts a YUYV 4:2:2 buffer to RGB24.
pub fn yuyv_to_rgb24(yuyv: &[u8], width: usize, height: usize, rgb: &mut [u8]) {
    let pixel_count = width * height;
    assert!(yuyv.len() >= pixel_count * 2, "YUYV buffer too small");
    assert!(rgb.len() >= pixel_count * 3, "RGB destination buffer too small");

    let num_pairs = pixel_count / 2;
    for i in 0..num_pairs {
        let yuyv_idx = i * 4;
        let rgb_idx = i * 6;

        let y0 = yuyv[yuyv_idx] as i32;
        let u = yuyv[yuyv_idx + 1] as i32 - 128;
        let y1 = yuyv[yuyv_idx + 2] as i32;
        let v = yuyv[yuyv_idx + 3] as i32 - 128;

        let r0 = clamp_u8(y0 + ((359 * v) >> 8));
        let g0 = clamp_u8(y0 - ((88 * u + 183 * v) >> 8));
        let b0 = clamp_u8(y0 + ((454 * u) >> 8));

        rgb[rgb_idx] = r0;
        rgb[rgb_idx + 1] = g0;
        rgb[rgb_idx + 2] = b0;

        let r1 = clamp_u8(y1 + ((359 * v) >> 8));
        let g1 = clamp_u8(y1 - ((88 * u + 183 * v) >> 8));
        let b1 = clamp_u8(y1 + ((454 * u) >> 8));

        rgb[rgb_idx + 3] = r1;
        rgb[rgb_idx + 4] = g1;
        rgb[rgb_idx + 5] = b1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgba_to_yuyv_and_back() {
        let width = 4;
        let height = 2;
        let mut original_rgba = vec![0u8; width * height * 4];

        // Fill with some recognizable colors
        for i in 0..(width * height) {
            original_rgba[i * 4] = 200;     // R
            original_rgba[i * 4 + 1] = 100; // G
            original_rgba[i * 4 + 2] = 50;  // B
            original_rgba[i * 4 + 3] = 255; // A
        }

        let mut yuyv = vec![0u8; width * height * 2];
        rgba_to_yuyv(&original_rgba, width, height, &mut yuyv);

        let mut reconstructed_rgba = vec![0u8; width * height * 4];
        yuyv_to_rgba(&yuyv, width, height, &mut reconstructed_rgba);

        // Check reasonable color fidelity after chroma subsampling and conversion
        for i in 0..(width * height) {
            let r_diff = (original_rgba[i * 4] as i32 - reconstructed_rgba[i * 4] as i32).abs();
            let g_diff = (original_rgba[i * 4 + 1] as i32 - reconstructed_rgba[i * 4 + 1] as i32).abs();
            let b_diff = (original_rgba[i * 4 + 2] as i32 - reconstructed_rgba[i * 4 + 2] as i32).abs();

            assert!(r_diff <= 10, "R difference too high: {}", r_diff);
            assert!(g_diff <= 10, "G difference too high: {}", g_diff);
            assert!(b_diff <= 10, "B difference too high: {}", b_diff);
            assert_eq!(reconstructed_rgba[i * 4 + 3], 255);
        }
    }
}

