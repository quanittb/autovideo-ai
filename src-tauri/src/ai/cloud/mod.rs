pub mod cost;
pub mod error;
pub mod job;
pub mod lifecycle;
pub mod provider;
pub mod providers;
pub mod registry;
pub mod resolver;
pub mod router;
pub mod segment;
pub mod store;
pub mod submission;
pub mod validator;

pub use cost::{
    CostBreakdown, CostConfidence, CostEstimate, CostGuard, CostStatus, LatencyTelemetry,
    DEFAULT_PREVIEW_BUDGET_USD, DEFAULT_STANDARD_JOB_BUDGET_USD,
};
pub use error::CloudProviderError;
pub use job::{
    CloudJobEventPayload, CloudJobRequest, CloudJobResult, CloudJobState, CloudJobStatus,
    CostRecord, InputAssets, JobErrorRecord, JobTimestamps, OutputArtifactRecord,
    PersistentCloudJob, RetryCounters, SubmissionState, ValidationPolicy,
    CURRENT_CLOUD_JOB_SCHEMA_VERSION,
};
pub use lifecycle::{
    CloudJobLifecycleService, EventSink, LifecycleTimingConfig, NoopEventSink, TauriEventSink,
    TestEventSink,
};
pub use provider::{
    CloudJobHandle, CloudVideoProvider, ProviderCapabilities, RemotePollResponse, RemoteStatus,
};
pub use providers::ReplicateProvider;
pub use registry::{ExecutionClass, PricingUnit, ProviderRecord, ProviderRegistry};
pub use resolver::{CloudProviderResolver, DefaultCloudProviderResolver};
pub use router::{
    GenerationRouter, GenerationTask, RoutingDecision, RoutingPreference, RoutingTarget, TaskClass,
    UserExecutionMode,
};
pub use segment::{SegmentPlanner, VideoSegment};
pub use store::{atomic_replace, validate_identifier, PersistentCloudJobStore};
pub use submission::{
    validate_and_prepare_cloud_submission, CloudSubmissionGate, DefaultCloudSubmissionGate,
    ValidatedSubmissionPlan,
};
pub use validator::CloudOutputValidator;
