export type ErrorCode =
  | 'INVALID_INPUT'
  | 'FILE_NOT_FOUND'
  | 'UNSUPPORTED_MEDIA'
  | 'MODEL_NOT_AVAILABLE'
  | 'RUNTIME_NOT_AVAILABLE'
  | 'INSUFFICIENT_RESOURCES'
  | 'PROCESS_FAILED'
  | 'CANCELLED'
  | 'UNKNOWN_ERROR';

export interface AppError {
  code: ErrorCode;
  message: string;
  details?: string;
}

export interface HardwareProfile {
  os: string;
  arch: string;
  cpuCores: number;
  totalMemoryBytes: number;
  gpuName?: string;
  vramBytes?: number;
  isDirectmlSupported: boolean;
  isMetalSupported: boolean;
  isCudaSupported: boolean;
}

export interface StoragePaths {
  appDataDir: string;
  projectsDir: string;
  modelsDir: string;
  cacheDir: string;
  logsDir: string;
  tempDir: string;
}

export interface VideoMetadata {
  width: number;
  height: number;
  durationSeconds: number;
  durationFormatted: string;
  fps: number;
  totalFrames: number;
  codec: string;
  audioCodec?: string;
  audioSampleRate?: number;
  bitrateKbps: number;
  fileSizeBytes: number;
  fileSizeFormatted: string;
}

export interface MediaAsset {
  id: string;
  fileName: string;
  filePath: string;
  metadata: VideoMetadata;
  thumbnailPath?: string;
  isFixture: boolean;
}

export interface PreservationOptions {
  preserveMotion: boolean;
  preserveCamera: boolean;
  preserveComposition: boolean;
  preserveOriginalAudio: boolean;
}

export interface TransformationRequest {
  category: 'character' | 'background' | 'environment' | 'style' | 'object' | 'custom';
  detectedCharacter?: string;
  originalCharacter?: string;
  replacementCharacter?: string;
  referenceImageUri?: string;
  prompt: string;
  negativePrompt?: string;
  preservation: PreservationOptions;
  seed?: number;
}

export interface TransformationPlan {
  estimatedFrames: number;
  pipelineSteps: string[];
  requiredModels: string[];
  estimatedDurationSeconds: number;
}

export interface SceneInfo {
  id: string;
  index: number;
  name: string;
  startTimeFormatted: string;
  endTimeFormatted: string;
  startFrame: number;
  endFrame: number;
  thumbnailEmoji: string;
  status: 'ready' | 'processing' | 'completed';
}

export interface QualityMetrics {
  temporalConsistencyScore: number; // e.g. 98.4
  identityPreservationScore: number; // e.g. 96.2
  audioSyncOffsetMs: number;         // e.g. 0 ms
  warnings: string[];
}

export interface Project {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  sourceAsset?: MediaAsset;
  transformationRequest: TransformationRequest;
  transformationPlan?: TransformationPlan;
  scenes: SceneInfo[];
  selectedSceneId: string;
  qualityMetrics?: QualityMetrics;
  outputVideoPath?: string;
  isFixture: boolean;
}

export interface ProjectSummary {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  thumbnailPath?: string;
  hasOutput: boolean;
  isFixture: boolean;
}

export type JobState =
  | 'QUEUED'
  | 'RUNNING'
  | 'PAUSED'
  | 'CANCELLING'
  | 'CANCELLED'
  | 'FAILED'
  | 'COMPLETED';

export type JobStage =
  | 'ANALYSIS'
  | 'PLANNING'
  | 'PREPARATION'
  | 'TRANSFORMATION'
  | 'TEMPORAL_REFINEMENT'
  | 'AUDIO'
  | 'QUALITY_CHECK'
  | 'EXPORT';

export interface JobProgress {
  stage: JobStage;
  stageIndex: number;
  totalStages: number;
  currentFrame: number;
  totalFrames: number;
  percentage: number;
  estimatedSecondsRemaining: number;
  currentSceneName?: string;
  gpuDevice?: string;
  vramUsageMB?: number;
}

export interface JobError {
  code: string;
  message: string;
  details?: string;
}

export interface Job {
  id: string;
  projectId: string;
  state: JobState;
  stage: JobStage;
  progress: JobProgress;
  error?: JobError;
  createdAt: string;
  updatedAt: string;
  isFixture: boolean;
}

export interface ModelDescriptor {
  id: string;
  name: string;
  version: string;
  task: 'character' | 'background' | 'environment' | 'style' | 'temporal' | 'audio';
  fileSizeBytes: number;
  license: string;
  runtime: string;
  vramRequirementMB: number;
  isDownloaded: boolean;
  isLoadedInVram: boolean;
  localPath?: string;
  sha256Checksum: string;
}

export interface ExportSettings {
  resolution: '1080p (1920x1080)' | '4K (3840x2160)' | '720p (1280x720)';
  fps: 24 | 30 | 60;
  codec: 'H.264 (AVC)' | 'HEVC (H.265)' | 'Apple ProRes';
  quality: 'High Quality' | 'Standard' | 'Lossless (Master)';
  audioOption: 'Preserve Original Audio' | 'AI Enhanced Audio';
  outputDirectory?: string;
}
