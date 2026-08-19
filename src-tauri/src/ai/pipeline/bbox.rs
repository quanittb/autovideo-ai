use serde::{Deserialize, Serialize};

use crate::ai::pipeline::preprocess::{CropMetadata, LetterboxTransform, TransformMetadata};
use crate::ai::pipeline::resize::ResizeMetadata;

/// Supported coordinate representation formats for bounding boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CoordinateFormat {
    Xyxy,
    Xywh,
}

/// Generic bounding box primitive with confidence and class label.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundingBox {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub confidence: f32,
    pub class_id: u32,
}

impl BoundingBox {
    /// Creates a new BoundingBox in XYXY format.
    pub fn new_xyxy(x1: f32, y1: f32, x2: f32, y2: f32, confidence: f32, class_id: u32) -> Self {
        Self {
            x1: x1.min(x2),
            y1: y1.min(y2),
            x2: x1.max(x2),
            y2: y1.max(y2),
            confidence,
            class_id,
        }
    }

    /// Creates a new BoundingBox from XYWH (center x, center y, width, height) format.
    pub fn from_xywh(cx: f32, cy: f32, w: f32, h: f32, confidence: f32, class_id: u32) -> Self {
        let (x1, y1, x2, y2) = xywh_to_xyxy(cx, cy, w, h);
        Self::new_xyxy(x1, y1, x2, y2, confidence, class_id)
    }

    /// Converts this bounding box to (cx, cy, width, height).
    pub fn to_xywh(&self) -> (f32, f32, f32, f32) {
        xyxy_to_xywh(self.x1, self.y1, self.x2, self.y2)
    }

    /// Reverses letterbox padding and scaling to restore bounding box to original frame coordinates.
    pub fn restore_from_letterbox(&self, transform: &LetterboxTransform) -> Self {
        let unpad_x1 = self.x1 - transform.pad_left as f32;
        let unpad_y1 = self.y1 - transform.pad_top as f32;
        let unpad_x2 = self.x2 - transform.pad_left as f32;
        let unpad_y2 = self.y2 - transform.pad_top as f32;

        let scale_x = if transform.scale_x > 0.0 {
            transform.scale_x
        } else {
            1.0
        };
        let scale_y = if transform.scale_y > 0.0 {
            transform.scale_y
        } else {
            1.0
        };

        let orig_x1 = (unpad_x1 / scale_x).clamp(0.0, transform.original_width as f32);
        let orig_y1 = (unpad_y1 / scale_y).clamp(0.0, transform.original_height as f32);
        let orig_x2 = (unpad_x2 / scale_x).clamp(0.0, transform.original_width as f32);
        let orig_y2 = (unpad_y2 / scale_y).clamp(0.0, transform.original_height as f32);

        Self::new_xyxy(
            orig_x1,
            orig_y1,
            orig_x2,
            orig_y2,
            self.confidence,
            self.class_id,
        )
    }

    /// Reverses crop offsets to restore bounding box to original frame coordinates.
    pub fn restore_from_crop(&self, crop: &CropMetadata) -> Self {
        let orig_x1 = (self.x1 + crop.offset_x as f32).clamp(0.0, crop.original_width as f32);
        let orig_y1 = (self.y1 + crop.offset_y as f32).clamp(0.0, crop.original_height as f32);
        let orig_x2 = (self.x2 + crop.offset_x as f32).clamp(0.0, crop.original_width as f32);
        let orig_y2 = (self.y2 + crop.offset_y as f32).clamp(0.0, crop.original_height as f32);

        Self::new_xyxy(
            orig_x1,
            orig_y1,
            orig_x2,
            orig_y2,
            self.confidence,
            self.class_id,
        )
    }

    /// Reverses direct resizing scaling to restore bounding box to original frame coordinates.
    pub fn restore_from_resize(&self, resize: &ResizeMetadata) -> Self {
        let scale_x = if resize.scale_x > 0.0 {
            resize.scale_x
        } else {
            1.0
        };
        let scale_y = if resize.scale_y > 0.0 {
            resize.scale_y
        } else {
            1.0
        };

        let orig_x1 = (self.x1 / scale_x).clamp(0.0, resize.source_width as f32);
        let orig_y1 = (self.y1 / scale_y).clamp(0.0, resize.source_height as f32);
        let orig_x2 = (self.x2 / scale_x).clamp(0.0, resize.source_width as f32);
        let orig_y2 = (self.y2 / scale_y).clamp(0.0, resize.source_height as f32);

        Self::new_xyxy(
            orig_x1,
            orig_y1,
            orig_x2,
            orig_y2,
            self.confidence,
            self.class_id,
        )
    }

    /// Reverses all transformations recorded in TransformMetadata.
    pub fn restore_coordinates(&self, transform: &TransformMetadata) -> Self {
        let mut bbox = *self;
        if let Some(lb) = &transform.letterbox {
            bbox = bbox.restore_from_letterbox(lb);
        }
        if let Some(crop) = &transform.crop {
            bbox = bbox.restore_from_crop(crop);
        }
        if let Some(resize) = &transform.resize {
            bbox = bbox.restore_from_resize(resize);
        }
        bbox
    }
}

/// Converts (x1, y1, x2, y2) to (center_x, center_y, width, height).
pub fn xyxy_to_xywh(x1: f32, y1: f32, x2: f32, y2: f32) -> (f32, f32, f32, f32) {
    let w = (x2 - x1).abs();
    let h = (y2 - y1).abs();
    let cx = x1.min(x2) + w / 2.0;
    let cy = y1.min(y2) + h / 2.0;
    (cx, cy, w, h)
}

/// Converts (center_x, center_y, width, height) to (x1, y1, x2, y2).
pub fn xywh_to_xyxy(cx: f32, cy: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
    let half_w = w / 2.0;
    let half_h = h / 2.0;
    (cx - half_w, cy - half_h, cx + half_w, cy + half_h)
}
