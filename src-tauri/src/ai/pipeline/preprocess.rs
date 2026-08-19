use serde::{Deserialize, Serialize};

use crate::ai::onnx::AiTensorInput;
use crate::ai::pipeline::image::{ImageFrame, PixelFormat};
use crate::ai::pipeline::layout::{
    convert_channel_order, reorder_and_transpose_to_tensor, ChannelOrder, TensorLayout,
};
use crate::ai::pipeline::normalize::{normalize_pixels, NormalizationConfig};
use crate::ai::pipeline::resize::{resize_image, ResizeConfig, ResizeFilter, ResizeMetadata};
use crate::ai::tensor::TensorDataType;
use crate::error::AppError;

/// Configuration for aspect-ratio-preserving letterbox padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LetterboxConfig {
    pub target_width: u32,
    pub target_height: u32,
    pub pad_value: [u8; 3],
    pub filter: ResizeFilter,
}

impl Default for LetterboxConfig {
    fn default() -> Self {
        Self {
            target_width: 640,
            target_height: 640,
            pad_value: [114, 114, 114],
            filter: ResizeFilter::Bilinear,
        }
    }
}

/// Metadata for reversing letterbox coordinate transformations.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LetterboxTransform {
    pub original_width: u32,
    pub original_height: u32,
    pub resized_width: u32,
    pub resized_height: u32,
    pub pad_left: u32,
    pub pad_top: u32,
    pub scale_x: f32,
    pub scale_y: f32,
}

/// Configuration for center cropping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropConfig {
    pub target_width: u32,
    pub target_height: u32,
}

/// Metadata for reversing crop coordinate transformations.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropMetadata {
    pub original_width: u32,
    pub original_height: u32,
    pub cropped_width: u32,
    pub cropped_height: u32,
    pub offset_x: u32,
    pub offset_y: u32,
}

/// Combined metadata covering all spatial transformations applied during preprocessing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformMetadata {
    pub letterbox: Option<LetterboxTransform>,
    pub crop: Option<CropMetadata>,
    pub resize: Option<ResizeMetadata>,
}

/// Comprehensive preprocessing configuration for neural pipeline execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreprocessConfig {
    pub target_width: u32,
    pub target_height: u32,
    pub resize_filter: ResizeFilter,
    pub letterbox: bool,
    pub letterbox_pad: [u8; 3],
    pub center_crop: bool,
    pub crop_width: Option<u32>,
    pub crop_height: Option<u32>,
    pub channel_order: ChannelOrder,
    pub normalization: NormalizationConfig,
    pub layout: TensorLayout,
    pub batch_size: u32,
}

impl Default for PreprocessConfig {
    fn default() -> Self {
        Self {
            target_width: 640,
            target_height: 640,
            resize_filter: ResizeFilter::Bilinear,
            letterbox: true,
            letterbox_pad: [114, 114, 114],
            center_crop: false,
            crop_width: None,
            crop_height: None,
            channel_order: ChannelOrder::Rgb,
            normalization: NormalizationConfig::zero_to_one(),
            layout: TensorLayout::Nchw,
            batch_size: 1,
        }
    }
}

/// Result produced by the preprocessing pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreprocessResult {
    pub tensor: AiTensorInput,
    pub transform: TransformMetadata,
    pub source_width: u32,
    pub source_height: u32,
    pub processed_width: u32,
    pub processed_height: u32,
}

