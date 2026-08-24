import { create } from 'zustand';
import { convertFileSrc } from '@tauri-apps/api/core';
import { editorApi } from '../../../lib/ipc';
import { ResolvedMediaAsset } from '../../../types/contracts';
import { PlaybackState, MediaLoadStatus } from '../types/editor';

// Runtime-only refs — NOT serialized into Zustand state
let _videoElement: HTMLVideoElement | null = null;
let _pendingSeekTime: number | null = null;

interface EditorStore {
  projectId: string | null;
  mediaAsset: ResolvedMediaAsset | null;
  mediaUrl: string | null;
  loadStatus: MediaLoadStatus;
  errorMessage: string | null;
  playback: PlaybackState;
  timelineZoom: number; // 0.5, 1.0, 1.5, 2.0, 3.0
  hoverTime: number | null;

  // Actions
  loadProjectMedia: (projectId: string) => Promise<void>;
  setMediaPlayable: (duration?: number) => void;
  setMediaError: (errorMessage: string) => void;
  setIsPlaying: (isPlaying: boolean) => void;
  setCurrentTime: (time: number) => void;
  setDuration: (duration: number) => void;
  setVolume: (volume: number) => void;
  setMuted: (muted: boolean) => void;
  setTimelineZoom: (zoom: number) => void;
  setHoverTime: (time: number | null) => void;
  seek: (timeSeconds: number) => void;
  stepForward: (seconds?: number) => void;
  stepBackward: (seconds?: number) => void;
  registerVideoElement: (el: HTMLVideoElement | null) => void;
  reset: () => void;
}

// Exported for use in useMediaPlayback (pending seek apply-on-ready)
export const getEditorVideoElement = () => _videoElement;
export const applyPendingSeekIfExists = () => {
  if (_pendingSeekTime !== null && _videoElement) {
    const target = _pendingSeekTime;
    _pendingSeekTime = null;
    _videoElement.currentTime = target;
  }
};

let persistTimeout: any = null;

const schedulePersist = (projectId: string, currentTime: number, timelineZoom: number) => {
  if (persistTimeout) clearTimeout(persistTimeout);
  persistTimeout = setTimeout(async () => {
    try {
      await editorApi.persistEditorState(projectId, {
        currentTime,
        timelineZoom,
      });
    } catch (e) {
      // Non-blocking persistence error logging
      console.warn('Failed to persist editor state:', e);
    }
  }, 500);
};

export const useEditorStore = create<EditorStore>((set, get) => ({
  projectId: null,
  mediaAsset: null,
  mediaUrl: null,
  loadStatus: 'IDLE',
  errorMessage: null,
  playback: {
    isPlaying: false,
    currentTime: 0,
    duration: 0,
    volume: 1.0,
    muted: false,
    playbackRate: 1.0,
  },
  timelineZoom: 1.0,
  hoverTime: null,

  loadProjectMedia: async (projectId: string) => {
    set({ loadStatus: 'LOADING', errorMessage: null, projectId });
    try {
      const asset = await editorApi.resolveProjectMedia(projectId);
      const safeUrl = convertFileSrc(asset.sourcePath);

      set((state) => ({
        mediaAsset: asset,
        mediaUrl: safeUrl,
        loadStatus: 'MEDIA_URL_READY',
        playback: {
          ...state.playback,
          isPlaying: false, // Always starts paused on project reload
          currentTime: 0, // Will be updated by project editor state if existing
          duration: asset.durationSeconds,
        },
      }));
    } catch (err: any) {
      set({
        loadStatus: 'ERROR',
        errorMessage: err?.message || 'Failed to resolve project media asset',
      });
    }
  },

  setMediaPlayable: (duration?: number) => {
    set((state) => ({
      loadStatus: 'READY',
      errorMessage: null,
      playback: {
        ...state.playback,
        duration: duration && duration > 0 ? duration : state.playback.duration,
      },
    }));
  },

  setMediaError: (errorMessage: string) => {
    set((state) => ({
      loadStatus: 'ERROR',
      errorMessage,
      playback: {
        ...state.playback,
        isPlaying: false,
      },
    }));
  },

  setIsPlaying: (isPlaying: boolean) => {
    set((state) => ({
      playback: { ...state.playback, isPlaying },
    }));
  },

  setCurrentTime: (currentTime: number) => {
    const { projectId, timelineZoom, playback } = get();
    const clamped = Math.max(0, Math.min(currentTime, playback.duration || 0));
    set((state) => ({
      playback: { ...state.playback, currentTime: clamped },
    }));
    if (projectId) {
      schedulePersist(projectId, clamped, timelineZoom);
    }
  },

  setDuration: (duration: number) => {
    set((state) => ({
      playback: { ...state.playback, duration },
    }));
  },

  setVolume: (volume: number) => {
    const clamped = Math.max(0, Math.min(volume, 1.0));
    set((state) => ({
      playback: { ...state.playback, volume: clamped, muted: clamped === 0 },
    }));
  },

  setMuted: (muted: boolean) => {
    set((state) => ({
      playback: { ...state.playback, muted },
    }));
  },

  setTimelineZoom: (zoom: number) => {
    const clamped = Math.max(0.5, Math.min(zoom, 4.0));
    const { projectId, playback } = get();
    set({ timelineZoom: clamped });
    if (projectId) {
      schedulePersist(projectId, playback.currentTime, clamped);
    }
  },

  setHoverTime: (hoverTime: number | null) => {
    set({ hoverTime });
  },

  seek: (timeSeconds: number) => {
    const { playback, projectId, timelineZoom } = get();
    const duration = playback.duration || 0;
    if (!isFinite(timeSeconds)) return;
    const clamped = Math.max(0, Math.min(timeSeconds, duration));
    set((state) => ({
      playback: { ...state.playback, currentTime: clamped },
    }));
    if (projectId) {
      schedulePersist(projectId, clamped, timelineZoom);
    }
    // Drive the real video element (authoritative seek path)
    if (_videoElement && _videoElement.readyState >= 1) {
      _videoElement.currentTime = clamped;
    } else {
      // Metadata not yet loaded; apply once element is ready
      _pendingSeekTime = clamped;
    }
  },

  stepForward: (seconds = 1.0) => {
    const { playback } = get();
    get().seek(playback.currentTime + seconds);
  },

  stepBackward: (seconds = 1.0) => {
    const { playback } = get();
    get().seek(playback.currentTime - seconds);
  },

  registerVideoElement: (el: HTMLVideoElement | null) => {
    _videoElement = el;
    if (el === null) {
      _pendingSeekTime = null;
    }
  },

  reset: () => {
    set({
      projectId: null,
      mediaAsset: null,
      mediaUrl: null,
      loadStatus: 'IDLE',
      errorMessage: null,
      playback: {
        isPlaying: false,
        currentTime: 0,
        duration: 0,
        volume: 1.0,
        muted: false,
        playbackRate: 1.0,
      },
      timelineZoom: 1.0,
      hoverTime: null,
    });
  },
}));
