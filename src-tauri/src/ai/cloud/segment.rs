use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSegment {
    pub segment_index: usize,
    pub start_sec: f64,
    pub end_sec: f64,
    pub duration_sec: f64,
    pub source_video: PathBuf,
    pub prompt: String,
    pub reference_image: Option<PathBuf>,
    pub output_path: Option<PathBuf>,
}

pub struct SegmentPlanner;

impl SegmentPlanner {
    pub fn plan_segments(
        source_video: &PathBuf,
        total_duration_sec: f64,
        segment_duration_sec: f64,
        prompt: &str,
        reference_image: Option<&PathBuf>,
    ) -> Vec<VideoSegment> {
        let seg_len = if segment_duration_sec <= 0.0 {
            6.0
        } else {
            segment_duration_sec
        };
        let mut segments = Vec::new();
        let mut cur_start = 0.0;
        let mut idx = 0;

        while cur_start < total_duration_sec {
            let cur_end = (cur_start + seg_len).min(total_duration_sec);
            let dur = cur_end - cur_start;
            if dur > 0.1 {
                segments.push(VideoSegment {
                    segment_index: idx,
                    start_sec: cur_start,
                    end_sec: cur_end,
                    duration_sec: dur,
                    source_video: source_video.clone(),
                    prompt: prompt.to_string(),
                    reference_image: reference_image.cloned(),
                    output_path: None,
                });
                idx += 1;
            }
            cur_start = cur_end;
        }

        if segments.is_empty() {
            segments.push(VideoSegment {
                segment_index: 0,
                start_sec: 0.0,
                end_sec: total_duration_sec,
                duration_sec: total_duration_sec,
                source_video: source_video.clone(),
                prompt: prompt.to_string(),
                reference_image: reference_image.cloned(),
                output_path: None,
            });
        }

        segments
    }
}
