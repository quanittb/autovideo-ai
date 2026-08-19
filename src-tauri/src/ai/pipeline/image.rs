use image::{DynamicImage, GenericImageView, ImageBuffer, ImageReader, Rgb, Rgba};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::Path;

use crate::error::AppError;

/// Supported internal pixel formats for raw image buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PixelFormat {
    Gray8,
    Rgb8,
    Rgba8,
}

impl PixelFormat {
    pub fn channels(&self) -> u8 {
        match self {
            PixelFormat::Gray8 => 1,
            PixelFormat::Rgb8 => 3,
            PixelFormat::Rgba8 => 4,
        }
    }
}

/// A safe, validated in-memory image frame with checked buffer dimensions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageFrame {
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub data: Vec<u8>,
    pub format: PixelFormat,
}

impl ImageFrame {
    /// Creates a new ImageFrame with strict bounds validation and checked arithmetic.
    pub fn new(
        width: u32,
        height: u32,
        format: PixelFormat,
        data: Vec<u8>,
    ) -> Result<Self, AppError> {
        if width == 0 || height == 0 {
            return Err(AppError::invalid_input(format!(
                "Invalid zero dimension: {}x{}",
                width, height
            )));
        }

        let channels = format.channels();
        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|wh| wh.checked_mul(channels as usize))
            .ok_or_else(|| {
                AppError::invalid_input(format!(
                    "Image buffer dimensions overflow: {}x{}x{}",
                    width, height, channels
                ))
            })?;

        if data.len() != expected_len {
            return Err(AppError::invalid_input(format!(
                "Image buffer length mismatch: expected {} bytes ({}x{}x{}), got {} bytes",
                expected_len,
                width,
                height,
                channels,
                data.len()
            )));
        }

