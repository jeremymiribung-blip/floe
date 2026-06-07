#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AsrPipelineMode {
    #[default]
    GroqCloud,
    ExperimentalNemotronStreaming,
}

pub fn configured_pipeline_mode() -> AsrPipelineMode {
    pipeline_mode_for_nemotron_flag(crate::experiments::nemotron_streaming_stt_enabled())
}

pub(crate) fn pipeline_mode_for_nemotron_flag(enabled: bool) -> AsrPipelineMode {
    if enabled {
        AsrPipelineMode::ExperimentalNemotronStreaming
    } else {
        AsrPipelineMode::GroqCloud
    }
}

#[cfg(test)]
mod tests {
    use super::{pipeline_mode_for_nemotron_flag, AsrPipelineMode};

    #[test]
    fn asr_pipeline_mode_defaults_to_groq_cloud() {
        assert_eq!(AsrPipelineMode::default(), AsrPipelineMode::GroqCloud);
    }

    #[test]
    fn disabled_experimental_mode_selects_groq_cloud() {
        assert_eq!(
            pipeline_mode_for_nemotron_flag(false),
            AsrPipelineMode::GroqCloud
        );
    }

    #[test]
    fn enabled_experimental_mode_selects_nemotron_streaming_placeholder() {
        assert_eq!(
            pipeline_mode_for_nemotron_flag(true),
            AsrPipelineMode::ExperimentalNemotronStreaming
        );
    }

    #[test]
    fn asr_pipeline_modes_serialize_to_stable_snake_case_values() {
        assert_eq!(
            serde_json::to_string(&AsrPipelineMode::GroqCloud).unwrap(),
            "\"groq_cloud\""
        );
        assert_eq!(
            serde_json::to_string(&AsrPipelineMode::ExperimentalNemotronStreaming).unwrap(),
            "\"experimental_nemotron_streaming\""
        );
    }
}
