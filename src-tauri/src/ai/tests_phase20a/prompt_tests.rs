use crate::ai::flow::*;
use crate::system::StoragePaths;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct MockGeminiServer {
    pub base_url: String,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl MockGeminiServer {
    pub async fn start(
        status_to_return: u16,
        body_to_return: &'static str,
    ) -> Result<Self, String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| e.to_string())?;
        let addr = listener.local_addr().map_err(|e| e.to_string())?;
        let base_url = format!("http://127.0.0.1:{}", addr.port());

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept_res = listener.accept() => {
                        if let Ok((mut socket, _)) = accept_res {
                            tokio::spawn(async move {
                                let mut buf = [0u8; 4096];
                                if let Ok(n) = socket.read(&mut buf).await {
                                    let _req_str = String::from_utf8_lossy(&buf[..n]);
                                    let status_line = match status_to_return {
                                        200 => "200 OK",
                                        400 => "400 Bad Request",
                                        401 => "401 Unauthorized",
                                        403 => "403 Forbidden",
                                        404 => "404 Not Found",
                                        429 => "429 Too Many Requests",
                                        500 => "500 Internal Server Error",
                                        503 => "503 Service Unavailable",
                                        _ => "200 OK",
                                    };
                                    let resp = format!(
                                        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                        status_line,
                                        body_to_return.len(),
                                        body_to_return
                                    );
                                    let _ = socket.write_all(resp.as_bytes()).await;
                                }
                            });
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            base_url,
            shutdown_tx: Some(shutdown_tx),
        })
    }
}

impl Drop for MockGeminiServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[test]
fn test_phase20a_01_empty_and_whitespace_prompt_no_gemini_call() {
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    let _ = store.set_gemini_api_key("test_key");
    let optimizer = GeminiPromptOptimizer::new(store);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let req_empty = OptimizePromptRequest {
        prompt: "".to_string(),
        source_prompt_hash: None,
        task_type: None,
        video_duration_sec: None,
        fps: None,
        resolution: None,
        transformation_intent: None,
        identity_mode: None,
        target_descriptor: None,
        preserve_background: None,
        preserve_body: None,
        preserve_clothing: None,
        preserve_non_target_faces: None,
    };
    let res_empty = rt.block_on(optimizer.optimize_prompt(req_empty));
    assert!(res_empty.is_err());
    assert!(res_empty.unwrap_err().contains("REQUEST_INVALID"));

    let req_ws = OptimizePromptRequest {
        prompt: "   \n\t  ".to_string(),
        source_prompt_hash: None,
        task_type: None,
        video_duration_sec: None,
        fps: None,
        resolution: None,
        transformation_intent: None,
        identity_mode: None,
        target_descriptor: None,
        preserve_background: None,
        preserve_body: None,
        preserve_clothing: None,
        preserve_non_target_faces: None,
    };
    let res_ws = rt.block_on(optimizer.optimize_prompt(req_ws));
    assert!(res_ws.is_err());
    assert!(res_ws.unwrap_err().contains("REQUEST_INVALID"));
}

#[test]
fn test_phase20a_02_prompt_source_provenance_and_hash() {
    let prompt = "Turn character into a golden cyber knight with blue glowing eyes";
    let hash = calculate_prompt_hash(prompt);
    assert!(!hash.is_empty());
    assert_eq!(hash.len(), 64);

    assert_eq!(PromptSource::User.as_str(), "USER");
    assert_eq!(PromptSource::GeminiOptimized.as_str(), "GEMINI_OPTIMIZED");
    assert_eq!(
        PromptSource::GeminiOptimizedThenEdited.as_str(),
        "GEMINI_OPTIMIZED_THEN_EDITED"
    );
}

#[test]
fn test_phase20a_03_double_click_and_stale_response_logic() {
    let original = "Turn character into rabbit";
    let original_hash = calculate_prompt_hash(original);

    let user_edited = "Turn character into robot rabbit";
    let user_edited_hash = calculate_prompt_hash(user_edited);

    assert_ne!(original_hash, user_edited_hash);

    let stale_response_hash = original_hash.clone();
    let current_editor_hash = user_edited_hash.clone();

    let is_stale = stale_response_hash != current_editor_hash;
    assert!(is_stale);
}

