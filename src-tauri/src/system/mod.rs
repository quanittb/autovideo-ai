use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HardwareProfile {
    pub os: String,
    pub arch: String,
    pub cpu_cores: usize,
    pub total_memory_bytes: u64,
    pub gpu_name: Option<String>,
    pub vram_bytes: Option<u64>,
    pub is_directml_supported: bool,
    pub is_metal_supported: bool,
    pub is_cuda_supported: bool,
}

impl HardwareProfile {
    pub fn detect() -> Self {
        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        let is_directml_supported = os == "windows";
        let is_metal_supported = os == "macos";
        let is_cuda_supported = false; // Detection placeholder for Phase 1 without CUDA binary bindings

        Self {
            os,
            arch,
            cpu_cores,
            total_memory_bytes: 16 * 1024 * 1024 * 1024, // 16GB default baseline
            gpu_name: Some("System Primary GPU".to_string()),
            vram_bytes: Some(8 * 1024 * 1024 * 1024), // 8GB default
            is_directml_supported,
            is_metal_supported,
            is_cuda_supported,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoragePaths {
    pub app_data_dir: PathBuf,
    pub projects_dir: PathBuf,
    pub models_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub temp_dir: PathBuf,
}

impl StoragePaths {
    pub fn resolve_from_base(base_dir: &Path) -> Self {
        Self {
            app_data_dir: base_dir.to_path_buf(),
            projects_dir: base_dir.join("projects"),
            models_dir: base_dir.join("models"),
            cache_dir: base_dir.join("cache"),
            logs_dir: base_dir.join("logs"),
            temp_dir: base_dir.join("temp"),
        }
    }

    pub fn default_paths() -> Self {
        let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::resolve_from_base(&base.join(".autovideo_data"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_profile_detection() {
        let profile = HardwareProfile::detect();
        assert!(!profile.os.is_empty());
        assert!(profile.cpu_cores > 0);
    }

    #[test]
    fn test_storage_paths_resolution() {
        let base = PathBuf::from("/tmp/autovideo_test");
        let paths = StoragePaths::resolve_from_base(&base);
        assert_eq!(paths.projects_dir, base.join("projects"));
        assert_eq!(paths.models_dir, base.join("models"));
        assert_eq!(paths.cache_dir, base.join("cache"));
        assert_eq!(paths.logs_dir, base.join("logs"));
        assert_eq!(paths.temp_dir, base.join("temp"));
    }
}
