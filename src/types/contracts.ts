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
  | 'MEDIA_FILE_NOT_FOUND'
  | 'MEDIA_UNSUPPORTED_FORMAT'
  | 'MEDIA_TOO_LARGE'
  | 'MEDIA_INVALID'
  | 'MEDIA_METADATA_FAILED'
  | 'MEDIA_IMPORT_FAILED'
  | 'FFMPEG_NOT_AVAILABLE'
  | 'FFPROBE_NOT_AVAILABLE'
  | 'MEDIA_PROBE_FAILED'
  | 'FRAME_EXTRACTION_FAILED'
  | 'AUDIO_EXTRACTION_FAILED'
  | 'NO_AUDIO_STREAM'
  | 'MEDIA_CACHE_FAILED'
  | 'MEDIA_PROCESS_CANCELLED'
  | 'RENDER_FAILED'
  | 'RENDER_CANCELLED'
  | 'OUTPUT_INVALID'
  | 'OUTPUT_NOT_FOUND'
  | 'OUTPUT_METADATA_FAILED'
  | 'AUDIO_MUX_FAILED'
  | 'FRAME_SEQUENCE_INVALID'
  | 'UNKNOWN_ERROR';

export interface ExecutableStatus {
  available: boolean;
  version?: string;
  path?: string;
}

export interface MediaRuntimeStatus {
  ffmpeg: ExecutableStatus;
  ffprobe: ExecutableStatus;
}

export interface FrameExtractionRequest {
  projectId: string;
  mediaId: string;
  startTimeSeconds?: number;
  endTimeSeconds?: number;
  fps?: number;
  width?: number;
  height?: number;
  format?: string;
}

export interface FrameExtractionResult {
  framesDir: string;
  frameCount: number;
  fps: number;
  width: number;
  height: number;
  format: string;
  isCached: boolean;
  startTimeSeconds?: number;
  endTimeSeconds?: number;
}

export interface AudioExtractionResult {
  audioPath?: string;
  sampleRate: number;
  channels: number;
  hasAudio: boolean;
  isCached: boolean;
}

export interface FrameFileInfo {
  fileName: string;
  path: string;
  sizeBytes: number;
  hasValidPngHeader: boolean;
}

export interface AudioFileInfo {
  fileName: string;
  path: string;
  sizeBytes: number;
  hasValidWavHeader: boolean;
}

export interface CacheValidationReport {
  mediaCacheDir: string;
  manifestExists: boolean;
  isManifestValid: boolean;
  totalFramesOnDisk: number;
  frames: FrameFileInfo[];
  audio?: AudioFileInfo;
  allPassed: boolean;
}

export interface MediaCacheManifest {
  schemaVersion: number;
  mediaId: string;
  sourceFileName: string;
  sourceFileSize: number;
  generatedAt: string;
  frames?: FrameExtractionResult;
  audio?: AudioExtractionResult;
}

export interface RenderRequest {
  projectId: string;
  mediaId: string;
  frameDirectory?: string;
  audioPath?: string;
  fps?: number;
  width?: number;
  height?: number;
  outputFormat?: string;
  outputName?: string;
  mode?: string;
}

export interface RenderOutputMetadata {
  valid: boolean;
  outputPath: string;
  durationMs: number;
  durationSeconds: number;
  width: number;
  height: number;
  fps: number;
  videoCodec: string;
  audioCodec?: string;
  hasAudio: boolean;
  fileSizeBytes: number;
  createdAt: string;
}

export interface SourceVsOutputComparison {
  mode: string;
  sourceDurationSeconds: number;
  outputDurationSeconds: number;
  durationDeltaSeconds: number;
  sourceResolution: string;
  outputResolution: string;
  sourceFps: number;
  outputFps: number;
  sourceHasAudio: boolean;
  outputHasAudio: boolean;
  resolutionMatches: boolean;
  fpsMatches: boolean;
  audioMatches: boolean;
  expectedFrameCount: number;
  actualFrameCount: number;
  frameCountMatches: boolean;
  durationToleranceSeconds: number;
  isFullMatch: boolean;
  timingExplanation: string;
  isCompatible: boolean;
}

export interface RenderResult {
  jobId: string;
  projectId: string;
  mediaId: string;
  mode: string;
  outputMetadata: RenderOutputMetadata;
  comparison: SourceVsOutputComparison;
  manifestPath: string;
  projectOutput: ProjectOutput;
}

export interface MediaMetadata {
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
  rotation: number;
  isPortrait: boolean;
}

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