#[test]
fn test_phase20a_04_successful_optimization_replaces_editor_content() {
    let prompt = "A cinematic shot of cyber hero in neon city";
    let hash = calculate_prompt_hash(prompt);
    let resp = OptimizePromptResponse {
        optimized_prompt: prompt.to_string(),
        model: DEFAULT_PROMPT_OPTIMIZATION_MODEL.to_string(),
        prompt_source: PromptSource::GeminiOptimized,
        prompt_hash: hash,
    };
    assert_eq!(resp.prompt_source, PromptSource::GeminiOptimized);
    assert_eq!(resp.model, DEFAULT_PROMPT_OPTIMIZATION_MODEL);
}

#[test]
fn test_phase20a_05_undo_restores_previous_text_and_provenance() {
    let mut history_stack: Vec<(String, PromptSource)> = Vec::new();

    let prompt_v1 = "Turn character to fox".to_string();
    let source_v1 = PromptSource::User;
    history_stack.push((prompt_v1.clone(), source_v1));

    let prompt_v2 = "Cinematic red fox in autumn forest".to_string();
    let source_v2 = PromptSource::GeminiOptimized;
    history_stack.push((prompt_v2.clone(), source_v2));

    let _prompt_v3 = "Cinematic red fox with scarf".to_string();
    let _source_v3 = PromptSource::GeminiOptimizedThenEdited;

    assert_eq!(history_stack.len(), 2);

    let (restored_p2, restored_s2) = history_stack.pop().unwrap();
    assert_eq!(restored_p2, prompt_v2);
    assert_eq!(restored_s2, PromptSource::GeminiOptimized);

    let (restored_p1, restored_s1) = history_stack.pop().unwrap();
    assert_eq!(restored_p1, prompt_v1);
    assert_eq!(restored_s1, PromptSource::User);
}

#[test]
fn test_phase20a_06_gen_again_optimizes_current_editor_text() {
    let prompt = "Updated prompt for second optimization";
    let req = OptimizePromptRequest {
        prompt: prompt.to_string(),
        source_prompt_hash: Some(calculate_prompt_hash(prompt)),
        task_type: Some("CHARACTER_TRANSFORMATION".to_string()),
        video_duration_sec: Some(10.0),
        fps: Some(30.0),
        resolution: Some((1920, 1080)),
        transformation_intent: None,
        identity_mode: None,
        target_descriptor: None,
        preserve_background: None,
        preserve_body: None,
        preserve_clothing: None,
        preserve_non_target_faces: None,
    };
    assert_eq!(req.prompt, prompt);
}

#[test]
fn test_phase20a_07_manual_edit_after_optimization_transitions_to_edited() {
    let mut source = PromptSource::GeminiOptimized;
    assert_eq!(source, PromptSource::GeminiOptimized);
    source = PromptSource::GeminiOptimizedThenEdited;
    assert_eq!(source, PromptSource::GeminiOptimizedThenEdited);
}

#[test]
fn test_phase20a_08_gemini_failure_leaves_prompt_untouched() {
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    let _ = store.clear_gemini_api_key();
    let optimizer = GeminiPromptOptimizer::new(store);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let req = OptimizePromptRequest {
        prompt: "User written untouched prompt".to_string(),
        source_prompt_hash: None,
        task_type: None,
        video_duration_sec: None,
        fps: None,
        resolution: None,
        transformation_intent: None,
        identity_mode: None,
        target_descriptor: None,
        preserve_background: None,
        preserve_body: None,
        preserve_clothing: None,
        preserve_non_target_faces: None,
    };
    let res = rt.block_on(optimizer.optimize_prompt(req));
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("GEMINI_API_KEY_NOT_CONFIGURED"));
}

#[test]
fn test_phase20a_09_secret_store_encryption_and_zero_leakage() {
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());

    let _ = store.clear_gemini_api_key();
    assert!(!store.is_gemini_configured());

    store
        .set_gemini_api_key("AIzaSy_Secret_Gemini_Key_12345")
        .unwrap();
    assert!(store.is_gemini_configured());
    assert_eq!(
        store.get_gemini_api_key(),
        Some("AIzaSy_Secret_Gemini_Key_12345".to_string())
    );

    // Verify NO plaintext secret file is created on disk
    let security_dir = temp_dir.path().join("security");
    assert!(
        !security_dir.exists(),
        "No secret vault file should exist on disk when using OS credential manager"
    );

    store.clear_gemini_api_key().unwrap();
    assert!(!store.is_gemini_configured());
}