        Ok(Self {
            width,
            height,
            channels,
            data,
            format,
        })
    }

    /// Decodes an image file (PNG/JPEG) from disk and normalizes it to RGB8 by default.
    pub fn decode_from_file<P: AsRef<Path>>(path: P) -> Result<Self, AppError> {
        let p = path.as_ref();
        if !p.exists() {
            return Err(AppError::file_not_found(p.to_string_lossy()));
        }

        let reader = ImageReader::open(p).map_err(|e| {
            AppError::media_metadata_failed(p.to_string_lossy(), format!("Cannot open file: {}", e))
        })?;

        let dynamic_img = reader.decode().map_err(|e| {
            AppError::media_metadata_failed(p.to_string_lossy(), format!("Decode error: {}", e))
        })?;

        Self::from_dynamic_image(dynamic_img)
    }

    /// Decodes image bytes (PNG/JPEG) from memory and converts to RGB8.
    pub fn decode_from_bytes(bytes: &[u8]) -> Result<Self, AppError> {
        if bytes.is_empty() {
            return Err(AppError::invalid_input("Cannot decode empty image bytes"));
        }

        let reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| AppError::media_metadata_failed("memory_buffer", e.to_string()))?;

        let dynamic_img = reader.decode().map_err(|e| {
            AppError::media_metadata_failed("memory_buffer", format!("Decode error: {}", e))
        })?;

        Self::from_dynamic_image(dynamic_img)
    }

    /// Converts a DynamicImage into a validated RGB8 ImageFrame.
    pub fn from_dynamic_image(img: DynamicImage) -> Result<Self, AppError> {
        let (width, height) = img.dimensions();
        let rgb_img = img.to_rgb8();
        let data = rgb_img.into_raw();
        Self::new(width, height, PixelFormat::Rgb8, data)
    }

    /// Converts the current ImageFrame to RGB8 format.
    pub fn to_rgb8(&self) -> Result<Self, AppError> {
        match self.format {
            PixelFormat::Rgb8 => Ok(self.clone()),
            PixelFormat::Gray8 => {
                let pixel_count = (self.width as usize)
                    .checked_mul(self.height as usize)
                    .ok_or_else(|| AppError::invalid_input("Dimension overflow"))?;
                let mut rgb_data = Vec::with_capacity(pixel_count * 3);
                for &g in &self.data {
                    rgb_data.push(g);
                    rgb_data.push(g);
                    rgb_data.push(g);
                }
                Self::new(self.width, self.height, PixelFormat::Rgb8, rgb_data)
            }
            PixelFormat::Rgba8 => {
                let pixel_count = (self.width as usize)
                    .checked_mul(self.height as usize)
                    .ok_or_else(|| AppError::invalid_input("Dimension overflow"))?;
                let mut rgb_data = Vec::with_capacity(pixel_count * 3);
                for chunk in self.data.chunks_exact(4) {
                    rgb_data.push(chunk[0]);
                    rgb_data.push(chunk[1]);
                    rgb_data.push(chunk[2]);
                }
                Self::new(self.width, self.height, PixelFormat::Rgb8, rgb_data)
            }
        }
    }

    /// Converts the current ImageFrame to RGBA8 format (with full alpha 255 if from RGB/Gray).
    pub fn to_rgba8(&self) -> Result<Self, AppError> {
        match self.format {
            PixelFormat::Rgba8 => Ok(self.clone()),
            PixelFormat::Gray8 => {
                let pixel_count = (self.width as usize)
                    .checked_mul(self.height as usize)
                    .ok_or_else(|| AppError::invalid_input("Dimension overflow"))?;
                let mut rgba_data = Vec::with_capacity(pixel_count * 4);
                for &g in &self.data {
                    rgba_data.push(g);
                    rgba_data.push(g);
                    rgba_data.push(g);
                    rgba_data.push(255);
                }
                Self::new(self.width, self.height, PixelFormat::Rgba8, rgba_data)
            }
            PixelFormat::Rgb8 => {
                let pixel_count = (self.width as usize)
                    .checked_mul(self.height as usize)
                    .ok_or_else(|| AppError::invalid_input("Dimension overflow"))?;
                let mut rgba_data = Vec::with_capacity(pixel_count * 4);
                for chunk in self.data.chunks_exact(3) {
                    rgba_data.push(chunk[0]);
                    rgba_data.push(chunk[1]);
                    rgba_data.push(chunk[2]);
                    rgba_data.push(255);
                }
                Self::new(self.width, self.height, PixelFormat::Rgba8, rgba_data)
            }
        }
    }

    /// Converts the current ImageFrame to grayscale (standard luminance weights: 0.299R + 0.587G + 0.114B).
    pub fn to_grayscale(&self) -> Result<Self, AppError> {
        match self.format {
            PixelFormat::Gray8 => Ok(self.clone()),
            PixelFormat::Rgb8 => {
                let pixel_count = (self.width as usize)
                    .checked_mul(self.height as usize)
                    .ok_or_else(|| AppError::invalid_input("Dimension overflow"))?;
                let mut gray_data = Vec::with_capacity(pixel_count);
                for chunk in self.data.chunks_exact(3) {
                    let r = chunk[0] as f32;
                    let g = chunk[1] as f32;
                    let b = chunk[2] as f32;
                    let luma = (0.299 * r + 0.587 * g + 0.114 * b)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    gray_data.push(luma);
                }
                Self::new(self.width, self.height, PixelFormat::Gray8, gray_data)
            }
            PixelFormat::Rgba8 => {
                let pixel_count = (self.width as usize)
                    .checked_mul(self.height as usize)
                    .ok_or_else(|| AppError::invalid_input("Dimension overflow"))?;
                let mut gray_data = Vec::with_capacity(pixel_count);
                for chunk in self.data.chunks_exact(4) {
                    let r = chunk[0] as f32;
                    let g = chunk[1] as f32;
                    let b = chunk[2] as f32;
                    let luma = (0.299 * r + 0.587 * g + 0.114 * b)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    gray_data.push(luma);
                }
                Self::new(self.width, self.height, PixelFormat::Gray8, gray_data)
            }
        }
    }

    /// Encodes this image frame as a real PNG file.
    pub fn encode_to_png<P: AsRef<Path>>(&self, path: P) -> Result<(), AppError> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match self.format {
            PixelFormat::Rgb8 => {
                let buffer: ImageBuffer<Rgb<u8>, _> =
                    ImageBuffer::from_raw(self.width, self.height, self.data.clone()).ok_or_else(
                        || AppError::process_failed("Failed to construct RGB image buffer for PNG"),
                    )?;
                buffer
                    .save_with_format(p, image::ImageFormat::Png)
                    .map_err(|e| AppError::storage_write_failed(p.to_string_lossy(), e.to_string()))
            }
            PixelFormat::Rgba8 => {
                let buffer: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(
                    self.width,
                    self.height,
                    self.data.clone(),
                )
                .ok_or_else(|| {
                    AppError::process_failed("Failed to construct RGBA image buffer for PNG")
                })?;
                buffer
                    .save_with_format(p, image::ImageFormat::Png)
                    .map_err(|e| AppError::storage_write_failed(p.to_string_lossy(), e.to_string()))
            }
            PixelFormat::Gray8 => {
                let buffer: ImageBuffer<image::Luma<u8>, _> = ImageBuffer::from_raw(
                    self.width,
                    self.height,
                    self.data.clone(),
                )
                .ok_or_else(|| {
                    AppError::process_failed("Failed to construct Gray image buffer for PNG")
                })?;
                buffer
                    .save_with_format(p, image::ImageFormat::Png)
                    .map_err(|e| AppError::storage_write_failed(p.to_string_lossy(), e.to_string()))
            }
        }
    }

    /// Encodes this image frame into in-memory PNG bytes.
    pub fn encode_to_png_bytes(&self) -> Result<Vec<u8>, AppError> {
        let mut bytes = Vec::new();
        let mut cursor = Cursor::new(&mut bytes);
        let rgb_frame = self.to_rgb8()?;
        let buffer: ImageBuffer<Rgb<u8>, _> =
            ImageBuffer::from_raw(rgb_frame.width, rgb_frame.height, rgb_frame.data)
                .ok_or_else(|| AppError::process_failed("Failed to construct RGB buffer"))?;
        buffer
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| AppError::process_failed(format!("Failed to encode PNG: {}", e)))?;
        Ok(bytes)
    }

    /// Encodes this image frame as a real JPEG file.
    pub fn encode_to_jpeg<P: AsRef<Path>>(&self, path: P) -> Result<(), AppError> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let bytes = self.encode_to_jpeg_bytes()?;
        std::fs::write(p, bytes)
            .map_err(|e| AppError::storage_write_failed(p.to_string_lossy(), e.to_string()))
    }

    /// Encodes this image frame into in-memory JPEG bytes.
    pub fn encode_to_jpeg_bytes(&self) -> Result<Vec<u8>, AppError> {
        let mut bytes = Vec::new();
        let mut cursor = Cursor::new(&mut bytes);
        let rgb_frame = self.to_rgb8()?;
        let buffer: ImageBuffer<Rgb<u8>, _> =
            ImageBuffer::from_raw(rgb_frame.width, rgb_frame.height, rgb_frame.data)
                .ok_or_else(|| AppError::process_failed("Failed to construct RGB buffer"))?;
        buffer
            .write_to(&mut cursor, image::ImageFormat::Jpeg)
            .map_err(|e| AppError::process_failed(format!("Failed to encode JPEG: {}", e)))?;
        Ok(bytes)
    }
}
