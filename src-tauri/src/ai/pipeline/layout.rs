use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Supported channel ordering for image tensors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChannelOrder {
    Rgb,
    Bgr,
    Rgba,
    Gray,
}

impl Default for ChannelOrder {
    fn default() -> Self {
        ChannelOrder::Rgb
    }
}

impl ChannelOrder {
    pub fn channels(&self) -> u32 {
        match self {
            ChannelOrder::Gray => 1,
            ChannelOrder::Rgb | ChannelOrder::Bgr => 3,
            ChannelOrder::Rgba => 4,
        }
    }
}

/// Supported tensor layouts for neural network models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TensorLayout {
    Nhwc,
    Nchw,
}

impl Default for TensorLayout {
    fn default() -> Self {
        TensorLayout::Nchw
    }
}

/// Converts raw pixel byte buffer from source channel order to destination channel order.
pub fn convert_channel_order(
    pixels: &[u8],
    src: ChannelOrder,
    dst: ChannelOrder,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, AppError> {
    if src == dst {
        return Ok(pixels.to_vec());
    }

    let src_channels = src.channels() as usize;
    let dst_channels = dst.channels() as usize;

    let num_pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| AppError::invalid_input("Image dimension overflow"))?;

    let expected_src_len = num_pixels
        .checked_mul(src_channels)
        .ok_or_else(|| AppError::invalid_input("Source buffer length overflow"))?;

    if pixels.len() != expected_src_len {
        return Err(AppError::invalid_input(format!(
            "Input buffer length mismatch: expected {}, got {}",
            expected_src_len,
            pixels.len()
        )));
    }

    let expected_dst_len = num_pixels
        .checked_mul(dst_channels)
        .ok_or_else(|| AppError::invalid_input("Dest buffer length overflow"))?;

    let mut out = Vec::with_capacity(expected_dst_len);

    match (src, dst) {
        (ChannelOrder::Rgb, ChannelOrder::Bgr) => {
            for chunk in pixels.chunks_exact(3) {
                out.push(chunk[2]); // B
                out.push(chunk[1]); // G
                out.push(chunk[0]); // R
            }
        }
        (ChannelOrder::Bgr, ChannelOrder::Rgb) => {
            for chunk in pixels.chunks_exact(3) {
                out.push(chunk[2]); // R
                out.push(chunk[1]); // G
                out.push(chunk[0]); // B
            }
        }
        (ChannelOrder::Rgb, ChannelOrder::Rgba) => {
            for chunk in pixels.chunks_exact(3) {
                out.push(chunk[0]);
                out.push(chunk[1]);
                out.push(chunk[2]);
                out.push(255);
            }
        }
        (ChannelOrder::Rgba, ChannelOrder::Rgb) => {
            for chunk in pixels.chunks_exact(4) {
                out.push(chunk[0]);
                out.push(chunk[1]);
                out.push(chunk[2]);
            }
        }
        (ChannelOrder::Rgb, ChannelOrder::Gray) => {
            for chunk in pixels.chunks_exact(3) {
                let luma =
                    (0.299 * chunk[0] as f32 + 0.587 * chunk[1] as f32 + 0.114 * chunk[2] as f32)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                out.push(luma);
            }
        }
        (ChannelOrder::Gray, ChannelOrder::Rgb) => {
            for &g in pixels {
                out.push(g);
                out.push(g);
                out.push(g);
            }
        }
        (ChannelOrder::Gray, ChannelOrder::Bgr) => {
            for &g in pixels {
                out.push(g);
                out.push(g);
                out.push(g);
            }
        }
        _ => {
            return Err(AppError::invalid_input(format!(
                "Unsupported channel conversion: {:?} -> {:?}",
                src, dst
            )));
        }
    }

    Ok(out)
}

/// Transposes interleaved normalized HWC Float32 data into the target TensorLayout (NHWC or NCHW).
pub fn reorder_and_transpose_to_tensor(
    hwc_data: &[f32],
    width: u32,
    height: u32,
    channels: u32,
    layout: TensorLayout,
    batch_size: u32,
) -> Result<(Vec<u64>, Vec<f32>), AppError> {
    if width == 0 || height == 0 || channels == 0 || batch_size == 0 {
        return Err(AppError::invalid_input(
            "Dimensions and batch size must be non-zero",
        ));
    }

    let hw = (height as usize)
        .checked_mul(width as usize)
        .ok_or_else(|| AppError::invalid_input("Height * Width overflow"))?;

    let hwc_len = hw
        .checked_mul(channels as usize)
        .ok_or_else(|| AppError::invalid_input("HWC buffer size overflow"))?;

    if hwc_data.len() != hwc_len {
        return Err(AppError::invalid_input(format!(
            "HWC buffer length mismatch: expected {}, got {}",
            hwc_len,
            hwc_data.len()
        )));
    }

    let single_batch_len = hwc_len;
    let total_len = single_batch_len
        .checked_mul(batch_size as usize)
        .ok_or_else(|| AppError::invalid_input("Batch buffer size overflow"))?;

    match layout {
        TensorLayout::Nhwc => {
            let shape = vec![
                batch_size as u64,
                height as u64,
                width as u64,
                channels as u64,
            ];
            let mut tensor_data = Vec::with_capacity(total_len);
            for _ in 0..batch_size {
                tensor_data.extend_from_slice(hwc_data);
            }
            Ok((shape, tensor_data))
        }
        TensorLayout::Nchw => {
            let shape = vec![
                batch_size as u64,
                channels as u64,
                height as u64,
                width as u64,
            ];
            let mut single_nchw = vec![0.0f32; single_batch_len];

            let h_usize = height as usize;
            let w_usize = width as usize;
            let c_usize = channels as usize;

            for h in 0..h_usize {
                for w in 0..w_usize {
                    for c in 0..c_usize {
                        let src_idx = h * (w_usize * c_usize) + w * c_usize + c;
                        let dst_idx = c * (h_usize * w_usize) + h * w_usize + w;
                        single_nchw[dst_idx] = hwc_data[src_idx];
                    }
                }
            }

            let mut tensor_data = Vec::with_capacity(total_len);
            for _ in 0..batch_size {
                tensor_data.extend_from_slice(&single_nchw);
            }
            Ok((shape, tensor_data))
        }
    }
}
