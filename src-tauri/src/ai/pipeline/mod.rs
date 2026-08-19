pub mod bbox;
pub mod image;
pub mod layout;
pub mod mask;
pub mod normalize;
pub mod postprocess;
pub mod preprocess;
pub mod resize;
pub mod service;
pub mod validate;

pub use bbox::{xywh_to_xyxy, xyxy_to_xywh, BoundingBox, CoordinateFormat};
pub use image::{ImageFrame, PixelFormat};
pub use layout::{
    convert_channel_order, reorder_and_transpose_to_tensor, ChannelOrder, TensorLayout,
};
pub use mask::{extract_mask_from_tensor, Mask};
pub use normalize::{normalize_pixels, NormalizationConfig, NormalizationMode};
pub use postprocess::{postprocess_outputs, PostprocessConfig, PostprocessResult};
pub use preprocess::{
    apply_center_crop, apply_letterbox, preprocess_image, CropConfig, CropMetadata,
    LetterboxConfig, LetterboxTransform, PreprocessConfig, PreprocessResult, TransformMetadata,
};
pub use resize::{resize_image, ResizeConfig, ResizeFilter, ResizeMetadata};
pub use service::{AiInferencePipeline, PipelineExecutionReport};
pub use validate::{validate_preprocess_against_model, PreprocessValidationResult};

use crate::error::AppError;
use std::path::Path;

/// Helper to generate a deterministic minimal ONNX graph for image tensors:
/// Shape: [1, 3, 2, 2], Operation: Y = X * 2 (Element-wise Mul with scalar 2.0).
pub fn generate_image_onnx_model(file_path: &Path) -> Result<(), AppError> {
    generate_image_onnx_model_with_weight(file_path, 2.0)
}