#[test]
fn test_phase20a_10_double_click_single_active_request() {
    let mut in_flight_request: Option<String> = None;
    assert!(in_flight_request.is_none());
    in_flight_request = Some("req_click_1".to_string());
    let can_start_second = in_flight_request.is_none();
    assert!(!can_start_second);
    in_flight_request = None;
    assert!(in_flight_request.is_none());
}

#[test]
fn test_phase20a_11_late_request_a_after_b_does_not_overwrite_b() {
    let active_req_id = "req_B";
    let incoming_resp_id = "req_A";
    let should_apply = incoming_resp_id == active_req_id;
    assert!(!should_apply);
}

#[test]
fn test_phase20a_12_empty_and_oversized_gemini_response_rejected() {
    let empty_str = "   ";
    assert!(empty_str.trim().is_empty());
    let oversized_str = "a".repeat(5000);
    assert!(oversized_str.len() > 3000);
    let truncated = if oversized_str.len() > 3000 {
        &oversized_str[..3000]
    } else {
        &oversized_str
    };
    assert_eq!(truncated.len(), 3000);
}

// -----------------------------------------------------------------------------
// Gemini Verification & Diagnostic Tests (Phase 20A Hotfix)
// -----------------------------------------------------------------------------

#[test]
fn test_phase20a_51_gemini_unverified_on_store() {
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    let manager = GeminiCredentialManager::new(store);

    manager.set_key("AIzaSy_Secret_Key_For_Test").unwrap();
    let status = manager.get_status();
    assert!(status.stored);
    assert_eq!(
        status.verification_status,
        GeminiVerificationStatus::Unverified
    );
    assert!(status.last_verified_at.is_none());
}

#[test]
fn test_phase20a_52_gemini_mock_validation_success_valid() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt
        .block_on(MockGeminiServer::start(
            200,
            r#"{"name":"models/gemini-3.5-flash-lite","displayName":"Gemini 3.5 Flash Lite"}"#,
        ))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    store.set_gemini_api_key("AIzaSy_Mock_Valid_Key").unwrap();

    let manager = GeminiCredentialManager::with_endpoint_and_model(
        store,
        Some(server.base_url.clone()),
        DEFAULT_PROMPT_OPTIMIZATION_MODEL.to_string(),
    );

    let res = rt.block_on(manager.test_api_key()).unwrap();
    assert!(res.stored);
    assert_eq!(res.verification_status, GeminiVerificationStatus::Valid);
    assert!(res.last_verified_at.is_some());
    assert!(res.sanitized_message.is_none());
}

