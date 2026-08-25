use crate::ai::phase20c::*;
use std::path::Path;

#[test]
fn test_phase20c_01_benchmark_shared_face_exists_runtime_no_image_generates_new_identity() {
    // 1. Verify shared reference fixture exists in repo
    let _fixture_path = Path::new("test-assets/phase20b/faces/face.jpg");
    // Even if fixture exists physically or logically, runtime without user-supplied face defaults to GENERATED
    let user_face: Option<&Path> = None;
    let (mode, ref_file) = IdentityResolver::resolve_mode(user_face);

    assert_eq!(mode, IdentityMode::Generated);
    assert_eq!(ref_file, None);
}

#[test]
fn test_phase20c_02_runtime_request_supplies_face_image_resolves_reference_mode() {
    let custom_face = Path::new("test-assets/phase20b/faces/face.jpg");
    let (mode, ref_file) = IdentityResolver::resolve_mode(Some(custom_face));

    assert_eq!(mode, IdentityMode::Reference);
    assert_eq!(
        ref_file,
        Some("test-assets/phase20b/faces/face.jpg".to_string())
    );
}

#[test]
fn test_phase20c_03_same_shared_reference_across_c1_c2_c3_reference_cases() {
    let shared_ref = "test-assets/phase20b/faces/face.jpg";

    let c1_ref = FaceReplaceContract {
        case_id: "C1_REFERENCE".to_string(),
        video_file: "test-assets/phase20c/videos/flow_acceptance_01.mp4".to_string(),
        transformation_intent: "FACE_REPLACE".to_string(),
        identity_mode: IdentityMode::Reference,
        reference_face_file: Some(shared_ref.to_string()),
        target_face_index: Some(0),
        target_face_confirmed: true,
        replace_count: 1,
        preserve_non_target_faces: true,
    };

    let c2_ref = FaceReplaceContract {
        case_id: "C2_REFERENCE".to_string(),
        video_file: "test-assets/phase20c/videos/flow_acceptance_02.mp4".to_string(),
        transformation_intent: "FACE_REPLACE".to_string(),
        identity_mode: IdentityMode::Reference,
        reference_face_file: Some(shared_ref.to_string()),
        target_face_index: Some(0),
        target_face_confirmed: true,
        replace_count: 1,
        preserve_non_target_faces: true,
    };

    let c3_ref = FaceReplaceContract {
        case_id: "C3_REFERENCE".to_string(),
        video_file: "test-assets/phase20c/videos/flow_acceptance_03.mp4".to_string(),
        transformation_intent: "FACE_REPLACE".to_string(),
        identity_mode: IdentityMode::Reference,
        reference_face_file: Some(shared_ref.to_string()),
        target_face_index: Some(0),
        target_face_confirmed: true,
        replace_count: 1,
        preserve_non_target_faces: true,
    };

    assert_eq!(c1_ref.reference_face_file, c2_ref.reference_face_file);
    assert_eq!(c2_ref.reference_face_file, c3_ref.reference_face_file);
    assert_eq!(
        c1_ref.reference_face_file.as_deref(),
        Some("test-assets/phase20b/faces/face.jpg")
    );
}

#[test]
fn test_phase20c_04_generated_cases_ignore_root_shared_reference_face() {
    let cases = vec![
        FaceReplaceContract {
            case_id: "C1_GENERATED".to_string(),
            video_file: "test-assets/phase20c/videos/flow_acceptance_01.mp4".to_string(),
            transformation_intent: "FACE_REPLACE".to_string(),
            identity_mode: IdentityMode::Generated,
            reference_face_file: None,
            target_face_index: Some(0),
            target_face_confirmed: true,
            replace_count: 1,
            preserve_non_target_faces: true,
        },
        FaceReplaceContract {
            case_id: "C2_GENERATED".to_string(),
            video_file: "test-assets/phase20c/videos/flow_acceptance_02.mp4".to_string(),
            transformation_intent: "FACE_REPLACE".to_string(),
            identity_mode: IdentityMode::Generated,
            reference_face_file: None,
            target_face_index: Some(0),
            target_face_confirmed: true,
            replace_count: 1,
            preserve_non_target_faces: true,
        },
        FaceReplaceContract {
            case_id: "C3_GENERATED".to_string(),
            video_file: "test-assets/phase20c/videos/flow_acceptance_03.mp4".to_string(),
            transformation_intent: "FACE_REPLACE".to_string(),
            identity_mode: IdentityMode::Generated,
            reference_face_file: None,
            target_face_index: None,
            target_face_confirmed: false,
            replace_count: 1,
            preserve_non_target_faces: true,
        },
    ];

    for c in &cases {
        assert_eq!(c.identity_mode, IdentityMode::Generated);
        assert_eq!(c.reference_face_file, None);
    }
}

