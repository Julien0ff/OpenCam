//! Head & Face Tracking with Dynamic Distance-Based Auto-Framing (Center Stage).
//! Provides smooth cinematic tracking of the human subject with spring damping.

use core_protocol::{FaceMetrics, FaceTrackingConfig, FramingMode};

pub struct HeadTracker {
    pub config: FaceTrackingConfig,
    // Filtered / smoothed current camera viewport
    pub current_center_x: f32,
    pub current_center_y: f32,
    pub current_zoom: f32,
    // Latest computed metrics
    pub latest_metrics: FaceMetrics,
}

impl Default for HeadTracker {
    fn default() -> Self {
        Self::new(FaceTrackingConfig::default())
    }
}

impl HeadTracker {
    pub fn new(config: FaceTrackingConfig) -> Self {
        Self {
            config,
            current_center_x: 0.5,
            current_center_y: 0.5,
            current_zoom: 1.0,
            latest_metrics: FaceMetrics::default(),
        }
    }

    /// Updates tracker with target face position and returns smoothed (crop_x, crop_y, zoom).
    pub fn update(
        &mut self,
        detected_face: Option<(f32, f32, f32, f32)>, // (center_x, center_y, width, height) in 0.0..1.0
        dt: f32,
    ) -> (f32, f32, f32) {
        if !self.config.enabled {
            self.latest_metrics.detected = false;
            // Return to neutral center
            let alpha = (dt * 3.0).clamp(0.01, 0.5);
            self.current_center_x += (0.5 - self.current_center_x) * alpha;
            self.current_center_y += (0.5 - self.current_center_y) * alpha;
            self.current_zoom += (1.0 - self.current_zoom) * alpha;
            return (self.current_center_x, self.current_center_y, self.current_zoom);
        }

        if let Some((face_x, face_y, face_w, face_h)) = detected_face {
            self.latest_metrics.detected = true;
            self.latest_metrics.center_x = face_x;
            self.latest_metrics.center_y = face_y;
            self.latest_metrics.width = face_w;
            self.latest_metrics.height = face_h;
            self.latest_metrics.confidence = 0.96;

            // Ideal face height proportion based on framing mode
            let ideal_face_ratio = match self.config.framing_mode {
                FramingMode::HeadAndShoulders => 0.35,
                FramingMode::Torso => 0.22,
                FramingMode::GroupWide => 0.14,
            };

            // Distance factor: > 1.0 means subject is far away (face appears smaller than ideal)
            let distance_factor = (ideal_face_ratio / face_h.max(0.05)).clamp(0.5, 4.0);
            self.latest_metrics.distance_factor = distance_factor;

            // Target zoom calculation
            let target_zoom = if self.config.auto_zoom {
                distance_factor.clamp(self.config.zoom_min, self.config.zoom_max)
            } else {
                self.config.zoom_min
            };

            // Target center tracking (with head offset to keep eyes in upper third)
            let target_cx = face_x;
            let target_cy = (face_y + 0.06).clamp(0.2, 0.8);

            // Damped spring-like exponential smoothing
            let alpha = (self.config.smoothness * dt * 20.0).clamp(0.02, 0.4);

            self.current_center_x += (target_cx - self.current_center_x) * alpha;
            self.current_center_y += (target_cy - self.current_center_y) * alpha;
            self.current_zoom += (target_zoom - self.current_zoom) * alpha;
        } else {
            self.latest_metrics.detected = false;
        }

        (self.current_center_x, self.current_center_y, self.current_zoom)
    }
}

