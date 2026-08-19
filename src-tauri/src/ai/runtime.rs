use crate::ai::device::DeviceInfo;
use crate::ai::manifest::{AiModelManifest, ModelState};
use crate::ai::provider::{select_provider, ExecutionProvider};
use crate::error::AppError;
use serde::{Deserialize, Serialize};

/// High-level AI Runtime lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "message", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeState {
    Uninitialized,
    Initializing,
    Ready,
    Running,
    Error(String),
}

/// Comprehensive telemetry snapshot of the AI Runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub state: RuntimeState,
    pub provider: ExecutionProvider,
    pub device: DeviceInfo,
    pub loaded_model_id: Option<String>,
    pub model_state: ModelState,
    pub error: Option<String>,
}

/// Abstract AI Runtime interface decoupled from specific inference engines (ONNX, TensorRT, CoreML).
pub trait AiRuntime: Send + Sync {
    fn initialize(&mut self, requested_provider: Option<ExecutionProvider>)
        -> Result<(), AppError>;
    fn load_model(&mut self, manifest: &AiModelManifest) -> Result<(), AppError>;
    fn unload_model(&mut self) -> Result<(), AppError>;
    fn status(&self) -> RuntimeStatus;
    fn provider(&self) -> ExecutionProvider;
}

/// Production Default AI Runtime implementing hardware lifecycle management.
#[derive(Debug, Clone)]
pub struct DefaultAiRuntime {
    state: RuntimeState,
    provider: ExecutionProvider,
    device: DeviceInfo,
    loaded_model: Option<AiModelManifest>,
    model_state: ModelState,
    error: Option<String>,
}

impl Default for DefaultAiRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultAiRuntime {
    pub fn new() -> Self {
        Self {
            state: RuntimeState::Uninitialized,
            provider: ExecutionProvider::Cpu,
            device: DeviceInfo::detect(),
            loaded_model: None,
            model_state: ModelState::Unloaded,
            error: None,
        }
    }

    pub fn status(&self) -> RuntimeStatus {
        RuntimeStatus {
            state: self.state.clone(),
            provider: self.provider,
            device: self.device.clone(),
            loaded_model_id: self.loaded_model.as_ref().map(|m| m.id.clone()),
            model_state: self.model_state,
            error: self.error.clone(),
        }
    }

    pub fn provider(&self) -> ExecutionProvider {
        self.provider
    }
}

impl AiRuntime for DefaultAiRuntime {
    fn initialize(
        &mut self,
        requested_provider: Option<ExecutionProvider>,
    ) -> Result<(), AppError> {
        self.state = RuntimeState::Initializing;
        self.error = None;

        match select_provider(requested_provider) {
            Ok(provider) => {
                self.provider = provider;
                self.device = DeviceInfo::detect();
                self.state = RuntimeState::Ready;
                Ok(())
            }
            Err(e) => {
                let err_msg = e.to_string();
                self.state = RuntimeState::Error(err_msg.clone());
                self.error = Some(err_msg);
                Err(e)
            }
        }
    }

    fn load_model(&mut self, manifest: &AiModelManifest) -> Result<(), AppError> {
        if self.state == RuntimeState::Uninitialized {
            self.initialize(None)?;
        }

        if !self.model_state.can_transition_to(ModelState::Loading) {
            return Err(AppError::invalid_input(format!(
                "Cannot load model from current state {:?}",
                self.model_state
            )));
        }

        self.model_state = ModelState::Loading;

        // Verify model requirements against selected runtime provider
        if manifest.requirements.requires_gpu && self.provider == ExecutionProvider::Cpu {
            self.model_state = ModelState::Error;
            let msg =
                "Model requires dedicated GPU acceleration, but CPU provider is active".to_string();
            self.error = Some(msg.clone());
            return Err(AppError::invalid_input(msg));
        }

        // Validate model file exists on disk
        if !manifest.path.exists() {
            self.model_state = ModelState::Error;
            let msg = format!(
                "Model weights file not found at: {}",
                manifest.path.display()
            );
            self.error = Some(msg);
            return Err(AppError::file_not_found(manifest.path.to_string_lossy()));
        }

        self.loaded_model = Some(manifest.clone());
        self.model_state = ModelState::Ready;
        self.error = None;
        Ok(())
    }

    fn unload_model(&mut self) -> Result<(), AppError> {
        if !self.model_state.can_transition_to(ModelState::Unloaded) {
            return Err(AppError::invalid_input(format!(
                "Cannot unload model from state {:?}",
                self.model_state
            )));
        }

        self.loaded_model = None;
        self.model_state = ModelState::Unloaded;
        Ok(())
    }

    fn status(&self) -> RuntimeStatus {
        RuntimeStatus {
            state: self.state.clone(),
            provider: self.provider,
            device: self.device.clone(),
            loaded_model_id: self.loaded_model.as_ref().map(|m| m.id.clone()),
            model_state: self.model_state,
            error: self.error.clone(),
        }
    }

    fn provider(&self) -> ExecutionProvider {
        self.provider
    }
}
