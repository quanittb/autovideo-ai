use crate::system::HardwareProfile;
use serde::{Deserialize, Serialize};

/// Real host device hardware diagnostics for AI model execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub os: String,
    pub arch: String,
    pub cpu_name: Option<String>,
    pub cpu_cores: usize,
    pub gpu_name: Option<String>,
    pub vram_bytes: Option<u64>,
    pub total_memory_bytes: Option<u64>,
    pub is_directml_supported: bool,
    pub is_cuda_supported: bool,
    pub is_metal_supported: bool,
}

impl DeviceInfo {
    pub fn detect() -> Self {
        let profile = HardwareProfile::detect();
        Self {
            os: profile.os,
            arch: profile.arch,
            cpu_name: Some(format!("Host CPU ({} Cores)", profile.cpu_cores)),
            cpu_cores: profile.cpu_cores,
            gpu_name: profile.gpu_name,
            vram_bytes: profile.vram_bytes,
            total_memory_bytes: Some(profile.total_memory_bytes),
            is_directml_supported: profile.is_directml_supported,
            is_cuda_supported: profile.is_cuda_supported,
            is_metal_supported: profile.is_metal_supported,
        }
    }
}
