export type WizardStep = 'upload' | 'transform' | 'preview' | 'export';
export type NavTab = 'home' | 'projects' | 'templates' | 'tools' | 'assets' | 'history' | 'settings';

export interface MediaInfo {
  fileName: string;
  filePath: string;
  durationSeconds: number;
  durationFormatted: string;
  resolution: string;
  width: number;
  height: number;
  fileSizeBytes: number;
  fileSizeFormatted: string;
  fps: number;
  codec: string;
}

export interface TransformationConfig {
  category: 'scene' | 'character' | 'style' | 'advanced';
  originalCharacter?: string;
  replacementCharacter?: string;
  prompt: string;
  resolution: string;
  quality: string;
  format: string;
  fps: number;
  removeWatermark: boolean;
}

export interface Project {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  sourceVideoPath?: string;
  mediaInfo?: MediaInfo;
  transformation: TransformationConfig;
  isMockDemo: boolean;
}

export type AiAvailabilityStatus =
  | { type: 'AVAILABLE' }
  | { type: 'MODEL_NOT_AVAILABLE'; model_name: string; guidance: string }
  | { type: 'MODEL_BLOCKED'; reason: string }
  | { type: 'RUNTIME_NOT_AVAILABLE'; runtime_type: string };

export type JobState =
  | 'QUEUED'
  | 'RUNNING'
  | 'PAUSED'
  | 'CANCELLING'
  | 'CANCELLED'
  | 'FAILED'
  | 'COMPLETED';

export interface JobProgress {
  currentStep: string;
  currentFrame: number;
  totalFrames: number;
  percentage: number;
  estimatedSecondsRemaining: number;
}

export interface Job {
  id: string;
  projectId: string;
  state: JobState;
  progress: JobProgress;
  errorMessage?: string;
  createdAt: string;
  updatedAt: string;
  isMock: boolean;
}