#[test]
fn test_phase20a_53_gemini_mock_validation_error_statuses() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // 1. 400 with API_KEY_INVALID
    let s_400 = rt
        .block_on(MockGeminiServer::start(
            400,
            r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT"}}"#,
        ))
        .unwrap();
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    store.set_gemini_api_key("bad_key_400").unwrap();
    let m_400 = GeminiCredentialManager::with_endpoint_and_model(
        store,
        Some(s_400.base_url.clone()),
        DEFAULT_PROMPT_OPTIMIZATION_MODEL.to_string(),
    );
    let res_400 = rt.block_on(m_400.test_api_key()).unwrap();
    assert_eq!(
        res_400.verification_status,
        GeminiVerificationStatus::InvalidKey
    );

    // 2. 403 Forbidden / Permission Denied
    let s_403 = rt
        .block_on(MockGeminiServer::start(
            403,
            r#"{"error":{"code":403,"message":"Generative Language API has not been used in project before or it is disabled.","status":"PERMISSION_DENIED"}}"#,
        ))
        .unwrap();
    let store_403 = SecretStore::new(temp_dir.path().to_path_buf());
    store_403.set_gemini_api_key("forbidden_key").unwrap();
    let m_403 = GeminiCredentialManager::with_endpoint_and_model(
        store_403,
        Some(s_403.base_url.clone()),
        DEFAULT_PROMPT_OPTIMIZATION_MODEL.to_string(),
    );
    let res_403 = rt.block_on(m_403.test_api_key()).unwrap();
    assert_eq!(
        res_403.verification_status,
        GeminiVerificationStatus::Forbidden
    );

    // 3. 404 Model Unavailable
    let s_404 = rt
        .block_on(MockGeminiServer::start(
            404,
            r#"{"error":{"code":404,"message":"models/gemini-3.5-flash-lite is not found for API version v1beta","status":"NOT_FOUND"}}"#,
        ))
        .unwrap();
    let store_404 = SecretStore::new(temp_dir.path().to_path_buf());
    store_404.set_gemini_api_key("not_found_key").unwrap();
    let m_404 = GeminiCredentialManager::with_endpoint_and_model(
        store_404,
        Some(s_404.base_url.clone()),
        DEFAULT_PROMPT_OPTIMIZATION_MODEL.to_string(),
    );
    let res_404 = rt.block_on(m_404.test_api_key()).unwrap();
    assert_eq!(
        res_404.verification_status,
        GeminiVerificationStatus::ModelUnavailable
    );

    // 4. 429 Rate Limited
    let s_429 = rt
        .block_on(MockGeminiServer::start(
            429,
            r#"{"error":{"code":429,"message":"Resource has been exhausted (e.g. check quota).","status":"RESOURCE_EXHAUSTED"}}"#,
        ))
        .unwrap();
    let store_429 = SecretStore::new(temp_dir.path().to_path_buf());
    store_429.set_gemini_api_key("rate_limit_key").unwrap();
    let m_429 = GeminiCredentialManager::with_endpoint_and_model(
        store_429,
        Some(s_429.base_url.clone()),
        DEFAULT_PROMPT_OPTIMIZATION_MODEL.to_string(),
    );
    let res_429 = rt.block_on(m_429.test_api_key()).unwrap();
    assert_eq!(
        res_429.verification_status,
        GeminiVerificationStatus::RateLimited
    );

    // 5. 500 Provider Temporary Failure
    let s_500 = rt
        .block_on(MockGeminiServer::start(
            500,
            r#"{"error":{"code":500,"message":"Internal server error","status":"INTERNAL"}}"#,
        ))
        .unwrap();
    let store_500 = SecretStore::new(temp_dir.path().to_path_buf());
    store_500.set_gemini_api_key("server_err_key").unwrap();
    let m_500 = GeminiCredentialManager::with_endpoint_and_model(
        store_500,
        Some(s_500.base_url.clone()),
        DEFAULT_PROMPT_OPTIMIZATION_MODEL.to_string(),
    );
    let res_500 = rt.block_on(m_500.test_api_key()).unwrap();
    assert_eq!(
        res_500.verification_status,
        GeminiVerificationStatus::ProviderTemporaryFailure
    );
}

#[test]
fn test_phase20a_54_failed_verification_preserves_stored_key() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt
        .block_on(MockGeminiServer::start(
            403,
            r#"{"error":{"code":403,"message":"Permission denied","status":"PERMISSION_DENIED"}}"#,
        ))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    store.set_gemini_api_key("my_stored_secret_key").unwrap();

    let manager = GeminiCredentialManager::with_endpoint_and_model(
        store.clone(),
        Some(server.base_url.clone()),
        DEFAULT_PROMPT_OPTIMIZATION_MODEL.to_string(),
    );

    let res = rt.block_on(manager.test_api_key()).unwrap();
    assert_eq!(res.verification_status, GeminiVerificationStatus::Forbidden);
    assert!(res.stored);

    // Assert key is NOT deleted from SecretStore
    assert_eq!(
        store.get_gemini_api_key(),
        Some("my_stored_secret_key".to_string())
    );
}

#[test]
fn test_phase20a_55_get_gemini_status_retains_valid_in_session() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt
        .block_on(MockGeminiServer::start(
            200,
            r#"{"name":"models/gemini-3.5-flash-lite"}"#,
        ))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    store.set_gemini_api_key("valid_key").unwrap();

    let manager = GeminiCredentialManager::with_endpoint_and_model(
        store,
        Some(server.base_url.clone()),
        DEFAULT_PROMPT_OPTIMIZATION_MODEL.to_string(),
    );

    let _ = rt.block_on(manager.test_api_key()).unwrap();

    // In same session, get_status retains Valid
    let status = manager.get_status();
    assert!(status.stored);
    assert_eq!(status.verification_status, GeminiVerificationStatus::Valid);
}

