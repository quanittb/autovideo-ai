import { useEffect, useCallback } from 'react';
import { useEditorStore } from '../stores/editorStore';

export const useMediaPlayback = (videoRef: React.RefObject<HTMLVideoElement | null>) => {
  const {
    playback,
    setIsPlaying,
    setCurrentTime,
    setDuration,
    setMuted,
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
    setDuration(video.duration);
    // Seek to initial restored currentTime if needed
    if (playback.currentTime > 0 && Math.abs(video.currentTime - playback.currentTime) > 0.5) {
      video.currentTime = playback.currentTime;
    }
  }, [videoRef, setDuration, playback.currentTime]);

  const handleEnded = useCallback(() => {
    setIsPlaying(false);
  }, [setIsPlaying]);

  const togglePlay = useCallback(() => {
    setIsPlaying(!playback.isPlaying);
  }, [playback.isPlaying, setIsPlaying]);

  const seekTo = useCallback(
    (timeSeconds: number) => {
      const video = videoRef.current;
      if (video) {
        video.currentTime = timeSeconds;
      }
      seek(timeSeconds);
    },
    [videoRef, seek]
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
          if (e.shiftKey) {
            stepBackward(0.1); // Small step
          } else {
            stepBackward(1.0); // Standard step
          }
          if (videoRef.current) {
            videoRef.current.currentTime = Math.max(0, videoRef.current.currentTime - (e.shiftKey ? 0.1 : 1.0));
          }
          break;
        case 'ArrowRight':
          e.preventDefault();
          if (e.shiftKey) {
            stepForward(0.1); // Small step
          } else {
            stepForward(1.0); // Standard step
          }
          if (videoRef.current) {
            videoRef.current.currentTime = Math.min(
              playback.duration,
              videoRef.current.currentTime + (e.shiftKey ? 0.1 : 1.0)
            );
          }
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
  }, [togglePlay, stepBackward, stepForward, seekTo, toggleMute, playback.duration, videoRef]);

  return {
    handleTimeUpdate,
    handleLoadedMetadata,
    handleEnded,
    togglePlay,
    seekTo,
    toggleMute,
  };
};
