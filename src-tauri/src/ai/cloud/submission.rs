use super::cost::{CostGuard, DEFAULT_STANDARD_JOB_BUDGET_USD};
use super::error::CloudProviderError;
use super::job::CloudJobRequest;
use super::provider::CloudVideoProvider;
use super::registry::ProviderRegistry;
use super::router::{
    GenerationRouter, RoutingDecision, RoutingPreference, RoutingTarget, TaskClass,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedSubmissionPlan {
    pub task_class: TaskClass,
    pub routing_decision: RoutingDecision,
    pub budget_limit: f64,
    pub provider_id: String,
}

pub fn validate_and_prepare_cloud_submission(
    request: &CloudJobRequest,
    max_cost: Option<f64>,
    cloud_provider: &dyn CloudVideoProvider,
    registry: &ProviderRegistry,
) -> Result<ValidatedSubmissionPlan, CloudProviderError> {
    // 1. Authoritative budget validation (defaults to DEFAULT_STANDARD_JOB_BUDGET_USD: $3.00)
    let budget_limit = match max_cost {
        Some(val) => CostGuard::validate_budget(val)?,
        None => DEFAULT_STANDARD_JOB_BUDGET_USD,
    };

    // 2. Determine real TaskClass
    let task_class = TaskClass::from_str_or_default(&request.task_type);

    // 3. Obtain routing decision through single GenerationRouter & ProviderRegistry
    let decision = GenerationRouter::route_with_registry(
        task_class,
        RoutingPreference::CostSaving,
        request,
        cloud_provider,
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

    // 7. Verify executable provider adapter exists in registry
    if !registry.has_executable_adapter(&decision.provider_id) {
        return Err(CloudProviderError::ProviderUnavailable(format!(
            "PROVIDER_UNAVAILABLE: No executable adapter found for provider '{}'",
            decision.provider_id
        )));
    }

    Ok(ValidatedSubmissionPlan {
        task_class,
        routing_decision: decision.clone(),
        budget_limit,
        provider_id: decision.provider_id,
    })
}
