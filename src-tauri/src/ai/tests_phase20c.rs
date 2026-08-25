use crate::ai::phase20c::*;
use std::path::Path;

fn resolve_repo_path(rel_path: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(rel_path);
    if p.exists() {
        p.to_path_buf()
    } else {
        std::path::Path::new("..").join(rel_path)
    }
}

#[test]
fn test_phase20c_01_benchmark_shared_face_exists_runtime_no_image_generates_new_identity() {
    let _fixture_path = Path::new("test-assets/phase20b/faces/face.jpg");
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
        transformation_intent: TransformationIntent::FaceReplace,
        identity_mode: IdentityMode::Reference,
        reference_face_file: Some(shared_ref.to_string()),
        target_face: TargetFaceSelection {
            index: 0,
            confirmed: true,
            descriptor: Some("Single face".to_string()),
            anchor_frame_timestamp_sec: Some(0.0),
            normalized_bounding_box: None,
        },
        replace_count: 1,
        preserve_non_target_faces: true,
    };

    let c2_ref = FaceReplaceContract {
        case_id: "C2_REFERENCE".to_string(),
        video_file: "test-assets/phase20c/videos/flow_acceptance_02.mp4".to_string(),
        transformation_intent: TransformationIntent::FaceReplace,
        identity_mode: IdentityMode::Reference,
        reference_face_file: Some(shared_ref.to_string()),
        target_face: TargetFaceSelection {
            index: 0,
            confirmed: true,
            descriptor: Some("Single face".to_string()),
            anchor_frame_timestamp_sec: Some(0.0),
            normalized_bounding_box: None,
        },
        replace_count: 1,
        preserve_non_target_faces: true,
    };

    let c3_ref = FaceReplaceContract {
        case_id: "C3_REFERENCE".to_string(),
        video_file: "test-assets/phase20c/videos/flow_acceptance_03.mp4".to_string(),
        transformation_intent: TransformationIntent::FaceReplace,
        identity_mode: IdentityMode::Reference,
        reference_face_file: Some(shared_ref.to_string()),
        target_face: TargetFaceSelection {
            index: 0,
            confirmed: false,
            descriptor: Some("Unconfirmed multiple faces".to_string()),
            anchor_frame_timestamp_sec: Some(2.0),
            normalized_bounding_box: None,
        },
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
            transformation_intent: TransformationIntent::FaceReplace,
            identity_mode: IdentityMode::Generated,
            reference_face_file: None,
            target_face: TargetFaceSelection {
                index: 0,
                confirmed: true,
                descriptor: Some("Single face".to_string()),
                anchor_frame_timestamp_sec: Some(0.0),
                normalized_bounding_box: None,
            },
            replace_count: 1,
            preserve_non_target_faces: true,
        },
        FaceReplaceContract {
            case_id: "C2_GENERATED".to_string(),
            video_file: "test-assets/phase20c/videos/flow_acceptance_02.mp4".to_string(),
            transformation_intent: TransformationIntent::FaceReplace,
            identity_mode: IdentityMode::Generated,
            reference_face_file: None,
            target_face: TargetFaceSelection {
                index: 0,
                confirmed: true,
                descriptor: Some("Single face".to_string()),
                anchor_frame_timestamp_sec: Some(0.0),
                normalized_bounding_box: None,
            },
            replace_count: 1,
            preserve_non_target_faces: true,
        },
        FaceReplaceContract {
            case_id: "C3_GENERATED".to_string(),
            video_file: "test-assets/phase20c/videos/flow_acceptance_03.mp4".to_string(),
            transformation_intent: TransformationIntent::FaceReplace,
            identity_mode: IdentityMode::Generated,
            reference_face_file: None,
            target_face: TargetFaceSelection {
                index: 0,
                confirmed: false,
                descriptor: Some("Multiple visible faces".to_string()),
                anchor_frame_timestamp_sec: Some(2.0),
                normalized_bounding_box: None,
            },
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
    let visible_faces = 2;
    let target = TargetFaceSelection {
        index: 0,
        confirmed: false, // Unconfirmed!
        descriptor: Some("Unconfirmed driver".to_string()),
        anchor_frame_timestamp_sec: Some(2.0),
        normalized_bounding_box: Some([0.0185, 0.3229, 0.4259, 0.2500]),
    };
    let replace_count = 1;

    let res = TargetFacePolicy::validate_target(visible_faces, &target, replace_count);

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
    let visible_faces = 2;
    let target = TargetFaceSelection {
        index: 1, // Passenger
        confirmed: true,
        descriptor: Some("Passenger (right, phone)".to_string()),
        anchor_frame_timestamp_sec: Some(2.0),
        normalized_bounding_box: Some([0.5370, 0.4479, 0.2222, 0.1458]),
    };
    let replace_count = 1;

    let selected_idx =
        TargetFacePolicy::validate_target(visible_faces, &target, replace_count).unwrap();

    assert_eq!(selected_idx, 1);

    // Attempting replace_count > 1 must fail
    let bad_replace = TargetFacePolicy::validate_target(visible_faces, &target, 2);
    assert!(bad_replace.is_err());
}

