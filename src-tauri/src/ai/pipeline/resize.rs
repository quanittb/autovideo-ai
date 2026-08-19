use image::imageops::{resize, FilterType};
use image::{ImageBuffer, Rgb, Rgba};
use serde::{Deserialize, Serialize};

use crate::ai::pipeline::image::{ImageFrame, PixelFormat};
use crate::error::AppError;

/// Supported resize interpolation filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResizeFilter {
    Nearest,
    Bilinear,
    Bicubic,
}

impl Default for ResizeFilter {
    fn default() -> Self {
        ResizeFilter::Bilinear
    }
}

impl ResizeFilter {
    pub fn to_image_filter(&self) -> FilterType {
        match self {
            ResizeFilter::Nearest => FilterType::Nearest,
            ResizeFilter::Bilinear => FilterType::Triangle,
            ResizeFilter::Bicubic => FilterType::CatmullRom,
        }
    }
}

/// Configuration for exact image resizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeConfig {
    pub target_width: u32,
    pub target_height: u32,
    pub filter: ResizeFilter,
}

/// Metadata describing an applied resize transformation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeMetadata {
    pub source_width: u32,
    pub source_height: u32,
    pub target_width: u32,
    pub target_height: u32,
    pub scale_x: f32,
    pub scale_y: f32,
}

/// Resizes an ImageFrame to exact target dimensions using the specified filter.
pub fn resize_image(
    frame: &ImageFrame,
    config: &ResizeConfig,
) -> Result<(ImageFrame, ResizeMetadata), AppError> {
    if config.target_width == 0 || config.target_height == 0 {
        return Err(AppError::invalid_input(format!(
            "Invalid resize dimensions: {}x{}",
            config.target_width, config.target_height
        )));
    }

    if frame.width == config.target_width && frame.height == config.target_height {
        let meta = ResizeMetadata {
            source_width: frame.width,
            source_height: frame.height,
            target_width: config.target_width,
            target_height: config.target_height,
            scale_x: 1.0,
            scale_y: 1.0,
        };
        return Ok((frame.clone(), meta));
    }

    let filter = config.filter.to_image_filter();

    let resized_frame = match frame.format {
        PixelFormat::Rgb8 => {
            let src_buf: ImageBuffer<Rgb<u8>, _> =
                ImageBuffer::from_raw(frame.width, frame.height, frame.data.clone()).ok_or_else(
                    || AppError::process_failed("Failed to wrap RGB buffer for resize"),
                )?;
            let dst_buf = resize(&src_buf, config.target_width, config.target_height, filter);
            ImageFrame::new(
                config.target_width,
                config.target_height,
                PixelFormat::Rgb8,
                dst_buf.into_raw(),
            )?
        }
        PixelFormat::Rgba8 => {
            let src_buf: ImageBuffer<Rgba<u8>, _> =
                ImageBuffer::from_raw(frame.width, frame.height, frame.data.clone()).ok_or_else(
                    || AppError::process_failed("Failed to wrap RGBA buffer for resize"),
                )?;
            let dst_buf = resize(&src_buf, config.target_width, config.target_height, filter);
            ImageFrame::new(
                config.target_width,
                config.target_height,
                PixelFormat::Rgba8,
                dst_buf.into_raw(),
            )?
        }
        PixelFormat::Gray8 => {
            let src_buf: ImageBuffer<image::Luma<u8>, _> =
                ImageBuffer::from_raw(frame.width, frame.height, frame.data.clone()).ok_or_else(
                    || AppError::process_failed("Failed to wrap Gray buffer for resize"),
                )?;
            let dst_buf = resize(&src_buf, config.target_width, config.target_height, filter);
            ImageFrame::new(
                config.target_width,
                config.target_height,
                PixelFormat::Gray8,
                dst_buf.into_raw(),
            )?
        }
    };

    let meta = ResizeMetadata {
        source_width: frame.width,
        source_height: frame.height,
        target_width: config.target_width,
        target_height: config.target_height,
        scale_x: config.target_width as f32 / frame.width as f32,
        scale_y: config.target_height as f32 / frame.height as f32,
    };

    Ok((resized_frame, meta))
}
