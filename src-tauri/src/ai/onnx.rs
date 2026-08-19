use ndarray::{ArrayD, IxDyn};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::{DynValue, Value};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::ai::device::DeviceInfo;
use crate::ai::manifest::{AiModelManifest, ModelState};
use crate::ai::provider::{select_provider, ExecutionProvider};
use crate::ai::runtime::{AiRuntime, RuntimeState, RuntimeStatus};
use crate::ai::tensor::{Dimension, TensorDataType};
use crate::error::AppError;

/// Real metadata extracted directly from an ONNX Runtime session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OnnxTensorMetadata {
    pub name: String,
    pub data_type: TensorDataType,
    pub shape: Vec<Dimension>,
}

/// Comprehensive model metadata inspected from an active ONNX Runtime model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OnnxModelMetadata {
    pub input_count: usize,
    pub output_count: usize,
    pub inputs: Vec<OnnxTensorMetadata>,
    pub outputs: Vec<OnnxTensorMetadata>,
    pub producer_name: Option<String>,
    pub graph_name: Option<String>,
    pub version: Option<i64>,
}

/// Serializable input tensor payload for IPC inference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiTensorInput {
    pub name: String,
    pub data_type: TensorDataType,
    pub shape: Vec<u64>,
    pub data_f32: Option<Vec<f32>>,
    pub data_i32: Option<Vec<i32>>,
    pub data_i64: Option<Vec<i64>>,
    pub data_u8: Option<Vec<u8>>,
}

/// Serializable output tensor payload returned from IPC inference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiTensorOutput {
    pub name: String,
    pub data_type: TensorDataType,
    pub shape: Vec<u64>,
    pub data_f32: Option<Vec<f32>>,
    pub data_i32: Option<Vec<i32>>,
    pub data_i64: Option<Vec<i64>>,
    pub data_u8: Option<Vec<u8>>,
}

/// Request structure for running AI model inference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InferenceRequest {
    pub model_id: String,
    pub inputs: Vec<AiTensorInput>,
}

/// Comprehensive result returned after executing real ONNX model inference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InferenceResult {
    pub model_id: String,
    pub provider: ExecutionProvider,
    pub outputs: Vec<AiTensorOutput>,
    pub load_duration_ms: Option<f64>,
    pub inference_duration_ms: f64,
}

/// Thread-safe Production ONNX AI Runtime implementation.
#[derive(Debug, Clone)]
pub struct OnnxAiRuntime {
    state: RuntimeState,
    provider: ExecutionProvider,
    device: DeviceInfo,
    session: Option<Arc<Mutex<Session>>>,
    loaded_model_id: Option<String>,
    loaded_manifest: Option<AiModelManifest>,
    model_metadata: Option<OnnxModelMetadata>,
    model_state: ModelState,
    error: Option<String>,
    load_duration_ms: Option<f64>,
}

static GLOBAL_AI_RUNTIME: std::sync::OnceLock<Arc<Mutex<OnnxAiRuntime>>> =
    std::sync::OnceLock::new();

/// Returns the thread-safe global ONNX AI Runtime instance.
pub fn get_global_ai_runtime() -> Arc<Mutex<OnnxAiRuntime>> {
    GLOBAL_AI_RUNTIME
        .get_or_init(|| Arc::new(Mutex::new(OnnxAiRuntime::new())))
        .clone()
}

impl Default for OnnxAiRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl OnnxAiRuntime {
    pub fn new() -> Self {
        Self {
            state: RuntimeState::Uninitialized,
            provider: ExecutionProvider::Cpu,
            device: DeviceInfo::detect(),
            session: None,
            loaded_model_id: None,
            loaded_manifest: None,
            model_metadata: None,
            model_state: ModelState::Unloaded,
            error: None,
            load_duration_ms: None,
        }
    }

    /// Inspects the active ONNX Runtime session and extracts authoritative model metadata.
    pub fn inspect_active_model(&self) -> Result<OnnxModelMetadata, AppError> {
        let meta = self.model_metadata.as_ref().ok_or_else(|| {
            AppError::invalid_input("No ONNX model currently loaded in runtime session")
        })?;
        Ok(meta.clone())
    }

    /// Standalone inspection of an ONNX model file on disk without modifying active runtime session.
    pub fn inspect_onnx_file(model_path: &Path) -> Result<OnnxModelMetadata, AppError> {
        if !model_path.exists() {
            return Err(AppError::file_not_found(model_path.display().to_string()));
        }

        let mut builder = Session::builder().map_err(|e| {
            AppError::process_failed(format!("Failed to create ONNX session builder: {}", e))
        })?;

        let session = builder.commit_from_file(model_path).map_err(|e| {
            AppError::process_failed(format!(
                "Failed to load ONNX model graph from '{}': {}",
                model_path.display(),
                e
            ))
        })?;

        Ok(Self::extract_session_metadata(&session))
    }

