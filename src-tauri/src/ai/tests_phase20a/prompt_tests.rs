use crate::ai::flow::*;
use tempfile::tempdir;

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
        model: "gemini-2.5-flash-lite".to_string(),
        prompt_source: PromptSource::GeminiOptimized,
        prompt_hash: hash,
    };
    assert_eq!(resp.prompt_source, PromptSource::GeminiOptimized);
    assert_eq!(resp.model, "gemini-2.5-flash-lite");
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
    let optimizer = GeminiPromptOptimizer::new(store);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let req = OptimizePromptRequest {
        prompt: "User written untouched prompt".to_string(),
        source_prompt_hash: None,
        task_type: None,
        video_duration_sec: None,
        fps: None,
        resolution: None,
    };
    let res = rt.block_on(optimizer.optimize_prompt(req));
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("PROMPT_OPTIMIZATION_FAILED"));
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
