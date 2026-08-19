use serde::{Deserialize, Serialize};

use crate::ai::generative::gate::ProductionGateErrorCode;

/// GPU hardware vendor classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Unknown,
}

impl GpuVendor {
    pub fn from_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("nvidia")
            || lower.contains("geforce")
            || lower.contains("rtx")
            || lower.contains("gtx")
            || lower.contains("quadro")
            || lower.contains("tesla")
        {
            Self::Nvidia
        } else if lower.contains("amd") || lower.contains("radeon") {
            Self::Amd
        } else if lower.contains("intel") || lower.contains("arc") || lower.contains("iris") {
            Self::Intel
        } else if lower.contains("apple")
            || lower.contains("m1")
            || lower.contains("m2")
            || lower.contains("m3")
            || lower.contains("m4")
        {
            Self::Apple
        } else {
            Self::Unknown
        }
    }
}

/// Detailed GPU device hardware profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuDeviceInfo {
    pub vendor: GpuVendor,
    pub device_name: String,
    pub total_vram_mb: u64,
    pub available_vram_mb: u64,
    pub allocated_vram_mb: u64,
    pub reserved_vram_mb: u64,
    pub cuda_available: bool,
    pub cuda_version: Option<String>,
    pub driver_version: Option<String>,
    pub compute_capability: Option<String>,
    pub device_count: usize,
    pub has_tensor_cores: bool,
}

/// Host CPU architecture and memory profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CpuDeviceInfo {
    pub architecture: String,
    pub logical_cores: usize,
    pub physical_cores: Option<usize>,
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
}

/// ML Python and inference library runtime environment info.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MlRuntimeInfo {
    pub python_version: Option<String>,
    pub pytorch_version: Option<String>,
    pub torch_cuda_version: Option<String>,
    pub diffusers_version: Option<String>,
    pub transformers_version: Option<String>,
    pub accelerate_version: Option<String>,
    pub safetensors_version: Option<String>,
}

/// Operating system details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    pub os_name: String,
    pub architecture: String,
}

/// Complete machine hardware and runtime probe report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HardwareProbeReport {
    pub gpu: Option<GpuDeviceInfo>,
    pub cpu: CpuDeviceInfo,
    pub runtime: MlRuntimeInfo,
    pub os: OsInfo,
}

/// Hardware capability tiers based on usable VRAM and compute support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityTier {
    Unsupported,
    CpuOnly,
    UltraLowVram,
    LowVram,
    Balanced,
    High,
    VeryHigh,
}

impl CapabilityTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unsupported => "UNSUPPORTED",
            Self::CpuOnly => "CPU_ONLY",
            Self::UltraLowVram => "ULTRA_LOW_VRAM",
            Self::LowVram => "LOW_VRAM",
            Self::Balanced => "BALANCED",
            Self::High => "HIGH",
            Self::VeryHigh => "VERY_HIGH",
        }
    }
}

/// Precision mode for neural execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PrecisionMode {
    Fp16,
    Fp32,
    Bf16,
}

impl PrecisionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fp16 => "FP16",
            Self::Fp32 => "FP32",
            Self::Bf16 => "BF16",
        }
    }
}

/// CPU/GPU memory offload strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OffloadStrategy {
    SequentialCpuOffload,
    ModelCpuOffload,
    None,
}

/// Full runtime execution profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProfile {
    pub tier: CapabilityTier,
    pub profile_name: String,
    pub target_width: u32,
    pub target_height: u32,
    pub precision: PrecisionMode,
    pub offload_strategy: OffloadStrategy,
    pub enable_vae_slicing: bool,
    pub enable_vae_tiling: bool,
    pub enable_attention_slicing: bool,
    pub max_temporal_window: usize,
    pub batch_size: usize,
    pub recommended_steps: u32,
    pub estimated_memory_envelope_mb: u64,
    pub warnings: Vec<String>,
    pub fallback_tiers: Vec<CapabilityTier>,
}

/// Explicit machine-readable hardware status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HardwareStatus {
    HardwareSupported,
    HardwareSupportedWithLimitations,
    HardwareProfileSelected,
    HardwareProfileFallback,
    ProductionModelHardwareBlocked,
    ProductionModelUnavailable,
}