export type TransformationIntent =
  | 'FACE_REPLACE'
  | 'BACKGROUND_REPLACE'
  | 'BACKGROUND_REMOVE'
  | 'LIGHTING_EDIT'
  | 'STYLE_EDIT'
  | 'OBJECT_EDIT'
  | 'GENERIC_PROMPT_EDIT';

export type IdentityMode = 'GENERATED' | 'REFERENCE';

export interface TargetFaceSelection {
  index: number;
  descriptor?: string;
  confirmed: boolean;
  anchorTimestampSec?: number;
  anchorFrameTimestampSec?: number;
  normalizedBoundingBox?: [number, number, number, number];
}

export interface DerivedMediaProvenance {
  provider: string;
  providerJobId: string;
  sourceMediaId: string;
  transformationIntent: TransformationIntent;
  identityMode: IdentityMode;
  promptHash: string;
  createdAt: string;
}

export interface DerivedMediaAsset {
  media: SourceMedia;
  provenance: DerivedMediaProvenance;
}

export interface UseFlowOutputResult {
  derivedAsset: DerivedMediaAsset;
  project: Project;
}

export interface ProjectEditorState {
  currentTime: number;
  timelineZoom: number;
  selectedTrack?: string;
  activeMediaId?: string;
}

export interface ResolvedMediaAsset {
  mediaId: string;
  originalFileName: string;
  sourcePath: string;
  durationSeconds: number;
  durationMs: number;
  width: number;
  height: number;
  fps: number;
  fileSizeBytes: number;
  container: string;
  videoCodec: string;
  audioCodec?: string;
  hasAudio: boolean;
  framesDir?: string;
  frameFiles: string[];
  audioPath?: string;
  isCacheAvailable: boolean;
}

export type MediaLoadStatus =
  | 'IDLE'
  | 'LOADING'
  | 'MEDIA_URL_READY'
  | 'PLAYABLE'
  | 'READY'
  | 'ERROR'
  | 'NOT_FOUND';

export interface PlaybackState {
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  muted: boolean;
}

