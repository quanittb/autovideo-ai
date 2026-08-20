import { invoke } from '@tauri-apps/api/core';
import {
  AudioExtractionResult,
  CacheValidationReport,
  FrameExtractionRequest,
  FrameExtractionResult,
  HardwareProfile,
  MediaMetadata,
  MediaRuntimeStatus,
  ModelDescriptor,
  Project,
  ProjectSummary,
  StoragePaths,
} from '../types/contracts';

export interface AppInfo {
  name: string;
  version: string;
  environment: string;
}

export async function invokeCommand<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return await invoke<T>(cmd, args);
}

export const mediaApi = {
  getRuntimeStatus: async (): Promise<MediaRuntimeStatus> => {
    return await invoke<MediaRuntimeStatus>('get_media_runtime_status');
  },

  prepareMedia: async (projectId: string, mediaId: string): Promise<string> => {
    return await invoke<string>('prepare_media', { projectId, mediaId });
  },

  extractFrames: async (request: FrameExtractionRequest): Promise<FrameExtractionResult> => {
    return await invoke<FrameExtractionResult>('extract_media_frames', { request });
  },

  extractAudio: async (projectId: string, mediaId: string): Promise<AudioExtractionResult> => {
    return await invoke<AudioExtractionResult>('extract_media_audio', { projectId, mediaId });
  },

  validateCache: async (projectId: string, mediaId: string): Promise<CacheValidationReport> => {
    return await invoke<CacheValidationReport>('validate_media_cache', { projectId, mediaId });
  },

  openDirectory: async (path: string): Promise<void> => {
    return await invoke<void>('open_directory', { path });
  },

  openFilePath: async (path: string): Promise<void> => {
    return await invoke<void>('open_file_path', { path });
  },
};

export const editorApi = {
  resolveProjectMedia: async (projectId: string): Promise<import('../types/contracts').ResolvedMediaAsset> => {
    return await invoke<import('../types/contracts').ResolvedMediaAsset>('resolve_project_media', { projectId });
  },

  persistEditorState: async (
    projectId: string,
    editorState: import('../types/contracts').ProjectEditorState
  ): Promise<void> => {
    return await invoke<void>('persist_editor_state', { projectId, editorState });
  },
};

export const renderApi = {
  renderTestVideo: async (
    request: import('../types/contracts').RenderRequest
  ): Promise<import('../types/contracts').RenderResult> => {
    return await invoke<import('../types/contracts').RenderResult>('render_test_video', { request });
  },
};

export const jobApi = {
  createPipelineJob: async (
    projectId: string,
    jobType?: string,
    inputFiles?: string[]
  ): Promise<import('../types/contracts').Job> => {
    return await invoke<import('../types/contracts').Job>('create_pipeline_job', {
      projectId,
      jobType,
      inputFiles,
    });
  },

  startJob: async (jobId: string): Promise<import('../types/contracts').Job> => {
    return await invoke<import('../types/contracts').Job>('start_pipeline_job', { jobId });
  },

  cancelJob: async (jobId: string): Promise<import('../types/contracts').Job> => {
    return await invoke<import('../types/contracts').Job>('cancel_pipeline_job', { jobId });
  },

  retryJob: async (jobId: string): Promise<import('../types/contracts').Job> => {
    return await invoke<import('../types/contracts').Job>('retry_pipeline_job', { jobId });
  },

  deleteJob: async (jobId: string): Promise<void> => {
    return await invoke<void>('delete_pipeline_job', { jobId });
  },

  getJob: async (jobId: string): Promise<import('../types/contracts').Job> => {
    return await invoke<import('../types/contracts').Job>('get_pipeline_job', { jobId });
  },

  listJobs: async (projectId?: string): Promise<import('../types/contracts').Job[]> => {
    return await invoke<import('../types/contracts').Job[]>('list_pipeline_jobs', { projectId });
  },

  getJobLogs: async (jobId: string): Promise<string[]> => {
    return await invoke<string[]>('get_job_logs', { jobId });
  },

  getJobArtifacts: async (jobId: string): Promise<import('../types/contracts').Artifact[]> => {
    return await invoke<import('../types/contracts').Artifact[]>('get_job_artifacts', { jobId });
  },

  validateJob: async (jobId: string): Promise<import('../types/contracts').JobValidationReport> => {
    return await invoke<import('../types/contracts').JobValidationReport>('validate_pipeline_job', { jobId });
  },

  getAllJobHistory: async (): Promise<import('../types/contracts').Job[]> => {
    return await invoke<import('../types/contracts').Job[]>('get_all_job_history');
  },
};

