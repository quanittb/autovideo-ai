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

export interface TransformationRequest {
  category: string; // 'character' (MVP), 'scene', 'style', 'advanced'
  originalCharacter?: string;
  replacementCharacter?: string;
  prompt: string;
  negativePrompt?: string;
  seed?: number;
}

export interface TransformationPlan {
  estimatedFrames: number;
  pipelineSteps: string[];
  requiredModels: string[];
  estimatedDurationSeconds: number;
}

export interface Project {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  sourceAsset?: MediaAsset;
  transformationRequest: TransformationRequest;
  transformationPlan?: TransformationPlan;
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
  | 'EXTRACTING_FRAMES'
  | 'ANALYZING'
  | 'GENERATING_MASKS'
  | 'INPAINTING'
  | 'TEMPORAL_SMOOTHING'
  | 'STITCHING_AUDIO'
  | 'ENCODING_VIDEO'
  | 'FINALIZING';

export interface JobProgress {
  stage: JobStage;
  stageIndex: number;
  totalStages: number;
  currentFrame: number;
  totalFrames: number;
  percentage: number;
  estimatedSecondsRemaining: number;
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
  task: string;
  fileSizeBytes: number;
  isDownloaded: boolean;
  isLoadedInVram: boolean;
  localPath?: string;
  sha256Checksum: string;
}

export interface ExportSettings {
  resolution: string;
  quality: string;
  format: string;
  fps: number;
  removeWatermark: boolean;
  outputDirectory?: string;
}
