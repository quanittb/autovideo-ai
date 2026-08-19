pub mod depth;
pub mod extractor;
pub mod models;
pub mod package;
pub mod pose;
pub mod segmentation;

pub use depth::{DepthExtractor, DepthExtractorConfig, DepthFrameResult};
pub use extractor::{ControlExtractionConfig, ControlExtractor};
pub use models::{
    ControlModelSpec, MODEL_ID_BIREFNET, MODEL_ID_DEPTH_ANYTHING_V2, MODEL_ID_DWPOSE,
};
pub use package::{ControlArtifactPaths, ControlExtractionReport, VideoControlPackage};
pub use pose::{Keypoint2D, PoseExtractor, PoseExtractorConfig, PoseFrameResult, BODY_LIMBS};
pub use segmentation::{
    SegmentationExtractor, SegmentationExtractorConfig, SegmentationFrameResult,
};
