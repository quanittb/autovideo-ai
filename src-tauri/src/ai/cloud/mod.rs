pub mod cost;
pub mod error;
pub mod job;
pub mod provider;
pub mod providers;
pub mod registry;
pub mod router;
pub mod segment;
pub mod submission;

pub use cost::{
    CostBreakdown, CostConfidence, CostEstimate, CostGuard, CostStatus, LatencyTelemetry,
    DEFAULT_PREVIEW_BUDGET_USD, DEFAULT_STANDARD_JOB_BUDGET_USD,
};
pub use error::CloudProviderError;
pub use job::{CloudJobManager, CloudJobRequest, CloudJobResult, CloudJobState, CloudJobStatus};
pub use provider::{
    CloudJobHandle, CloudVideoProvider, ProviderCapabilities, RemotePollResponse, RemoteStatus,
};
pub use providers::ReplicateProvider;
pub use registry::{ExecutionClass, PricingUnit, ProviderRecord, ProviderRegistry};
pub use router::{
    GenerationRouter, GenerationTask, RoutingDecision, RoutingPreference, RoutingTarget, TaskClass,
    UserExecutionMode,
};
pub use segment::{SegmentPlanner, VideoSegment};
pub use submission::{validate_and_prepare_cloud_submission, ValidatedSubmissionPlan};
