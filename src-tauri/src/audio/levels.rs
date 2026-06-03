use std::sync::atomic::{AtomicU32, Ordering};

use serde::Serialize;

pub const RECORDING_LEVEL_EVENT: &str = "recording-level";
pub const EMIT_INTERVAL_MS: u64 = 33;
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

#[cfg_attr(not(test), allow(dead_code))]
pub fn rms_level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let mut sum_squares: f64 = 0.0;
    for sample in samples {
        let value = sample.clamp(-1.0, 1.0) as f64;
        sum_squares += value * value;
    }

    let mean = sum_squares / samples.len() as f64;
    (mean.sqrt() as f32).clamp(0.0, 1.0)
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
    use super::{
        fold_level, normalize_rms, rms_level, ATTACK_COEFFICIENT, NOISE_FLOOR, RELEASE_COEFFICIENT,
    };

    #[test]
    fn rms_is_zero_for_silence() {
        assert_eq!(rms_level(&[0.0; 1024]), 0.0);
    }

    #[test]
    fn rms_is_positive_for_signal() {
        let level = rms_level(&[0.5_f32; 1024]);
        assert!((level - 0.5).abs() < 0.01, "expected ~0.5 got {level}");
    }

    #[test]
    fn rms_clamps_out_of_range_inputs() {
        let level = rms_level(&[2.0_f32, -3.0, 0.0]);
        assert!(level > 0.0);
        assert!(level <= 1.0);
    }

    #[test]
    fn rms_handles_empty_input() {
        assert_eq!(rms_level(&[]), 0.0);
    }

    #[test]
    fn rms_handles_alternating_polarity() {
        let level = rms_level(&[0.5_f32, -0.5, 0.5, -0.5]);
        assert!((level - 0.5).abs() < 0.01, "expected ~0.5 got {level}");
    }

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