impl HardwareStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HardwareSupported => "HARDWARE_SUPPORTED",
            Self::HardwareSupportedWithLimitations => "HARDWARE_SUPPORTED_WITH_LIMITATIONS",
            Self::HardwareProfileSelected => "HARDWARE_PROFILE_SELECTED",
            Self::HardwareProfileFallback => "HARDWARE_PROFILE_FALLBACK",
            Self::ProductionModelHardwareBlocked => "PRODUCTION_MODEL_HARDWARE_BLOCKED",
            Self::ProductionModelUnavailable => "PRODUCTION_MODEL_UNAVAILABLE",
        }
    }
}

/// User override preference options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UserOverridePreference {
    Auto,
    Performance,
    Balanced,
    Quality,
    LowMemory,
}

/// Empirical precision test measurement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrecisionProbeResult {
    pub tested_precision: PrecisionMode,
    pub stable: bool,
    pub nan_detected: bool,
    pub inf_detected: bool,
    pub reason: String,
}

/// Empirical benchmark telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkMeasurement {
    pub peak_allocated_mb: f64,
    pub peak_reserved_mb: f64,
    pub inference_latency_ms: f64,
    pub oom_occurred: bool,
    pub nan_inf_occurred: bool,
    pub success: bool,
}

/// Individual profile fallback attempt record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFallbackAttempt {
    pub tier: CapabilityTier,
    pub result: String,
    pub reason: Option<String>,
}

/// Comprehensive persisted hardware capability report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityReport {
    pub timestamp: String,
    pub hardware: HardwareProbeReport,
    pub precision_test: PrecisionProbeResult,
    pub benchmark: Option<BenchmarkMeasurement>,
    pub selected_tier: CapabilityTier,
    pub selected_profile: RuntimeProfile,
    pub status: HardwareStatus,
    pub user_override: UserOverridePreference,
    pub warnings: Vec<String>,
    pub fallback_history: Vec<ProfileFallbackAttempt>,
}

/// Capability classifier mapping empirical hardware data to runtime profiles.
pub struct CapabilityClassifier;

impl CapabilityClassifier {
    /// Computes safe usable VRAM accounting for desktop allocations and safety margins.
    pub fn calculate_usable_vram(total_mb: u64, free_mb: u64, safety_margin_mb: u64) -> u64 {
        let conservative_free = std::cmp::min(total_mb.saturating_sub(500), free_mb);
        conservative_free.saturating_sub(safety_margin_mb)
    }

    /// Determines the capability tier based on usable VRAM and compute support.
    pub fn determine_tier(gpu: Option<&GpuDeviceInfo>, usable_vram_mb: u64) -> CapabilityTier {
        match gpu {
            None => CapabilityTier::CpuOnly,
            Some(g) if !g.cuda_available => CapabilityTier::CpuOnly,
            Some(_) => {
                if usable_vram_mb >= 15000 {
                    CapabilityTier::VeryHigh
                } else if usable_vram_mb >= 10000 {
                    CapabilityTier::High
                } else if usable_vram_mb >= 5500 {
                    CapabilityTier::Balanced
                } else if usable_vram_mb >= 2500 {
                    CapabilityTier::LowVram
                } else if usable_vram_mb >= 1500 {
                    CapabilityTier::UltraLowVram
                } else {
                    CapabilityTier::Unsupported
                }
            }
        }
    }