export const api = {
  getAppInfo: async (): Promise<AppInfo> => {
    try {
      return await invoke<AppInfo>('get_app_info');
    } catch {
      return { name: 'AutoVideo AI', version: '0.1.0', environment: 'web-fallback' };
    }
  },

  getHardwareProfile: async (): Promise<HardwareProfile> => {
    try {
      return await invoke<HardwareProfile>('get_hardware_profile');
    } catch {
      return {
        os: 'windows',
        arch: 'x86_64',
        cpuCores: 8,
        totalMemoryBytes: 16 * 1024 * 1024 * 1024,
        gpuName: 'DirectX 12 Compatible GPU',
        vramBytes: 8 * 1024 * 1024 * 1024,
        isDirectmlSupported: true,
        isMetalSupported: false,
        isCudaSupported: false,
      };
    }
  },

  getStoragePaths: async (): Promise<StoragePaths> => {
    try {
      return await invoke<StoragePaths>('get_storage_paths');
    } catch {
      return {
        appDataDir: './.autovideo_data',
        projectsDir: './.autovideo_data/projects',
        modelsDir: './.autovideo_data/models',
        cacheDir: './.autovideo_data/cache',
        logsDir: './.autovideo_data/logs',
        tempDir: './.autovideo_data/temp',
      };
    }
  },

  getStorageUsage: async (): Promise<import('../types/contracts').StorageUsageReport> => {
    return await invoke<import('../types/contracts').StorageUsageReport>('get_storage_usage');
  },

  clearStorageCache: async (): Promise<number> => {
    return await invoke<number>('clear_storage_cache');
  },

  cleanupTempStorage: async (): Promise<number> => {
    return await invoke<number>('cleanup_temp_storage');
  },

  listProjects: async (): Promise<ProjectSummary[]> => {
    try {
      return await invoke<ProjectSummary[]>('list_projects');
    } catch {
      return [];
    }
  },

  getProject: async (id: string): Promise<Project> => {
    return await invoke<Project>('get_project', { id });
  },

  createProject: async (name: string): Promise<Project> => {
    return await invoke<Project>('create_project', { name });
  },

  updateProject: async (project: Project): Promise<Project> => {
    return await invoke<Project>('update_project', { project });
  },

  deleteProject: async (id: string): Promise<void> => {
    return await invoke<void>('delete_project', { id });
  },

  importMedia: async (projectId: string, filePath: string): Promise<Project> => {
    return await invoke<Project>('import_media', { projectId, filePath });
  },

  probeMedia: async (filePath: string): Promise<MediaMetadata> => {
    return await invoke<MediaMetadata>('probe_media', { filePath });
  },

  listModels: async (): Promise<ModelDescriptor[]> => {
    try {
      return await invoke<ModelDescriptor[]>('list_models');
    } catch {
      return [];
    }
  },

  getAiStatus: async (): Promise<string> => {
    return await invoke<string>('get_ai_status');
  },
};

