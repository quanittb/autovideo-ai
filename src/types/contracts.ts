export type ErrorCode =
  | 'INVALID_INPUT'
  | 'FILE_NOT_FOUND'
  | 'UNSUPPORTED_MEDIA'
  | 'MODEL_NOT_AVAILABLE'
  | 'RUNTIME_NOT_AVAILABLE'
  | 'INSUFFICIENT_RESOURCES'
  | 'PROCESS_FAILED'
  | 'CANCELLED'
  | 'PROJECT_NOT_FOUND'
  | 'PROJECT_CREATE_FAILED'
  | 'PROJECT_LOAD_FAILED'
  | 'PROJECT_SAVE_FAILED'
  | 'PROJECT_DELETE_FAILED'
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

export type ProjectStatus =
  | 'EMPTY'
  | 'IMPORTED'
  | 'ANALYZING'
  | 'READY'
  | 'PROCESSING'
  | 'COMPLETED'
  | 'FAILED';

export interface SourceMedia {
  mediaId: string;
  originalFileName: string;
  sourcePath: string;
  durationMs: number;
  width: number;
  height: number;
  fps: number;
  fileSizeBytes: number;
  container: string;
  videoCodec: string;
  audioCodec?: string;
  hasAudio: boolean;
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

export interface PreservationConfig {
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
  preservation: PreservationConfig;
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
  temporalConsistencyScore: number;
  identityPreservationScore: number;
  audioSyncOffsetMs: number;
  warnings: string[];
}

export interface ProjectOutput {
  outputId: string;
  fileName: string;
  filePath: string;
  fileSizeBytes: number;
  durationMs: number;
  width: number;
  height: number;
  fps: number;
  createdAt: string;
}

export interface Project {
  schemaVersion: number;
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  status: ProjectStatus;
  sourceMedia?: SourceMedia;
  sourceAsset?: MediaAsset;
  transformationConfig: TransformationRequest;
  transformationRequest?: TransformationRequest;
  transformationPlan?: TransformationPlan;
  outputs: ProjectOutput[];
  isFixture: boolean;
  // UI convenience helper fields for active session
  scenes?: SceneInfo[];
  selectedSceneId?: string;
  qualityMetrics?: QualityMetrics;
}

export interface ProjectSummary {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  status: ProjectStatus;
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
