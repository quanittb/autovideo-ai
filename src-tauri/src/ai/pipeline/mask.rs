use image::{ImageBuffer, Luma};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::Path;

use crate::ai::onnx::AiTensorOutput;
use crate::error::AppError;

/// A 2D single-channel mask representation containing continuous or binary Float32 probabilities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub data: Vec<f32>,
}

impl Mask {
    /// Creates a new Mask with dimensions validation.
    pub fn new(width: u32, height: u32, data: Vec<f32>) -> Result<Self, AppError> {
        if width == 0 || height == 0 {
            return Err(AppError::invalid_input("Mask dimensions must be non-zero"));
        }

        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| AppError::invalid_input("Mask dimension overflow"))?;

        if data.len() != expected_len {
            return Err(AppError::invalid_input(format!(
                "Mask buffer length mismatch: expected {}, got {}",
                expected_len,
                data.len()
            )));
        }

        for (i, &v) in data.iter().enumerate() {
            if !v.is_finite() {
                return Err(AppError::invalid_input(format!(
                    "Mask element at index {} is not finite: {}",
                    i, v
                )));
            }
        }

        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Applies binary thresholding: value >= threshold => 1.0, otherwise 0.0.
    pub fn apply_threshold(&self, threshold: f32) -> Self {
        let binary_data = self
            .data
            .iter()
            .map(|&v| if v >= threshold { 1.0 } else { 0.0 })
            .collect();

        Self {
            width: self.width,
            height: self.height,
            data: binary_data,
        }
    }

    /// Encodes this mask to a real grayscale PNG file on disk.
    pub fn mask_to_png<P: AsRef<Path>>(&self, path: P) -> Result<(), AppError> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let png_bytes = self.mask_to_png_bytes()?;
        std::fs::write(p, png_bytes)
            .map_err(|e| AppError::storage_write_failed(p.to_string_lossy(), e.to_string()))
    }

    /// Encodes this mask into in-memory PNG bytes.
    pub fn mask_to_png_bytes(&self) -> Result<Vec<u8>, AppError> {
        let raw_u8: Vec<u8> = self
            .data
            .iter()
            .map(|&v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect();

        let buffer: ImageBuffer<Luma<u8>, _> =
            ImageBuffer::from_raw(self.width, self.height, raw_u8).ok_or_else(|| {
                AppError::process_failed("Failed to build Luma image buffer for mask")
            })?;

        let mut png_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut png_bytes);
        buffer
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| AppError::process_failed(format!("Failed to encode mask PNG: {}", e)))?;

        Ok(png_bytes)
    }
}

/// Extracts a 2D Mask from an ONNX output tensor.
/// Supports shapes: [N, 1, H, W], [N, H, W, 1], [1, H, W], [H, W].
pub fn extract_mask_from_tensor(tensor: &AiTensorOutput) -> Result<Mask, AppError> {
    let data = tensor
        .data_f32
        .as_ref()
        .ok_or_else(|| AppError::invalid_input("Mask extraction requires Float32 tensor data"))?;

    let shape = &tensor.shape;

    let (height, width, offset, length) = match shape.as_slice() {
        // [H, W]
        [h, w] => (*h as u32, *w as u32, 0usize, (*h as usize) * (*w as usize)),
        // [1, H, W]
        [1, h, w] => (*h as u32, *w as u32, 0usize, (*h as usize) * (*w as usize)),
        // [N, 1, H, W] (extract batch index 0)
        [n, 1, h, w] if *n >= 1 => {
            let hw = (*h as usize) * (*w as usize);
            (*h as u32, *w as u32, 0usize, hw)
        }
        // [N, H, W, 1] (extract batch index 0)
        [n, h, w, 1] if *n >= 1 => {
            let hw = (*h as usize) * (*w as usize);
            (*h as u32, *w as u32, 0usize, hw)
        }
        _ => {
            return Err(AppError::invalid_input(format!(
                "Cannot extract 2D mask from tensor shape {:?}",
                shape
            )));
        }
    };

    if data.len() < offset + length {
        return Err(AppError::invalid_input(format!(
            "Tensor buffer too small for mask: expected at least {} elements, got {}",
            offset + length,
            data.len()
        )));
    }

    let mask_slice = data[offset..offset + length].to_vec();
    Mask::new(width, height, mask_slice)
}