/// Resizes an image preserving aspect ratio and pads remaining canvas area.
pub fn apply_letterbox(
    frame: &ImageFrame,
    config: &LetterboxConfig,
) -> Result<(ImageFrame, LetterboxTransform), AppError> {
    if config.target_width == 0 || config.target_height == 0 {
        return Err(AppError::invalid_input(
            "Letterbox dimensions must be non-zero",
        ));
    }

    let orig_w = frame.width as f32;
    let orig_h = frame.height as f32;
    let target_w = config.target_width as f32;
    let target_h = config.target_height as f32;

    let scale = f32::min(target_w / orig_w, target_h / orig_h);
    let new_w = ((orig_w * scale).round() as u32)
        .max(1)
        .min(config.target_width);
    let new_h = ((orig_h * scale).round() as u32)
        .max(1)
        .min(config.target_height);

    let (scaled_frame, _) = resize_image(
        frame,
        &ResizeConfig {
            target_width: new_w,
            target_height: new_h,
            filter: config.filter,
        },
    )?;

    let pad_left = (config.target_width - new_w) / 2;
    let pad_top = (config.target_height - new_h) / 2;

    let target_pixel_count = (config.target_width as usize)
        .checked_mul(config.target_height as usize)
        .ok_or_else(|| AppError::invalid_input("Canvas size overflow"))?;

    let channels = frame.channels as usize;
    let mut canvas_data = Vec::with_capacity(target_pixel_count * channels);

    match frame.format {
        PixelFormat::Rgb8 => {
            for _ in 0..target_pixel_count {
                canvas_data.extend_from_slice(&config.pad_value);
            }
            for y in 0..new_h {
                for x in 0..new_w {
                    let src_idx = (y as usize * new_w as usize + x as usize) * 3;
                    let dst_x = pad_left + x;
                    let dst_y = pad_top + y;
                    let dst_idx =
                        (dst_y as usize * config.target_width as usize + dst_x as usize) * 3;
                    canvas_data[dst_idx..dst_idx + 3]
                        .copy_from_slice(&scaled_frame.data[src_idx..src_idx + 3]);
                }
            }
        }
        PixelFormat::Rgba8 => {
            for _ in 0..target_pixel_count {
                canvas_data.extend_from_slice(&[
                    config.pad_value[0],
                    config.pad_value[1],
                    config.pad_value[2],
                    255,
                ]);
            }
            for y in 0..new_h {
                for x in 0..new_w {
                    let src_idx = (y as usize * new_w as usize + x as usize) * 4;
                    let dst_x = pad_left + x;
                    let dst_y = pad_top + y;
                    let dst_idx =
                        (dst_y as usize * config.target_width as usize + dst_x as usize) * 4;
                    canvas_data[dst_idx..dst_idx + 4]
                        .copy_from_slice(&scaled_frame.data[src_idx..src_idx + 4]);
                }
            }
        }
        PixelFormat::Gray8 => {
            let gray_pad = (0.299 * config.pad_value[0] as f32
                + 0.587 * config.pad_value[1] as f32
                + 0.114 * config.pad_value[2] as f32)
                .round() as u8;
            canvas_data.resize(target_pixel_count, gray_pad);
            for y in 0..new_h {
                for x in 0..new_w {
                    let src_idx = y as usize * new_w as usize + x as usize;
                    let dst_x = pad_left + x;
                    let dst_y = pad_top + y;
                    let dst_idx = dst_y as usize * config.target_width as usize + dst_x as usize;
                    canvas_data[dst_idx] = scaled_frame.data[src_idx];
                }
            }
        }
    }

    let letterboxed_frame = ImageFrame::new(
        config.target_width,
        config.target_height,
        frame.format,
        canvas_data,
    )?;

    let transform = LetterboxTransform {
        original_width: frame.width,
        original_height: frame.height,
        resized_width: new_w,
        resized_height: new_h,
        pad_left,
        pad_top,
        scale_x: new_w as f32 / frame.width as f32,
        scale_y: new_h as f32 / frame.height as f32,
    };

    Ok((letterboxed_frame, transform))
}