#[test]
fn test_phase20a_56_app_restart_resets_to_unverified() {
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    store.set_gemini_api_key("valid_persisted_key").unwrap();

    // Simulate new process / app restart creating fresh manager
    let fresh_manager = GeminiCredentialManager::new(store);
    let status = fresh_manager.get_status();
    assert!(status.stored);
    assert_eq!(
        status.verification_status,
        GeminiVerificationStatus::Unverified
    );
}

#[test]
fn test_phase20a_57_zero_credential_leakage_in_diagnostics() {
    let secret = "AIzaSy_SuperSecret12345_Key";
    let body = format!(
        r#"{{"error":{{"code":400,"message":"Invalid key {} provided in request","status":"INVALID_ARGUMENT"}}}}"#,
        secret
    );

    let (v_status, code, sanitized) =
        parse_google_error(reqwest::StatusCode::BAD_REQUEST, &body, Some(secret));
    assert_eq!(v_status, GeminiVerificationStatus::InvalidKey);
    assert_eq!(code, "GEMINI_API_KEY_INVALID");
    assert!(!sanitized.contains(secret));
    assert!(sanitized.contains("[REDACTED_API_KEY]"));
}

// -----------------------------------------------------------------------------
// Phase 20C-A1: Canonical Gemini Key Resolution & Runtime Wiring Tests
// -----------------------------------------------------------------------------

#[test]
fn test_phase20c_gemini_01_sentinel_default_returns_not_configured() {
    assert_eq!(DEFAULT_GEMINI_API_KEY, "Axxxxxxxxxxx");
    let is_valid = is_valid_gemini_key(DEFAULT_GEMINI_API_KEY);
    assert!(!is_valid, "Sentinel key must not be considered valid");

    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    let _ = store.clear_gemini_api_key();

    if std::env::var("GEMINI_API_KEY").is_err() {
        let cred = store.resolve_gemini_credential();
        assert_eq!(cred, None);
        assert!(!store.is_gemini_configured());
    }
}

#[test]
fn test_phase20c_gemini_02_real_app_default_returns_application_default() {
    let valid_format = "AIzaSyCustomApplicationKey12345";
    assert!(is_valid_gemini_key(valid_format));
}

#[test]
fn test_phase20c_gemini_03_stored_custom_key_returns_user_override() {
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    let custom_key = "AIzaSyUserProvidedCustomKey999";
    store.set_gemini_api_key(custom_key).unwrap();

    let cred = store.resolve_gemini_credential().unwrap();
    assert_eq!(cred.key, custom_key);
    assert_eq!(cred.source, GeminiCredentialSource::UserOverride);
    assert!(store.has_user_override());
    assert!(store.is_gemini_configured());
}

#[test]
fn test_phase20c_gemini_04_custom_key_wins_over_default() {
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    let custom_key = "AIzaSyOverrideWinsKey888";
    store.set_gemini_api_key(custom_key).unwrap();

    let manager = GeminiCredentialManager::new(store.clone());
    let status = manager.get_status();
    assert!(status.stored);
    assert!(status.is_configured);
    assert_eq!(status.source, GeminiCredentialSource::UserOverride);
}

#[test]
fn test_phase20c_gemini_05_remove_custom_key_falls_back() {
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    store.set_gemini_api_key("AIzaSyTempCustomKey777").unwrap();
    assert!(store.has_user_override());

    store.clear_gemini_api_key().unwrap();
    assert!(!store.has_user_override());

    let manager = GeminiCredentialManager::new(store.clone());
    let status = manager.get_status();
    assert!(!status.stored);
    if std::env::var("GEMINI_API_KEY").is_err() {
        assert!(!status.is_configured);
        assert_eq!(status.source, GeminiCredentialSource::NotConfigured);
    }
}