    /// Builds the optimal runtime profile for a given capability tier and precision.
    pub fn build_profile_for_tier(
        tier: CapabilityTier,
        precision: PrecisionMode,
    ) -> RuntimeProfile {
        match tier {
            CapabilityTier::VeryHigh => RuntimeProfile {
                tier,
                profile_name: "ProfileVeryHigh".to_string(),
                target_width: 576,
                target_height: 1024,
                precision,
                offload_strategy: OffloadStrategy::None,
                enable_vae_slicing: false,
                enable_vae_tiling: false,
                enable_attention_slicing: false,
                max_temporal_window: 16,
                batch_size: 1,
                recommended_steps: 25,
                estimated_memory_envelope_mb: 12000,
                warnings: Vec::new(),
                fallback_tiers: vec![
                    CapabilityTier::High,
                    CapabilityTier::Balanced,
                    CapabilityTier::LowVram,
                ],
            },
            CapabilityTier::High => RuntimeProfile {
                tier,
                profile_name: "ProfileHigh".to_string(),
                target_width: 576,
                target_height: 1024,
                precision,
                offload_strategy: OffloadStrategy::ModelCpuOffload,
                enable_vae_slicing: true,
                enable_vae_tiling: false,
                enable_attention_slicing: false,
                max_temporal_window: 16,
                batch_size: 1,
                recommended_steps: 20,
                estimated_memory_envelope_mb: 8000,
                warnings: Vec::new(),
                fallback_tiers: vec![
                    CapabilityTier::Balanced,
                    CapabilityTier::LowVram,
                    CapabilityTier::UltraLowVram,
                ],
            },
            CapabilityTier::Balanced => RuntimeProfile {
                tier,
                profile_name: "ProfileBalanced".to_string(),
                target_width: 512,
                target_height: 768,
                precision,
                offload_strategy: OffloadStrategy::ModelCpuOffload,
                enable_vae_slicing: true,
                enable_vae_tiling: true,
                enable_attention_slicing: true,
                max_temporal_window: 12,
                batch_size: 1,
                recommended_steps: 20,
                estimated_memory_envelope_mb: 5000,
                warnings: Vec::new(),
                fallback_tiers: vec![CapabilityTier::LowVram, CapabilityTier::UltraLowVram],
            },
            CapabilityTier::LowVram => RuntimeProfile {
                tier,
                profile_name: "ProfileLowVram".to_string(),
                target_width: 288,
                target_height: 512,
                precision,
                offload_strategy: OffloadStrategy::SequentialCpuOffload,
                enable_vae_slicing: true,
                enable_vae_tiling: true,
                enable_attention_slicing: true,
                max_temporal_window: 8,
                batch_size: 1,
                recommended_steps: 15,
                estimated_memory_envelope_mb: 3200,
                warnings: vec![
                    "Limited VRAM: using sequential CPU offloading and 288x512 neural resolution"
                        .to_string(),
                ],
                fallback_tiers: vec![CapabilityTier::UltraLowVram],
            },
            CapabilityTier::UltraLowVram => RuntimeProfile {
                tier,
                profile_name: "ProfileUltraLowVram".to_string(),
                target_width: 256,
                target_height: 384,
                precision: PrecisionMode::Fp32,
                offload_strategy: OffloadStrategy::SequentialCpuOffload,
                enable_vae_slicing: true,
                enable_vae_tiling: true,
                enable_attention_slicing: true,
                max_temporal_window: 4,
                batch_size: 1,
                recommended_steps: 12,
                estimated_memory_envelope_mb: 2200,
                warnings: vec![
                    "Ultra-low VRAM envelope: using 256x384 resolution with minimum batch size"
                        .to_string(),
                ],
                fallback_tiers: vec![],
            },
            CapabilityTier::CpuOnly | CapabilityTier::Unsupported => RuntimeProfile {
                tier,
                profile_name: "ProfileUnsupported".to_string(),
                target_width: 256,
                target_height: 256,
                precision: PrecisionMode::Fp32,
                offload_strategy: OffloadStrategy::SequentialCpuOffload,
                enable_vae_slicing: true,
                enable_vae_tiling: true,
                enable_attention_slicing: true,
                max_temporal_window: 1,
                batch_size: 1,
                recommended_steps: 10,
                estimated_memory_envelope_mb: 1000,
                warnings: vec!["GPU acceleration is unavailable on this device".to_string()],
                fallback_tiers: vec![],
            },
        }
    }

    /// Full classification pipeline.
    pub fn classify(
        probe: &HardwareProbeReport,
        precision_test: &PrecisionProbeResult,
    ) -> (CapabilityTier, RuntimeProfile, HardwareStatus, Vec<String>) {
        let mut warnings = Vec::new();

        let (_usable_vram, tier) = match &probe.gpu {
            Some(gpu) => {
                let usable =
                    Self::calculate_usable_vram(gpu.total_vram_mb, gpu.available_vram_mb, 512);
                let t = Self::determine_tier(Some(gpu), usable);
                (usable, t)
            }
            None => (0, CapabilityTier::CpuOnly),
        };

        if !precision_test.stable {
            warnings.push(format!("Precision downgrade: {}", precision_test.reason));
        }

        let selected_precision = if precision_test.stable {
            precision_test.tested_precision
        } else {
            PrecisionMode::Fp32
        };

        let profile = Self::build_profile_for_tier(tier, selected_precision);

        let status = match tier {
            CapabilityTier::VeryHigh | CapabilityTier::High | CapabilityTier::Balanced => {
                HardwareStatus::HardwareSupported
            }
            CapabilityTier::LowVram | CapabilityTier::UltraLowVram => {
                HardwareStatus::HardwareSupportedWithLimitations
            }
            CapabilityTier::CpuOnly | CapabilityTier::Unsupported => {
                HardwareStatus::ProductionModelHardwareBlocked
            }
        };

        (tier, profile, status, warnings)
    }