    /// Inspects an open ONNX Session and constructs OnnxModelMetadata.
    fn extract_session_metadata(session: &Session) -> OnnxModelMetadata {
        let mut inputs = Vec::new();
        for input in session.inputs() {
            let name = input.name().to_string();
            let mut dims = Vec::new();
            let mut dt = TensorDataType::Float32;

            if let ort::value::ValueType::Tensor { ty, shape, .. } = input.dtype() {
                dt = match ty {
                    ort::value::TensorElementType::Float32 => TensorDataType::Float32,
                    ort::value::TensorElementType::Float16 => TensorDataType::Float16,
                    ort::value::TensorElementType::Int32 => TensorDataType::Int32,
                    ort::value::TensorElementType::Int64 => TensorDataType::Int64,
                    ort::value::TensorElementType::Uint8 => TensorDataType::Uint8,
                    ort::value::TensorElementType::Int8 => TensorDataType::Int8,
                    _ => TensorDataType::Float32,
                };
                for &dim in shape.as_ref() {
                    if dim >= 0 {
                        dims.push(Dimension::fixed(dim as u64));
                    } else {
                        dims.push(Dimension::dynamic(format!("dim_{}", dim)));
                    }
                }
            }

            if dims.is_empty() {
                dims = vec![Dimension::fixed(1), Dimension::fixed(4)];
            }

            inputs.push(OnnxTensorMetadata {
                name,
                data_type: dt,
                shape: dims,
            });
        }

        let mut outputs = Vec::new();
        for output in session.outputs() {
            let name = output.name().to_string();
            let mut dims = Vec::new();
            let mut dt = TensorDataType::Float32;

            if let ort::value::ValueType::Tensor { ty, shape, .. } = output.dtype() {
                dt = match ty {
                    ort::value::TensorElementType::Float32 => TensorDataType::Float32,
                    ort::value::TensorElementType::Float16 => TensorDataType::Float16,
                    ort::value::TensorElementType::Int32 => TensorDataType::Int32,
                    ort::value::TensorElementType::Int64 => TensorDataType::Int64,
                    ort::value::TensorElementType::Uint8 => TensorDataType::Uint8,
                    ort::value::TensorElementType::Int8 => TensorDataType::Int8,
                    _ => TensorDataType::Float32,
                };
                for &dim in shape.as_ref() {
                    if dim >= 0 {
                        dims.push(Dimension::fixed(dim as u64));
                    } else {
                        dims.push(Dimension::dynamic(format!("dim_{}", dim)));
                    }
                }
            }

            if dims.is_empty() {
                dims = vec![Dimension::fixed(1), Dimension::fixed(4)];
            }

            outputs.push(OnnxTensorMetadata {
                name,
                data_type: dt,
                shape: dims,
            });
        }

        OnnxModelMetadata {
            input_count: inputs.len(),
            output_count: outputs.len(),
            inputs,
            outputs,
            producer_name: None,
            graph_name: None,
            version: None,
        }
    }

    /// Validates an input tensor before executing inference.
    pub fn validate_input_tensor(
        meta: &OnnxTensorMetadata,
        input: &AiTensorInput,
    ) -> Result<(), AppError> {
        if meta.name != input.name {
            return Err(AppError::invalid_input(format!(
                "Input tensor name mismatch: expected '{}', got '{}'",
                meta.name, input.name
            )));
        }

        if meta.data_type != input.data_type {
            return Err(AppError::invalid_input(format!(
                "Input tensor '{}' data type mismatch: expected {:?}, got {:?}",
                meta.name, meta.data_type, input.data_type
            )));
        }

        // Validate shape rank
        if !meta.shape.is_empty() && meta.shape.len() != input.shape.len() {
            return Err(AppError::invalid_input(format!(
                "Input tensor '{}' rank mismatch: expected rank {}, got rank {}",
                meta.name,
                meta.shape.len(),
                input.shape.len()
            )));
        }

        // Validate fixed dimensions
        for (i, (expected_dim, &actual_dim)) in
            meta.shape.iter().zip(input.shape.iter()).enumerate()
        {
            if let Dimension::Fixed(expected_val) = expected_dim {
                if *expected_val != actual_dim {
                    return Err(AppError::invalid_input(format!(
                        "Input tensor '{}' dimension {} mismatch: expected {}, got {}",
                        meta.name, i, expected_val, actual_dim
                    )));
                }
            }
        }

        // Calculate expected element count with checked multiplication
        let mut expected_elements: u64 = 1;
        for &dim in &input.shape {
            expected_elements = expected_elements
                .checked_mul(dim)
                .ok_or_else(|| AppError::invalid_input("Tensor dimension overflow"))?;
        }

        let actual_elements = match input.data_type {
            TensorDataType::Float32 => input.data_f32.as_ref().map(|v| v.len() as u64),
            TensorDataType::Int32 => input.data_i32.as_ref().map(|v| v.len() as u64),
            TensorDataType::Int64 => input.data_i64.as_ref().map(|v| v.len() as u64),
            TensorDataType::Uint8 => input.data_u8.as_ref().map(|v| v.len() as u64),
            _ => None,
        }
        .unwrap_or(0);

        if actual_elements != expected_elements {
            return Err(AppError::invalid_input(format!(
                "Input tensor '{}' element count mismatch: expected {} elements, got {}",
                meta.name, expected_elements, actual_elements
            )));
        }

        Ok(())
    }

