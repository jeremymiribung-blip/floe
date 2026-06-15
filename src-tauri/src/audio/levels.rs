use std::sync::atomic::{AtomicU32, Ordering};

use serde::Serialize;

pub use crate::contract::{
    EVENT_RECORDING_LEVEL as RECORDING_LEVEL_EVENT,
    EVENT_RECORDING_STATE_CHANGED as RECORDING_STATE_EVENT,
    LEVEL_EMIT_INTERVAL_MS as EMIT_INTERVAL_MS,
};
pub const ATTACK_COEFFICIENT: f32 = 0.7;
pub const RELEASE_COEFFICIENT: f32 = 0.12;
pub const NOISE_FLOOR: f32 = 0.005;
const MIN_DB: f64 = -50.0;
const MAX_DB: f64 = 0.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingLevelPayload {
    pub level: f32,
}

pub fn normalize_rms(rms: f32) -> f32 {
    if !rms.is_finite() || rms <= NOISE_FLOOR {
        return 0.0;
    }

    let linear = rms.max(1.0e-9) as f64;
    let db = 20.0 * linear.log10();
    let normalized = (db - MIN_DB) / (MAX_DB - MIN_DB);
    (normalized.clamp(0.0, 1.0) as f32).clamp(0.0, 1.0)
}

pub fn fold_level(previous: f32, next: f32) -> f32 {
    let coefficient = if next > previous {
        ATTACK_COEFFICIENT
    } else {
        RELEASE_COEFFICIENT
    };
    let coefficient = coefficient.clamp(0.0, 1.0);
    let smoothed = previous + (next - previous) * coefficient;
    smoothed.clamp(0.0, 1.0)
}

pub struct LevelMeter {
    latest: AtomicU32,
}

impl LevelMeter {
    pub fn new() -> Self {
        Self {
            latest: AtomicU32::new(0.0_f32.to_bits()),
        }
    }

    pub fn store(&self, level: f32) {
        let clamped = if level.is_finite() {
            level.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.latest.store(clamped.to_bits(), Ordering::Relaxed);
    }

    pub fn load(&self) -> f32 {
        f32::from_bits(self.latest.load(Ordering::Relaxed))
    }
}

impl Default for LevelMeter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{fold_level, normalize_rms, ATTACK_COEFFICIENT, NOISE_FLOOR, RELEASE_COEFFICIENT};

    #[test]
    fn normalize_returns_zero_for_silence() {
        assert_eq!(normalize_rms(0.0), 0.0);
    }

    #[test]
    fn normalize_returns_zero_below_noise_floor() {
        assert_eq!(normalize_rms(NOISE_FLOOR / 2.0), 0.0);
        assert_eq!(normalize_rms(0.005), 0.0);
    }

    #[test]
    fn normalize_clamps_output_to_unit_range() {
        assert_eq!(normalize_rms(1.0), 1.0);
        assert_eq!(normalize_rms(0.0), 0.0);
    }

    #[test]
    fn normalize_boosts_quiet_inputs() {
        let quiet = normalize_rms(0.05);
        let loud = normalize_rms(0.5);
        assert!(
            quiet > 0.0,
            "quiet signals should map above zero, got {quiet}"
        );
        assert!(loud > quiet, "loud signals should map higher than quiet");
        assert!(loud <= 1.0);
    }

    #[test]
    fn normalize_rejects_nan_and_infinity() {
        assert_eq!(normalize_rms(f32::NAN), 0.0);
        assert_eq!(normalize_rms(f32::INFINITY), 0.0);
    }

    #[test]
    fn fold_uses_attack_when_rising() {
        let next = fold_level(0.1, 0.9);
        let expected = 0.1 + (0.9 - 0.1) * ATTACK_COEFFICIENT;
        assert!(
            (next - expected).abs() < 1.0e-5,
            "got {next} expected {expected}"
        );
    }

    #[test]
    fn fold_uses_release_when_falling() {
        let next = fold_level(0.9, 0.1);
        let expected = 0.9 + (0.1 - 0.9) * RELEASE_COEFFICIENT;
        assert!(
            (next - expected).abs() < 1.0e-5,
            "got {next} expected {expected}"
        );
    }

    #[test]
    fn fold_clamps_output_to_unit_range() {
        assert_eq!(fold_level(1.0, 1.5).clamp(0.0, 1.0), fold_level(1.0, 1.5));
        let low = fold_level(-0.5, -0.2);
        assert!((0.0..=1.0).contains(&low));
    }
}