export const aiApi = {
  listModels: async (): Promise<import('../types/contracts').AiModelManifest[]> => {
    return await invoke<import('../types/contracts').AiModelManifest[]>('list_ai_models');
  },

  getModel: async (modelId: string): Promise<import('../types/contracts').AiModelManifest> => {
    return await invoke<import('../types/contracts').AiModelManifest>('get_ai_model', { modelId });
  },

  registerModel: async (
    manifest: import('../types/contracts').AiModelManifest
  ): Promise<import('../types/contracts').AiModelManifest> => {
    return await invoke<import('../types/contracts').AiModelManifest>('register_ai_model', { manifest });
  },

  unregisterModel: async (modelId: string): Promise<void> => {
    return await invoke<void>('unregister_ai_model', { modelId });
  },

  getRuntimeStatus: async (): Promise<import('../types/contracts').RuntimeStatus> => {
    return await invoke<import('../types/contracts').RuntimeStatus>('get_ai_runtime_status');
  },

  getDevices: async (): Promise<import('../types/contracts').DeviceInfo> => {
    return await invoke<import('../types/contracts').DeviceInfo>('get_ai_devices');
  },

  getProviders: async (): Promise<import('../types/contracts').ProviderInfo[]> => {
    return await invoke<import('../types/contracts').ProviderInfo[]>('get_ai_providers');
  },

  loadModel: async (
    modelId: string,
    provider?: import('../types/contracts').ExecutionProvider
  ): Promise<import('../types/contracts').OnnxModelMetadata> => {
    return await invoke<import('../types/contracts').OnnxModelMetadata>('load_ai_model', { modelId, provider });
  },

  unloadModel: async (): Promise<void> => {
    return await invoke<void>('unload_ai_model');
  },

  inspectModel: async (): Promise<import('../types/contracts').OnnxModelMetadata> => {
    return await invoke<import('../types/contracts').OnnxModelMetadata>('inspect_ai_model');
  },

  runInference: async (
    request: import('../types/contracts').InferenceRequest
  ): Promise<import('../types/contracts').InferenceResult> => {
    return await invoke<import('../types/contracts').InferenceResult>('run_ai_inference', { request });
  },

  generateTestModel: async (
    targetPath?: string
  ): Promise<import('../types/contracts').AiModelManifest> => {
    return await invoke<import('../types/contracts').AiModelManifest>('generate_test_model', { targetPath });
  },

  generateImageTestModel: async (
    targetPath?: string
  ): Promise<import('../types/contracts').AiModelManifest> => {
    return await invoke<import('../types/contracts').AiModelManifest>('generate_image_test_model', { targetPath });
  },

  previewPreprocess: async (
    imagePath: string,
    config: import('../types/contracts').PreprocessConfig
  ): Promise<import('../types/contracts').PreprocessResult> => {
    return await invoke<import('../types/contracts').PreprocessResult>('preview_ai_preprocess', { imagePath, config });
  },

  validatePreprocess: async (
    modelId: string,
    config: import('../types/contracts').PreprocessConfig
  ): Promise<import('../types/contracts').PreprocessValidationResult> => {
    return await invoke<import('../types/contracts').PreprocessValidationResult>('validate_ai_preprocess', { modelId, config });
  },

  runPipeline: async (
    modelId: string,
    imagePath: string,
    preprocessConfig: import('../types/contracts').PreprocessConfig,
    postprocessConfig?: import('../types/contracts').PostprocessConfig
  ): Promise<import('../types/contracts').PipelineExecutionReport> => {
    return await invoke<import('../types/contracts').PipelineExecutionReport>('run_ai_pipeline', {
      modelId,
      imagePath,
      preprocessConfig,
      postprocessConfig,
    });
  },

  decodeMask: async (
    tensor: import('../types/contracts').AiTensorOutput,
    threshold?: number
  ): Promise<import('../types/contracts').Mask> => {
    return await invoke<import('../types/contracts').Mask>('decode_ai_mask', { tensor, threshold });
  },

  createAiPipelineJob: async (
    projectId: string,
    inputFiles: string[],
    aiConfig: import('../types/contracts').AiJobConfig
  ): Promise<import('../types/contracts').Job> => {
    return await invoke<import('../types/contracts').Job>('create_ai_pipeline_job', {
      projectId,
      inputFiles,
      aiConfig,
    });
  },

  getAiJobMetrics: async (
    jobId: string
  ): Promise<import('../types/contracts').AiJobMetrics | null> => {
    return await invoke<import('../types/contracts').AiJobMetrics | null>('get_ai_job_metrics', {
      jobId,
    });
  },

  validateAiFrameArtifacts: async (
    projectId: string,
    jobId: string
  ): Promise<{ validCount: number; totalCount: number; isValid: boolean }> => {
    return await invoke<{ validCount: number; totalCount: number; isValid: boolean }>(
      'validate_ai_frame_artifacts',
      { projectId, jobId }
    );
  },

  listModelFamilies: async (): Promise<import('../types/contracts').AiModelFamily[]> => {
    return await invoke<import('../types/contracts').AiModelFamily[]>('list_ai_model_families');
  },

  listModelPackages: async (): Promise<import('../types/contracts').AiModelPackage[]> => {
    return await invoke<import('../types/contracts').AiModelPackage[]>('list_ai_model_packages');
  },

  getModelPackage: async (
    modelId: string,
    version?: string
  ): Promise<import('../types/contracts').AiModelPackage> => {
    return await invoke<import('../types/contracts').AiModelPackage>('get_ai_model_package', {
      modelId,
      version,
    });
  },

  validateModelPackage: async (
    modelId: string,
    version: string
  ): Promise<import('../types/contracts').ModelValidationReport> => {
    return await invoke<import('../types/contracts').ModelValidationReport>(
      'validate_ai_model_package',
      { modelId, version }
    );
  },

  importModel: async (
    req: import('../types/contracts').ImportModelRequest
  ): Promise<import('../types/contracts').AiModelPackage> => {
    return await invoke<import('../types/contracts').AiModelPackage>('import_ai_model', req as any);
  },

  activateModelVersion: async (
    modelId: string,
    version: string
  ): Promise<import('../types/contracts').AiModelPackage> => {
    return await invoke<import('../types/contracts').AiModelPackage>(
      'activate_ai_model_version',
      { modelId, version }
    );
  },

  rollbackModel: async (
    modelId: string
  ): Promise<import('../types/contracts').AiModelPackage> => {
    return await invoke<import('../types/contracts').AiModelPackage>('rollback_ai_model', {
      modelId,
    });
  },

  removeModelVersion: async (
    modelId: string,
    version: string
  ): Promise<import('../types/contracts').AiModelPackage> => {
    return await invoke<import('../types/contracts').AiModelPackage>(
      'remove_ai_model_version',
      { modelId, version }
    );
  },

  resolveProductionModel: async (
    modelId?: string,
    version?: string,
    provider?: import('../types/contracts').ExecutionProvider
  ): Promise<import('../types/contracts').ResolvedProductionModel> => {
    return await invoke<import('../types/contracts').ResolvedProductionModel>(
      'resolve_production_model',
      { modelId, version, provider }
    );
  },

  validateJobPreflight: async (
    sourcePath: string,
    aiConfig: import('../types/contracts').AiJobConfig
  ): Promise<import('../types/contracts').AiJobPreflightReport> => {
    return await invoke<import('../types/contracts').AiJobPreflightReport>(
      'validate_ai_job_preflight',
      { sourcePath, aiConfig }
    );
  },

  createProductionAiJob: async (
    projectId: string,
    inputFiles: string[],
    aiConfig: import('../types/contracts').AiJobConfig
  ): Promise<import('../types/contracts').Job> => {
    return await invoke<import('../types/contracts').Job>('create_production_ai_job', {
      projectId,
      inputFiles,
      aiConfig,
    });
  },

  getResourceLimits: async (): Promise<import('../types/contracts').AiResourceLimits> => {
    return await invoke<import('../types/contracts').AiResourceLimits>('get_ai_resource_limits');
  },

  getRuntimeResources: async (
    modelId?: string
  ): Promise<import('../types/contracts').AiRuntimeResources> => {
    return await invoke<import('../types/contracts').AiRuntimeResources>(
      'get_ai_runtime_resources',
      { modelId }
    );
  },

  getExecutionReport: async (
    projectId: string,
    jobId: string
  ): Promise<import('../types/contracts').AiProductionExecutionReport> => {
    return await invoke<import('../types/contracts').AiProductionExecutionReport>(
      'get_ai_execution_report',
      { projectId, jobId }
    );
  },

  validateAiArtifacts: async (
    projectId: string,
    jobId: string
  ): Promise<import('../types/contracts').AiFrameMetadata[]> => {
    return await invoke<import('../types/contracts').AiFrameMetadata[]>(
      'validate_ai_artifacts',
      { projectId, jobId }
    );
  },

  // =========================================================================
  // PHASE 7B: GENERATIVE STUDIO IPC METHODS
  // =========================================================================

  getGenerativeCapabilities: async (): Promise<import('../types/contracts').BackendCapabilities> => {
    return await invoke<import('../types/contracts').BackendCapabilities>(
      'get_generative_capabilities'
    );
  },

  checkGenerativePreflight: async (): Promise<import('../types/contracts').GenerativePreflightReport> => {
    return await invoke<import('../types/contracts').GenerativePreflightReport>(
      'check_generative_preflight'
    );
  },

  generateKeyframe: async (
    request: {
      jobId: string;
      sourceVideoPath: string;
      sourceFrameIndex: number;
      characterReferencePaths: string[];
      positivePrompt: string;
      negativePrompt: string;
      stylePreset: string;
      steps: number;
      cfgScale: number;
      denoiseStrength: number;
      seed: number;
      width: number;
      height: number;
    }
  ): Promise<{
    result: import('../types/contracts').KeyframeGenerationResult;
    quality: import('../types/contracts').KeyframeQualityReport;
  }> => {
    return await invoke<{
      result: import('../types/contracts').KeyframeGenerationResult;
      quality: import('../types/contracts').KeyframeQualityReport;
    }>('generate_keyframe', { request });
  },

  generateVideoPipeline: async (
    request: {
      jobId: string;
      sourceVideoPath: string;
      characterReferencePaths: string[];
      positivePrompt: string;
      negativePrompt: string;
      stylePreset: string;
      steps: number;
      cfgScale: number;
      denoiseStrength: number;
      seed: number;
      width: number;
      height: number;
      contextSize: number;
      overlap: number;
    }
  ): Promise<import('../types/contracts').GenerativeVideoReport> => {
    return await invoke<import('../types/contracts').GenerativeVideoReport>(
      'generate_video_pipeline',
      { request }
    );
  },

  importControlModel: async (
    modelId: string,
    filePath: string,
    version?: string
  ): Promise<import('../types/contracts').AiModelPackage> => {
    return await invoke<import('../types/contracts').AiModelPackage>(
      'import_control_model',
      { modelId, filePath, version }
    );
  },
};

