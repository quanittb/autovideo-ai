import React, { useRef } from 'react';
import { 
  Play, 
  AlertCircle, 
  Loader2, 
  Maximize2, 
  Film 
} from 'lucide-react';
import { useEditorStore } from '../stores/editorStore';
import { useMediaPlayback } from '../hooks/useMediaPlayback';

interface VideoPreviewProps {
  className?: string;
}

export const VideoPreview: React.FC<VideoPreviewProps> = ({ className = '' }) => {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const { mediaAsset, mediaUrl, loadStatus, errorMessage, playback } = useEditorStore();
  const { handleTimeUpdate, handleLoadedMetadata, handleEnded, togglePlay } =
    useMediaPlayback(videoRef);

  const containerRef = useRef<HTMLDivElement | null>(null);

  const handleToggleFullscreen = () => {
    if (!containerRef.current) return;
    if (!document.fullscreenElement) {
      containerRef.current.requestFullscreen().catch((err) => {
        console.warn('Fullscreen request failed:', err);
      });
    } else {
      document.exitFullscreen().catch((err) => {
        console.warn('Exit fullscreen failed:', err);
      });
    }
  };

  const formatTime = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    const ms = Math.floor((secs % 1) * 10);
    return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}.${ms}`;
  };

  return (
    <div
      ref={containerRef}
      className={`relative rounded-2xl border border-slate-800 bg-slate-950 overflow-hidden shadow-2xl flex flex-col items-center justify-center group select-none ${className}`}
      style={{ minHeight: '340px' }}
    >
      {/* Video Content / State Views */}
      {loadStatus === 'LOADING' && (
        <div className="flex flex-col items-center gap-3 text-slate-400">
          <Loader2 className="w-8 h-8 animate-spin text-indigo-400" />
          <span className="text-xs font-mono">Loading real media asset...</span>
        </div>
      )}

      {loadStatus === 'ERROR' && (
        <div className="flex flex-col items-center gap-3 text-rose-400 p-6 text-center max-w-md">
          <AlertCircle className="w-8 h-8 text-rose-500" />
          <span className="text-xs font-bold font-mono">MEDIA_PLAYBACK_ERROR</span>
          <p className="text-[11px] text-slate-400">{errorMessage || 'Could not load video source file.'}</p>
        </div>
      )}

      {loadStatus === 'IDLE' && !mediaUrl && (
        <div className="flex flex-col items-center gap-2 text-slate-500 p-6 text-center">
          <Film className="w-10 h-10 stroke-1 text-slate-600" />
          <span className="text-xs font-semibold text-slate-400">No Video Ingested</span>
          <p className="text-[11px] text-slate-500">Import a video in Step 1 to preview and edit.</p>
        </div>
      )}

      {mediaUrl && (
        <>
          <video
            ref={videoRef}
            src={mediaUrl}
            onTimeUpdate={handleTimeUpdate}
            onLoadedMetadata={handleLoadedMetadata}
            onEnded={handleEnded}
            onClick={togglePlay}
            playsInline
            className="w-full h-full object-contain max-h-[460px] cursor-pointer"
          />

          {/* Top Info HUD Bar */}
          <div className="absolute top-3 left-3 right-3 flex items-center justify-between pointer-events-none z-10">
            <div className="flex items-center gap-2">
              <span className="px-2.5 py-1 rounded-lg bg-slate-950/80 backdrop-blur-md border border-slate-800 text-[11px] font-mono text-slate-300 font-semibold shadow-lg">
                {formatTime(playback.currentTime)} / {formatTime(playback.duration)}
              </span>
              {mediaAsset && (
                <span className="px-2 py-1 rounded-lg bg-slate-950/80 backdrop-blur-md border border-slate-800 text-[10px] font-mono text-indigo-300 font-medium shadow-lg">
                  {mediaAsset.width}×{mediaAsset.height} • {mediaAsset.fps} FPS
                </span>
              )}
            </div>

            <div className="flex items-center gap-1.5 pointer-events-auto">
              <button
                onClick={handleToggleFullscreen}
                className="p-1.5 rounded-lg bg-slate-950/80 backdrop-blur-md border border-slate-800 text-slate-400 hover:text-white hover:bg-slate-800/80 transition-colors shadow-lg"
                title="Fullscreen Preview"
              >
                <Maximize2 className="w-3.5 h-3.5" />
              </button>
            </div>
          </div>

          {/* Center Large Play Icon Overlay when paused */}
          {!playback.isPlaying && (
            <button
              onClick={togglePlay}
              className="absolute inset-0 m-auto w-16 h-16 rounded-full bg-indigo-600/90 hover:bg-indigo-500 text-white flex items-center justify-center shadow-2xl backdrop-blur-md hover:scale-105 transition-all z-10 cursor-pointer"
              aria-label="Play video"
            >
              <Play className="w-7 h-7 fill-current ml-0.5" />
            </button>
          )}
        </>
      )}
    </div>
  );
};
