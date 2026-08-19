use super::planner::{QualityMode, TransformationIntent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyframeSelectionReason {
    InitialFrame,
    SceneCut,
    HighMotionPeak,
    PoseChangeThreshold,
    PeriodicAnchor,
    FinalFrame,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyframeEntry {
    pub frame_index: usize,
    pub timestamp_sec: f64,
    pub reason: KeyframeSelectionReason,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyframePlan {
    pub total_source_frames: usize,
    pub selected_keyframes: Vec<KeyframeEntry>,
    pub keyframe_count: usize,
    pub keyframe_density_pct: f64,
    pub reduction_ratio: f64,
    pub quality_mode: QualityMode,
    pub intent: TransformationIntent,
}

pub struct KeyframePlanner;

impl KeyframePlanner {
    pub fn plan_keyframes(
        total_frames: usize,
        fps: f64,
        scene_cut_indices: &[usize],
        motion_peaks: &[usize],
        quality_mode: QualityMode,
        intent: TransformationIntent,
    ) -> KeyframePlan {
        if total_frames == 0 {
            return KeyframePlan {
                total_source_frames: 0,
                selected_keyframes: vec![],
                keyframe_count: 0,
                keyframe_density_pct: 0.0,
                reduction_ratio: 1.0,
                quality_mode,
                intent,
            };
        }

        let base_stride = match quality_mode {
            QualityMode::Economy => 24,   // ~0.8s between anchors
            QualityMode::Balanced => 10,  // ~0.33s between anchors
            QualityMode::Quality => 6,    // ~0.2s between anchors
            QualityMode::SmartAuto => 12, // ~0.4s default
        };

        let mut keyframes: Vec<KeyframeEntry> = Vec::new();
        let mut added_indices = std::collections::HashSet::new();

        // 1. Initial Frame is mandatory
        keyframes.push(KeyframeEntry {
            frame_index: 0,
            timestamp_sec: 0.0,
            reason: KeyframeSelectionReason::InitialFrame,
            score: 1.0,
        });
        added_indices.insert(0);

        // 2. Scene cuts are mandatory
        for &cut in scene_cut_indices {
            if cut < total_frames && added_indices.insert(cut) {
                keyframes.push(KeyframeEntry {
                    frame_index: cut,
                    timestamp_sec: cut as f64 / fps,
                    reason: KeyframeSelectionReason::SceneCut,
                    score: 0.95,
                });
            }
        }

        // 3. Motion peaks (if Balanced or Quality mode)
        if quality_mode != QualityMode::Economy {
            for &peak in motion_peaks {
                if peak < total_frames && added_indices.insert(peak) {
                    keyframes.push(KeyframeEntry {
                        frame_index: peak,
                        timestamp_sec: peak as f64 / fps,
                        reason: KeyframeSelectionReason::HighMotionPeak,
                        score: 0.85,
                    });
                }
            }
        }

        // 4. Periodic anchors for temporal stability
        let mut cur = base_stride;
        while cur < total_frames {
            if added_indices.insert(cur) {
                keyframes.push(KeyframeEntry {
                    frame_index: cur,
                    timestamp_sec: cur as f64 / fps,
                    reason: KeyframeSelectionReason::PeriodicAnchor,
                    score: 0.70,
                });
            }
            cur += base_stride;
        }

        // 5. Final Frame
        let last_idx = total_frames - 1;
        if added_indices.insert(last_idx) {
            keyframes.push(KeyframeEntry {
                frame_index: last_idx,
                timestamp_sec: last_idx as f64 / fps,
                reason: KeyframeSelectionReason::FinalFrame,
                score: 0.80,
            });
        }

        keyframes.sort_by_key(|k| k.frame_index);
        let count = keyframes.len();
        let density = (count as f64 / total_frames as f64) * 100.0;
        let reduction = total_frames as f64 / count.max(1) as f64;

        KeyframePlan {
            total_source_frames: total_frames,
            selected_keyframes: keyframes,
            keyframe_count: count,
            keyframe_density_pct: density,
            reduction_ratio: reduction,
            quality_mode,
            intent,
        }
    }
}