export type ExecutionClass =
  | 'LOCAL_DETERMINISTIC'
  | 'UTILITY_CLOUD'
  | 'SPECIALIZED_VIDEO_TRANSFORMATION'
  | 'GENERATIVE_FALLBACK'
  | 'LOCAL_EXPERIMENTAL';

export type TaskClass =
  | 'CHARACTER_REPLACEMENT'
  | 'BACKGROUND_REMOVAL'
  | 'BACKGROUND_COMPOSITE'
  | 'STYLE_FILTER'
  | 'AUDIO_TRANSFORMATION'
  | 'ACTION_REGENERATION'
  | 'FULL_GENERATIVE_TRANSFORMATION';

export type RoutingPreference = 'COST_SAVING' | 'QUALITY' | 'LOCAL_ONLY' | 'CLOUD_ONLY';
export type CostConfidence = 'EXACT' | 'ESTIMATED' | 'UNKNOWN';

export interface CostBreakdown {
  providerId: string;
  modelId: string;
  billableDurationSec: number;
  resolution: [number, number];
  segmentCount: number;
  overlapDurationSec: number;
  retryAllowanceUsd: number;
  inferenceCostUsd?: number;
  transferStorageCostUsd?: number;
  totalUsd?: number;
  confidence: CostConfidence;
  currency: string;
  breakdown: string;
}

