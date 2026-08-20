use super::cost::{CostGuard, DEFAULT_STANDARD_JOB_BUDGET_USD};
use super::error::CloudProviderError;
use super::job::CloudJobRequest;
use super::provider::ProviderKey;
use super::registry::ProviderRegistry;
use super::router::{
    GenerationRouter, RoutingDecision, RoutingPreference, RoutingTarget, TaskClass,
};
use super::spec::{SourceMediaFacts, SourceMediaProbe};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedSubmissionPlan {
    pub task_class: TaskClass,
    pub routing_decision: RoutingDecision,
    pub budget_limit: f64,
    pub provider_key: ProviderKey,
    pub source_facts: Option<SourceMediaFacts>,
}

pub trait CloudSubmissionGate: Send + Sync {
    fn validate_and_prepare(
        &self,
        request: &CloudJobRequest,
        max_cost: Option<f64>,
        registry: &ProviderRegistry,
    ) -> Result<ValidatedSubmissionPlan, CloudProviderError>;
}

#[derive(Default, Clone)]
pub struct DefaultCloudSubmissionGate;

impl DefaultCloudSubmissionGate {
    pub fn new() -> Self {
        Self
    }
}

impl CloudSubmissionGate for DefaultCloudSubmissionGate {
    fn validate_and_prepare(
        &self,
        request: &CloudJobRequest,
        max_cost: Option<f64>,
        registry: &ProviderRegistry,
    ) -> Result<ValidatedSubmissionPlan, CloudProviderError> {
        validate_and_prepare_cloud_submission(request, max_cost, registry)
    }
}

pub fn validate_and_prepare_cloud_submission(
    request: &CloudJobRequest,
    max_cost: Option<f64>,
    registry: &ProviderRegistry,
) -> Result<ValidatedSubmissionPlan, CloudProviderError> {
    // 1. Authoritative budget validation (defaults to DEFAULT_STANDARD_JOB_BUDGET_USD: $3.00)
    let budget_limit = match max_cost {
        Some(val) => CostGuard::validate_budget(val)?,
        None => DEFAULT_STANDARD_JOB_BUDGET_USD,
    };

    // 2. Determine real TaskClass using STRICT parsing (reject unknown tasks)
    let task_class = TaskClass::from_str_strict(&request.task_type)?;

    // 3. Probing source media facts (if source_video is provided or required)
    let source_facts = if let Some(ref source_path) = request.source_video {
        Some(SourceMediaProbe::probe_file(source_path)?)
    } else if task_class == TaskClass::BackgroundRemoval {
        return Err(CloudProviderError::RequestInvalid(
            "SOURCE_VIDEO_REQUIRED: Source video is required for background removal".to_string(),
        ));
    } else {
        None
    };

    // 4. Background removal strictly forbids reference images
    if task_class == TaskClass::BackgroundRemoval {
        let has_references = request
            .reference_images
            .as_ref()
            .map(|r| !r.is_empty())
            .unwrap_or(false)
            || request.reference_image.is_some();
        if has_references {
            return Err(CloudProviderError::RequestInvalid(
                "UNEXPECTED_REFERENCE_INPUTS_FOR_BACKGROUND_REMOVAL: Background removal requires 0 reference images".to_string(),
            ));
        }
    }

    // 5. Obtain routing decision through single GenerationRouter & ProviderRegistry with COST_SAVING policy
    let decision = GenerationRouter::route_with_facts(
        task_class,
        RoutingPreference::CostSaving,
        request,
        source_facts.as_ref(),
        None,
        registry,
    );

    // 4. Reject local deterministic tasks from paid cloud submission
    if decision.target == RoutingTarget::Local {
        return Err(CloudProviderError::RequestInvalid(format!(
            "TASK_ROUTES_TO_LOCAL_EXECUTION: Task {:?} routes to local deterministic execution ($0.00) and cannot be submitted to cloud.",
            task_class
        )));
    }

    // 5. Reject unavailable or non-auto-submittable routes
    if decision.target == RoutingTarget::Unavailable || !decision.auto_submit_allowed {
        return Err(CloudProviderError::RequestInvalid(format!(
            "ROUTING_UNAVAILABLE: Task {:?} cannot be submitted to cloud provider. Reason: {}",
            task_class, decision.reason
        )));
    }

    // 6. Authoritative backend CostGuard budget check
    let cost_guard = CostGuard::new(budget_limit);
    cost_guard.check_breakdown(&decision.cost_breakdown)?;

    // 7. Verify executable provider adapter exists in registry for specific (provider_id, model_id)
    if !registry.has_executable_adapter(&decision.provider_id, &decision.model_id) {
        return Err(CloudProviderError::ProviderUnavailable(format!(
            "PROVIDER_UNAVAILABLE: No executable adapter found for provider '{}' and model '{}'",
            decision.provider_id, decision.model_id
        )));
    }

    Ok(ValidatedSubmissionPlan {
        task_class,
        routing_decision: decision.clone(),
        budget_limit,
        provider_key: ProviderKey::new(decision.provider_id, decision.model_id),
        source_facts,
    })
}