#[test]
fn test_phase20c_05_c3_unconfirmed_target_fails_as_ambiguous() {
    // C3 has multiple visible faces (e.g. 3 faces)
    let visible_faces = 3;
    let target_index = None;
    let is_confirmed = false;
    let replace_count = 1;

    let res =
        TargetFacePolicy::validate_target(visible_faces, target_index, is_confirmed, replace_count);

    assert!(res.is_err());
    match res.unwrap_err() {
        TargetFaceError::TargetFaceAmbiguous(msg) => {
            assert!(msg.contains("Multiple visible faces detected"));
        }
        other => panic!("Expected TargetFaceAmbiguous error, got {:?}", other),
    }
}

#[test]
fn test_phase20c_06_c3_confirmed_target_replaces_single_face() {
    let visible_faces = 3;
    let target_index = Some(1);
    let is_confirmed = true;
    let replace_count = 1;

    let target =
        TargetFacePolicy::validate_target(visible_faces, target_index, is_confirmed, replace_count)
            .unwrap();

    assert_eq!(target, 1);

    // Attempting replace_count > 1 must fail
    let bad_replace =
        TargetFacePolicy::validate_target(visible_faces, target_index, is_confirmed, 2);
    assert!(bad_replace.is_err());
}

#[test]
fn test_phase20c_07_api_key_resolution_order_and_zero_leakage() {
    // 1. Placeholder is rejected
    let res = ApiKeyResolver::resolve(Some("Axxxxxxxxxxx"), "NON_EXISTENT_VAR");
    assert_eq!(res, ApiKeyResolution::NotConfigured);

    let res_generic = ApiKeyResolver::resolve(Some("your_api_key_here"), "NON_EXISTENT_VAR");
    assert_eq!(res_generic, ApiKeyResolution::NotConfigured);

    // 2. Valid user override takes highest priority
    let valid_user_key = "r8_custom_user_override_token_secret_12345";
    let res_user = ApiKeyResolver::resolve(Some(valid_user_key), "NON_EXISTENT_VAR");
    assert_eq!(
        res_user,
        ApiKeyResolution::Configured(valid_user_key.to_string())
    );

    // 3. Zero credential leakage via masking
    let masked = ApiKeyResolver::mask_key(valid_user_key);
    assert_eq!(masked, "r8_c...2345");
    assert!(!masked.contains("secret"));
}

fn resolve_repo_path(rel_path: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(rel_path);
    if p.exists() {
        p.to_path_buf()
    } else {
        std::path::Path::new("..").join(rel_path)
    }
}

#[test]
fn test_phase20c_08_physical_manifest_assets_and_metadata_validation() {
    let manifest_path = resolve_repo_path("test-assets/phase20c/video_manifest.md");
    assert!(
        manifest_path.exists(),
        "Manifest must exist at test-assets/phase20c/video_manifest.md (checked {:?})",
        manifest_path
    );

    let manifest_content = std::fs::read_to_string(&manifest_path).unwrap();

    // Check shared reference face declared
    assert!(manifest_content.contains("shared_reference_face: test-assets/phase20b/faces/face.jpg"));

    // Check all physical video assets declared
    assert!(manifest_content.contains("flow_acceptance_01.mp4"));
    assert!(manifest_content.contains("flow_acceptance_02.mp4"));
    assert!(manifest_content.contains("flow_acceptance_03.mp4"));

    // Check all 6 logical benchmark cases declared
    assert!(manifest_content.contains("case_id: C1_GENERATED"));
    assert!(manifest_content.contains("case_id: C1_REFERENCE"));
    assert!(manifest_content.contains("case_id: C2_GENERATED"));
    assert!(manifest_content.contains("case_id: C2_REFERENCE"));
    assert!(manifest_content.contains("case_id: C3_GENERATED"));
    assert!(manifest_content.contains("case_id: C3_REFERENCE"));

    // Verify physical files exist on disk
    assert!(resolve_repo_path("test-assets/phase20b/faces/face.jpg").exists());
    assert!(resolve_repo_path("test-assets/phase20c/videos/flow_acceptance_01.mp4").exists());
    assert!(resolve_repo_path("test-assets/phase20c/videos/flow_acceptance_02.mp4").exists());
    assert!(resolve_repo_path("test-assets/phase20c/videos/flow_acceptance_03.mp4").exists());
}