export type CloudJobState =
  | 'CREATED'
  | 'VALIDATING'
  | 'COST_APPROVAL_REQUIRED'
  | 'UPLOADING'
  | 'SUBMITTED'
  | 'PROCESSING'
  | 'DOWNLOADING'
  | 'VALIDATING_OUTPUT'
  | 'COMPLETED'
  | 'FAILED'
  | 'CANCELLED'
  | 'BLOCKED'
  | 'Created'
  | 'Validating'
  | 'Submitted'
  | 'Processing'
  | 'Downloading'
  | 'Completed'
  | 'Failed'
  | 'Cancelled'
  | 'Blocked'
  | 'Queued';

export type SubmissionState = 'NEVER_ATTEMPTED' | 'IN_FLIGHT' | 'ACKNOWLEDGED' | 'AMBIGUOUS';

export interface RetryCounters {
  submitAttempts: number;
  pollAttempts: number;
  downloadAttempts: number;
}

export interface JobErrorRecord {
  code: string;
  sanitizedMessage: string;
}

export interface CloudJobRequest {
  jobId: string;
  projectId?: string;
  prompt: string;
  negativePrompt?: string;
  sourceVideo?: string;
  referenceImage?: string;
  referenceImages?: string[];
  durationSeconds: number;
  fps: number;
  resolution: [number, number];
  taskType: string;
}