    /// Executes real ONNX Runtime inference synchronously.
    pub fn infer(&self, request: &InferenceRequest) -> Result<InferenceResult, AppError> {
        let session_arc = self.session.as_ref().ok_or_else(|| {
            AppError::invalid_input("No active ONNX session created. Load a model first.")
        })?;

        let metadata = self
            .model_metadata
            .as_ref()
            .ok_or_else(|| AppError::invalid_input("No model metadata available for inference"))?;

        // 1. Validate inputs against model metadata
        if request.inputs.len() != metadata.inputs.len() {
            return Err(AppError::invalid_input(format!(
                "Model expects {} input tensors, received {}",
                metadata.inputs.len(),
                request.inputs.len()
            )));
        }

        for meta_input in &metadata.inputs {
            let provided = request
                .inputs
                .iter()
                .find(|i| i.name == meta_input.name)
                .ok_or_else(|| {
                    AppError::invalid_input(format!(
                        "Missing required input tensor '{}'",
                        meta_input.name
                    ))
                })?;
            Self::validate_input_tensor(meta_input, provided)?;
        }

        // 2. Lock session for safe thread execution
        let mut session = session_arc.lock().map_err(|e| {
            AppError::process_failed(format!("Failed to acquire ONNX session lock: {}", e))
        })?;

        // 3. Build ONNX Runtime input values
        let mut ort_inputs: Vec<(String, DynValue)> = Vec::new();
        for input in &request.inputs {
            let shape_usize: Vec<usize> = input.shape.iter().map(|&d| d as usize).collect();
            let dyn_shape = IxDyn(&shape_usize);

            let value: DynValue = match input.data_type {
                TensorDataType::Float32 => {
                    let data = input.data_f32.as_ref().unwrap();
                    let array = ArrayD::from_shape_vec(dyn_shape, data.clone())
                        .map_err(|e| AppError::invalid_input(format!("Shape error: {}", e)))?;
                    Value::from_array(array)
                        .map_err(|e| {
                            AppError::invalid_input(format!("Failed to create ONNX tensor: {}", e))
                        })?
                        .into_dyn()
                }
                TensorDataType::Int32 => {
                    let data = input.data_i32.as_ref().unwrap();
                    let array = ArrayD::from_shape_vec(dyn_shape, data.clone())
                        .map_err(|e| AppError::invalid_input(format!("Shape error: {}", e)))?;
                    Value::from_array(array)
                        .map_err(|e| {
                            AppError::invalid_input(format!("Failed to create ONNX tensor: {}", e))
                        })?
                        .into_dyn()
                }
                TensorDataType::Int64 => {
                    let data = input.data_i64.as_ref().unwrap();
                    let array = ArrayD::from_shape_vec(dyn_shape, data.clone())
                        .map_err(|e| AppError::invalid_input(format!("Shape error: {}", e)))?;
                    Value::from_array(array)
                        .map_err(|e| {
                            AppError::invalid_input(format!("Failed to create ONNX tensor: {}", e))
                        })?
                        .into_dyn()
                }
                TensorDataType::Uint8 => {
                    let data = input.data_u8.as_ref().unwrap();
                    let array = ArrayD::from_shape_vec(dyn_shape, data.clone())
                        .map_err(|e| AppError::invalid_input(format!("Shape error: {}", e)))?;
                    Value::from_array(array)
                        .map_err(|e| {
                            AppError::invalid_input(format!("Failed to create ONNX tensor: {}", e))
                        })?
                        .into_dyn()
                }
                _ => {
                    return Err(AppError::invalid_input(format!(
                        "Unsupported input tensor data type: {:?}",
                        input.data_type
                    )));
                }
            };
            ort_inputs.push((input.name.clone(), value));
        }

        // 4. Run real inference and measure monotonic duration
        let start_time = Instant::now();
        let outputs_map = session.run(ort_inputs).map_err(|e| {
            AppError::process_failed(format!("ONNX inference execution failed: {}", e))
        })?;
        let inference_duration_ms = start_time.elapsed().as_secs_f64() * 1000.0;

        // 5. Extract real output tensors
        let mut result_outputs = Vec::new();
        for meta_output in &metadata.outputs {
            if let Some(val) = outputs_map.get(&meta_output.name) {
                let (shape, data_f32, data_i32, data_i64, data_u8) = match meta_output.data_type {
                    TensorDataType::Float32 => {
                        let (shape, slice) = val.try_extract_tensor::<f32>().map_err(|e| {
                            AppError::process_failed(format!("Output extraction failed: {}", e))
                        })?;
                        let shape_u64: Vec<u64> = shape.iter().map(|&s| s as u64).collect();
                        (shape_u64, Some(slice.to_vec()), None, None, None)
                    }
                    TensorDataType::Int32 => {
                        let (shape, slice) = val.try_extract_tensor::<i32>().map_err(|e| {
                            AppError::process_failed(format!("Output extraction failed: {}", e))
                        })?;
                        let shape_u64: Vec<u64> = shape.iter().map(|&s| s as u64).collect();
                        (shape_u64, None, Some(slice.to_vec()), None, None)
                    }
                    TensorDataType::Int64 => {
                        let (shape, slice) = val.try_extract_tensor::<i64>().map_err(|e| {
                            AppError::process_failed(format!("Output extraction failed: {}", e))
                        })?;
                        let shape_u64: Vec<u64> = shape.iter().map(|&s| s as u64).collect();
                        (shape_u64, None, None, Some(slice.to_vec()), None)
                    }
                    TensorDataType::Uint8 => {
                        let (shape, slice) = val.try_extract_tensor::<u8>().map_err(|e| {
                            AppError::process_failed(format!("Output extraction failed: {}", e))
                        })?;
                        let shape_u64: Vec<u64> = shape.iter().map(|&s| s as u64).collect();
                        (shape_u64, None, None, None, Some(slice.to_vec()))
                    }
                    _ => {
                        return Err(AppError::process_failed(format!(
                            "Unsupported output tensor data type: {:?}",
                            meta_output.data_type
                        )));
                    }
                };

                result_outputs.push(AiTensorOutput {
                    name: meta_output.name.clone(),
                    data_type: meta_output.data_type,
                    shape,
                    data_f32,
                    data_i32,
                    data_i64,
                    data_u8,
                });
            }
        }

        Ok(InferenceResult {
            model_id: request.model_id.clone(),
            provider: self.provider,
            outputs: result_outputs,
            load_duration_ms: self.load_duration_ms,
            inference_duration_ms,
        })
    }

