use crate::ai::provider::ExecutionProvider;
use crate::ai::tensor::TensorSpec;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Supported machine learning model serialization formats.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelFormat {
    Onnx,
}

/// Requirements and hardware constraints for model execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRequirements {
    pub min_memory_mb: Option<u64>,
    pub preferred_provider: Option<ExecutionProvider>,
    pub requires_gpu: bool,
}

impl Default for ModelRequirements {
    fn default() -> Self {
        Self {
            min_memory_mb: None,
            preferred_provider: None,
            requires_gpu: false,
        }
    }
}

/// Model lifecycle state machine states.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelState {
    Unloaded,
    Loading,
    Ready,
    Running,
    Error,
}

impl ModelState {
    /// Validates allowed model state transitions.
    pub fn can_transition_to(&self, next: ModelState) -> bool {
        match (self, next) {
            // UNLOADED -> LOADING
            (ModelState::Unloaded, ModelState::Loading) => true,
            // LOADING -> READY or ERROR
            (ModelState::Loading, ModelState::Ready) => true,
            (ModelState::Loading, ModelState::Error) => true,
            // READY -> RUNNING, LOADING, UNLOADED, or ERROR
            (ModelState::Ready, ModelState::Running) => true,
            (ModelState::Ready, ModelState::Loading) => true,
            (ModelState::Ready, ModelState::Unloaded) => true,
            (ModelState::Ready, ModelState::Error) => true,
            // RUNNING -> READY, ERROR, or UNLOADED
            (ModelState::Running, ModelState::Ready) => true,
            (ModelState::Running, ModelState::Error) => true,
            (ModelState::Running, ModelState::Unloaded) => true,
            // ERROR -> LOADING or UNLOADED
            (ModelState::Error, ModelState::Loading) => true,
            (ModelState::Error, ModelState::Unloaded) => true,
            // Self transitions / idempotent
            (a, b) if a == &b => true,
            _ => false,
        }
    }
}

/// Strongly typed AI Model Manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiModelManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub format: ModelFormat,
    pub path: PathBuf,
    pub description: String,
    pub input_specs: Vec<TensorSpec>,
    pub output_specs: Vec<TensorSpec>,
    pub requirements: ModelRequirements,
    #[serde(default)]
    pub is_production: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl AiModelManifest {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        format: ModelFormat,
        path: PathBuf,
        description: impl Into<String>,
        input_specs: Vec<TensorSpec>,
        output_specs: Vec<TensorSpec>,
        requirements: ModelRequirements,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            format,
            path,
            description: description.into(),
            input_specs,
            output_specs,
            requirements,
            is_production: false,
            created_at: now.clone(),
            updated_at: now,
            metadata: serde_json::json!({}),
        }
    }

    pub fn with_production(mut self, is_production: bool) -> Self {
        self.is_production = is_production;
        self
    }
}