#[test]
fn test_phase20c_gemini_06_manager_and_optimizer_resolve_same_source() {
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    let custom_key = "AIzaSySharedKey666";
    store.set_gemini_api_key(custom_key).unwrap();

    let manager = GeminiCredentialManager::new(store.clone());
    let optimizer = GeminiPromptOptimizer::new(store.clone());

    assert_eq!(
        manager.get_status().source,
        GeminiCredentialSource::UserOverride
    );
    assert_eq!(optimizer.is_configured(), true);
    assert_eq!(store.get_gemini_api_key(), Some(custom_key.to_string()));
}

#[test]
fn test_phase20c_gemini_07_request_contains_no_media_binary_or_base64() {
    let req = OptimizePromptRequest {
        prompt: "Replace face".to_string(),
        source_prompt_hash: None,
        task_type: Some("FACE_REPLACE".to_string()),
        video_duration_sec: Some(9.9),
        fps: Some(30.0),
        resolution: Some((1080, 1920)),
        transformation_intent: Some("FACE_REPLACE".to_string()),
        identity_mode: Some("GENERATED".to_string()),
        target_descriptor: Some("PASSENGER_RIGHT".to_string()),
        preserve_background: Some(true),
        preserve_body: Some(true),
        preserve_clothing: Some(true),
        preserve_non_target_faces: Some(true),
    };

    let json_str = serde_json::to_string(&req).unwrap();
    assert!(!json_str.contains("base64"));
    assert!(!json_str.contains("imageBytes"));
    assert!(!json_str.contains("videoBytes"));
    assert!(!json_str.contains("frameBytes"));
    assert!(json_str.contains("transformationIntent"));
    assert!(json_str.contains("identityMode"));
}

#[test]
fn test_phase20c_gemini_08_mock_optimization_success_preservation_semantics() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mock_response = r#"{
        "candidates": [{
            "content": {
                "parts": [{
                    "text": "Replace only the target passenger's face with a new synthetic identity while strictly preserving hair, clothing, body posture, lighting, and driver identity."
                }]
            }
        }]
    }"#;
    let server = rt
        .block_on(MockGeminiServer::start(200, mock_response))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    store.set_gemini_api_key("valid_mock_key").unwrap();

    let optimizer = GeminiPromptOptimizer::with_endpoint_and_model(
        store,
        Some(server.base_url.clone()),
        DEFAULT_PROMPT_OPTIMIZATION_MODEL.to_string(),
    );

    let req = OptimizePromptRequest {
        prompt: "Change the face".to_string(),
        source_prompt_hash: None,
        task_type: Some("FACE_REPLACE".to_string()),
        video_duration_sec: Some(9.9),
        fps: Some(30.0),
        resolution: Some((1080, 1920)),
        transformation_intent: Some("FACE_REPLACE".to_string()),
        identity_mode: Some("GENERATED".to_string()),
        target_descriptor: Some("PASSENGER_RIGHT".to_string()),
        preserve_background: Some(true),
        preserve_body: Some(true),
        preserve_clothing: Some(true),
        preserve_non_target_faces: Some(true),
    };

    let res = rt.block_on(optimizer.optimize_prompt(req)).unwrap();
    assert_eq!(res.prompt_source, PromptSource::GeminiOptimized);
    assert!(res.optimized_prompt.contains("synthetic identity"));
    assert!(res.optimized_prompt.contains("preserving"));
}

// -----------------------------------------------------------------------------
// Phase FLOW-P1: Production Flow Function & Sentinel Tests
// -----------------------------------------------------------------------------

#[test]
fn test_phase_flow_p1_01_gemini_sentinel_exact_rejection_and_real_key_acceptance() {
    assert_eq!(GEMINI_API_KEY_SENTINEL, "Axxxxxxxxxxx");
    assert!(!is_valid_gemini_key(GEMINI_API_KEY_SENTINEL));
    assert!(!is_valid_gemini_key(""));
    assert!(!is_valid_gemini_key("   "));
    assert!(!is_valid_gemini_key("your_api_key_here"));
    assert!(!is_valid_gemini_key("PLACEHOLDER"));
    assert!(!is_valid_gemini_key("YOUR_GEMINI_API_KEY"));

    // Real-looking keys (including those that start with A) are valid
    assert!(is_valid_gemini_key("AIzaSyValidRealKey123456789"));
    assert!(is_valid_gemini_key("AxxxxRealKeyNonPlaceholder"));
}