export interface Project {
  schemaVersion: number;
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  status: ProjectStatus;
  sourceMedia?: SourceMedia;
  derivedMediaAssets?: DerivedMediaAsset[];
  sourceAsset?: MediaAsset;
  transformationConfig: TransformationRequest;
  transformationRequest?: TransformationRequest;
  transformationPlan?: TransformationPlan;
  outputs: ProjectOutput[];
  editorState?: ProjectEditorState;
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

export type JobStatus =
  | 'QUEUED'
  | 'PREPARING'
  | 'RUNNING'
  | 'PAUSED'
  | 'CANCELLING'
  | 'CANCELLED'
  | 'COMPLETED'
  | 'FAILED'
  | 'INTERRUPTED';

export type StageStatus =
  | 'PENDING'
  | 'RUNNING'
  | 'COMPLETED'
  | 'FAILED'
  | 'SKIPPED'
  | 'CANCELLED'
  | 'PAUSE_UNSUPPORTED';

export interface PipelineStage {
  id: string;
  name: string;
  status: StageStatus;
  progress: number;
  indeterminate: boolean;
  startedAt?: string;
  completedAt?: string;
  error?: JobError;
  inputArtifacts: string[];
  outputArtifacts: string[];
  message: string;
}

export interface Artifact {
  id: string;
  artifactType: string;
  path: string;
  fileSizeBytes: number;
  createdAt: string;
  stageId?: string;
  status?: string;
  metadata: any;
}

export interface JobLogEntry {
  timestamp: string;
  level: 'INFO' | 'DEBUG' | 'WARN' | 'ERROR';
  stage: string;
  message: string;
}

export interface JobError {
  code: string;
  message: string;
  details?: string;
}

export interface Job {
  id: string;
  projectId: string;
  jobType: string;
  status: JobStatus;
  createdAt: string;
  updatedAt: string;
  startedAt?: string;
  completedAt?: string;
  cancelledAt?: string;
  currentStage?: string;
  currentStageIndex: number;
  totalStages: number;
  progress: number;
  message: string;
  error?: JobError;
  inputFiles: string[];
  outputFiles: string[];
  stages: PipelineStage[];
  retryCount: number;
  aiConfig?: AiJobConfig;
  aiMetrics?: AiJobMetrics;
  metadata: any;
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

// -----------------------------------------------------------------------------
// Phase 5D Event Contracts
// -----------------------------------------------------------------------------

export interface JobCreatedEvent {
  jobId: string;
  projectId: string;
  jobType: string;
  timestamp: string;
  job: Job;
}

export interface JobQueuedEvent {
  jobId: string;
  projectId: string;
  timestamp: string;
  job: Job;
}

export interface JobStartedEvent {
  jobId: string;
  projectId: string;
  timestamp: string;
  job: Job;
}

export interface JobStageStartedEvent {
  jobId: string;
  projectId: string;
  stageId: string;
  stageIndex: number;
  stageName: string;
  stageStatus: StageStatus;
  timestamp: string;
}

export interface JobStageProgressEvent {
  jobId: string;
  projectId: string;
  stageId: string;
  stageIndex: number;
  stageProgress: number;
  overallProgress: number;
  message?: string;
  timestamp: string;
}

export interface JobStageCompletedEvent {
  jobId: string;
  projectId: string;
  stageId: string;
  stageIndex: number;
  stageName: string;
  stageStatus: StageStatus;
  message: string;
  timestamp: string;
}

export interface JobProgressEvent {
  jobId: string;
  projectId: string;
  overallProgress: number;
  stageProgress: number;
  currentStage?: string;
  currentStageIndex: number;
  completedStages: number;
  totalStages: number;
  message: string;
  timestamp: string;
  job: Job;
}

export interface JobLogEvent {
  jobId: string;
  projectId: string;
  timestamp: string;
  level: string;
  stageId: string;
  message: string;
}

export interface JobArtifactEvent {
  jobId: string;
  projectId: string;
  artifactId: string;
  artifactType: string;
  path: string;
  fileSizeBytes: number;
  stageId?: string;
  status: string;
  timestamp: string;
  artifact: Artifact;
}

export interface JobCompletedEvent {
  jobId: string;
  projectId: string;
  durationSeconds: number;
  outputFiles: string[];
  message: string;
  timestamp: string;
  job: Job;
}

export interface JobFailedEvent {
  jobId: string;
  projectId: string;
  stageId?: string;
  errorCode: string;
  message: string;
  recoverable: boolean;
  details?: string;
  timestamp: string;
  job: Job;
}

export interface JobCancelRequestedEvent {
  jobId: string;
  projectId: string;
  message: string;
  timestamp: string;
  job: Job;
}

export interface JobStageCancelledEvent {
  jobId: string;
  projectId: string;
  stageId: string;
  stageIndex: number;
  stageName: string;
  timestamp: string;
}

export interface JobCancelledEvent {
  jobId: string;
  projectId: string;
  message: string;
  timestamp: string;
  job: Job;
}

export interface JobRetryingEvent {
  jobId: string;
  projectId: string;
  retryCount: number;
  timestamp: string;
  job: Job;
}

export interface JobInterruptedEvent {
  jobId: string;
  projectId: string;
  message: string;
  timestamp: string;
  job: Job;
}

export interface StageArtifactValidation {
  stageIndex: number;
  stageId: string;
  stageName: string;
  isValid: boolean;
  reason: string;
}

export interface JobValidationReport {
  jobId: string;
  projectId: string;
  isFullyValid: boolean;
  resumeStageIndex: number;
  stageValidations: StageArtifactValidation[];
}

// -------------------------------------------------------------
// PHASE 6A: AI MODEL RUNTIME & REGISTRY CONTRACTS
// -------------------------------------------------------------

export type TensorDataType = 'FLOAT32' | 'FLOAT16' | 'INT32' | 'INT64' | 'UINT8' | 'INT8';

export type Dimension =
  | { type: 'fixed'; value: number }
  | { type: 'dynamic'; value: string };

export interface TensorSpec {
  name: string;
  dataType: TensorDataType;
  shape: Dimension[];
}

export type ModelFormat = 'onnx';

export type ExecutionProvider = 'CPU' | 'DIRECTML' | 'CUDA' | 'TENSORRT' | 'COREML';

export interface ModelRequirements {
  minMemoryMb?: number;
  preferredProvider?: ExecutionProvider;
  requiresGpu: boolean;
}

export type ModelState = 'UNLOADED' | 'LOADING' | 'READY' | 'RUNNING' | 'ERROR';

export interface AiModelManifest {
  id: string;
  name: string;
  version: string;
  format: ModelFormat;
  path: string;
  description: string;
  inputSpecs: TensorSpec[];
  outputSpecs: TensorSpec[];
  requirements: ModelRequirements;
  isProduction?: boolean;
  createdAt: string;
  updatedAt: string;
  metadata?: Record<string, any>;
}

export interface ProviderInfo {
  provider: ExecutionProvider;
  supported: boolean;
  available: boolean;
  reason?: string;
}

export type RuntimeState =
  | { type: 'UNINITIALIZED' }
  | { type: 'INITIALIZING' }
  | { type: 'READY' }
  | { type: 'RUNNING' }
  | { type: 'ERROR'; message: string };

export interface DeviceInfo {
  os: string;
  arch: string;
  cpuName?: string;
  cpuCores: number;
  gpuName?: string;
  vramBytes?: number;
  totalMemoryBytes?: number;
  isDirectmlSupported: boolean;
  isCudaSupported: boolean;
  isMetalSupported: boolean;
}

export interface RuntimeStatus {
  state: RuntimeState;
  provider: ExecutionProvider;
  device: DeviceInfo;
  loadedModelId?: string;
  modelState: ModelState;
  error?: string;
}

// -------------------------------------------------------------
// PHASE 6B: ONNX RUNTIME & REAL INFERENCE CONTRACTS
// -------------------------------------------------------------

export interface OnnxTensorMetadata {
  name: string;
  dataType: TensorDataType;
  shape: Dimension[];
}

export interface OnnxModelMetadata {
  inputCount: number;
  outputCount: number;
  inputs: OnnxTensorMetadata[];
  outputs: OnnxTensorMetadata[];
  producerName?: string;
  graphName?: string;
  version?: number;
}

export interface AiTensorInput {
  name: string;
  dataType: TensorDataType;
  shape: number[];
  dataF32?: number[];
  dataI32?: number[];
  dataI64?: number[];
  dataU8?: number[];
}

export interface AiTensorOutput {
  name: string;
  dataType: TensorDataType;
  shape: number[];
  dataF32?: number[];
  dataI32?: number[];
  dataI64?: number[];
  dataU8?: number[];
}

export interface InferenceRequest {
  modelId: string;
  inputs: AiTensorInput[];
}

export interface InferenceResult {
  modelId: string;
  provider: ExecutionProvider;
  outputs: AiTensorOutput[];
  loadDurationMs?: number;
  inferenceDurationMs: number;
}

// -------------------------------------------------------------
// PHASE 6C: PREPROCESSING & POSTPROCESSING TENSOR PIPELINES
// -------------------------------------------------------------

export type ResizeFilter = 'NEAREST' | 'BILINEAR' | 'BICUBIC';
export type ChannelOrder = 'RGB' | 'BGR' | 'RGBA' | 'GRAY';
export type NormalizationMode = 'IDENTITY' | 'ZERO_TO_ONE' | 'MINUS_ONE_TO_ONE' | 'MEAN_STD';
export type TensorLayout = 'NHWC' | 'NCHW';

export interface NormalizationConfig {
  mode: NormalizationMode;
  mean?: [number, number, number];
  std?: [number, number, number];
}

export interface LetterboxTransform {
  originalWidth: number;
  originalHeight: number;
  resizedWidth: number;
  resizedHeight: number;
  padLeft: number;
  padTop: number;
  scaleX: number;
  scaleY: number;
}

export interface CropMetadata {
  cropWidth: number;
  cropHeight: number;
  offsetX: number;
  offsetY: number;
  originalWidth: number;
  originalHeight: number;
}

export interface TransformMetadata {
  letterbox?: LetterboxTransform;
  crop?: CropMetadata;
  sourceWidth: number;
  sourceHeight: number;
  targetWidth: number;
  targetHeight: number;
}

export interface PreprocessConfig {
  targetWidth: number;
  targetHeight: number;
  resizeFilter: ResizeFilter;
  letterbox: boolean;
  letterboxPad: [number, number, number];
  centerCrop: boolean;
  cropWidth?: number;
  cropHeight?: number;
  channelOrder: ChannelOrder;
  normalization: NormalizationConfig;
  layout: TensorLayout;
  batchSize: number;
}

export interface PreprocessResult {
  tensor: AiTensorInput;
  transform: TransformMetadata;
  sourceWidth: number;
  sourceHeight: number;
  processedWidth: number;
  processedHeight: number;
}

export interface PreprocessValidationResult {
  isValid: boolean;
  errors: string[];
  expectedShape: Dimension[];
  producedShape: number[];
  expectedDataType: TensorDataType;
  producedDataType: TensorDataType;
}

export interface BoundingBox {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  confidence?: number;
  classId?: number;
  label?: string;
}

export interface Mask {
  width: number;
  height: number;
  data: number[];
}

export interface PostprocessConfig {
  extractMask: boolean;
  maskThreshold?: number;
  extractBboxes: boolean;
  bboxConfidenceThreshold?: number;
}

export interface PostprocessResult {
  rawOutputs: AiTensorOutput[];
  mask?: Mask;
  bboxes?: BoundingBox[];
  executionDurationMs: number;
}

export interface PipelineExecutionReport {
  modelId: string;
  provider: ExecutionProvider;
  preprocessResult: PreprocessResult;
  inferenceResult: InferenceResult;
  postprocessResult?: PostprocessResult;
  decodeDurationMs: number;
  preprocessDurationMs: number;
  inferenceDurationMs: number;
  postprocessDurationMs: number;
  totalDurationMs: number;
}

// =========================================================================
// PHASE 6D: AI VIDEO FRAME INFERENCE PIPELINE CONTRACTS
// =========================================================================

export type FrameSamplingMode = 'all' | 'every_nth' | 'range';

export interface FrameSamplingConfig {
  mode: FrameSamplingMode;
  nth?: number;
  start?: number;
  end?: number;
}

export type AiFrameOutputMode = 'image' | 'mask';

export interface AiJobConfig {
  enabled: boolean;
  modelId: string;
  modelVersion?: string;
  modelHash?: string;
  profileHash?: string;
  provider?: ExecutionProvider;
  preprocessing: PreprocessConfig;
  postprocessing?: PostprocessConfig;
  frameSampling: FrameSamplingConfig;
  outputMode: AiFrameOutputMode;
}

export type AiFrameStatus =
  | 'COMPLETED'
  | 'PASSTHROUGH'
  | 'REUSED'
  | 'FAILED'
  | 'SKIPPED';

export interface AiFrameMetadata {
  frameIndex: number;
  status: AiFrameStatus;
  modelId: string;
  provider: string;
  decodeDurationMs: number;
  preprocessDurationMs: number;
  inferenceDurationMs: number;
  postprocessDurationMs: number;
  totalDurationMs: number;
  inputWidth: number;
  inputHeight: number;
  outputWidth: number;
  outputHeight: number;
  outputArtifactPath: string;
  configHash: string;
}

export interface AiJobMetrics {
  framesTotal: number;
  framesSelected: number;
  framesProcessed: number;
  framesReused: number;
  framesPassthrough: number;
  framesFailed: number;
  totalInferenceDurationMs: number;
  averageInferenceDurationMs: number;
  minInferenceDurationMs: number;
  maxInferenceDurationMs: number;
  totalPipelineDurationMs: number;
  artifactBytesWritten?: number;
  etaMs?: number;
}

export interface AiFrameProgressEvent {
  jobId: string;
  frameIndex: number;
  totalFrames: number;
  progressPercent: number;
  frameMetadata?: AiFrameMetadata;
  metrics: AiJobMetrics;
  timestamp: string;
}

// =========================================================================
// PHASE 6E: AI VIDEO RECONSTRUCTION & PRODUCTION INTEGRATION CONTRACTS
// =========================================================================

export interface RationalFps {
  num: number;
  den: number;
}

export type VideoCodec = 'h264' | 'h265' | 'av1' | 'prores';

export type AudioPreservationMode =
  | 'preserve_original'
  | 'transcode_aac'
  | 'none';

export interface VideoReconstructionConfig {
  sourceVideoPath: string;
  framesDir: string;
  outputPath: string;
  framePattern: string;
  expectedFrameCount: number;
  width: number;
  height: number;
  fps: RationalFps;
  pixelFormat: string;
  codec: VideoCodec;
  crf: number;
  audioSource?: string;
  audioMode: AudioPreservationMode;
  overwrite: boolean;
}

export interface FrameManifestEntry {
  frameIndex: number;
  artifactPath: string;
  status: string;
  width: number;
  height: number;
  fileSizeBytes: number;
  configHash?: string;
}

export interface ReconstructionTelemetry {
  totalDurationMs: number;
  validationDurationMs: number;
  encodingDurationMs: number;
  muxDurationMs: number;
  outputSizeBytes: number;
  framesReconstructed: number;
}

export interface ReconstructionManifest {
  jobId: string;
  sourcePath: string;
  modelId?: string;
  modelConfigHash?: string;
  frameCount: number;
  fpsNum: number;
  fpsDen: number;
  fpsF64: number;
  width: number;
  height: number;
  codec: VideoCodec;
  hasAudio: boolean;
  frames: FrameManifestEntry[];
  outputPath: string;
  outputSizeBytes: number;
  telemetry: ReconstructionTelemetry;
  createdAt: string;
}

export interface AiReconstructionProgressEvent {
  jobId: string;
  projectId: string;
  framesEncoded: number;
  totalFrames: number;
  progressPercent: number;
  overallProgress: number;
  message: string;
  timestamp: string;
}

// =========================================================================
// PHASE 6F: AI MODEL MANAGEMENT CONTRACTS
// =========================================================================

export type AspectHandlingMode = 'STRETCH' | 'LETTERBOX' | 'CENTER_CROP';

export interface AspectHandling {
  mode: AspectHandlingMode;
  padValue?: [number, number, number];
}

export type OutputInterpretationType = 'IMAGE' | 'MASK' | 'BBOX';
export type MaskInterpretation = 'BINARY' | 'GRAYSCALE' | 'PROBABILITY_MAP';
export type BboxInterpretation = 'YOLO_V8' | 'PASCAL_VOC' | 'NORMALIZED_CENTER';

export interface InputProfile {
  targetWidth: number;
  targetHeight: number;
  channelOrder: ChannelOrder;
  colorSpace: string;
  layout: TensorLayout;
  normalization: NormalizationConfig;
  resizeFilter: ResizeFilter;
  aspectHandling: AspectHandling;
  tensorName?: string;
  dataType: TensorDataType;
}

export interface OutputProfile {
  outputType: OutputInterpretationType;
  tensorName?: string;
  layout?: TensorLayout;
  threshold?: number;
  maskInterpretation?: MaskInterpretation;
  bboxInterpretation?: BboxInterpretation;
  coordinateRestoration: boolean;
}

export interface AiModelProfile {
  input: InputProfile;
  output: OutputProfile;
}

export interface AiModelPackage {
  modelId: string;
  modelName: string;
  version: string;
  displayName: string;
  description: string;
  modelFormat: 'onnx';
  modelFile: string;
  fileSizeBytes: number;
  sha256: string;
  manifest: AiModelManifest;
  profile: AiModelProfile;
  requirements: ModelRequirements;
  supportedProviders: ExecutionProvider[];
  isProduction?: boolean;
  metadata: Record<string, any>;
  createdAt: string;
  packageSchemaVersion: number;
}

export interface AiModelFamily {
  modelId: string;
  name: string;
  activeVersion?: string;
  previousVersion?: string;
  versions: Record<string, AiModelPackage>;
  createdAt: string;
  updatedAt: string;
}

export interface ProviderCompatibility {
  provider: ExecutionProvider;
  supported: boolean;
  availableOnHost: boolean;
  reason?: string;
}

export interface ModelValidationReport {
  valid: boolean;
  modelId: string;
  version: string;
  integrityValid: boolean;
  sha256: string;
  onnxValid: boolean;
  onnxMetadata?: OnnxModelMetadata;
  profileValid: boolean;
  providerCompatibility: ProviderCompatibility[];
  warnings: string[];
  errors: string[];
}

export interface ImportModelRequest {
  sourcePath: string;
  modelId: string;
  modelName: string;
  version: string;
  displayName: string;
  description: string;
  profile: AiModelProfile;
  requirements?: ModelRequirements;
  supportedProviders?: ExecutionProvider[];
}

export interface AiModelActivatedEvent {
  modelId: string;
  version: string;
  previousVersion?: string;
  timestamp: string;
}

export interface AiModelRollbackEvent {
  modelId: string;
  restoredVersion: string;
  previousVersion: string;
  timestamp: string;
}

export interface AiModelImportedEvent {
  modelId: string;
  version: string;
  sha256: string;
  timestamp: string;
}

export interface ResolvedProductionModel {
  modelId: string;
  modelVersion: string;
  modelName: string;
  displayName: string;
  modelPath: string;
  modelHash: string;
  profileHash: string;
  profile: AiModelProfile;
  provider: ExecutionProvider;
  manifest: AiModelManifest;
  fileSizeBytes: number;
  supportedProviders: ExecutionProvider[];
}

export type PreflightCheckStatus = 'PASS' | 'WARN' | 'FAIL';
export type PreflightCheckSeverity = 'INFO' | 'WARNING' | 'ERROR';

export interface PreflightCheckResult {
  check: string;
  status: PreflightCheckStatus;
  severity: PreflightCheckSeverity;
  message: string;
  technicalDetail?: string;
}

export interface AiJobPreflightReport {
  isValid: boolean;
  checks: PreflightCheckResult[];
  resolvedModel?: ResolvedProductionModel;
  warnings: string[];
  errors: string[];
}

export interface AiJobCreationRequest {
  projectId: string;
  inputFiles: string[];
  aiConfig: AiJobConfig;
}

export interface AiPreflightEvent {
  sourcePath: string;
  modelId: string;
  isValid: boolean;
  timestamp: string;
}

export interface AiModelResolvedEvent {
  modelId: string;
  version: string;
  modelHash: string;
  provider: string;
  timestamp: string;
}

export interface AiResourceLimits {
  maxMemoryBytes: number;
  maxInflightFrames: number;
  maxConcurrentInference: number;
  maxFrameWidth: number;
  maxFrameHeight: number;
  maxFramePixels: number;
  maxTensorElements: number;
  maxJobDiskBytes: number;
}

export interface AiRuntimeResources {
  processMemoryBytes: number;
  systemMemoryBytes: number;
  cpuUtilization?: number;
  gpuUtilization?: number;
  activeInferenceCount: number;
  queuedFrameCount: number;
  activeProvider: string;
  providerName: string;
  modelVersion?: string;
}

export type FrameQualityStatus = 'PASS' | 'WARNING' | 'FAIL';

export interface TechnicalQualityMetrics {
  decodedWidth: number;
  decodedHeight: number;
  fileSizeBytes: number;
  hasAlpha: boolean;
  nonZeroPixelRatio: number;
  minPixelValue: number;
  maxPixelValue: number;
  meanPixelValue: number;
  variance?: number;
  clippingRatio?: number;
  blackFrameDetected?: boolean;
  nanOrInfDetected?: boolean;
}

export interface FrameQualityReport {
  frameIndex: number;
  status: FrameQualityStatus;
  isValid: boolean;
  errors: string[];
  warnings: string[];
  metrics?: TechnicalQualityMetrics;
}

export interface FrameSequenceValidationReport {
  isValid: boolean;
  totalExpected: number;
  totalFound: number;
  missingIndices: number[];
  duplicateIndices: number[];
  passthroughMismatches: number[];
  errors: string[];
  warnings: string[];
}

export interface AiJobBenchmarkReport {
  jobId: string;
  modelId: string;
  modelVersion?: string;
  modelHash?: string;
  isProduction: boolean;
  provider: string;
  frameWidth: number;
  frameHeight: number;
  totalFrames: number;
  selectedFrames: number;
  processedFrames: number;
  reusedFrames: number;
  passthroughFrames: number;
  modelLoadMs: number;
  decodeAvgMs: number;
  preprocessAvgMs: number;
  inferenceAvgMs: number;
  inferenceMinMs: number;
  inferenceMaxMs: number;
  postprocessAvgMs: number;
  reconstructionMs: number;
  totalDurationSeconds: number;
  effectiveFps: number;
  effectiveInferenceFps: number;
}

export interface AiProductionExecutionReport {
  jobId: string;
  modelId: string;
  modelVersion?: string;
  modelHash?: string;
  profileHash?: string;
  isProduction?: boolean;
  provider: string;
  sourceDurationMs: number;
  sourceWidth: number;
  sourceHeight: number;
  sourceFps: number;
  sourceTotalFrames: number;
  selectedFrames: number;
  processedFrames: number;
  reusedFrames: number;
  passthroughFrames: number;
  failedFrames: number;
  preprocessingMs: number;
  inferenceMs: number;
  postprocessingMs: number;
  reconstructionMs: number;
  validationMs: number;
  totalMs: number;
  artifactsWritten: number;
  bytesWritten: number;
  validFrames: number;
  invalidFrames: number;
  qualityWarnings: number;
  outputPath?: string;
  outputSizeBytes?: number;
  outputDurationMs?: number;
  outputFps?: number;
  outputWidth?: number;
  outputHeight?: number;
  audioPreserved: boolean;
  validationStatus: string;
  status: string;
  createdAt: string;
}

export interface StorageUsageReport {
  projectsBytes: number;
  cacheBytes: number;
  aiCacheBytes: number;
  modelsBytes: number;
  tempBytes: number;
  logsBytes: number;
  totalBytes: number;
}

export type AiPreset = 'fast' | 'balanced' | 'quality';

// =========================================================================
// PHASE 7A: CONTROL-SIGNAL EXTRACTION & VIDEO-TO-VIDEO CONTRACTS
// =========================================================================

export interface ControlArtifactPaths {
  poseFramesDir?: string;
  depthFramesDir?: string;
  maskFramesDir?: string;
  audioFilePath?: string;
}

export interface VideoControlPackage {
  jobId: string;
  sourceVideoPath: string;
  sourceVideoHash: string;
  width: number;
  height: number;
  fps: RationalFps;
  totalFrames: number;
  durationMs: number;
  artifacts: ControlArtifactPaths;
  poseHash?: string;
  depthHash?: string;
  maskHash?: string;
  audioHash?: string;
  packageHash: string;
  isValid: boolean;
  createdAt: string;
  schemaVersion: number;
}

export interface PoseExtractorConfig {
  targetWidth: number;
  targetHeight: number;
  confidenceThreshold: number;
  includeHands: boolean;
  includeFace: boolean;
  modelId: string;
  modelVersion?: string;
}

export interface DepthExtractorConfig {
  targetWidth: number;
  targetHeight: number;
  invert: boolean;
  modelId: string;
  modelVersion?: string;
}

export interface SegmentationExtractorConfig {
  targetWidth: number;
  targetHeight: number;
  threshold: number;
  binaryMask: boolean;
  modelId: string;
  modelVersion?: string;
}

export interface ControlExtractionConfig {
  extractPose: boolean;
  extractDepth: boolean;
  extractMask: boolean;
  preserveAudio: boolean;
  poseConfig: PoseExtractorConfig;
  depthConfig: DepthExtractorConfig;
  segmentationConfig: SegmentationExtractorConfig;
}

export interface ControlExtractionReport {
  jobId: string;
  totalFrames: number;
  poseExtractedCount: number;
  depthExtractedCount: number;
  maskExtractedCount: number;
  poseDurationMs: number;
  depthDurationMs: number;
  maskDurationMs: number;
  totalDurationMs: number;
  cacheHitsCount: number;
  packageHash: string;
  isValid: boolean;
  errors: string[];
}

// =========================================================================
// PHASE 7B: REAL GENERATIVE VIDEO & KEYFRAME MVP CONTRACTS
// =========================================================================

export interface CharacterReference {
  imagePaths: string[];
  identityWeight: number;
  appearanceWeight: number;
  cropMode: string;
}

export interface EnvironmentCondition {
  positivePrompt: string;
  negativePrompt: string;
  stylePreset: string;
}

export interface GenerationParams {
  steps: number;
  cfgScale: number;
  denoiseStrength: number;
  seed: number;
  width: number;
  height: number;
  controlWeights: Record<string, number>;
}

export interface KeyframeGenerationRequest {
  jobId: string;
  sourceVideoPath: string;
  sourceFrameIndex: number;
  characterReference: CharacterReference;
  environment: EnvironmentCondition;
  params: GenerationParams;
  outputPath: string;
}

export interface KeyframeGenerationResult {
  jobId: string;
  outputPath: string;
  width: number;
  height: number;
  loadDurationMs: number;
  inferenceDurationMs: number;
  totalDurationMs: number;
  vramPeakBytes?: number;
  modelId: string;
  modelVersion?: string;
  modelHash?: string;
  backend: string;
  provider: string;
  isProduction: boolean;
  parameters: GenerationParams;
}

export interface KeyframeQualityReport {
  isValid: boolean;
  fileSizeBytes: number;
  decodedWidth: number;
  decodedHeight: number;
  variance: number;
  blackFrameDetected: boolean;
  errors: string[];
}

export interface BackendHealthStatus {
  healthy: boolean;
  backendName: string;
  version: string;
  cudaAvailable: boolean;
  gpuName?: string;
  vramTotalMb?: number;
  vramFreeMb?: number;
  error?: string;
}

export interface BackendCapabilities {
  supportedResolutions: [number, number][];
  supportsCharacterReference: boolean;
  supportsDepthControl: boolean;
  supportsPoseControl: boolean;
  supportsMaskControl: boolean;
  supportsFp8: boolean;
  supportsLora: boolean;
  backendName: string;
}

export interface GenerativePreflightReport {
  isValid: boolean;
  backendStatus: BackendHealthStatus;
  capabilities: BackendCapabilities;
  poseModelInstalled: boolean;
  depthModelInstalled: boolean;
  segmentationModelInstalled: boolean;
  missingModels: string[];
  warnings: string[];
  errors: string[];
}

// =========================================================================
// PHASE 7C: TEMPORAL VIDEO-TO-VIDEO CONTRACTS
// =========================================================================

export interface TemporalConfig {
  contextSize: number;
  overlap: number;
  enableSeamBlending: boolean;
  enableLatentContinuity: boolean;
}

export interface GenerativeVideoJobConfig {
  jobId: string;
  sourceVideoPath: string;
  characterReference: CharacterReference;
  environment: EnvironmentCondition;
  params: GenerationParams;
  temporalConfig: TemporalConfig;
  outputVideoPath: string;
}

export interface GenerativeVideoReport {
  jobId: string;
  totalFrames: number;
  totalWindows: number;
  completedWindows: number;
  reusedWindows: number;
  sourceFps: number;
  sourceDurationMs: number;
  controlExtractionMs: number;
  diffusionInferenceMs: number;
  blendingMs: number;
  reconstructionMs: number;
  totalDurationMs: number;
  outputVideoPath: string;
  outputFileSizeBytes: number;
  audioPreserved: boolean;
  qualityStatus: string;
}