/// Helper to generate a deterministic minimal ONNX graph with configurable weight
pub fn generate_image_onnx_model_with_weight(
    file_path: &Path,
    weight: f32,
) -> Result<(), AppError> {
    let mut bytes = Vec::new();

    fn write_varint(buf: &mut Vec<u8>, mut val: u64) {
        while val >= 0x80 {
            buf.push(((val & 0x7F) | 0x80) as u8);
            val >>= 7;
        }
        buf.push((val & 0x7F) as u8);
    }

    fn write_tag(buf: &mut Vec<u8>, field_num: u32, wire_type: u8) {
        write_varint(buf, ((field_num as u64) << 3) | (wire_type as u64));
    }

    fn write_string_field(buf: &mut Vec<u8>, field_num: u32, s: &str) {
        write_tag(buf, field_num, 2);
        write_varint(buf, s.len() as u64);
        buf.extend_from_slice(s.as_bytes());
    }

    fn write_message_field(buf: &mut Vec<u8>, field_num: u32, msg: &[u8]) {
        write_tag(buf, field_num, 2);
        write_varint(buf, msg.len() as u64);
        buf.extend_from_slice(msg);
    }

    // 1. Initializer Tensor "W" with [1, 3, 2, 2] filled with weight
    let mut init_tensor = Vec::new();
    write_tag(&mut init_tensor, 1, 0); // dims: 1
    write_varint(&mut init_tensor, 1);
    write_tag(&mut init_tensor, 1, 0); // dims: 3
    write_varint(&mut init_tensor, 3);
    write_tag(&mut init_tensor, 1, 0); // dims: 2
    write_varint(&mut init_tensor, 2);
    write_tag(&mut init_tensor, 1, 0); // dims: 2
    write_varint(&mut init_tensor, 2);
    write_tag(&mut init_tensor, 2, 0); // data_type: 1 (FLOAT)
    write_varint(&mut init_tensor, 1);
    for _ in 0..12 {
        write_tag(&mut init_tensor, 4, 5); // float_data
        init_tensor.extend_from_slice(&weight.to_le_bytes());
    }
    write_string_field(&mut init_tensor, 8, "W");

    // 2. Node "mul_node"
    let mut node_proto = Vec::new();
    write_string_field(&mut node_proto, 1, "images"); // input
    write_string_field(&mut node_proto, 1, "W"); // input
    write_string_field(&mut node_proto, 2, "output"); // output
    write_string_field(&mut node_proto, 3, "mul_node"); // name
    write_string_field(&mut node_proto, 4, "Mul"); // op_type

    // 3. ValueInfoProto helper
    fn make_value_info(name: &str, dims: &[i64]) -> Vec<u8> {
        let mut vi = Vec::new();
        write_string_field(&mut vi, 1, name);

        let mut shape_proto = Vec::new();
        for &d in dims {
            let mut dim_msg = Vec::new();
            write_tag(&mut dim_msg, 1, 0);
            write_varint(&mut dim_msg, d as u64);
            write_message_field(&mut shape_proto, 1, &dim_msg);
        }

        let mut tensor_type = Vec::new();
        write_tag(&mut tensor_type, 1, 0); // elem_type = 1 (FLOAT)
        write_varint(&mut tensor_type, 1);
        write_message_field(&mut tensor_type, 2, &shape_proto); // shape

        let mut type_proto = Vec::new();
        write_message_field(&mut type_proto, 1, &tensor_type);

        write_message_field(&mut vi, 2, &type_proto);
        vi
    }

    let val_x = make_value_info("images", &[1, 3, 2, 2]);
    let val_y = make_value_info("output", &[1, 3, 2, 2]);

    // 4. GraphProto
    let mut graph_proto = Vec::new();
    write_message_field(&mut graph_proto, 1, &node_proto); // node
    write_string_field(&mut graph_proto, 2, "image_test_graph"); // name
    write_message_field(&mut graph_proto, 5, &init_tensor); // initializer
    write_message_field(&mut graph_proto, 11, &val_x); // input
    write_message_field(&mut graph_proto, 12, &val_y); // output

    // 5. OperatorSetIdProto (version 17)
    let mut opset = Vec::new();
    write_string_field(&mut opset, 1, ""); // domain
    write_tag(&mut opset, 2, 0); // version
    write_varint(&mut opset, 17);

    // 6. ModelProto
    write_tag(&mut bytes, 1, 0); // ir_version
    write_varint(&mut bytes, 8);
    write_message_field(&mut bytes, 8, &opset); // opset_import
    write_string_field(&mut bytes, 3, "autovideo_ai_image_pipeline_test");
    write_string_field(&mut bytes, 6, "1.0.0");
    write_message_field(&mut bytes, 7, &graph_proto);

    if let Some(parent) = file_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    std::fs::write(file_path, bytes)
        .map_err(|e| AppError::storage_write_failed(file_path.to_string_lossy(), e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::manifest::{AiModelManifest, ModelFormat, ModelRequirements};
    use crate::ai::onnx::{
        get_global_ai_runtime, OnnxAiRuntime, OnnxModelMetadata, OnnxTensorMetadata,
    };
    use crate::ai::runtime::AiRuntime;
    use crate::ai::tensor::{Dimension, TensorDataType, TensorSpec};
    use tempfile::tempdir;

    fn create_sample_rgb_frame(w: u32, h: u32) -> ImageFrame {
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let r = ((x * 255) / w.max(1)) as u8;
                let g = ((y * 255) / h.max(1)) as u8;
                let b = 128u8;
                data.push(r);
                data.push(g);
                data.push(b);
            }
        }
        ImageFrame::new(w, h, PixelFormat::Rgb8, data).unwrap()
    }

    // -------------------------------------------------------------
    // Image Tests
    // -------------------------------------------------------------
    #[test]
    fn test_phase6c_decode_png() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.png");
        let frame = create_sample_rgb_frame(10, 10);
        frame.encode_to_png(&path).unwrap();

        let decoded = ImageFrame::decode_from_file(&path).unwrap();
        assert_eq!(decoded.width, 10);
        assert_eq!(decoded.height, 10);
        assert_eq!(decoded.format, PixelFormat::Rgb8);
        assert_eq!(decoded.data.len(), 10 * 10 * 3);
    }

    #[test]
    fn test_phase6c_decode_jpeg() {
        let frame = create_sample_rgb_frame(8, 8);
        let jpeg_bytes = frame.encode_to_jpeg_bytes().unwrap();
        let decoded = ImageFrame::decode_from_bytes(&jpeg_bytes).unwrap();
        assert_eq!(decoded.width, 8);
        assert_eq!(decoded.height, 8);
        assert_eq!(decoded.format, PixelFormat::Rgb8);
    }

    #[test]
    fn test_phase6c_invalid_image() {
        let res = ImageFrame::decode_from_bytes(b"INVALID_HEADER_GARBAGE");
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6c_empty_image() {
        let res = ImageFrame::decode_from_bytes(&[]);
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6c_rgb_conversion() {
        let gray_frame = ImageFrame::new(2, 2, PixelFormat::Gray8, vec![10, 20, 30, 40]).unwrap();
        let rgb_frame = gray_frame.to_rgb8().unwrap();
        assert_eq!(rgb_frame.format, PixelFormat::Rgb8);
        assert_eq!(
            rgb_frame.data,
            vec![10, 10, 10, 20, 20, 20, 30, 30, 30, 40, 40, 40]
        );
    }

    #[test]
    fn test_phase6c_grayscale_conversion() {
        let rgb_frame = ImageFrame::new(1, 1, PixelFormat::Rgb8, vec![255, 0, 0]).unwrap();
        let gray_frame = rgb_frame.to_grayscale().unwrap();
        assert_eq!(gray_frame.format, PixelFormat::Gray8);
        // 0.299 * 255 = 76
        assert_eq!(gray_frame.data[0], 76);
    }

    #[test]
    fn test_phase6c_rgba_conversion() {
        let rgb_frame = ImageFrame::new(1, 1, PixelFormat::Rgb8, vec![100, 150, 200]).unwrap();
        let rgba_frame = rgb_frame.to_rgba8().unwrap();
        assert_eq!(rgba_frame.format, PixelFormat::Rgba8);
        assert_eq!(rgba_frame.data, vec![100, 150, 200, 255]);
    }

    // -------------------------------------------------------------
    // Resize Tests
    // -------------------------------------------------------------
    #[test]
    fn test_phase6c_resize_exact_dimensions() {
        let frame = create_sample_rgb_frame(100, 50);
        let (resized, meta) = resize_image(
            &frame,
            &ResizeConfig {
                target_width: 64,
                target_height: 32,
                filter: ResizeFilter::Bilinear,
            },
        )
        .unwrap();
        assert_eq!(resized.width, 64);
        assert_eq!(resized.height, 32);
        assert_eq!(meta.target_width, 64);
        assert_eq!(meta.target_height, 32);
    }

    #[test]
    fn test_phase6c_resize_invalid_dimensions() {
        let frame = create_sample_rgb_frame(10, 10);
        let res = resize_image(
            &frame,
            &ResizeConfig {
                target_width: 0,
                target_height: 10,
                filter: ResizeFilter::Nearest,
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6c_resize_deterministic() {
        let frame = create_sample_rgb_frame(20, 20);
        let config = ResizeConfig {
            target_width: 10,
            target_height: 10,
            filter: ResizeFilter::Bilinear,
        };
        let (r1, _) = resize_image(&frame, &config).unwrap();
        let (r2, _) = resize_image(&frame, &config).unwrap();
        assert_eq!(r1.data, r2.data);
    }

    // -------------------------------------------------------------
    // Letterbox Tests
    // -------------------------------------------------------------
    #[test]
    fn test_phase6c_letterbox_preserves_aspect_ratio() {
        let frame = create_sample_rgb_frame(1920, 1080);
        let (letterboxed, transform) = apply_letterbox(
            &frame,
            &LetterboxConfig {
                target_width: 640,
                target_height: 640,
                pad_value: [114, 114, 114],
                filter: ResizeFilter::Bilinear,
            },
        )
        .unwrap();

        assert_eq!(letterboxed.width, 640);
        assert_eq!(letterboxed.height, 640);
        assert_eq!(transform.resized_width, 640);
        assert_eq!(transform.resized_height, 360);
        assert_eq!(transform.pad_left, 0);
        assert_eq!(transform.pad_top, 140);
    }

    #[test]
    fn test_phase6c_letterbox_padding() {
        let frame = create_sample_rgb_frame(100, 100);
        let (letterboxed, _) = apply_letterbox(
            &frame,
            &LetterboxConfig {
                target_width: 200,
                target_height: 100,
                pad_value: [114, 114, 114],
                filter: ResizeFilter::Nearest,
            },
        )
        .unwrap();

        // Check corner pixel is padded with 114
        assert_eq!(letterboxed.data[0], 114);
        assert_eq!(letterboxed.data[1], 114);
        assert_eq!(letterboxed.data[2], 114);
    }

    #[test]
    fn test_phase6c_letterbox_transform_metadata() {
        let frame = create_sample_rgb_frame(800, 600);
        let (_, meta) = apply_letterbox(
            &frame,
            &LetterboxConfig {
                target_width: 400,
                target_height: 400,
                pad_value: [0, 0, 0],
                filter: ResizeFilter::Bilinear,
            },
        )
        .unwrap();

        assert_eq!(meta.original_width, 800);
        assert_eq!(meta.original_height, 600);
        assert_eq!(meta.scale_x, 0.5);
    }

    #[test]
    fn test_phase6c_letterbox_reverse_coordinates() {
        let frame = create_sample_rgb_frame(1920, 1080);
        let (_, transform) = apply_letterbox(
            &frame,
            &LetterboxConfig {
                target_width: 640,
                target_height: 640,
                pad_value: [114, 114, 114],
                filter: ResizeFilter::Bilinear,
            },
        )
        .unwrap();

        // A box centered in the 640x640 letterbox image inside the 640x360 active area (y in [140, 500])
        let box_in_letterbox = BoundingBox::new_xyxy(0.0, 140.0, 640.0, 500.0, 0.95, 0);
        let restored = box_in_letterbox.restore_from_letterbox(&transform);

        assert!((restored.x1 - 0.0).abs() < 1.0);
        assert!((restored.y1 - 0.0).abs() < 1.0);
        assert!((restored.x2 - 1920.0).abs() < 1.0);
        assert!((restored.y2 - 1080.0).abs() < 1.0);
    }

    // -------------------------------------------------------------
    // Crop Tests
    // -------------------------------------------------------------
    #[test]
    fn test_phase6c_center_crop() {
        let frame = create_sample_rgb_frame(100, 100);
        let (cropped, meta) = apply_center_crop(
            &frame,
            &CropConfig {
                target_width: 50,
                target_height: 50,
            },
        )
        .unwrap();

        assert_eq!(cropped.width, 50);
        assert_eq!(cropped.height, 50);
        assert_eq!(meta.offset_x, 25);
        assert_eq!(meta.offset_y, 25);
    }

    #[test]
    fn test_phase6c_invalid_crop() {
        let frame = create_sample_rgb_frame(50, 50);
        let res = apply_center_crop(
            &frame,
            &CropConfig {
                target_width: 100,
                target_height: 100,
            },
        );
        assert!(res.is_err());
    }

    // -------------------------------------------------------------
    // Normalization Tests
    // -------------------------------------------------------------
    #[test]
    fn test_phase6c_normalization_identity() {
        let pixels = vec![0, 128, 255];
        let norm = normalize_pixels(&pixels, &NormalizationConfig::identity(), 3).unwrap();
        assert_eq!(norm, vec![0.0, 128.0, 255.0]);
    }

    #[test]
    fn test_phase6c_normalization_zero_one() {
        let pixels = vec![0, 255];
        let norm = normalize_pixels(&pixels, &NormalizationConfig::zero_to_one(), 2).unwrap();
        assert_eq!(norm, vec![0.0, 1.0]);
    }

    #[test]
    fn test_phase6c_normalization_minus_one_one() {
        let pixels = vec![0, 127, 255];
        let norm = normalize_pixels(&pixels, &NormalizationConfig::minus_one_to_one(), 3).unwrap();
        assert_eq!(norm[0], -1.0);
        assert!((norm[1] - (-0.0039215)).abs() < 1e-4);
        assert_eq!(norm[2], 1.0);
    }

    #[test]
    fn test_phase6c_normalization_mean_std() {
        let pixels = vec![255, 0, 0];
        let cfg = NormalizationConfig::mean_std([0.5, 0.5, 0.5], [0.5, 0.5, 0.5]);
        let norm = normalize_pixels(&pixels, &cfg, 3).unwrap();
        assert_eq!(norm[0], (1.0 - 0.5) / 0.5); // 1.0
        assert_eq!(norm[1], (0.0 - 0.5) / 0.5); // -1.0
    }

    #[test]
    fn test_phase6c_normalization_invalid_std() {
        let pixels = vec![255, 0, 0];
        let cfg = NormalizationConfig::mean_std([0.0, 0.0, 0.0], [0.0, 1.0, 1.0]);
        let res = normalize_pixels(&pixels, &cfg, 3);
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6c_normalization_no_nan() {
        let pixels = vec![100, 200, 50];
        let cfg = NormalizationConfig::imagenet();
        let norm = normalize_pixels(&pixels, &cfg, 3).unwrap();
        for val in norm {
            assert!(!val.is_nan());
            assert!(val.is_finite());
        }
    }

    // -------------------------------------------------------------
    // Layout Tests
    // -------------------------------------------------------------
    #[test]
    fn test_phase6c_nhwc_tensor_values() {
        let hwc_data = vec![
            1.0, 0.0, 0.0, // Pixel 0 (R)
            0.0, 1.0, 0.0, // Pixel 1 (G)
            0.0, 0.0, 1.0, // Pixel 2 (B)
            1.0, 1.0, 1.0, // Pixel 3 (W)
        ];
        let (shape, data) =
            reorder_and_transpose_to_tensor(&hwc_data, 2, 2, 3, TensorLayout::Nhwc, 1).unwrap();
        assert_eq!(shape, vec![1, 2, 2, 3]);
        assert_eq!(data, hwc_data);
    }

    #[test]
    fn test_phase6c_nchw_tensor_values() {
        let hwc_data = vec![
            1.0, 0.0, 0.0, // P0
            0.0, 1.0, 0.0, // P1
            0.0, 0.0, 1.0, // P2
            1.0, 1.0, 1.0, // P3
        ];
        let (shape, data) =
            reorder_and_transpose_to_tensor(&hwc_data, 2, 2, 3, TensorLayout::Nchw, 1).unwrap();
        assert_eq!(shape, vec![1, 3, 2, 2]);

        // Channel R: [1.0, 0.0, 0.0, 1.0]
        assert_eq!(&data[0..4], &[1.0, 0.0, 0.0, 1.0]);
        // Channel G: [0.0, 1.0, 0.0, 1.0]
        assert_eq!(&data[4..8], &[0.0, 1.0, 0.0, 1.0]);
        // Channel B: [0.0, 0.0, 1.0, 1.0]
        assert_eq!(&data[8..12], &[0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn test_phase6c_rgb_bgr_conversion() {
        let rgb = vec![255, 128, 0, 10, 20, 30];
        let bgr = convert_channel_order(&rgb, ChannelOrder::Rgb, ChannelOrder::Bgr, 2, 1).unwrap();
        assert_eq!(bgr, vec![0, 128, 255, 30, 20, 10]);
    }

    #[test]
    fn test_phase6c_layout_roundtrip() {
        let rgb = vec![10, 20, 30, 40, 50, 60];
        let bgr = convert_channel_order(&rgb, ChannelOrder::Rgb, ChannelOrder::Bgr, 2, 1).unwrap();
        let back_to_rgb =
            convert_channel_order(&bgr, ChannelOrder::Bgr, ChannelOrder::Rgb, 2, 1).unwrap();
        assert_eq!(rgb, back_to_rgb);
    }

    // -------------------------------------------------------------
    // Tensor Safety Tests
    // -------------------------------------------------------------
    #[test]
    fn test_phase6c_tensor_overflow() {
        let hwc_data = vec![1.0, 2.0, 3.0, 4.0];
        let res = reorder_and_transpose_to_tensor(&hwc_data, u32::MAX, 2, 3, TensorLayout::Nchw, 1);
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6c_tensor_shape_mismatch() {
        let hwc_data = vec![1.0, 2.0];
        let res = reorder_and_transpose_to_tensor(&hwc_data, 2, 2, 3, TensorLayout::Nchw, 1);
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6c_invalid_dimension() {
        let hwc_data = vec![1.0, 2.0];
        let res = reorder_and_transpose_to_tensor(&hwc_data, 0, 2, 3, TensorLayout::Nchw, 1);
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6c_invalid_channel_count() {
        let pixels = vec![1, 2, 3];
        let cfg = NormalizationConfig::identity();
        let res = normalize_pixels(&pixels, &cfg, 0);
        assert!(res.is_err());
    }

    // -------------------------------------------------------------
    // Model Validation Tests
    // -------------------------------------------------------------
    #[test]
    fn test_phase6c_preprocess_matches_model() {
        let model_meta = OnnxModelMetadata {
            input_count: 1,
            output_count: 1,
            inputs: vec![OnnxTensorMetadata {
                name: "images".to_string(),
                data_type: TensorDataType::Float32,
                shape: vec![
                    Dimension::fixed(1),
                    Dimension::fixed(3),
                    Dimension::fixed(640),
                    Dimension::fixed(640),
                ],
            }],
            outputs: vec![],
            producer_name: None,
            graph_name: None,
            version: None,
        };

        let config = PreprocessConfig {
            target_width: 640,
            target_height: 640,
            channel_order: ChannelOrder::Rgb,
            layout: TensorLayout::Nchw,
            batch_size: 1,
            ..Default::default()
        };

        let report = validate_preprocess_against_model(&config, &model_meta, Some("images"));
        assert!(report.is_valid);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn test_phase6c_preprocess_shape_mismatch() {
        let model_meta = OnnxModelMetadata {
            input_count: 1,
            output_count: 1,
            inputs: vec![OnnxTensorMetadata {
                name: "images".to_string(),
                data_type: TensorDataType::Float32,
                shape: vec![
                    Dimension::fixed(1),
                    Dimension::fixed(3),
                    Dimension::fixed(512),
                    Dimension::fixed(512),
                ],
            }],
            outputs: vec![],
            producer_name: None,
            graph_name: None,
            version: None,
        };

        let config = PreprocessConfig {
            target_width: 640,
            target_height: 640,
            channel_order: ChannelOrder::Rgb,
            layout: TensorLayout::Nchw,
            batch_size: 1,
            ..Default::default()
        };

        let report = validate_preprocess_against_model(&config, &model_meta, Some("images"));
        assert!(!report.is_valid);
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn test_phase6c_preprocess_dtype_mismatch() {
        let model_meta = OnnxModelMetadata {
            input_count: 1,
            output_count: 1,
            inputs: vec![OnnxTensorMetadata {
                name: "images".to_string(),
                data_type: TensorDataType::Int32,
                shape: vec![
                    Dimension::fixed(1),
                    Dimension::fixed(3),
                    Dimension::fixed(640),
                    Dimension::fixed(640),
                ],
            }],
            outputs: vec![],
            producer_name: None,
            graph_name: None,
            version: None,
        };

        let config = PreprocessConfig::default();
        let report = validate_preprocess_against_model(&config, &model_meta, Some("images"));
        assert!(!report.is_valid);
    }

    #[test]
    fn test_phase6c_preprocess_channel_mismatch() {
        let model_meta = OnnxModelMetadata {
            input_count: 1,
            output_count: 1,
            inputs: vec![OnnxTensorMetadata {
                name: "images".to_string(),
                data_type: TensorDataType::Float32,
                shape: vec![
                    Dimension::fixed(1),
                    Dimension::fixed(1),
                    Dimension::fixed(640),
                    Dimension::fixed(640),
                ],
            }],
            outputs: vec![],
            producer_name: None,
            graph_name: None,
            version: None,
        };

        let config = PreprocessConfig {
            channel_order: ChannelOrder::Rgb, // 3 channels vs 1 expected
            ..Default::default()
        };
        let report = validate_preprocess_against_model(&config, &model_meta, Some("images"));
        assert!(!report.is_valid);
    }

    // -------------------------------------------------------------
    // Mask Tests
    // -------------------------------------------------------------
    #[test]
    fn test_phase6c_mask_nchw() {
        let tensor = crate::ai::onnx::AiTensorOutput {
            name: "mask".to_string(),
            data_type: TensorDataType::Float32,
            shape: vec![1, 1, 2, 2],
            data_f32: Some(vec![0.1, 0.9, 0.4, 0.8]),
            data_i32: None,
            data_i64: None,
            data_u8: None,
        };

        let mask = extract_mask_from_tensor(&tensor).unwrap();
        assert_eq!(mask.width, 2);
        assert_eq!(mask.height, 2);
        assert_eq!(mask.data, vec![0.1, 0.9, 0.4, 0.8]);
    }

    #[test]
    fn test_phase6c_mask_nhwc() {
        let tensor = crate::ai::onnx::AiTensorOutput {
            name: "mask".to_string(),
            data_type: TensorDataType::Float32,
            shape: vec![1, 2, 2, 1],
            data_f32: Some(vec![0.2, 0.7, 0.3, 0.6]),
            data_i32: None,
            data_i64: None,
            data_u8: None,
        };

        let mask = extract_mask_from_tensor(&tensor).unwrap();
        assert_eq!(mask.width, 2);
        assert_eq!(mask.height, 2);
    }

    #[test]
    fn test_phase6c_mask_threshold() {
        let mask = Mask::new(2, 2, vec![0.2, 0.8, 0.4, 0.9]).unwrap();
        let binary = mask.apply_threshold(0.5);
        assert_eq!(binary.data, vec![0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn test_phase6c_mask_invalid_shape() {
        let tensor = crate::ai::onnx::AiTensorOutput {
            name: "mask".to_string(),
            data_type: TensorDataType::Float32,
            shape: vec![1, 3, 2, 2], // 3 channels is not a single-channel mask
            data_f32: Some(vec![0.0; 12]),
            data_i32: None,
            data_i64: None,
            data_u8: None,
        };
        let res = extract_mask_from_tensor(&tensor);
        assert!(res.is_err());
    }

    // -------------------------------------------------------------
    // Bounding Box Tests
    // -------------------------------------------------------------
    #[test]
    fn test_phase6c_xyxy_to_xywh() {
        let (cx, cy, w, h) = xyxy_to_xywh(10.0, 20.0, 30.0, 60.0);
        assert_eq!(w, 20.0);
        assert_eq!(h, 40.0);
        assert_eq!(cx, 20.0);
        assert_eq!(cy, 40.0);
    }

    #[test]
    fn test_phase6c_xywh_to_xyxy() {
        let (x1, y1, x2, y2) = xywh_to_xyxy(20.0, 40.0, 20.0, 40.0);
        assert_eq!(x1, 10.0);
        assert_eq!(y1, 20.0);
        assert_eq!(x2, 30.0);
        assert_eq!(y2, 60.0);
    }

    #[test]
    fn test_phase6c_letterbox_coordinate_restore() {
        let transform = LetterboxTransform {
            original_width: 1920,
            original_height: 1080,
            resized_width: 640,
            resized_height: 360,
            pad_left: 0,
            pad_top: 140,
            scale_x: 640.0 / 1920.0,
            scale_y: 360.0 / 1080.0,
        };

        let bbox = BoundingBox::new_xyxy(320.0, 320.0, 640.0, 500.0, 0.9, 1);
        let restored = bbox.restore_from_letterbox(&transform);

        assert!((restored.x1 - 960.0).abs() < 1.0);
        assert!((restored.y1 - 540.0).abs() < 1.0);
        assert!((restored.x2 - 1920.0).abs() < 1.0);
        assert!((restored.y2 - 1080.0).abs() < 1.0);
    }

    #[test]
    fn test_phase6c_crop_coordinate_restore() {
        let crop = CropMetadata {
            original_width: 1000,
            original_height: 800,
            cropped_width: 500,
            cropped_height: 400,
            offset_x: 250,
            offset_y: 200,
        };

        let bbox = BoundingBox::new_xyxy(50.0, 50.0, 150.0, 150.0, 0.8, 0);
        let restored = bbox.restore_from_crop(&crop);

        assert_eq!(restored.x1, 300.0);
        assert_eq!(restored.y1, 250.0);
        assert_eq!(restored.x2, 400.0);
        assert_eq!(restored.y2, 350.0);
    }

    // -------------------------------------------------------------
    // End-to-End Real Inference Tests
    // -------------------------------------------------------------
    #[test]
    fn test_phase6c_real_image_to_onnx() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("image_model.onnx");
        let image_path = dir.path().join("frame.png");

        // 1. Generate real deterministic image ONNX graph [1, 3, 2, 2] -> Y = X * 2
        generate_image_onnx_model(&model_path).unwrap();

        // 2. Generate and save a real test PNG image (2x2 RGB)
        let frame = ImageFrame::new(
            2,
            2,
            PixelFormat::Rgb8,
            vec![
                255, 0, 0, // P0 (Red)
                0, 255, 0, // P1 (Green)
                0, 0, 255, // P2 (Blue)
                255, 255, 255, // P3 (White)
            ],
        )
        .unwrap();
        frame.encode_to_png(&image_path).unwrap();

        // 3. Load ONNX model in runtime
        let manifest = AiModelManifest::new(
            "model-image-test",
            "Image Test Multiplier",
            "1.0.0",
            ModelFormat::Onnx,
            model_path.clone(),
            "Image model testing",
            vec![TensorSpec::new(
                "images",
                TensorDataType::Float32,
                vec![
                    Dimension::fixed(1),
                    Dimension::fixed(3),
                    Dimension::fixed(2),
                    Dimension::fixed(2),
                ],
            )],
            vec![TensorSpec::new(
                "output",
                TensorDataType::Float32,
                vec![
                    Dimension::fixed(1),
                    Dimension::fixed(3),
                    Dimension::fixed(2),
                    Dimension::fixed(2),
                ],
            )],
            ModelRequirements {
                min_memory_mb: Some(32),
                requires_gpu: false,
                preferred_provider: None,
            },
        );

        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        // 4. Preprocess real image
        let prep_config = PreprocessConfig {
            target_width: 2,
            target_height: 2,
            letterbox: false,
            center_crop: false,
            channel_order: ChannelOrder::Rgb,
            normalization: NormalizationConfig::zero_to_one(), // 255 -> 1.0, 0 -> 0.0
            layout: TensorLayout::Nchw,
            batch_size: 1,
            ..Default::default()
        };

        let prep_res = preprocess_image(&frame, &prep_config, "images").unwrap();
        assert_eq!(prep_res.tensor.shape, vec![1, 3, 2, 2]);

        // 5. Run real ONNX inference
        let req = crate::ai::onnx::InferenceRequest {
            model_id: "model-image-test".to_string(),
            inputs: vec![prep_res.tensor],
        };
        let infer_res = runtime.infer(&req).unwrap();

        // 6. Verify real output values: (X * 2)
        // Red channel: [1.0, 0.0, 0.0, 1.0] * 2 = [2.0, 0.0, 0.0, 2.0]
        let out_f32 = infer_res.outputs[0].data_f32.as_ref().unwrap();
        assert_eq!(&out_f32[0..4], &[2.0, 0.0, 0.0, 2.0]);
        // Green channel: [0.0, 1.0, 0.0, 1.0] * 2 = [0.0, 2.0, 0.0, 2.0]
        assert_eq!(&out_f32[4..8], &[0.0, 2.0, 0.0, 2.0]);
        // Blue channel: [0.0, 0.0, 1.0, 1.0] * 2 = [0.0, 0.0, 2.0, 2.0]
        assert_eq!(&out_f32[8..12], &[0.0, 0.0, 2.0, 2.0]);
    }

    #[test]
    fn test_phase6c_real_onnx_output_decode() {
        let tensor = crate::ai::onnx::AiTensorOutput {
            name: "output".to_string(),
            data_type: TensorDataType::Float32,
            shape: vec![1, 1, 2, 2],
            data_f32: Some(vec![0.8, 0.2, 0.1, 0.9]),
            data_i32: None,
            data_i64: None,
            data_u8: None,
        };

        let post_cfg = PostprocessConfig {
            extract_mask: true,
            mask_threshold: Some(0.5),
            extract_bboxes: false,
            bbox_confidence_threshold: None,
        };

        let res = postprocess_outputs(&[tensor], &post_cfg).unwrap();
        assert!(res.mask.is_some());
        let mask = res.mask.unwrap();
        assert_eq!(mask.data, vec![1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn test_phase6c_real_pipeline_timing() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("image_model.onnx");
        let image_path = dir.path().join("frame.png");

        generate_image_onnx_model(&model_path).unwrap();
        let frame = create_sample_rgb_frame(2, 2);
        frame.encode_to_png(&image_path).unwrap();

        let manifest = AiModelManifest::new(
            "model-pipeline-timing",
            "Timing Model",
            "1.0.0",
            ModelFormat::Onnx,
            model_path,
            "Timing model",
            vec![TensorSpec::new(
                "images",
                TensorDataType::Float32,
                vec![
                    Dimension::fixed(1),
                    Dimension::fixed(3),
                    Dimension::fixed(2),
                    Dimension::fixed(2),
                ],
            )],
            vec![TensorSpec::new(
                "output",
                TensorDataType::Float32,
                vec![
                    Dimension::fixed(1),
                    Dimension::fixed(3),
                    Dimension::fixed(2),
                    Dimension::fixed(2),
                ],
            )],
            ModelRequirements {
                min_memory_mb: Some(32),
                requires_gpu: false,
                preferred_provider: None,
            },
        );

        let global = get_global_ai_runtime();
        {
            let mut r = global.lock().unwrap();
            let _ = r.unload_model();
            r.load_model(&manifest).unwrap();
        }

        let prep_config = PreprocessConfig {
            target_width: 2,
            target_height: 2,
            letterbox: false,
            center_crop: false,
            channel_order: ChannelOrder::Rgb,
            normalization: NormalizationConfig::zero_to_one(),
            layout: TensorLayout::Nchw,
            batch_size: 1,
            ..Default::default()
        };

        let report = AiInferencePipeline::run_pipeline(
            &image_path,
            "model-pipeline-timing",
            &prep_config,
            None,
        )
        .unwrap();

        assert_eq!(report.model_id, "model-pipeline-timing");
        assert!(report.decode_duration_ms >= 0.0);
        assert!(report.preprocess_duration_ms >= 0.0);
        assert!(report.inference_duration_ms >= 0.0);
        assert!(report.total_duration_ms >= 0.0);

        {
            let mut r = global.lock().unwrap();
            let _ = r.unload_model();
        }
    }
}
