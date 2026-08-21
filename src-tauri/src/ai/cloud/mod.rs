pub mod cache;
pub mod cost;
pub mod error;
pub mod job;
pub mod lifecycle;
pub mod live_execution_guard;
pub mod manifest;
pub mod orchestrator;
pub mod provider;
pub mod providers;
pub mod registry;
pub mod resolver;
pub mod router;
pub mod segment;
pub mod spec;
pub mod store;
pub mod submission;
pub mod uploader;
pub mod validator;

pub use cache::{SegmentCacheManager, SegmentCacheMeta};
pub use cost::{
    CostBreakdown, CostConfidence, CostEstimate, CostGuard, CostStatus, LatencyTelemetry,
    DEFAULT_PREVIEW_BUDGET_USD, DEFAULT_STANDARD_JOB_BUDGET_USD,
};
pub use error::CloudProviderError;
pub use job::{
    ArtifactContainer, ArtifactDescriptor, ArtifactVideoCodec, AuthorizedAssetPreview,
    CloudJobEventPayload, CloudJobRequest, CloudJobResult, CloudJobState, CloudJobStatus,
    CloudSubmissionPreflight, CostRecord, InputAssets, JobErrorRecord, JobTimestamps,
    OutputArtifactRecord, PersistentCloudJob, PreviewAssetKind, RetryCounters, SubmissionState,
    ValidationPolicy, CURRENT_CLOUD_JOB_SCHEMA_VERSION,
};
pub use lifecycle::{
    CloudJobLifecycleService, EventSink, LifecycleTimingConfig, NoopEventSink, TauriEventSink,
    TestEventSink,
};
pub use live_execution_guard::{
    EnvLiveExecutionPolicy, LiveExecutionPolicy, MockLiveExecutionPolicy, PaidLiveExecutionGuard,
};
pub use manifest::{
    FinalAudioPolicy, SegmentBoundary, SegmentChildRecord, SegmentPlan, SegmentedChildSnapshot,
    SegmentedCloudJobManifest, SegmentedCloudJobSnapshot, SegmentedJobState,
};
pub use orchestrator::{SegmentedCloudJobOrchestrator, SegmentedCloudSubmissionPreflight};
pub use provider::{
    CloudJobHandle, CloudVideoProvider, ProviderCapabilities, ProviderKey, RemotePollResponse,
    RemoteStatus, ResolutionPolicy, ResolutionTier, TargetFps,
};
pub use providers::{
    PrunaPVideoReplaceProvider, ReplicateBriaBgRemovalProvider, ReplicateProvider,
};
pub use registry::{ExecutionClass, PricingTier, PricingUnit, ProviderRecord, ProviderRegistry};
pub use resolver::{CloudProviderResolver, DefaultCloudProviderResolver, ResolvedProviderRuntime};
pub use router::{
    GenerationRouter, GenerationTask, RoutingBlockCode, RoutingDecision, RoutingPreference,
    RoutingTarget, TaskClass, UserExecutionMode,
};
pub use segment::{
    FinalAudioMuxer, SegmentPlanner, SegmentSplitter, SegmentStitcher,
    DEFAULT_MAX_SEGMENT_DURATION_SEC, SEGMENTATION_POLICY_VERSION, SPLIT_ENCODING_POLICY_VERSION,
};
pub use spec::{
    BackgroundRemovalSpec, DetailedTimingFacts, PreparedBackgroundRemoval,
    PreparedCharacterReplacement, PreparedProviderSubmission, ProviderSubmissionSpec,
    ProviderTaskSpec, Rational, SourceMediaFacts, SourceMediaProbe,
};
pub use store::{
    atomic_replace, validate_identifier, PersistentCloudJobStore, SegmentedCloudJobStore,
};
pub use submission::{
    evaluate_cloud_submission_preflight, validate_and_prepare_cloud_submission,
    CloudPreflightEvaluation, CloudSubmissionGate, DefaultCloudSubmissionGate,
    ValidatedSubmissionPlan,
};
pub use uploader::{
    MockAssetUploader, ProviderAssetUploader, ReplicateAssetUploader, UploadedAsset,
};
pub use validator::CloudOutputValidator;
