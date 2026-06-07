#![allow(dead_code)]

use std::ffi::OsStr;

pub const NEMOTRON_STREAMING_STT_FLAG: &str = "FLOE_EXPERIMENTAL_NEMOTRON_STREAMING_STT";

pub fn nemotron_streaming_stt_enabled() -> bool {
    experiment_flag_enabled(std::env::var_os(NEMOTRON_STREAMING_STT_FLAG).as_deref())
}

fn experiment_flag_enabled(value: Option<&OsStr>) -> bool {
    value
        .and_then(OsStr::to_str)
        .is_some_and(is_truthy_flag_value)
}

fn is_truthy_flag_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::{experiment_flag_enabled, NEMOTRON_STREAMING_STT_FLAG};

    #[test]
    fn nemotron_streaming_stt_flag_name_is_stable() {
        assert_eq!(
            NEMOTRON_STREAMING_STT_FLAG,
            "FLOE_EXPERIMENTAL_NEMOTRON_STREAMING_STT"
        );
    }

    #[test]
    fn nemotron_streaming_stt_defaults_disabled_when_missing() {
        assert!(!experiment_flag_enabled(None));
    }

    #[test]
    fn nemotron_streaming_stt_accepts_explicit_truthy_values() {
        for value in ["1", "true", "TRUE", "yes", "on", " On "] {
            assert!(
                experiment_flag_enabled(Some(OsStr::new(value))),
                "{value:?} should enable the experiment"
            );
        }
    }

    #[test]
    fn nemotron_streaming_stt_rejects_disabled_or_ambiguous_values() {
        for value in ["", "0", "false", "no", "off", "nemotron", "enabled"] {
            assert!(
                !experiment_flag_enabled(Some(OsStr::new(value))),
                "{value:?} should not enable the experiment"
            );
        }
    }
}