export interface CostEstimate {
  provider: string;
  model: string;
  estimatedUsd?: number;
  minUsd?: number;
  maxUsd?: number;
  confidence: number;
  currency: string;
  status: CostConfidence | 'Exact' | 'Estimated' | 'Unknown';
  breakdown: string;
}

export interface CloudJobStatus {
  jobId: string;
  state: CloudJobState;
  progressPct: number;
  remoteId?: string;
  remoteStatus?: string;
  errorMessage?: string;
  outputUrl?: string;
  elapsedSeconds: number;
  costEstimate?: CostEstimate;
  actualCost?: number;
}

export interface CloudJobEventPayload {
  jobId: string;
  internalJobId: string;
  projectId: string;
  providerId: string;
  modelId: string;
  taskType: string;
  executionClass: ExecutionClass;
  state: CloudJobState;
  submissionState: SubmissionState;
  remoteJobId?: string;
  costEstimate?: CostEstimate;
  actualCost?: number;
  budgetLimit: number;
  outputPath?: string;
  retryCounters: RetryCounters;
  error?: JobErrorRecord;
  createdAt: string;
  updatedAt: string;
  submittedAt?: string;
  completedAt?: string;
  cancellationRequested: boolean;
  progressPct?: number;
  remoteStatus?: string;
}

export interface RoutingDecision {
  target: 'LOCAL' | 'CLOUD' | 'HYBRID' | 'UNAVAILABLE' | 'Local' | 'Cloud' | 'Hybrid' | 'Unavailable';
  executionClass: ExecutionClass;
  providerId: string;
  modelId: string;
  task: TaskClass | string;
  mode: RoutingPreference | string;
  reason: string;
  costBreakdown: CostBreakdown;
  estimatedCost: CostEstimate;
  fallbackAvailable: boolean;
  autoSubmitAllowed: boolean;
}

export const cloudApi = {
  getCostEstimate: async (request: CloudJobRequest): Promise<CostEstimate> => {
    return await invoke('get_cloud_cost_estimate', { request });
  },

  getGenerationRoute: async (task: string, mode: string, request: CloudJobRequest): Promise<RoutingDecision> => {
    return await invoke('get_generation_route', { task, mode, request });
  },

  startCloudGeneration: async (request: CloudJobRequest, maxCost?: number): Promise<CloudJobStatus> => {
    return await invoke('start_cloud_generation', { request, maxCost });
  },

  getCloudJobStatus: async (jobId: string, projectId?: string, remoteId?: string): Promise<CloudJobStatus> => {
    return await invoke('get_cloud_job_status', { jobId, projectId, remoteId });
  },

  cancelCloudGeneration: async (jobId?: string, projectId?: string, remoteId?: string): Promise<void> => {
    return await invoke('cancel_cloud_generation', { jobId, projectId, remoteId });
  },
};




