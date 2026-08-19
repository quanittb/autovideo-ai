use serde::{Deserialize, Serialize};

use crate::ai::pipeline::postprocess::PostprocessConfig;
use crate::ai::pipeline::preprocess::PreprocessConfig;
use crate::ai::provider::ExecutionProvider;
use crate::error::AppError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FrameSamplingMode {
    All,
    EveryNth,
    Range,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrameSamplingConfig {
    pub mode: FrameSamplingMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
}

impl FrameSamplingConfig {
    pub fn all() -> Self {
        Self {
            mode: FrameSamplingMode::All,
            nth: None,
            start: None,
            end: None,
        }
    }

    pub fn every_nth(nth: u32) -> Self {
        Self {
            mode: FrameSamplingMode::EveryNth,
            nth: Some(nth),
            start: None,
            end: None,
        }
    }

    pub fn range(start: u64, end: u64) -> Self {
        Self {
            mode: FrameSamplingMode::Range,
            nth: None,
            start: Some(start),
            end: Some(end),
        }
    }
}

impl Default for FrameSamplingConfig {
    fn default() -> Self {
        Self::all()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AiFrameOutputMode {
    Image,
    Mask,
}

impl Default for AiFrameOutputMode {
    fn default() -> Self {
        Self::Image
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiJobConfig {
    pub enabled: bool,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ExecutionProvider>,
    pub preprocessing: PreprocessConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postprocessing: Option<PostprocessConfig>,
    #[serde(default)]
    pub frame_sampling: FrameSamplingConfig,
    #[serde(default)]
    pub output_mode: AiFrameOutputMode,
}

impl Default for AiJobConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model_id: String::new(),
            model_version: None,
            model_hash: None,
            profile_hash: None,
            provider: None,
            preprocessing: PreprocessConfig::default(),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        }
    }
}

/// Selects deterministic frame indices based on total frame count and sampling configuration.
pub fn select_frames(
    total_frames: usize,
    config: &FrameSamplingConfig,
) -> Result<Vec<usize>, AppError> {
    if total_frames == 0 {
        return Ok(Vec::new());
    }

    match config.mode {
        FrameSamplingMode::All => Ok((0..total_frames).collect()),
        FrameSamplingMode::EveryNth => {
            let nth = config.nth.unwrap_or(1) as usize;
            if nth == 0 {
                return Err(AppError::invalid_input(
                    "Frame sampling nth parameter must be greater than 0",
                ));
            }
            let selected: Vec<usize> = (0..total_frames).step_by(nth).collect();
            Ok(selected)
        }
        FrameSamplingMode::Range => {
            let start = config.start.unwrap_or(0) as usize;
            let end = config.end.unwrap_or((total_frames - 1) as u64) as usize;

            if start > end {
                return Err(AppError::invalid_input(format!(
                    "Frame sampling range start ({}) cannot be greater than end ({})",
                    start, end
                )));
            }

            if start >= total_frames {
                return Ok(Vec::new());
            }

            let clamped_end = end.min(total_frames - 1);
            Ok((start..=clamped_end).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_frames_all() {
        let config = FrameSamplingConfig {
            mode: FrameSamplingMode::All,
            nth: None,
            start: None,
            end: None,
        };
        let selected = select_frames(5, &config).unwrap();
        assert_eq!(selected, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_select_frames_every_nth() {
        let config = FrameSamplingConfig {
            mode: FrameSamplingMode::EveryNth,
            nth: Some(2),
            start: None,
            end: None,
        };
        let selected = select_frames(6, &config).unwrap();
        assert_eq!(selected, vec![0, 2, 4]);

        let invalid_config = FrameSamplingConfig {
            mode: FrameSamplingMode::EveryNth,
            nth: Some(0),
            start: None,
            end: None,
        };
        assert!(select_frames(6, &invalid_config).is_err());
    }

    #[test]
    fn test_select_frames_range() {
        let config = FrameSamplingConfig {
            mode: FrameSamplingMode::Range,
            nth: None,
            start: Some(2),
            end: Some(4),
        };
        let selected = select_frames(10, &config).unwrap();
        assert_eq!(selected, vec![2, 3, 4]);

        // Clamped end
        let config_clamped = FrameSamplingConfig {
            mode: FrameSamplingMode::Range,
            nth: None,
            start: Some(8),
            end: Some(20),
        };
        let selected = select_frames(10, &config_clamped).unwrap();
        assert_eq!(selected, vec![8, 9]);

        // Invalid range start > end
        let config_invalid = FrameSamplingConfig {
            mode: FrameSamplingMode::Range,
            nth: None,
            start: Some(5),
            end: Some(3),
        };
        assert!(select_frames(10, &config_invalid).is_err());
    }
}