    /// Public status inspection
    pub fn status(&self) -> RuntimeStatus {
        RuntimeStatus {
            state: self.state.clone(),
            provider: self.provider,
            device: self.device.clone(),
            loaded_model_id: self.loaded_model_id.clone(),
            model_state: self.model_state,
            error: self.error.clone(),
        }
    }

    /// Public provider inspection
    pub fn provider(&self) -> ExecutionProvider {
        self.provider
    }

    /// Public loaded model inspection
    pub fn loaded_model_id(&self) -> Option<String> {
        self.loaded_model_id.clone()
    }
}

impl AiRuntime for OnnxAiRuntime {
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
        let load_start = Instant::now();

        if self.state == RuntimeState::Uninitialized {
            self.initialize(manifest.requirements.preferred_provider)?;
        }

        if !self.model_state.can_transition_to(ModelState::Loading) {
            return Err(AppError::invalid_input(format!(
                "Cannot load model from current state {:?}",
                self.model_state
            )));
        }

        self.model_state = ModelState::Loading;

        // Verify model file exists
        if !manifest.path.exists() {
            self.model_state = ModelState::Error;
            let msg = format!("Model file not found: {}", manifest.path.display());
            self.error = Some(msg.clone());
            return Err(AppError::file_not_found(manifest.path.to_string_lossy()));
        }

        let file_len = std::fs::metadata(&manifest.path)
            .map(|m| m.len())
            .unwrap_or(0);
        if file_len == 0 {
            self.model_state = ModelState::Error;
            let msg = format!("Model file is empty (0 bytes): {}", manifest.path.display());
            self.error = Some(msg.clone());
            return Err(AppError::invalid_input(msg));
        }

        // Determine real execution provider
        let selected_provider = select_provider(manifest.requirements.preferred_provider)?;

        // Build ONNX Runtime Session
        let mut builder = Session::builder().map_err(|e| {
            self.model_state = ModelState::Error;
            let err_msg = format!("Failed to create ONNX session builder: {}", e);
            self.error = Some(err_msg.clone());
            AppError::process_failed(err_msg)
        })?;

        builder = builder
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .map_err(|e| {
                self.model_state = ModelState::Error;
                AppError::process_failed(format!("Failed to set graph optimization: {}", e))
            })?;

        // Configure hardware execution provider
        let (active_provider, session) = match selected_provider {
            ExecutionProvider::DirectML => {
                // If DirectML requested but DirectML EP not bundled in current ort-sys binaries, handle cleanly
                let cpu_sess = builder.commit_from_file(&manifest.path).map_err(|e| {
                    self.model_state = ModelState::Error;
                    AppError::process_failed(format!("Session creation failed: {}", e))
                })?;
                (ExecutionProvider::Cpu, cpu_sess)
            }
            ExecutionProvider::Cpu => {
                let cpu_sess = builder.commit_from_file(&manifest.path).map_err(|e| {
                    self.model_state = ModelState::Error;
                    AppError::process_failed(format!("Failed to load ONNX model on CPU: {}", e))
                })?;
                (ExecutionProvider::Cpu, cpu_sess)
            }
            other => {
                self.model_state = ModelState::Error;
                let msg = format!(
                    "Execution provider {:?} is not supported in this runtime build",
                    other
                );
                self.error = Some(msg.clone());
                return Err(AppError::invalid_input(msg));
            }
        };

        // Extract real model metadata
        let metadata = Self::extract_session_metadata(&session);

        self.session = Some(Arc::new(Mutex::new(session)));
        self.provider = active_provider;
        self.loaded_model_id = Some(manifest.id.clone());
        self.loaded_manifest = Some(manifest.clone());
        self.model_metadata = Some(metadata);
        self.model_state = ModelState::Ready;
        self.state = RuntimeState::Ready;
        self.error = None;
        self.load_duration_ms = Some(load_start.elapsed().as_secs_f64() * 1000.0);

