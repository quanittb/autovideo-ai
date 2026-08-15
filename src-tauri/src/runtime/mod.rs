use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    DirectMl,
    Metal,
    Cuda,
    Cpu,
    Cloud,
}

impl RuntimeKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::DirectMl => "DirectML (Windows DirectX 12)",
            Self::Metal => "Metal (Apple Silicon)",
            Self::Cuda => "NVIDIA CUDA",
            Self::Cpu => "Fallback CPU Runtime",
            Self::Cloud => "Remote Cloud Inference Adapter",
        }
    }
}

pub trait InferenceRuntime: Send + Sync {
    fn kind(&self) -> RuntimeKind;
    fn is_available(&self) -> bool;
    fn initialize(&self) -> Result<(), AppError>;
    fn unload(&self) -> Result<(), AppError>;
}