/// Applies a center crop to an ImageFrame.
pub fn apply_center_crop(
    frame: &ImageFrame,
    config: &CropConfig,
) -> Result<(ImageFrame, CropMetadata), AppError> {
    if config.target_width == 0 || config.target_height == 0 {
        return Err(AppError::invalid_input("Crop dimensions must be non-zero"));
    }

    if config.target_width > frame.width || config.target_height > frame.height {
        return Err(AppError::invalid_input(format!(
            "Crop dimensions {}x{} exceed image dimensions {}x{}",
            config.target_width, config.target_height, frame.width, frame.height
        )));
    }

    let offset_x = (frame.width - config.target_width) / 2;
    let offset_y = (frame.height - config.target_height) / 2;

    let channels = frame.channels as usize;
    let target_len = (config.target_width as usize)
        .checked_mul(config.target_height as usize)
        .and_then(|wh| wh.checked_mul(channels))
        .ok_or_else(|| AppError::invalid_input("Cropped buffer size overflow"))?;

    let mut cropped_data = Vec::with_capacity(target_len);

    for y in 0..config.target_height {
        let src_y = offset_y + y;
        let src_row_start = (src_y as usize * frame.width as usize + offset_x as usize) * channels;
        let src_row_end = src_row_start + config.target_width as usize * channels;
        cropped_data.extend_from_slice(&frame.data[src_row_start..src_row_end]);
    }

    let cropped_frame = ImageFrame::new(
        config.target_width,
        config.target_height,
        frame.format,
        cropped_data,
    )?;

    let meta = CropMetadata {
        original_width: frame.width,
        original_height: frame.height,
        cropped_width: config.target_width,
        cropped_height: config.target_height,
        offset_x,
        offset_y,
    };

    Ok((cropped_frame, meta))
}

/// Executes the full, composable preprocessing pipeline on an ImageFrame.
pub fn preprocess_image(
    frame: &ImageFrame,
    config: &PreprocessConfig,
    tensor_name: &str,
) -> Result<PreprocessResult, AppError> {
    let source_width = frame.width;
    let source_height = frame.height;
    let mut current_frame = frame.clone();
    let mut transform_meta = TransformMetadata::default();

    // 1. Center crop if configured
    if config.center_crop {
        let crop_w = config.crop_width.unwrap_or(config.target_width);
        let crop_h = config.crop_height.unwrap_or(config.target_height);
        let (cropped, crop_meta) = apply_center_crop(
            &current_frame,
            &CropConfig {
                target_width: crop_w,
                target_height: crop_h,
            },
        )?;
        current_frame = cropped;
        transform_meta.crop = Some(crop_meta);
    }

    // 2. Letterbox or Direct Resize
    if config.letterbox {
        let (letterboxed, lb_meta) = apply_letterbox(
            &current_frame,
            &LetterboxConfig {
                target_width: config.target_width,
                target_height: config.target_height,
                pad_value: config.letterbox_pad,
                filter: config.resize_filter,
            },
        )?;
        current_frame = letterboxed;
        transform_meta.letterbox = Some(lb_meta);
    } else if current_frame.width != config.target_width
        || current_frame.height != config.target_height
    {
        let (resized, resize_meta) = resize_image(
            &current_frame,
            &ResizeConfig {
                target_width: config.target_width,
                target_height: config.target_height,
                filter: config.resize_filter,
            },
        )?;
        current_frame = resized;
        transform_meta.resize = Some(resize_meta);
    }

    let processed_width = current_frame.width;
    let processed_height = current_frame.height;

    // 3. Channel conversion
    let src_channel_order = match current_frame.format {
        PixelFormat::Rgb8 => ChannelOrder::Rgb,
        PixelFormat::Rgba8 => ChannelOrder::Rgba,
        PixelFormat::Gray8 => ChannelOrder::Gray,
    };

    let reordered_pixels = convert_channel_order(
        &current_frame.data,
        src_channel_order,
        config.channel_order,
        current_frame.width,
        current_frame.height,
    )?;

    // 4. Pixel normalization to Float32
    let normalized = normalize_pixels(
        &reordered_pixels,
        &config.normalization,
        config.channel_order.channels() as usize,
    )?;

    // 5. Layout Transpose (NHWC or NCHW)
    let (tensor_shape, tensor_data) = reorder_and_transpose_to_tensor(
        &normalized,
        current_frame.width,
        current_frame.height,
        config.channel_order.channels(),
        config.layout,
        config.batch_size,
    )?;

    let tensor = AiTensorInput {
        name: tensor_name.to_string(),
        data_type: TensorDataType::Float32,
        shape: tensor_shape,
        data_f32: Some(tensor_data),
        data_i32: None,
        data_i64: None,
        data_u8: None,
    };

    Ok(PreprocessResult {
        tensor,
        transform: transform_meta,
        source_width,
        source_height,
        processed_width,
        processed_height,
    })
}
