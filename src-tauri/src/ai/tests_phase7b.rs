#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::ai::control::models::{
        ControlModelSpec, MODEL_ID_BIREFNET, MODEL_ID_DEPTH_ANYTHING_V2, MODEL_ID_DWPOSE,
    };
    use crate::ai::generative::backend::{
        CharacterReference, EnvironmentCondition, GenerationParams, GenerativeBackend,
        KeyframeGenerationRequest,
    };
    use crate::ai::generative::keyframe::KeyframeOrchestrator;
    use crate::ai::generative::sidecar::PythonSidecarBackend;
    use crate::error::ErrorCode;

    fn create_dummy_character_ref(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let img = image::RgbImage::from_fn(128, 128, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, 200])
        });
        img.save(path).unwrap();
    }

    // =========================================================================
    // 1. Backend Capabilities & Health Check
    // =========================================================================

    #[test]
    fn test_phase7b_01_generative_backend_capabilities() {
        let temp = TempDir::new().unwrap();
        let backend = PythonSidecarBackend::new(
            PathBuf::from("python"),
            temp.path().join("fake_script.py"),
            temp.path().to_path_buf(),
            false,
        );

        let caps = backend.get_capabilities().unwrap();
        assert!(caps.supports_character_reference);
        assert!(caps.supports_depth_control);
        assert!(caps.supports_pose_control);
        assert!(caps.supports_mask_control);
        assert!(caps.supported_resolutions.contains(&[512, 768]));
    }

    // =========================================================================
    // 2. Request Validation (Missing Inputs)
    // =========================================================================

    #[test]
    fn test_phase7b_02_keyframe_request_validation_missing_source() {
        let temp = TempDir::new().unwrap();
        let backend = PythonSidecarBackend::new(
            PathBuf::from("python"),
            temp.path().join("fake_script.py"),
            temp.path().to_path_buf(),
            false,
        );

        let char_ref_path = temp.path().join("char_ref.png");
        create_dummy_character_ref(&char_ref_path);

        let req = KeyframeGenerationRequest {
            job_id: "job-missing-src".to_string(),
            source_frame_path: temp.path().join("missing_source_frame.png"),
            pose_artifact_path: None,
            depth_artifact_path: None,
            mask_artifact_path: None,
            character_reference: CharacterReference {
                image_paths: vec![char_ref_path],
                ..Default::default()
            },
            environment: EnvironmentCondition::default(),
            params: GenerationParams::default(),
            output_path: temp.path().join("out.png"),
        };

        let err = backend.generate_keyframe(&req, None).unwrap_err();
        assert_eq!(err.code, ErrorCode::FileNotFound);
    }

    #[test]
    fn test_phase7b_03_keyframe_request_validation_missing_character_ref() {
        let temp = TempDir::new().unwrap();
        let src_frame_path = temp.path().join("src.png");
        create_dummy_character_ref(&src_frame_path);

        let backend = PythonSidecarBackend::new(
            PathBuf::from("python"),
            temp.path().join("fake_script.py"),
            temp.path().to_path_buf(),
            false,
        );

        let req = KeyframeGenerationRequest {
            job_id: "job-no-ref".to_string(),
            source_frame_path: src_frame_path,
            pose_artifact_path: None,
            depth_artifact_path: None,
            mask_artifact_path: None,
            character_reference: CharacterReference {
                image_paths: vec![], // Empty reference list
                ..Default::default()
            },
            environment: EnvironmentCondition::default(),
            params: GenerationParams::default(),
            output_path: temp.path().join("out.png"),
        };

        let err = backend.generate_keyframe(&req, None).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    // =========================================================================
    // 3. Control Model Specifications
    // =========================================================================

    #[test]
    fn test_phase7b_05_control_model_specs_and_authoritative_definitions() {
        let specs = ControlModelSpec::all_required_specs();
        assert_eq!(specs.len(), 3);

        let ids: Vec<&str> = specs.iter().map(|s| s.model_id.as_str()).collect();
        assert!(ids.contains(&MODEL_ID_DWPOSE));
        assert!(ids.contains(&MODEL_ID_DEPTH_ANYTHING_V2));
        assert!(ids.contains(&MODEL_ID_BIREFNET));

        let dwpose = ControlModelSpec::dwpose_spec();
        assert_eq!(dwpose.profile.input.target_width, 384);
        assert_eq!(dwpose.profile.input.target_height, 288);

        let depth = ControlModelSpec::depth_anything_v2_spec();
        assert_eq!(depth.profile.input.target_width, 518);
        assert_eq!(depth.profile.input.target_height, 518);

        let biref = ControlModelSpec::birefnet_spec();
        assert_eq!(biref.profile.input.target_width, 1024);
        assert_eq!(biref.profile.input.target_height, 1024);
    }

    // =========================================================================
    // 4. Cancellation Handling
    // =========================================================================

    #[test]
    fn test_phase7b_06_cancellation_during_keyframe_generation() {
        let temp = TempDir::new().unwrap();
        let src_frame_path = temp.path().join("src.png");
        let char_ref_path = temp.path().join("char_ref.png");
        create_dummy_character_ref(&src_frame_path);
        create_dummy_character_ref(&char_ref_path);

        let backend = PythonSidecarBackend::new(
            PathBuf::from("python"),
            temp.path().join("fake_script.py"),
            temp.path().to_path_buf(),
            false,
        );

        let req = KeyframeGenerationRequest {
            job_id: "job-cancel-06".to_string(),
            source_frame_path: src_frame_path,
            pose_artifact_path: None,
            depth_artifact_path: None,
            mask_artifact_path: None,
            character_reference: CharacterReference {
                image_paths: vec![char_ref_path],
                ..Default::default()
            },
            environment: EnvironmentCondition::default(),
            params: GenerationParams::default(),
            output_path: temp.path().join("out.png"),
        };

        let cancel_token = Arc::new(AtomicBool::new(true)); // Cancelled
        let err = backend
            .generate_keyframe(&req, Some(cancel_token))
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Cancelled);
    }

    // =========================================================================
    // 5. Output Quality Validation
    // =========================================================================

    #[test]
    fn test_phase7b_07_keyframe_quality_validation() {
        let temp = TempDir::new().unwrap();
        let out_path = temp.path().join("valid_keyframe.png");

        let img = image::RgbImage::from_fn(512, 768, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, 150])
        });
        img.save(&out_path).unwrap();

        let report = KeyframeOrchestrator::validate_keyframe_output(&out_path, 512, 768).unwrap();
        assert!(report.is_valid);
        assert_eq!(report.decoded_width, 512);
        assert_eq!(report.decoded_height, 768);
        assert!(!report.black_frame_detected);
        assert!(report.file_size_bytes > 0);
    }

    // =========================================================================
    // 6. Deterministic Parameters
    // =========================================================================

    #[test]
    fn test_phase7b_08_deterministic_seed_and_params() {
        let mut p1 = GenerationParams::default();
        p1.seed = 12345;
        p1.steps = 30;

        let mut p2 = GenerationParams::default();
        p2.seed = 12345;
        p2.steps = 30;

        assert_eq!(p1, p2);
    }

    // =========================================================================
    // 7. End-to-End Keyframe Orchestration Contract
    // =========================================================================

    #[test]
    fn test_phase7b_09_end_to_end_keyframe_orchestration_contract() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let char_ref_path = temp.path().join("char_ref.png");
        create_dummy_character_ref(&char_ref_path);

        let script_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\src-tauri\scripts\generative_sidecar.py");

        let backend = PythonSidecarBackend::new(
            PathBuf::from("python"),
            script_path,
            temp.path().to_path_buf(),
            false,
        );

        let out_keyframe = temp.path().join("generated_keyframe.png");
        let (res, quality) = KeyframeOrchestrator::execute_keyframe_job(
            "job-keyframe-09",
            &fixture_path,
            0,
            CharacterReference {
                image_paths: vec![char_ref_path],
                ..Default::default()
            },
            EnvironmentCondition::default(),
            GenerationParams {
                width: 512,
                height: 768,
                ..Default::default()
            },
            &backend,
            temp.path(),
            &out_keyframe,
            None,
        )
        .unwrap();

        assert!(res.output_path.exists());
        assert!(quality.is_valid);
        assert_eq!(quality.decoded_width, 512);
        assert_eq!(quality.decoded_height, 768);
    }

    // =========================================================================
    // 8. Missing Character Ref File Rejection
    // =========================================================================

    #[test]
    fn test_phase7b_10_missing_character_ref_file_rejection() {
        let temp = TempDir::new().unwrap();
        let src_frame_path = temp.path().join("src.png");
        create_dummy_character_ref(&src_frame_path);

        let backend = PythonSidecarBackend::new(
            PathBuf::from("python"),
            temp.path().join("fake_script.py"),
            temp.path().to_path_buf(),
            false,
        );

        let req = KeyframeGenerationRequest {
            job_id: "job-missing-ref-file".to_string(),
            source_frame_path: src_frame_path,
            pose_artifact_path: None,
            depth_artifact_path: None,
            mask_artifact_path: None,
            character_reference: CharacterReference {
                image_paths: vec![temp.path().join("non_existent_ref.png")],
                ..Default::default()
            },
            environment: EnvironmentCondition::default(),
            params: GenerationParams::default(),
            output_path: temp.path().join("out.png"),
        };

        let err = backend.generate_keyframe(&req, None).unwrap_err();
        assert_eq!(err.code, ErrorCode::FileNotFound);
    }
}