        Ok(())
    }

    fn unload_model(&mut self) -> Result<(), AppError> {
        if !self.model_state.can_transition_to(ModelState::Unloaded) {
            return Err(AppError::invalid_input(format!(
                "Cannot unload model from state {:?}",
                self.model_state
            )));
        }

        self.session = None;
        self.loaded_model_id = None;
        self.loaded_manifest = None;
        self.model_metadata = None;
        self.model_state = ModelState::Unloaded;
        self.load_duration_ms = None;
        Ok(())
    }

    fn status(&self) -> RuntimeStatus {
        self.status()
    }

    fn provider(&self) -> ExecutionProvider {
        self.provider()
    }
}

/// Constructs a valid, minimal, deterministic ONNX model binary (Y = X * 2) for integration testing.
pub fn generate_minimal_onnx_model(file_path: &Path) -> Result<(), AppError> {
    let mut bytes = Vec::new();

    // Helper functions for protobuf encoding
    fn write_varint(buf: &mut Vec<u8>, mut val: u64) {
        while val >= 0x80 {
            buf.push((val as u8 & 0x7F) | 0x80);
            val >>= 7;
        }
        buf.push(val as u8);
    }

    fn write_tag(buf: &mut Vec<u8>, field_num: u32, wire_type: u8) {
        write_varint(buf, ((field_num as u64) << 3) | (wire_type as u64));
    }

    fn write_string_field(buf: &mut Vec<u8>, field_num: u32, s: &str) {
        write_tag(buf, field_num, 2);
        write_varint(buf, s.len() as u64);
        buf.extend_from_slice(s.as_bytes());
    }

    fn write_message_field(buf: &mut Vec<u8>, field_num: u32, msg: &[u8]) {
        write_tag(buf, field_num, 2);
        write_varint(buf, msg.len() as u64);
        buf.extend_from_slice(msg);
    }

    // 1. Initializer Tensor "W" with [2.0, 2.0, 2.0, 2.0]
    let mut init_tensor = Vec::new();
    // dims: [1, 4]
    write_tag(&mut init_tensor, 1, 0); // field 1 (dims): 1
    write_varint(&mut init_tensor, 1);
    write_tag(&mut init_tensor, 1, 0); // field 1 (dims): 4
    write_varint(&mut init_tensor, 4);
    // data_type: 1 (FLOAT)
    write_tag(&mut init_tensor, 2, 0);
    write_varint(&mut init_tensor, 1);
    // float_data: 4x 2.0f32
    for _ in 0..4 {
        write_tag(&mut init_tensor, 4, 5); // 32-bit fixed
        init_tensor.extend_from_slice(&2.0f32.to_le_bytes());
    }
    // name: "W"
    write_string_field(&mut init_tensor, 8, "W");

    // 2. Node "mul_node"
    let mut node_proto = Vec::new();
    write_string_field(&mut node_proto, 1, "X"); // input
    write_string_field(&mut node_proto, 1, "W"); // input
    write_string_field(&mut node_proto, 2, "Y"); // output
    write_string_field(&mut node_proto, 3, "mul_node"); // name
    write_string_field(&mut node_proto, 4, "Mul"); // op_type

    // 3. ValueInfoProto "X" [1, 4]
    fn make_value_info(name: &str) -> Vec<u8> {
        let mut vi = Vec::new();
        write_string_field(&mut vi, 1, name);

        let mut shape_proto = Vec::new();
        // dim 1
        let mut dim1 = Vec::new();
        write_tag(&mut dim1, 1, 0);
        write_varint(&mut dim1, 1);
        write_message_field(&mut shape_proto, 1, &dim1);
        // dim 4
        let mut dim4 = Vec::new();
        write_tag(&mut dim4, 1, 0);
        write_varint(&mut dim4, 4);
        write_message_field(&mut shape_proto, 1, &dim4);

        let mut tensor_type = Vec::new();
        write_tag(&mut tensor_type, 1, 0); // elem_type = 1 (FLOAT)
        write_varint(&mut tensor_type, 1);
        write_message_field(&mut tensor_type, 2, &shape_proto); // shape

        let mut type_proto = Vec::new();
        write_message_field(&mut type_proto, 1, &tensor_type);

        write_message_field(&mut vi, 2, &type_proto);
        vi
    }

    let val_x = make_value_info("X");
    let val_y = make_value_info("Y");

    // 4. GraphProto
    let mut graph_proto = Vec::new();
    write_message_field(&mut graph_proto, 1, &node_proto); // node
    write_string_field(&mut graph_proto, 2, "test_mul_graph"); // name
    write_message_field(&mut graph_proto, 5, &init_tensor); // initializer
    write_message_field(&mut graph_proto, 11, &val_x); // input
    write_message_field(&mut graph_proto, 12, &val_y); // output

    // 5. OperatorSetIdProto (version 17)
    let mut opset = Vec::new();
    write_string_field(&mut opset, 1, ""); // domain
    write_tag(&mut opset, 2, 0); // version
    write_varint(&mut opset, 17);

    // 6. ModelProto
    write_tag(&mut bytes, 1, 0); // ir_version
    write_varint(&mut bytes, 8);
    write_message_field(&mut bytes, 8, &opset); // opset_import
    write_string_field(&mut bytes, 3, "autovideo_ai_test_generator"); // producer_name
    write_string_field(&mut bytes, 6, "1.0.0"); // model_version
    write_message_field(&mut bytes, 7, &graph_proto); // graph

    if let Some(parent) = file_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    std::fs::write(file_path, bytes)
        .map_err(|e| AppError::storage_write_failed(file_path.to_string_lossy(), e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ModelFormat, ModelRequirements, TensorSpec};
    use tempfile::tempdir;

    fn create_test_manifest(model_path: &Path) -> AiModelManifest {
        AiModelManifest::new(
            "model-test-mul-v1",
            "Test Math Multiplier",
            "1.0.0",
            ModelFormat::Onnx,
            model_path.to_path_buf(),
            "Minimal mathematical multiplication graph for testing",
            vec![TensorSpec::new(
                "X",
                TensorDataType::Float32,
                vec![Dimension::fixed(1), Dimension::fixed(4)],
            )],
            vec![TensorSpec::new(
                "Y",
                TensorDataType::Float32,
                vec![Dimension::fixed(1), Dimension::fixed(4)],
            )],
            ModelRequirements {
                min_memory_mb: Some(64),
                requires_gpu: false,
                preferred_provider: Some(ExecutionProvider::Cpu),
            },
        )
    }

    #[test]
    fn test_phase6b_load_valid_onnx_model() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        assert_eq!(runtime.status().state, RuntimeState::Ready);
        assert_eq!(
            runtime.loaded_model_id(),
            Some("model-test-mul-v1".to_string())
        );
        assert_eq!(runtime.provider(), ExecutionProvider::Cpu);
    }

    #[test]
    fn test_phase6b_missing_model() {
        let dir = tempdir().unwrap();
        let missing_path = dir.path().join("missing.onnx");
        let manifest = create_test_manifest(&missing_path);

        let mut runtime = OnnxAiRuntime::new();
        let res = runtime.load_model(&manifest);
        assert!(res.is_err());
        assert_eq!(runtime.status().model_state, ModelState::Error);
    }

    #[test]
    fn test_phase6b_empty_model() {
        let dir = tempdir().unwrap();
        let empty_path = dir.path().join("empty.onnx");
        std::fs::write(&empty_path, b"").unwrap();

        let manifest = create_test_manifest(&empty_path);
        let mut runtime = OnnxAiRuntime::new();
        let res = runtime.load_model(&manifest);
        assert!(res.is_err());
        assert_eq!(runtime.status().model_state, ModelState::Error);
    }

    #[test]
    fn test_phase6b_invalid_onnx_model() {
        let dir = tempdir().unwrap();
        let invalid_path = dir.path().join("invalid.onnx");
        std::fs::write(&invalid_path, b"NOT_A_VALID_ONNX_MODEL_HEADER").unwrap();

        let manifest = create_test_manifest(&invalid_path);
        let mut runtime = OnnxAiRuntime::new();
        let res = runtime.load_model(&manifest);
        assert!(res.is_err());
        assert_eq!(runtime.status().model_state, ModelState::Error);
    }

    #[test]
    fn test_phase6b_real_inference_and_output_values() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let request = InferenceRequest {
            model_id: "model-test-mul-v1".to_string(),
            inputs: vec![AiTensorInput {
                name: "X".to_string(),
                data_type: TensorDataType::Float32,
                shape: vec![1, 4],
                data_f32: Some(vec![1.0, 2.0, 3.0, 4.0]),
                data_i32: None,
                data_i64: None,
                data_u8: None,
            }],
        };

        let result = runtime.infer(&request).unwrap();
        assert_eq!(result.model_id, "model-test-mul-v1");
        assert_eq!(result.provider, ExecutionProvider::Cpu);
        assert!(result.inference_duration_ms >= 0.0);
        assert_eq!(result.outputs.len(), 1);

        let output = &result.outputs[0];
        assert_eq!(output.name, "Y");
        assert_eq!(output.shape, vec![1, 4]);
        assert_eq!(output.data_f32, Some(vec![2.0, 4.0, 6.0, 8.0]));
    }

    #[test]
    fn test_phase6b_multiple_inferences() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        for i in 1..=5 {
            let input_val = i as f32;
            let request = InferenceRequest {
                model_id: "model-test-mul-v1".to_string(),
                inputs: vec![AiTensorInput {
                    name: "X".to_string(),
                    data_type: TensorDataType::Float32,
                    shape: vec![1, 4],
                    data_f32: Some(vec![
                        input_val,
                        input_val * 2.0,
                        input_val * 3.0,
                        input_val * 4.0,
                    ]),
                    data_i32: None,
                    data_i64: None,
                    data_u8: None,
                }],
            };

            let result = runtime.infer(&request).unwrap();
            let expected = vec![
                input_val * 2.0,
                input_val * 4.0,
                input_val * 6.0,
                input_val * 8.0,
            ];
            assert_eq!(result.outputs[0].data_f32, Some(expected));
        }
    }

    #[test]
    fn test_phase6b_invalid_input_name() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let request = InferenceRequest {
            model_id: "model-test-mul-v1".to_string(),
            inputs: vec![AiTensorInput {
                name: "WRONG_NAME".to_string(),
                data_type: TensorDataType::Float32,
                shape: vec![1, 4],
                data_f32: Some(vec![1.0, 2.0, 3.0, 4.0]),
                data_i32: None,
                data_i64: None,
                data_u8: None,
            }],
        };

        let result = runtime.infer(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_phase6b_invalid_input_shape() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let request = InferenceRequest {
            model_id: "model-test-mul-v1".to_string(),
            inputs: vec![AiTensorInput {
                name: "X".to_string(),
                data_type: TensorDataType::Float32,
                shape: vec![1, 8], // Expected 4
                data_f32: Some(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]),
                data_i32: None,
                data_i64: None,
                data_u8: None,
            }],
        };

        let result = runtime.infer(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_phase6b_invalid_input_dtype() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let request = InferenceRequest {
            model_id: "model-test-mul-v1".to_string(),
            inputs: vec![AiTensorInput {
                name: "X".to_string(),
                data_type: TensorDataType::Int32, // Expected Float32
                shape: vec![1, 4],
                data_f32: None,
                data_i32: Some(vec![1, 2, 3, 4]),
                data_i64: None,
                data_u8: None,
            }],
        };

        let result = runtime.infer(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_phase6b_unload_model() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();
        assert_eq!(runtime.status().model_state, ModelState::Ready);

        runtime.unload_model().unwrap();
        assert_eq!(runtime.status().model_state, ModelState::Unloaded);
        assert_eq!(runtime.loaded_model_id(), None);
    }

    #[test]
    fn test_phase6b_loading_state() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        assert_eq!(runtime.status().model_state, ModelState::Unloaded);
        runtime.load_model(&manifest).unwrap();
        assert_eq!(runtime.status().model_state, ModelState::Ready);
    }

    #[test]
    fn test_phase6b_ready_state() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        assert_eq!(runtime.status().state, RuntimeState::Ready);
        assert_eq!(runtime.status().model_state, ModelState::Ready);
    }

    #[test]
    fn test_phase6b_error_state() {
        let dir = tempdir().unwrap();
        let invalid_path = dir.path().join("invalid.onnx");
        std::fs::write(&invalid_path, b"CORRUPTED").unwrap();

        let manifest = create_test_manifest(&invalid_path);
        let mut runtime = OnnxAiRuntime::new();
        let res = runtime.load_model(&manifest);
        assert!(res.is_err());
        assert_eq!(runtime.status().model_state, ModelState::Error);
    }

    #[test]
    fn test_phase6b_real_inference() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let request = InferenceRequest {
            model_id: "model-test-mul-v1".to_string(),
            inputs: vec![AiTensorInput {
                name: "X".to_string(),
                data_type: TensorDataType::Float32,
                shape: vec![1, 4],
                data_f32: Some(vec![2.0, 4.0, 6.0, 8.0]),
                data_i32: None,
                data_i64: None,
                data_u8: None,
            }],
        };

        let result = runtime.infer(&request).unwrap();
        assert_eq!(result.outputs.len(), 1);
        assert_eq!(result.outputs[0].name, "Y");
    }

    #[test]
    fn test_phase6b_real_output_values() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let request = InferenceRequest {
            model_id: "model-test-mul-v1".to_string(),
            inputs: vec![AiTensorInput {
                name: "X".to_string(),
                data_type: TensorDataType::Float32,
                shape: vec![1, 4],
                data_f32: Some(vec![3.0, 6.0, 9.0, 12.0]),
                data_i32: None,
                data_i64: None,
                data_u8: None,
            }],
        };

        let result = runtime.infer(&request).unwrap();
        assert_eq!(
            result.outputs[0].data_f32,
            Some(vec![6.0, 12.0, 18.0, 24.0])
        );
    }

    #[test]
    fn test_phase6b_real_input_metadata() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let metadata = runtime.inspect_active_model().unwrap();
        assert_eq!(metadata.input_count, 1);
        assert_eq!(metadata.inputs[0].name, "X");
        assert_eq!(metadata.inputs[0].data_type, TensorDataType::Float32);
    }

    #[test]
    fn test_phase6b_real_output_metadata() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let metadata = runtime.inspect_active_model().unwrap();
        assert_eq!(metadata.output_count, 1);
        assert_eq!(metadata.outputs[0].name, "Y");
        assert_eq!(metadata.outputs[0].data_type, TensorDataType::Float32);
    }

    #[test]
    fn test_phase6b_tensor_dtype_validation() {
        let meta = OnnxTensorMetadata {
            name: "X".to_string(),
            data_type: TensorDataType::Float32,
            shape: vec![Dimension::fixed(1), Dimension::fixed(4)],
        };
        let input_wrong = AiTensorInput {
            name: "X".to_string(),
            data_type: TensorDataType::Int32,
            shape: vec![1, 4],
            data_f32: None,
            data_i32: Some(vec![1, 2, 3, 4]),
            data_i64: None,
            data_u8: None,
        };
        let res = OnnxAiRuntime::validate_input_tensor(&meta, &input_wrong);
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6b_tensor_shape_validation() {
        let meta = OnnxTensorMetadata {
            name: "X".to_string(),
            data_type: TensorDataType::Float32,
            shape: vec![Dimension::fixed(1), Dimension::fixed(4)],
        };
        let input_wrong = AiTensorInput {
            name: "X".to_string(),
            data_type: TensorDataType::Float32,
            shape: vec![1, 8],
            data_f32: Some(vec![1.0; 8]),
            data_i32: None,
            data_i64: None,
            data_u8: None,
        };
        let res = OnnxAiRuntime::validate_input_tensor(&meta, &input_wrong);
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6b_cpu_session_creation() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let mut manifest = create_test_manifest(&model_path);
        manifest.requirements.preferred_provider = Some(ExecutionProvider::Cpu);

        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();
        assert_eq!(runtime.provider(), ExecutionProvider::Cpu);
    }

    #[test]
    fn test_phase6b_explicit_unavailable_provider_error() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let mut manifest = create_test_manifest(&model_path);
        manifest.requirements.preferred_provider = Some(ExecutionProvider::TensorRT);

        let mut runtime = OnnxAiRuntime::new();
        let res = runtime.load_model(&manifest);
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6b_actual_provider_reported() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let status = runtime.status();
        assert_eq!(status.provider, ExecutionProvider::Cpu);
    }

    #[test]
    fn test_phase6b_inference_duration_is_real() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let request = InferenceRequest {
            model_id: "model-test-mul-v1".to_string(),
            inputs: vec![AiTensorInput {
                name: "X".to_string(),
                data_type: TensorDataType::Float32,
                shape: vec![1, 4],
                data_f32: Some(vec![1.0, 2.0, 3.0, 4.0]),
                data_i32: None,
                data_i64: None,
                data_u8: None,
            }],
        };

        let result = runtime.infer(&request).unwrap();
        assert!(result.inference_duration_ms >= 0.0);
    }

    #[test]
    fn test_phase6b_tensor_size_overflow() {
        let meta = OnnxTensorMetadata {
            name: "X".to_string(),
            data_type: TensorDataType::Float32,
            shape: vec![Dimension::fixed(1), Dimension::fixed(4)],
        };
        let input_overflow = AiTensorInput {
            name: "X".to_string(),
            data_type: TensorDataType::Float32,
            shape: vec![u64::MAX, 2],
            data_f32: Some(vec![1.0, 2.0]),
            data_i32: None,
            data_i64: None,
            data_u8: None,
        };
        let res = OnnxAiRuntime::validate_input_tensor(&meta, &input_overflow);
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6b_registered_model_loadable() {
        let dir = tempdir().unwrap();
        let registry_dir = dir.path().join("models");
        let model_file = dir.path().join("custom.onnx");
        generate_minimal_onnx_model(&model_file).unwrap();

        let registry = crate::ai::ModelRegistry::new(registry_dir);
        let manifest = create_test_manifest(&model_file);
        let registered = registry.register_model(manifest).unwrap();

        let mut runtime = OnnxAiRuntime::new();
        let res = runtime.load_model(&registered);
        assert!(res.is_ok());
        assert_eq!(runtime.status().state, RuntimeState::Ready);
    }

    #[test]
    fn test_phase6b_registered_but_invalid_model() {
        let dir = tempdir().unwrap();
        let registry_dir = dir.path().join("models");
        let invalid_file = dir.path().join("damaged.onnx");
        std::fs::write(&invalid_file, b"DAMAGED_HEADER").unwrap();

        let registry = crate::ai::ModelRegistry::new(registry_dir);
        let manifest = create_test_manifest(&invalid_file);
        let registered = registry.register_model(manifest).unwrap();

        let mut runtime = OnnxAiRuntime::new();
        let res = runtime.load_model(&registered);
        assert!(res.is_err());
        assert_eq!(runtime.status().model_state, ModelState::Error);
    }

    #[test]
    fn test_phase6b_load_model_command() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let registry = crate::ai::ModelRegistry::new(dir.path().join("models"));
        registry.register_model(manifest).unwrap();

        let global = get_global_ai_runtime();
        let mut r = global.lock().unwrap();
        let m = registry.get_model("model-test-mul-v1").unwrap();
        r.load_model(&m).unwrap();

        assert_eq!(r.loaded_model_id(), Some("model-test-mul-v1".to_string()));
        let meta = r.inspect_active_model().unwrap();
        assert_eq!(meta.input_count, 1);
        assert_eq!(meta.output_count, 1);
    }

    #[test]
    fn test_phase6b_inspect_model_command() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let meta = runtime.inspect_active_model().unwrap();
        assert_eq!(meta.inputs[0].name, "X");
        assert_eq!(meta.outputs[0].name, "Y");
    }

    #[test]
    fn test_phase6b_inference_command() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("model.onnx");
        generate_minimal_onnx_model(&model_path).unwrap();

        let manifest = create_test_manifest(&model_path);
        let mut runtime = OnnxAiRuntime::new();
        runtime.load_model(&manifest).unwrap();

        let request = InferenceRequest {
            model_id: "model-test-mul-v1".to_string(),
            inputs: vec![AiTensorInput {
                name: "X".to_string(),
                data_type: TensorDataType::Float32,
                shape: vec![1, 4],
                data_f32: Some(vec![5.0, 10.0, 15.0, 20.0]),
                data_i32: None,
                data_i64: None,
                data_u8: None,
            }],
        };

        let result = runtime.infer(&request).unwrap();
        assert_eq!(
            result.outputs[0].data_f32,
            Some(vec![10.0, 20.0, 30.0, 40.0])
        );
    }
}
