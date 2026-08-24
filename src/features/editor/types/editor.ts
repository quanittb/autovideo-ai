export interface PlaybackState {
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  muted: boolean;
  playbackRate: number;
}

export type MediaLoadStatus =
  | 'IDLE'
  | 'LOADING'
  | 'MEDIA_URL_READY'
  | 'PLAYABLE'
  | 'READY'
  | 'ERROR'
  | 'NOT_FOUND';

export interface TimelineDimensions {
  containerWidth: number;
  totalTrackWidth: number;
  pixelsPerSecond: number;
}

export interface TimeRulerTick {
  timeSeconds: number;
  label: string;
  isMajor: boolean;
  leftPercent: number;
  leftPixel: number;
}

export interface EditorShortcut {
  key: string;
  description: string;
  action: () => void;
}