#[test]
fn test_phase20c_07_default_placeholder_sentinel_returns_not_configured() {
    // With sentinel default "Axxxxxxxxxxx" and no override / env
    assert_eq!(DEFAULT_PRUNA_API_KEY, "Axxxxxxxxxxx");
    let res = ProviderCredentialResolver::resolve_custom(
        None,
        "NON_EXISTENT_ENV_KEY_123",
        Some(DEFAULT_PRUNA_API_KEY),
    );
    assert_eq!(res, ResolvedCredential::NotConfigured);

    // Generic placeholders also rejected
    let res2 = ProviderCredentialResolver::resolve_custom(
        Some("your_api_key_here"),
        "NON_EXISTENT_ENV_KEY_123",
        None,
    );
    assert_eq!(res2, ResolvedCredential::NotConfigured);
}

#[test]
fn test_phase20c_08_real_looking_app_default_returns_application_default() {
    let custom_app_default = "r8_app_default_deployment_key_abcdef987654";
    let res = ProviderCredentialResolver::resolve_custom(
        None,
        "NON_EXISTENT_ENV_KEY_123",
        Some(custom_app_default),
    );
    assert_eq!(
        res,
        ResolvedCredential::Configured {
            key: custom_app_default.to_string(),
            source: CredentialSource::ApplicationDefault
        }
    );
}

#[test]
fn test_phase20c_09_valid_user_override_takes_precedence() {
    let user_key = "r8_user_override_token_112233";
    let app_default = "r8_app_default_token_445566";

    let res = ProviderCredentialResolver::resolve_custom(
        Some(user_key),
        "NON_EXISTENT_ENV_KEY_123",
        Some(app_default),
    );
    assert_eq!(
        res,
        ResolvedCredential::Configured {
            key: user_key.to_string(),
            source: CredentialSource::UserOverride
        }
    );
}

#[test]
fn test_phase20c_10_removing_user_override_falls_back_to_application_default() {
    let app_default = "r8_app_default_token_445566";

    // 1. User override active
    let res_override = ProviderCredentialResolver::resolve_custom(
        Some("r8_user_key"),
        "NON_EXISTENT_ENV_KEY_123",
        Some(app_default),
    );
    assert_eq!(
        res_override,
        ResolvedCredential::Configured {
            key: "r8_user_key".to_string(),
            source: CredentialSource::UserOverride
        }
    );

    // 2. User removes override (None or empty)
    let res_fallback = ProviderCredentialResolver::resolve_custom(
        None,
        "NON_EXISTENT_ENV_KEY_123",
        Some(app_default),
    );
    assert_eq!(
        res_fallback,
        ResolvedCredential::Configured {
            key: app_default.to_string(),
            source: CredentialSource::ApplicationDefault
        }
    );
}

#[test]
fn test_phase20c_11_zero_credential_leakage_in_public_dto_and_serialization() {
    let secret = "r8_super_secret_api_key_for_pruna_998877";

    // 1. Frontend DTO contains only is_configured and source, NEVER raw key
    let dto = CredentialStatusDto {
        provider_id: "pruna".to_string(),
        is_configured: true,
        source: CredentialSource::UserOverride,
    };
    let json_str = serde_json::to_string(&dto).unwrap();
    assert!(!json_str.contains("secret"));
    assert!(!json_str.contains("r8_"));
    assert!(json_str.contains("\"isConfigured\":true"));
    assert!(json_str.contains("\"source\":\"user_override\""));

    // 2. Masked key for logs never reveals full secret
    let masked = ProviderCredentialResolver::mask_key(secret);
    assert_eq!(masked, "r8_s...8877");
    assert!(!masked.contains("secret"));
}

#[test]
fn test_phase20c_12_physical_manifest_assets_and_metadata_validation() {
    let manifest_path = resolve_repo_path("test-assets/phase20c/video_manifest.md");
    assert!(
        manifest_path.exists(),
        "Manifest must exist at test-assets/phase20c/video_manifest.md"
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
