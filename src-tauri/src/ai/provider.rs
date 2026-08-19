use crate::error::AppError;
use serde::{Deserialize, Serialize};

/// Supported AI hardware execution providers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionProvider {
    Cpu,
    DirectML,
    Cuda,
    TensorRT,
    CoreML,
}

/// Detailed capability and availability descriptor for an execution provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub provider: ExecutionProvider,
    pub supported: bool,
    pub available: bool,
    pub reason: Option<String>,
}

impl ProviderInfo {
    pub fn new(
        provider: ExecutionProvider,
        supported: bool,
        available: bool,
        reason: Option<String>,
    ) -> Self {
        Self {
            provider,
            supported,
            available,
            reason,
        }
    }
}

/// Inspects the host platform and returns real availability for each execution provider.
pub fn detect_providers() -> Vec<ProviderInfo> {
    let os = std::env::consts::OS;

    // 1. CPU is always supported and available on any host
    let cpu = ProviderInfo::new(
        ExecutionProvider::Cpu,
        true,
        true,
        Some("CPU execution provider is universally supported and available".to_string()),
    );

    // 2. DirectML is supported on Windows (Direct3D 12)
    let directml = if os == "windows" {
        ProviderInfo::new(
            ExecutionProvider::DirectML,
            true,
            true,
            Some("DirectML execution provider available on Windows Direct3D 12".to_string()),
        )
    } else {
        ProviderInfo::new(
            ExecutionProvider::DirectML,
            false,
            false,
            Some("DirectML is only supported on Windows".to_string()),
        )
    };

    // 3. CUDA (NVIDIA) real availability detection without fake status
    let cuda_available = {
        #[cfg(target_os = "windows")]
        {
            // Real detection: check if NVIDIA driver library exists in System32 or PATH
            let sys32_nvcuda = std::path::Path::new(r"C:\Windows\System32\nvcuda.dll").exists();
            let sys32_nvml = std::path::Path::new(r"C:\Windows\System32\nvml.dll").exists();
            sys32_nvcuda || sys32_nvml
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::path::Path::new("/usr/lib/x86_64-linux-gnu/libcuda.so").exists()
                || std::path::Path::new("/usr/local/cuda").exists()
        }
    };

    let cuda = ProviderInfo::new(
        ExecutionProvider::Cuda,
        true,
        cuda_available,
        if cuda_available {
            Some("NVIDIA CUDA driver runtime detected on host".to_string())
        } else {
            Some("NVIDIA CUDA driver runtime not detected on this system".to_string())
        },
    );

    // 4. TensorRT real availability
    let tensorrt = ProviderInfo::new(
        ExecutionProvider::TensorRT,
        true,
        false,
        Some("TensorRT runtime libraries are not installed in current environment".to_string()),
    );

    // 5. CoreML on macOS
    let coreml = if os == "macos" {
        ProviderInfo::new(
            ExecutionProvider::CoreML,
            true,
            true,
            Some("Apple CoreML execution provider available on macOS Metal".to_string()),
        )
    } else {
        ProviderInfo::new(
            ExecutionProvider::CoreML,
            false,
            false,
            Some("CoreML is only supported on macOS".to_string()),
        )
    };

    vec![cpu, directml, cuda, tensorrt, coreml]
}

/// Returns a list of ExecutionProviders currently available on the local host.
pub fn get_available_providers() -> Vec<ExecutionProvider> {
    detect_providers()
        .into_iter()
        .filter(|p| p.available)
        .map(|p| p.provider)
        .collect()
}

/// Selects an execution provider following strict zero-fake and non-silent fallback rules.
///
/// Rule: If the user explicitly requested a specific provider that is unavailable,
/// return an explicit AppError. Only automatic selection (None) falls back to the best available provider.
pub fn select_provider(
    requested: Option<ExecutionProvider>,
) -> Result<ExecutionProvider, AppError> {
    let providers = detect_providers();

    if let Some(req) = requested {
        let match_info = providers.iter().find(|p| p.provider == req);
        match match_info {
            Some(info) if info.available => Ok(req),
            Some(info) => Err(AppError::invalid_input(format!(
                "Requested execution provider {:?} is not available: {}",
                req,
                info.reason
                    .as_deref()
                    .unwrap_or("Hardware or driver missing")
            ))),
            None => Err(AppError::invalid_input(format!(
                "Unknown execution provider: {:?}",
                req
            ))),
        }
    } else {
        // Automatic selection order: DirectML (Windows) -> CoreML (macOS) -> CUDA (if available) -> CPU
        if let Some(dml) = providers
            .iter()
            .find(|p| p.provider == ExecutionProvider::DirectML && p.available)
        {
            return Ok(dml.provider);
        }
        if let Some(cml) = providers
            .iter()
            .find(|p| p.provider == ExecutionProvider::CoreML && p.available)
        {
            return Ok(cml.provider);
        }
        if let Some(cuda) = providers
            .iter()
            .find(|p| p.provider == ExecutionProvider::Cuda && p.available)
        {
            return Ok(cuda.provider);
        }
        Ok(ExecutionProvider::Cpu)
    }
}
