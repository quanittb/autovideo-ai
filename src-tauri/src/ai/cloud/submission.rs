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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloudPreflightEvaluation {
    pub task_class: TaskClass,
    pub source_facts: Option<SourceMediaFacts>,
    pub routing_decision: RoutingDecision,
    pub budget_limit: f64,
    pub budget_approved: bool,
    pub submittable: bool,
    pub blocking_code: Option<String>,
}

pub fn evaluate_cloud_submission_preflight(
    request: &CloudJobRequest,
    max_cost: Option<f64>,
    registry: &ProviderRegistry,
) -> Result<CloudPreflightEvaluation, CloudProviderError> {
    // 1. Authoritative budget validation (defaults to DEFAULT_STANDARD_JOB_BUDGET_USD: $3.00)
    let budget_limit = match max_cost {
        Some(val) => CostGuard::validate_budget(val)?,
        None => DEFAULT_STANDARD_JOB_BUDGET_USD,
    };

    // 2. Determine real TaskClass using STRICT parsing (reject unknown tasks)
    let task_class = TaskClass::from_str_strict(&request.task_type)?;

    // 3. Background removal strictly forbids reference images
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

    // 4. Probing source media facts (if source_video is provided or required)
    let source_facts = if let Some(ref source_path) = request.source_video {
        Some(SourceMediaProbe::probe_file(source_path)?)
    } else if task_class == TaskClass::BackgroundRemoval {
        return Err(CloudProviderError::RequestInvalid(
            "SOURCE_VIDEO_REQUIRED: Source video is required for background removal".to_string(),
        ));
    } else {
        None
    };

    // 5. Obtain routing decision through single GenerationRouter & ProviderRegistry with COST_SAVING policy
    let decision = GenerationRouter::route_with_facts(
        task_class,
        RoutingPreference::CostSaving,
        request,
        source_facts.as_ref(),
        None,
        registry,
    );

    // 6. Check submittability and blocking codes
    let mut submittable = true;
    let mut blocking_code = None;
    let mut budget_approved = true;

    if decision.target == RoutingTarget::Local {
        submittable = false;
        blocking_code = Some("TASK_ROUTES_TO_LOCAL_EXECUTION".to_string());
    } else if decision.target == RoutingTarget::Unavailable || !decision.auto_submit_allowed {
        submittable = false;
        blocking_code = Some("ROUTING_UNAVAILABLE".to_string());
    } else {
        let cost_guard = CostGuard::new(budget_limit);
        if cost_guard
            .check_breakdown(&decision.cost_breakdown)
            .is_err()
        {
            budget_approved = false;
            submittable = false;
            blocking_code = Some("COST_BUDGET_EXCEEDED".to_string());
        } else if !registry.has_executable_adapter(&decision.provider_id, &decision.model_id) {
            submittable = false;
            blocking_code = Some("PROVIDER_UNAVAILABLE".to_string());
        }
    }

    Ok(CloudPreflightEvaluation {
        task_class,
        source_facts,
        routing_decision: decision,
        budget_limit,
        budget_approved,
        submittable,
        blocking_code,
    })
}

pub fn validate_and_prepare_cloud_submission(
    request: &CloudJobRequest,
    max_cost: Option<f64>,
    registry: &ProviderRegistry,
) -> Result<ValidatedSubmissionPlan, CloudProviderError> {
    let eval = evaluate_cloud_submission_preflight(request, max_cost, registry)?;

    if !eval.submittable {
        let code = eval
            .blocking_code
            .unwrap_or_else(|| "SUBMISSION_BLOCKED".to_string());
        let reason = eval.routing_decision.reason.clone();
        return match code.as_str() {
            "TASK_ROUTES_TO_LOCAL_EXECUTION" => Err(CloudProviderError::RequestInvalid(format!(
                "TASK_ROUTES_TO_LOCAL_EXECUTION: Task {:?} routes to local deterministic execution ($0.00) and cannot be submitted to cloud.",
                eval.task_class
            ))),
            "ROUTING_UNAVAILABLE" => Err(CloudProviderError::RequestInvalid(format!(
                "ROUTING_UNAVAILABLE: Task {:?} cannot be submitted to cloud provider. Reason: {}",
                eval.task_class, reason
            ))),
            "COST_BUDGET_EXCEEDED" => {
                let cost_guard = CostGuard::new(eval.budget_limit);
                cost_guard.check_breakdown(&eval.routing_decision.cost_breakdown)?;
                Err(CloudProviderError::CostLimitExceeded {
                    estimated: eval
                        .routing_decision
                        .estimated_cost
                        .estimated_usd
                        .unwrap_or(0.0),
                    limit: eval.budget_limit,
                })
            }
            "PROVIDER_UNAVAILABLE" => Err(CloudProviderError::ProviderUnavailable(format!(
                "PROVIDER_UNAVAILABLE: No executable adapter found for provider '{}' and model '{}'",
                eval.routing_decision.provider_id, eval.routing_decision.model_id
            ))),
            _ => Err(CloudProviderError::RequestInvalid(format!("{}: {}", code, reason))),
        };
    }

    Ok(ValidatedSubmissionPlan {
        task_class: eval.task_class,
        routing_decision: eval.routing_decision.clone(),
        budget_limit: eval.budget_limit,
        provider_key: ProviderKey::new(
            eval.routing_decision.provider_id,
            eval.routing_decision.model_id,
        ),
        source_facts: eval.source_facts,
    })
}
