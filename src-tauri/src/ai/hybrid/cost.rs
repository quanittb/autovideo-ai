use super::planner::QualityMode;
use super::provider::{CostEstimate, GenerationError, ProviderConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostStatus {
    Exact,
    Estimated,
    Unknown,
}

pub struct CostEstimator;

impl CostEstimator {
    pub fn estimate_for_keyframes(
        provider_cfg: &ProviderConfig,
        keyframe_count: usize,
        video_duration_sec: f64,
    ) -> CostEstimate {
        if let Some(price_per_img) = provider_cfg.pricing_per_image {
            let total_cost = price_per_img * (keyframe_count as f64);
            CostEstimate {
                estimated_cost: Some(total_cost),
                currency: provider_cfg.currency.clone(),
                estimated_requests: keyframe_count,
                estimated_generated_seconds: video_duration_sec,
                estimated_keyframes: keyframe_count,
                estimated_local_processing_time_sec: (keyframe_count as f64) * 0.8 + 2.0,
                confidence: 0.95,
                status: CostStatus::Estimated,
            }
        } else {
            CostEstimate {
                estimated_cost: None,
                currency: provider_cfg.currency.clone(),
                estimated_requests: keyframe_count,
                estimated_generated_seconds: video_duration_sec,
                estimated_keyframes: keyframe_count,
                estimated_local_processing_time_sec: (keyframe_count as f64) * 0.8 + 2.0,
                confidence: 0.0,
                status: CostStatus::Unknown,
            }
        }
    }

    pub fn estimate_for_video(
        provider_cfg: &ProviderConfig,
        video_duration_sec: f64,
    ) -> CostEstimate {
        if let Some(price_per_sec) = provider_cfg.pricing_per_video_second {
            let total_cost = price_per_sec * video_duration_sec;
            CostEstimate {
                estimated_cost: Some(total_cost),
                currency: provider_cfg.currency.clone(),
                estimated_requests: 1,
                estimated_generated_seconds: video_duration_sec,
                estimated_keyframes: 0,
                estimated_local_processing_time_sec: 4.0,
                confidence: 0.90,
                status: CostStatus::Estimated,
            }
        } else {
            CostEstimate {
                estimated_cost: None,
                currency: provider_cfg.currency.clone(),
                estimated_requests: 1,
                estimated_generated_seconds: video_duration_sec,
                estimated_keyframes: 0,
                estimated_local_processing_time_sec: 4.0,
                confidence: 0.0,
                status: CostStatus::Unknown,
            }
        }
    }
}

pub struct BudgetController;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetAlternative {
    pub suggested_quality_mode: QualityMode,
    pub suggested_keyframe_stride: usize,
    pub description: String,
}

impl BudgetController {
    pub fn check_budget(
        estimate: &CostEstimate,
        max_budget: Option<f64>,
    ) -> Result<(), GenerationError> {
        if let (Some(max), Some(cost)) = (max_budget, estimate.estimated_cost) {
            if cost > max {
                return Err(GenerationError::BudgetExceeded {
                    estimated: cost,
                    budget: max,
                });
            }
        }
        Ok(())
    }

    pub fn suggest_alternatives(estimated_cost: f64, max_budget: f64) -> Vec<BudgetAlternative> {
        let ratio = estimated_cost / max_budget.max(0.01);
        let mut alts = Vec::new();

        if ratio > 1.5 {
            alts.push(BudgetAlternative {
                suggested_quality_mode: QualityMode::Economy,
                suggested_keyframe_stride: 12,
                description:
                    "Switch to Economy mode with wider keyframe spacing to cut cost by ~60%"
                        .to_string(),
            });
        } else {
            alts.push(BudgetAlternative {
                suggested_quality_mode: QualityMode::Balanced,
                suggested_keyframe_stride: 8,
                description: "Switch to Balanced mode with moderate keyframe density".to_string(),
            });
        }

        alts.push(BudgetAlternative {
            suggested_quality_mode: QualityMode::SmartAuto,
            suggested_keyframe_stride: 15,
            description: "Switch to Local processing for key components to eliminate cloud cost"
                .to_string(),
        });

        alts
    }
}