    /// Applies user override with hard safety clamping to prevent OOM.
    pub fn apply_user_override(
        base_profile: &RuntimeProfile,
        override_pref: UserOverridePreference,
        usable_vram_mb: u64,
    ) -> (RuntimeProfile, Option<String>) {
        let mut profile = base_profile.clone();
        let mut warning = None;

        match override_pref {
            UserOverridePreference::Auto => {}
            UserOverridePreference::LowMemory => {
                profile = Self::build_profile_for_tier(CapabilityTier::LowVram, profile.precision);
            }
            UserOverridePreference::Balanced => {
                if usable_vram_mb >= 5000 {
                    profile =
                        Self::build_profile_for_tier(CapabilityTier::Balanced, profile.precision);
                } else {
                    warning = Some("Balanced mode exceeds safe memory limit; maintaining safe low-VRAM configuration".to_string());
                }
            }
            UserOverridePreference::Quality => {
                if usable_vram_mb >= 10000 {
                    profile = Self::build_profile_for_tier(CapabilityTier::High, profile.precision);
                } else if usable_vram_mb >= 5000 {
                    profile =
                        Self::build_profile_for_tier(CapabilityTier::Balanced, profile.precision);
                    warning = Some(
                        "Quality mode clamped to Balanced profile due to memory envelope"
                            .to_string(),
                    );
                } else {
                    warning = Some("Quality mode exceeds safe memory capability of this device; maintaining safe configuration".to_string());
                }
            }
            UserOverridePreference::Performance => {
                profile.recommended_steps =
                    std::cmp::max(10, profile.recommended_steps.saturating_sub(5));
            }
        }

        (profile, warning)
    }
}

/// Compatible pipeline execution plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompatiblePipelinePlan {
    pub profile: RuntimeProfile,
    pub active_models: Vec<String>,
    pub temporal_window_size: usize,
    pub temporal_overlap: usize,
    pub upscale_needed: bool,
    pub target_resolution: (u32, u32),
    pub total_windows: usize,
}

/// Capability-aware pipeline planner.
pub struct PipelinePlanner;

impl PipelinePlanner {
    pub fn plan_pipeline(
        models_present: &[String],
        profile: &RuntimeProfile,
        requested_resolution: (u32, u32),
        requested_duration_s: f32,
    ) -> Result<CompatiblePipelinePlan, ProductionGateErrorCode> {
        if profile.tier == CapabilityTier::Unsupported || profile.tier == CapabilityTier::CpuOnly {
            return Err(ProductionGateErrorCode::ProductionModelHardwareBlocked);
        }

        if !models_present
            .iter()
            .any(|m| m.contains("sd15") || m.contains("v1-5"))
        {
            return Err(ProductionGateErrorCode::ProductionModelUnavailable);
        }

        let total_frames = (requested_duration_s * 30.0).round() as usize;
        let window_size = profile.max_temporal_window;
        let overlap = std::cmp::min(2, window_size / 4);
        let stride = std::cmp::max(1, window_size.saturating_sub(overlap));
        let total_windows = if total_frames <= window_size {
            1
        } else {
            ((total_frames - overlap) as f64 / stride as f64).ceil() as usize
        };

        let upscale_needed = profile.target_width != requested_resolution.0
            || profile.target_height != requested_resolution.1;

        Ok(CompatiblePipelinePlan {
            profile: profile.clone(),
            active_models: models_present.to_vec(),
            temporal_window_size: window_size,
            temporal_overlap: overlap,
            upscale_needed,
            target_resolution: requested_resolution,
            total_windows,
        })
    }
}
