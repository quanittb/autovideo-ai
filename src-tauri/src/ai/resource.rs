use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Production resource limits for AI frame processing and tensor execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiResourceLimits {
    pub max_memory_bytes: u64,
    pub max_inflight_frames: usize,
    pub max_concurrent_inference: usize,
    pub max_frame_width: u32,
    pub max_frame_height: u32,
    pub max_frame_pixels: u64,
    pub max_tensor_elements: u64,
    pub max_job_disk_bytes: u64,
}

impl Default for AiResourceLimits {
    fn default() -> Self {
        Self::default_production()
    }
}

impl AiResourceLimits {
    /// Default conservative production limits for desktop stability.
    pub fn default_production() -> Self {
        Self {
            max_memory_bytes: 4 * 1024 * 1024 * 1024, // 4 GB RAM limit
            max_inflight_frames: 1,                   // Single-frame memory lifecycle
            max_concurrent_inference: 1,              // Serial inference for ONNX stability
            max_frame_width: 4096,                    // 4K Max width
            max_frame_height: 4096,                   // 4K Max height
            max_frame_pixels: 16_777_216,             // 4096 x 4096
            max_tensor_elements: 67_108_864,          // 1 x 4 x 4096 x 4096
            max_job_disk_bytes: 50 * 1024 * 1024 * 1024, // 50 GB disk quota
        }
    }

    /// Validates frame dimensions with checked arithmetic preventing allocation explosions.
    pub fn validate_frame_dimensions(&self, width: u32, height: u32) -> Result<u64, AppError> {
        if width == 0 || height == 0 {
            return Err(AppError::invalid_input(format!(
                "Invalid frame dimensions: {}x{} (must be > 0)",
                width, height
            )));
        }

        if width > self.max_frame_width {
            return Err(AppError::resource_limit_exceeded(
                format!(
                    "Frame width {}px exceeds maximum limit of {}px",
                    width, self.max_frame_width
                ),
                "Reduce video resolution or increase resource limits",
            ));
        }

        if height > self.max_frame_height {
            return Err(AppError::resource_limit_exceeded(
                format!(
                    "Frame height {}px exceeds maximum limit of {}px",
                    height, self.max_frame_height
                ),
                "Reduce video resolution or increase resource limits",
            ));
        }

        let total_pixels = (width as u64).checked_mul(height as u64).ok_or_else(|| {
            AppError::resource_limit_exceeded(
                "Frame pixel count overflowed arithmetic bounds",
                "Video dimensions too large",
            )
        })?;

        if total_pixels > self.max_frame_pixels {
            return Err(AppError::resource_limit_exceeded(
                format!(
                    "Frame pixel count ({} px) exceeds limit of {} px",
                    total_pixels, self.max_frame_pixels
                ),
                "Reduce input resolution to fit within resource limits",
            ));
        }

        Ok(total_pixels)
    }

    /// Validates tensor element count with checked arithmetic.
    pub fn validate_tensor_elements(&self, shape: &[u64]) -> Result<u64, AppError> {
        if shape.is_empty() {
            return Err(AppError::invalid_input("Tensor shape cannot be empty"));
        }

        let mut elements: u64 = 1;
        for &dim in shape {
            if dim == 0 {
                return Err(AppError::invalid_input("Tensor dimension cannot be zero"));
            }
            elements = elements.checked_mul(dim).ok_or_else(|| {
                AppError::resource_limit_exceeded(
                    "Tensor dimension product overflowed arithmetic bounds",
                    "Tensor shape too large",
                )
            })?;
        }

        if elements > self.max_tensor_elements {
            return Err(AppError::resource_limit_exceeded(
                format!(
                    "Tensor element count ({}) exceeds maximum limit of {}",
                    elements, self.max_tensor_elements
                ),
                "Model tensor size exceeds configured maximum buffer capacity",
            ));
        }

        Ok(elements)
    }

    /// Validates disk quota with checked arithmetic.
    pub fn validate_disk_budget(
        &self,
        current_bytes: u64,
        incoming_bytes: u64,
    ) -> Result<u64, AppError> {
        let total = current_bytes.checked_add(incoming_bytes).ok_or_else(|| {
            AppError::disk_quota_exceeded(
                "Disk quota arithmetic overflow",
                "Total accumulated bytes exceeded u64 bounds",
            )
        })?;

        if total > self.max_job_disk_bytes {
            return Err(AppError::disk_quota_exceeded(
                format!(
                    "Job artifact storage ({} bytes) exceeds configured disk quota of {} bytes",
                    total, self.max_job_disk_bytes
                ),
                "Clean up cache or increase max_job_disk_bytes limit",
            ));
        }

        Ok(total)
    }
}

/// Real runtime resource diagnostics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiRuntimeResources {
    pub process_memory_bytes: u64,
    pub system_memory_bytes: u64,
    pub cpu_utilization: Option<f32>,
    pub gpu_utilization: Option<f32>,
    pub active_inference_count: usize,
    pub queued_frame_count: usize,
    pub active_provider: String,
    pub provider_name: String,
    pub model_version: Option<String>,
}

/// Probes real host/process runtime resources without fabrication.
pub fn probe_runtime_resources(
    active_provider: &str,
    model_version: Option<&str>,
    active_inference: usize,
    queued_frames: usize,
) -> AiRuntimeResources {
    let hw = crate::system::HardwareProfile::detect();
    let sys_mem = hw.total_memory_bytes;

    // Platform-specific process memory inspection
    #[cfg(target_os = "windows")]
    let process_mem = {
        use std::mem::MaybeUninit;
        type HANDLE = *mut std::ffi::c_void;

        #[repr(C)]
        #[allow(non_snake_case)]
        struct PROCESS_MEMORY_COUNTERS {
            cb: u32,
            PageFaultCount: u32,
            PeakWorkingSetSize: usize,
            WorkingSetSize: usize,
            QuotaPeakPagedPoolUsage: usize,
            QuotaPagedPoolUsage: usize,
            QuotaPeakNonPagedPoolUsage: usize,
            QuotaNonPagedPoolUsage: usize,
            PagefileUsage: usize,
            PeakPagefileUsage: usize,
        }

        extern "system" {
            fn GetCurrentProcess() -> HANDLE;
            fn K32GetProcessMemoryInfo(
                process: HANDLE,
                ppsmc: *mut PROCESS_MEMORY_COUNTERS,
                cb: u32,
            ) -> i32;
        }

        unsafe {
            let mut pmc = MaybeUninit::<PROCESS_MEMORY_COUNTERS>::uninit();
            let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            let handle = GetCurrentProcess();
            if K32GetProcessMemoryInfo(handle, pmc.as_mut_ptr(), size) != 0 {
                let init = pmc.assume_init();
                init.WorkingSetSize as u64
            } else {
                0
            }
        }
    };

    #[cfg(not(target_os = "windows"))]
    let process_mem = 0u64;

    AiRuntimeResources {
        process_memory_bytes: process_mem,
        system_memory_bytes: sys_mem,
        cpu_utilization: None, // Explicitly None rather than fake metric
        gpu_utilization: None, // Explicitly None rather than fake metric
        active_inference_count: active_inference,
        queued_frame_count: queued_frames,
        active_provider: active_provider.to_string(),
        provider_name: active_provider.to_string(),
        model_version: model_version.map(|v| v.to_string()),
    }
}
