pub mod browser_session;
pub mod capability;
pub mod manifest;
pub mod manual_chrome;
pub mod mock_flow_server;
pub mod orchestrator;
pub mod output_validator;
pub mod playwright_bridge;
pub mod profile;
pub mod prompt_optimizer;
pub mod segment;
pub mod stitcher;
pub mod store;

pub use browser_session::{FlowBrowserSessionManager, ManualLoginBrowserSession};
pub use capability::{
    FlowCapabilityContext, FlowCapabilityPolicy, FlowCapabilitySource, FlowCreditRecord,
    FlowGenerationMode, FlowModelCapabilitiesSnapshot, FlowModelCapability,
    OMNI_EDIT_UPLOADED_VIDEO_ESTIMATED_CREDITS_PER_GENERATION,
    OMNI_VIDEO_GENERATE_ESTIMATED_CREDITS_PER_GENERATION,
};
pub use manifest::{
    FlowChildSegmentRecord, FlowChildSubmissionState, FlowFinalAudioPolicy, FlowGenerationManifest,
    FlowJobEventPayload, FlowJobSnapshot, FlowJobState, FlowObservedGenerationConfig,
    FlowOutputArtifactRecord, FlowRequestedGenerationConfig, FlowSegmentPlan,
    CURRENT_FLOW_MANIFEST_SCHEMA_VERSION,
};
pub use manual_chrome::{ManualChromeProcess, SystemChromeLauncher};
pub use mock_flow_server::{MockFlowServer, MockFlowServerHandle, MockScenario};
pub use orchestrator::{
    compute_configuration_fingerprint, FlowCancellationRegistry, FlowCostProvenance,
    FlowCreditSource, FlowCreditStatus, FlowGenerationPreflight, FlowGenerationRequest,
    FlowOrchestrator, FlowProfileCreditStatus, FlowRuntimeService,
};
pub use output_validator::FlowOutputValidator;
pub use playwright_bridge::{
    FlowAuthStatus, FlowGenerationSettings, FlowPollResult, FlowSettingsReadback, PlaywrightBridge,
    PlaywrightSidecarProcess,
};
pub use profile::{FlowProfileGuard, FlowProfileInfo, FlowProfileManager, FlowProfileSnapshot};
pub use prompt_optimizer::{
    calculate_prompt_hash, is_valid_gemini_key, parse_google_error, GeminiCredentialManager,
    GeminiCredentialSource, GeminiCredentialStatus, GeminiPromptOptimizer, GeminiStatusResponse,
    GeminiVerificationStatus, OptimizePromptRequest, OptimizePromptResponse,
    PromptOptimizationCapabilityPolicy, PromptSource, ResolvedGeminiCredential, SecretStore,
    DEFAULT_GEMINI_API_KEY, DEFAULT_PROMPT_OPTIMIZATION_MODEL, GEMINI_API_KEY_SENTINEL,
};
pub use segment::FlowVideoSegmenter;
pub use stitcher::FlowStitcher;
pub use store::FlowJobStore;