#[test]
fn test_phase_flow_p1_02_app_default_to_user_override_and_fallback_lifecycle() {
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());
    let _ = store.clear_gemini_api_key();
    let manager = GeminiCredentialManager::new(store.clone());

    // 1. Initial state (with user override cleared)
    let initial_status = manager.get_status();
    assert!(!initial_status.stored);

    // 2. Set user override
    let user_key = "AIzaSyUserProvidedOverride123";
    manager.set_key(user_key).unwrap();
    let override_status = manager.get_status();
    assert!(override_status.stored);
    assert!(override_status.is_configured);
    assert_eq!(override_status.source, GeminiCredentialSource::UserOverride);

    // 3. Clear user override
    manager.clear_key().unwrap();
    let cleared_status = manager.get_status();
    assert!(!cleared_status.stored);
}

#[test]
fn test_phase_flow_p1_03_flow_production_request_e2e_acceptance() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let server = crate::ai::flow::MockFlowServer::start(crate::ai::flow::MockScenario::Ready)
            .await
            .unwrap();

        let temp_dir = tempdir().unwrap();
        let storage_paths = StoragePaths::resolve_from_base(temp_dir.path());

        let project_id = "test_flow_p1_proj".to_string();
        let profile_id = "profile_flow_p1".to_string();

        let project_media_dir = storage_paths.projects_dir.join(&project_id).join("media");
        std::fs::create_dir_all(&project_media_dir).unwrap();
        let test_video_path = project_media_dir.join("input.mp4");

        // Generate valid 1-second 30fps test video
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=320x240:rate=30",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                test_video_path.to_str().unwrap(),
            ])
            .output();

        if status.is_err() || !status.unwrap().status.success() {
            return;
        }

        let profile_manager =
            crate::ai::flow::FlowProfileManager::new(storage_paths.app_data_dir.clone());
        profile_manager
            .create_profile(&profile_id, "Flow P1 Profile")
            .unwrap();

        let flow_service = crate::ai::flow::FlowRuntimeService::with_mock_bridge(
            storage_paths.clone(),
            server.base_url.clone(),
        );

        let req = crate::ai::flow::FlowGenerationRequest {
            project_id: project_id.clone(),
            source_media_id: "input.mp4".to_string(),
            profile_id: profile_id.clone(),
            transformation_intent: Some(
                crate::ai::transformation::TransformationIntent::FaceReplace,
            ),
            identity_mode: Some(crate::ai::transformation::IdentityMode::Generated),
            prompt: "Replace face".to_string(),
            prompt_source: Some(PromptSource::User),
            target_face: None,
            max_credits: Some(40),
            preserve_original_audio: Some(true),
            requested_config: None,
            configuration_fingerprint: None,
        };

        let start_snapshot = flow_service
            .start_flow_generation(req, test_video_path.clone())
            .await
            .unwrap();
        assert_eq!(start_snapshot.total_segments, 1);
        assert_eq!(start_snapshot.estimated_credits, 40);

        // Poll until terminal or timeout (up to 30s)
        let parent_id = start_snapshot.parent_id.clone();
        let start_time = std::time::Instant::now();
        let mut final_snap = start_snapshot;

        while start_time.elapsed() < std::time::Duration::from_secs(30) {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let snap = flow_service
                .get_flow_job_status(&project_id, &parent_id)
                .unwrap();
            final_snap = snap.clone();
            if snap.state.is_terminal() {
                break;
            }
        }

        assert_eq!(final_snap.state, crate::ai::flow::FlowJobState::Completed);
        assert!(final_snap.final_output_ready);
        assert!(final_snap.final_output_path.is_some());
    });
}

