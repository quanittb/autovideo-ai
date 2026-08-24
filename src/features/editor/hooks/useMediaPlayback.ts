import { useEffect, useCallback } from 'react';
import { useEditorStore } from '../stores/editorStore';
import { applyPendingSeekIfExists } from '../stores/editorStore';

export function mapMediaError(
  err: MediaError | { code: number; message?: string } | null
): string {
  if (!err) {
    return 'MEDIA_PLAYBACK_ERROR: Video could not be loaded by the desktop preview runtime.';
  }

  switch (err.code) {
    case 1: // MEDIA_ERR_ABORTED
      return 'MEDIA_ERR_ABORTED: Video playback was aborted.';
    case 2: // MEDIA_ERR_NETWORK
      return 'MEDIA_ERR_NETWORK: A network error occurred while loading the video asset.';
    case 3: // MEDIA_ERR_DECODE
      return 'MEDIA_DECODE_ERROR: The video playback could not be decoded by the desktop preview runtime.';
    case 4: // MEDIA_ERR_SRC_NOT_SUPPORTED
      return 'MEDIA_SOURCE_NOT_SUPPORTED: The video source format, codec, or desktop asset protocol path is not supported or was blocked.';
    default:
      return `MEDIA_PLAYBACK_ERROR: Video playback error (code ${err.code}): ${err.message || 'Unknown media error'}`;
  }
}

export const useMediaPlayback = (videoRef: React.RefObject<HTMLVideoElement | null>) => {
  const {
    playback,
    setIsPlaying,
    setCurrentTime,
    setDuration,
    setMuted,
    setMediaPlayable,
    setMediaError,
    seek,
    stepForward,
    stepBackward,
  } = useEditorStore();

  // Synchronize playing state with HTML5 video element
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    if (playback.isPlaying && video.paused) {
      video.play().catch((err) => {
        console.warn('Playback play() was prevented:', err);
        setIsPlaying(false);
      });
    } else if (!playback.isPlaying && !video.paused) {
      video.pause();
    }
  }, [playback.isPlaying, videoRef, setIsPlaying]);

  // Synchronize volume and mute
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    video.volume = playback.volume;
    video.muted = playback.muted;
  }, [playback.volume, playback.muted, videoRef]);

  // Video Element Native Event Handlers
  const handleTimeUpdate = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    setCurrentTime(video.currentTime);
  }, [videoRef, setCurrentTime]);

  const handleLoadedMetadata = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    if (video.duration && !isNaN(video.duration)) {
      setDuration(video.duration);
    }
    // Apply any pending seek that was issued before metadata was available
    applyPendingSeekIfExists();
    // Also restore persisted currentTime if no pending seek covered it
    if (playback.currentTime > 0 && Math.abs(video.currentTime - playback.currentTime) > 0.5) {
      video.currentTime = playback.currentTime;
    }
  }, [videoRef, setDuration, playback.currentTime]);

  const handleLoadedData = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    setMediaPlayable(video.duration && !isNaN(video.duration) ? video.duration : undefined);
  }, [videoRef, setMediaPlayable]);

  const handleCanPlay = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    setMediaPlayable(video.duration && !isNaN(video.duration) ? video.duration : undefined);
  }, [videoRef, setMediaPlayable]);

  const handleError = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    const errorMsg = mapMediaError(video.error);
    setMediaError(errorMsg);
  }, [videoRef, setMediaError]);

  const handleEnded = useCallback(() => {
    setIsPlaying(false);
  }, [setIsPlaying]);

  const togglePlay = useCallback(() => {
    setIsPlaying(!playback.isPlaying);
  }, [playback.isPlaying, setIsPlaying]);

  const seekTo = useCallback(
    (timeSeconds: number) => {
      // store.seek() is the single authoritative path: updates Zustand + video.currentTime
      seek(timeSeconds);
    },
    [seek]
  );

  const toggleMute = useCallback(() => {
    setMuted(!playback.muted);
  }, [playback.muted, setMuted]);

  // Keyboard Shortcuts Handler
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't intercept when user is typing in an input/textarea
      const tag = (e.target as HTMLElement)?.tagName?.toLowerCase();
      if (tag === 'input' || tag === 'textarea' || tag === 'select') {
        return;
      }

      switch (e.code) {
        case 'Space':
          e.preventDefault();
          togglePlay();
          break;
        case 'ArrowLeft':
          e.preventDefault();
          stepBackward(e.shiftKey ? 0.1 : 1.0);
          break;
        case 'ArrowRight':
          e.preventDefault();
          stepForward(e.shiftKey ? 0.1 : 1.0);
          break;
        case 'Home':
          e.preventDefault();
          seekTo(0);
          break;
        case 'End':
          e.preventDefault();
          seekTo(playback.duration);
          break;
        case 'KeyM':
          e.preventDefault();
          toggleMute();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [togglePlay, stepBackward, stepForward, seekTo, toggleMute, playback.duration]);

  return {
    handleTimeUpdate,
    handleLoadedMetadata,
    handleLoadedData,
    handleCanPlay,
    handleError,
    handleEnded,
    togglePlay,
    seekTo,
    toggleMute,
  };
};
