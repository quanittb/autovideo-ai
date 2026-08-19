use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Modes for pixel normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NormalizationMode {
    Identity,
    ZeroToOne,
    MinusOneToOne,
    MeanStd,
}

impl Default for NormalizationMode {
    fn default() -> Self {
        NormalizationMode::ZeroToOne
    }
}

/// Configuration for pixel normalization.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizationConfig {
    pub mode: NormalizationMode,
    pub mean: Option<[f32; 3]>,
    pub std: Option<[f32; 3]>,
}

impl Default for NormalizationConfig {
    fn default() -> Self {
        Self {
            mode: NormalizationMode::ZeroToOne,
            mean: None,
            std: None,
        }
    }
}

impl NormalizationConfig {
    pub fn identity() -> Self {
        Self {
            mode: NormalizationMode::Identity,
            mean: None,
            std: None,
        }
    }

    pub fn zero_to_one() -> Self {
        Self {
            mode: NormalizationMode::ZeroToOne,
            mean: None,
            std: None,
        }
    }

    pub fn minus_one_to_one() -> Self {
        Self {
            mode: NormalizationMode::MinusOneToOne,
            mean: None,
            std: None,
        }
    }

    pub fn imagenet() -> Self {
        Self {
            mode: NormalizationMode::MeanStd,
            mean: Some([0.485, 0.456, 0.406]),
            std: Some([0.229, 0.224, 0.225]),
        }
    }

    pub fn mean_std(mean: [f32; 3], std: [f32; 3]) -> Self {
        Self {
            mode: NormalizationMode::MeanStd,
            mean: Some(mean),
            std: Some(std),
        }
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.mode == NormalizationMode::MeanStd {
            let mean = self.mean.ok_or_else(|| {
                AppError::invalid_input("MeanStd normalization requires mean values")
            })?;
            let std = self.std.ok_or_else(|| {
                AppError::invalid_input("MeanStd normalization requires std values")
            })?;

            for i in 0..3 {
                if !mean[i].is_finite() {
                    return Err(AppError::invalid_input(format!(
                        "Mean value at index {} is not finite: {}",
                        i, mean[i]
                    )));
                }
                if !std[i].is_finite() {
                    return Err(AppError::invalid_input(format!(
                        "Std value at index {} is not finite: {}",
                        i, std[i]
                    )));
                }
                if std[i].abs() < 1e-7 {
                    return Err(AppError::invalid_input(format!(
                        "Std value at index {} cannot be zero: {}",
                        i, std[i]
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Normalizes a raw pixel slice into Float32 values according to the configuration.
pub fn normalize_pixels(
    pixels: &[u8],
    config: &NormalizationConfig,
    channels: usize,
) -> Result<Vec<f32>, AppError> {
    if channels == 0 {
        return Err(AppError::invalid_input("Channel count cannot be 0"));
    }

    config.validate()?;

    let mut output = Vec::with_capacity(pixels.len());

    match config.mode {
        NormalizationMode::Identity => {
            for &p in pixels {
                output.push(p as f32);
            }
        }
        NormalizationMode::ZeroToOne => {
            for &p in pixels {
                output.push((p as f32) / 255.0);
            }
        }
        NormalizationMode::MinusOneToOne => {
            for &p in pixels {
                let v = (p as f32) / 127.5 - 1.0;
                output.push(v);
            }
        }
        NormalizationMode::MeanStd => {
            let mean = config.mean.unwrap_or([0.0, 0.0, 0.0]);
            let std = config.std.unwrap_or([1.0, 1.0, 1.0]);

            if channels == 0 {
                return Err(AppError::invalid_input("Channel count cannot be 0"));
            }

            for (idx, &p) in pixels.iter().enumerate() {
                let c = idx % channels;
                let m = if c < 3 { mean[c] } else { 0.0 };
                let s = if c < 3 { std[c] } else { 1.0 };

                let norm = ((p as f32) / 255.0 - m) / s;
                if !norm.is_finite() {
                    return Err(AppError::invalid_input(format!(
                        "Non-finite normalized value produced for pixel {} (channel {})",
                        p, c
                    )));
                }
                output.push(norm);
            }
        }
    }

    Ok(output)
}