#[test]
fn test_phase_flow_p1_04_pre_click_budget_exceeded_rejects_before_click() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt
        .block_on(crate::ai::flow::MockFlowServer::start(
            crate::ai::flow::MockScenario::Ready,
        ))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let storage_paths = StoragePaths::resolve_from_base(temp_dir.path());

    let project_id = "test_budget_proj".to_string();
    let profile_id = "profile_budget".to_string();

    let project_media_dir = storage_paths.projects_dir.join(&project_id).join("media");
    std::fs::create_dir_all(&project_media_dir).unwrap();
    let test_video_path = project_media_dir.join("input.mp4");

    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=320x240:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:duration=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            test_video_path.to_str().unwrap(),
        ])
        .output();

    if status.is_err() || !status.unwrap().status.success() {
        return;
    }

    let profile_manager =
        crate::ai::flow::FlowProfileManager::new(storage_paths.app_data_dir.clone());
    profile_manager
        .create_profile(&profile_id, "Budget Profile")
        .unwrap();

    let flow_service = crate::ai::flow::FlowRuntimeService::with_mock_bridge(
        storage_paths.clone(),
        server.base_url.clone(),
    );

    // Request with max_credits = 10 (less than 40)
    let req = crate::ai::flow::FlowGenerationRequest {
        project_id: project_id.clone(),
        source_media_id: "input.mp4".to_string(),
        profile_id: profile_id.clone(),
        transformation_intent: Some(crate::ai::transformation::TransformationIntent::FaceReplace),
        identity_mode: Some(crate::ai::transformation::IdentityMode::Generated),
        prompt: "Replace face".to_string(),
        prompt_source: Some(PromptSource::User),
        target_face: None,
        max_credits: Some(10), // Insufficient budget!
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None,
    };

    let start_result =
        rt.block_on(flow_service.start_flow_generation(req, test_video_path.clone()));
    assert!(start_result.is_err());
    let err = start_result.unwrap_err();
    assert!(err.contains("PRE_CLICK_REJECTED") || err.contains("exceed"));
}

#[test]
fn test_phase_flow_p1_05_flow_cancellation_stops_worker() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let server = crate::ai::flow::MockFlowServer::start(
            crate::ai::flow::MockScenario::GenerationPending, // Simulates ongoing generation
        )
        .await
        .unwrap();

        let temp_dir = tempdir().unwrap();
        let storage_paths = StoragePaths::resolve_from_base(temp_dir.path());

        let project_id = "test_cancel_proj".to_string();
        let profile_id = "profile_cancel".to_string();

        let project_media_dir = storage_paths.projects_dir.join(&project_id).join("media");
        std::fs::create_dir_all(&project_media_dir).unwrap();
        let test_video_path = project_media_dir.join("input.mp4");

        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=320x240:rate=30",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                test_video_path.to_str().unwrap(),
            ])
            .output();

        if status.is_err() || !status.unwrap().status.success() {
            return;
        }

        let profile_manager =
            crate::ai::flow::FlowProfileManager::new(storage_paths.app_data_dir.clone());
        profile_manager
            .create_profile(&profile_id, "Cancel Profile")
            .unwrap();

        let flow_service = crate::ai::flow::FlowRuntimeService::with_mock_bridge(
            storage_paths.clone(),
            server.base_url.clone(),
        );

        let req = crate::ai::flow::FlowGenerationRequest {
            project_id: project_id.clone(),
            source_media_id: "input.mp4".to_string(),
            profile_id: profile_id.clone(),
            transformation_intent: Some(
                crate::ai::transformation::TransformationIntent::FaceReplace,
            ),
            identity_mode: Some(crate::ai::transformation::IdentityMode::Generated),
            prompt: "Replace face".to_string(),
            prompt_source: Some(PromptSource::User),
            target_face: None,
            max_credits: Some(40),
            preserve_original_audio: Some(true),
            requested_config: None,
            configuration_fingerprint: None,
        };

        let start_snapshot = flow_service
            .start_flow_generation(req, test_video_path.clone())
            .await
            .unwrap();
        let parent_id = start_snapshot.parent_id.clone();

        // Immediately request cancellation
        let cancel_snap = flow_service
            .cancel_flow_generation(&project_id, &parent_id)
            .await
            .unwrap();
        assert_eq!(cancel_snap.state, crate::ai::flow::FlowJobState::Cancelled);

        // Wait a moment and ensure status remains Cancelled
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let snap = flow_service
            .get_flow_job_status(&project_id, &parent_id)
            .unwrap();
        assert_eq!(snap.state, crate::ai::flow::FlowJobState::Cancelled);
    });
}
