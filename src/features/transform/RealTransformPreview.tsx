import React, { useRef, useState, useEffect, useCallback } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import {
  Play,
  Pause,
  Volume2,
  VolumeX,
  RotateCcw,
  ExternalLink,
  FolderOpen,
  AlertCircle,
  Loader2,
} from 'lucide-react';
import {
  cloudApi,
  type AuthorizedAssetPreview,
  type CloudJobEventPayload,
} from '../../lib/ipc';
import { getCloudJobVisualState } from '../../stores/cloudJobHelpers';

interface RealTransformPreviewProps {
  projectId: string;
  selectedJob: CloudJobEventPayload | null;
  authorizedSource: AuthorizedAssetPreview | null;
  authorizedArtifact: AuthorizedAssetPreview | null;
  onRefreshSource?: () => void;
  onRefreshArtifact?: () => void;
}

export const RealTransformPreview: React.FC<RealTransformPreviewProps> = ({
  projectId,
  selectedJob,
  authorizedSource,
  authorizedArtifact,
}) => {
  const sourceVideoRef = useRef<HTMLVideoElement | null>(null);
  const artifactVideoRef = useRef<HTMLVideoElement | null>(null);

  const [isPlaying, setIsPlaying] = useState(false);
  const [isMuted, setIsMuted] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [isWebmAlphaSupported, setIsWebmAlphaSupported] = useState(true);
  const [splitPos, setSplitPos] = useState(50);
  const [isDraggingSplit, setIsDraggingSplit] = useState(false);

  // Check browser capability for WebM VP9 Alpha
  useEffect(() => {
    const video = document.createElement('video');
    const canPlayWebm = video.canPlayType('video/webm; codecs="vp9"');
    if (canPlayWebm === '' && authorizedArtifact?.container === 'webm') {
      setIsWebmAlphaSupported(false);
    } else {
      setIsWebmAlphaSupported(true);
    }
  }, [authorizedArtifact]);

  // Synchronized playback controls
  const handleTogglePlay = useCallback(() => {
    const source = sourceVideoRef.current;
    const artifact = artifactVideoRef.current;

    if (isPlaying) {
      source?.pause();
      artifact?.pause();
      setIsPlaying(false);
    } else {
      source?.play().catch(() => {});
      artifact?.play().catch(() => {});
      setIsPlaying(true);
    }
  }, [isPlaying]);

  const handleTimeUpdate = () => {
    const source = sourceVideoRef.current;
    if (source) {
      setCurrentTime(source.currentTime);
      if (source.duration && !isNaN(source.duration)) {
        setDuration(source.duration);
      }
    }
  };

  const handleSeek = (e: React.ChangeEvent<HTMLInputElement>) => {
    const time = parseFloat(e.target.value);
    setCurrentTime(time);
    if (sourceVideoRef.current) {
      sourceVideoRef.current.currentTime = time;
    }
    if (artifactVideoRef.current) {
      artifactVideoRef.current.currentTime = time;
    }
  };

  const handleRestart = () => {
    if (sourceVideoRef.current) sourceVideoRef.current.currentTime = 0;
    if (artifactVideoRef.current) artifactVideoRef.current.currentTime = 0;
    setCurrentTime(0);
  };

  const handleToggleMute = () => {
    const next = !isMuted;
    setIsMuted(next);
    if (sourceVideoRef.current) sourceVideoRef.current.muted = next;
    if (artifactVideoRef.current) artifactVideoRef.current.muted = true; // Keep artifact muted to avoid double audio
  };

  const formatTime = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  };

  const visualCategory = getCloudJobVisualState(selectedJob?.state);
  const isJobCompleted = visualCategory === 'success';
  const isJobRunning = visualCategory === 'running';
  const isJobFailed = visualCategory === 'failed';
  const isJobCancelled = visualCategory === 'cancelled';
  const isJobBlocked = visualCategory === 'blocked';
  const isJobApprovalRequired = visualCategory === 'approval_required';

  const sourceSrc = authorizedSource ? convertFileSrc(authorizedSource.localPath) : null;
  const artifactSrc = authorizedArtifact ? convertFileSrc(authorizedArtifact.localPath) : null;

  return (
    <div className="flex flex-col gap-4 h-full">
      {/* Header & Badges */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-semibold text-slate-200">Transformation Preview</h3>
          {selectedJob && (
            <span
              className={`px-2 py-0.5 rounded text-[10px] font-bold uppercase font-mono ${
                isJobCompleted
                  ? 'bg-emerald-950/80 text-emerald-300 border border-emerald-700/60'
                  : isJobRunning
                  ? 'bg-indigo-950/80 text-indigo-300 border border-indigo-700/60 animate-pulse'
                  : isJobFailed
                  ? 'bg-rose-950/80 text-rose-300 border border-rose-700/60'
                  : isJobCancelled
                  ? 'bg-amber-950/80 text-amber-300 border border-amber-700/60'
                  : isJobBlocked
                  ? 'bg-purple-950/80 text-purple-300 border border-purple-700/60'
                  : isJobApprovalRequired
                  ? 'bg-amber-950/80 text-amber-300 border border-amber-700/60'
                  : 'bg-slate-900 text-slate-400 border border-slate-700'
              }`}
            >
              {selectedJob.state}
            </span>
          )}
        </div>

        {/* Truthful Format Badges */}
        <div className="flex items-center gap-2">
          {authorizedArtifact && isJobCompleted && (
            <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-slate-900 border border-slate-700 text-[11px] font-mono text-slate-300">
              {authorizedArtifact.alphaValidated ? (
                <>
                  <span className="w-2 h-2 rounded-full bg-cyan-400"></span>
                  <span>WebM • VP9 • Alpha</span>
                </>
              ) : (
                <>
                  <span className="w-2 h-2 rounded-full bg-emerald-400"></span>
                  <span>MP4 • H.264</span>
                </>
              )}
              {authorizedArtifact.audioRequired && (
                <span className="ml-1 text-[10px] text-amber-300 font-semibold">• Audio preserved</span>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Main Dual Player Canvas */}
      <div
        className="relative rounded-2xl border border-slate-800 bg-slate-950 overflow-hidden shadow-2xl aspect-video select-none group"
        onMouseMove={(e) => {
          if (!isDraggingSplit) return;
          const rect = e.currentTarget.getBoundingClientRect();
          const pos = Math.max(5, Math.min(95, ((e.clientX - rect.left) / rect.width) * 100));
          setSplitPos(pos);
        }}
        onMouseUp={() => setIsDraggingSplit(false)}
        onMouseLeave={() => setIsDraggingSplit(false)}
      >
        {/* Transparent Checkerboard Pattern for Alpha */}
        <div
          className="absolute inset-0 opacity-15"
          style={{
            backgroundImage: `radial-gradient(#64748b 1px, transparent 1px)`,
            backgroundSize: '16px 16px',
          }}
        />

        {/* Source Media Layer (Left / Base) */}
        <div className="absolute inset-0 flex items-center justify-center bg-black">
          {sourceSrc ? (
            <video
              ref={sourceVideoRef}
              src={sourceSrc}
              className="w-full h-full object-contain"
              onTimeUpdate={handleTimeUpdate}
              onLoadedMetadata={handleTimeUpdate}
              muted={isMuted}
              playsInline
            />
          ) : (
            <div className="flex flex-col items-center justify-center p-6 text-slate-500 space-y-2">
              <AlertCircle className="w-8 h-8 opacity-40" />
              <span className="text-xs">Source media not authorized</span>
            </div>
          )}
          <div className="absolute top-3 left-3 px-2 py-0.5 rounded text-[10px] font-bold bg-slate-950/80 text-slate-300 border border-slate-700">
            Source
          </div>
        </div>

        {/* Artifact Layer (Right / Overlaid with Split Clip) */}
        {isJobCompleted && artifactSrc && (
          <div
            className="absolute inset-0 flex items-center justify-center border-l border-indigo-500/60 bg-transparent"
            style={{
              clipPath: `polygon(${splitPos}% 0, 100% 0, 100% 100%, ${splitPos}% 100%)`,
            }}
          >
            {isWebmAlphaSupported ? (
              <video
                ref={artifactVideoRef}
                src={artifactSrc}
                className="w-full h-full object-contain"
                muted={true}
                playsInline
              />
            ) : (
              <div className="flex flex-col items-center justify-center p-6 bg-slate-900/90 text-slate-200 text-center space-y-3">
                <AlertCircle className="w-8 h-8 text-amber-400" />
                <div>
                  <h4 className="text-xs font-bold text-slate-100">WebM VP9 Alpha Playback Unavailable</h4>
                  <p className="text-[11px] text-slate-400 mt-1 max-w-xs">
                    This WebView does not support native transparent VP9 playback. Open the artifact directly.
                  </p>
                </div>
                <div className="flex gap-2">
                  <button
                    onClick={() => selectedJob && cloudApi.openCloudArtifact(projectId, selectedJob.internalJobId)}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold"
                  >
                    <ExternalLink className="w-3.5 h-3.5" />
                    <span>Open Video</span>
                  </button>
                  <button
                    onClick={() => selectedJob && cloudApi.openCloudArtifactFolder(projectId, selectedJob.internalJobId)}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold"
                  >
                    <FolderOpen className="w-3.5 h-3.5" />
                    <span>Open Folder</span>
                  </button>
                </div>
              </div>
            )}
            <div className="absolute top-3 right-3 px-2 py-0.5 rounded text-[10px] font-bold bg-purple-950/80 text-purple-200 border border-purple-700">
              Output
            </div>
          </div>
        )}

        {/* Split Dragger */}
        {isJobCompleted && artifactSrc && (
          <div
            className="absolute top-0 bottom-0 w-1.5 bg-indigo-500 cursor-ew-resize z-20 flex items-center justify-center shadow-lg hover:w-2 transition-all"
            style={{ left: `${splitPos}%` }}
            onMouseDown={() => setIsDraggingSplit(true)}
          >
            <div className="w-6 h-6 rounded-full bg-indigo-600 border-2 border-white text-white flex items-center justify-center text-[9px] font-bold shadow-lg shadow-indigo-950">
              ⚡
            </div>
          </div>
        )}

        {/* Running Progress Overlay */}
        {isJobRunning && selectedJob && (
          <div className="absolute inset-0 bg-slate-950/80 backdrop-blur-sm flex flex-col items-center justify-center p-6 text-center space-y-4 z-30">
            <Loader2 className="w-10 h-10 text-indigo-400 animate-spin" />
            <div className="space-y-1">
              <h4 className="text-sm font-bold text-slate-100 uppercase tracking-wide">
                Transformation in Progress
              </h4>
              <p className="text-xs text-slate-400 font-mono">
                State: {selectedJob.state} {selectedJob.remoteStatus ? `(${selectedJob.remoteStatus})` : ''}
              </p>
            </div>

            {/* Truthful Progress Percentage: show if present, otherwise indeterminate */}
            {selectedJob.progressPct != null ? (
              <div className="w-64 space-y-1.5">
                <div className="flex justify-between text-[11px] font-mono text-slate-400">
                  <span>Progress</span>
                  <span>{Math.round(selectedJob.progressPct)}%</span>
                </div>
                <div className="h-2 w-full bg-slate-800 rounded-full overflow-hidden">
                  <div
                    className="h-full bg-indigo-500 transition-all duration-300"
                    style={{ width: `${Math.max(5, selectedJob.progressPct)}%` }}
                  />
                </div>
              </div>
            ) : (
              <div className="w-48 h-1.5 bg-slate-800 rounded-full overflow-hidden">
                <div className="h-full bg-indigo-500 animate-pulse w-full" />
              </div>
            )}
          </div>
        )}

        {/* Failed Overlay */}
        {isJobFailed && selectedJob && (
          <div className="absolute inset-0 bg-rose-950/80 backdrop-blur-sm flex flex-col items-center justify-center p-6 text-center space-y-3 z-30">
            <AlertCircle className="w-10 h-10 text-rose-400" />
            <h4 className="text-sm font-bold text-rose-200">Transformation Failed</h4>
            <p className="text-xs text-rose-300/80 max-w-md font-mono">
              {selectedJob.error?.sanitizedMessage || 'Unknown provider error'}
            </p>
          </div>
        )}
      </div>

      {/* Playback Control Bar */}
      <div className="p-3 bg-slate-900/80 border border-slate-800/80 rounded-xl flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <button
            onClick={handleTogglePlay}
            className="p-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white transition-colors"
          >
            {isPlaying ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4 fill-current" />}
          </button>
          <button
            onClick={handleRestart}
            className="p-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 transition-colors"
          >
            <RotateCcw className="w-4 h-4" />
          </button>
          <button
            onClick={handleToggleMute}
            className="p-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 transition-colors"
          >
            {isMuted ? <VolumeX className="w-4 h-4" /> : <Volume2 className="w-4 h-4" />}
          </button>
          <span className="font-mono text-xs text-slate-400">
            {formatTime(currentTime)} / {formatTime(duration)}
          </span>
        </div>

        {/* Timeline Slider */}
        <div className="flex-1 max-w-md">
          <input
            type="range"
            min={0}
            max={duration || 100}
            step={0.01}
            value={currentTime}
            onChange={handleSeek}
            className="w-full accent-indigo-500 h-1.5 bg-slate-800 rounded-lg cursor-pointer"
          />
        </div>

        {/* Actions for Completed Artifacts */}
        {isJobCompleted && selectedJob && (
          <div className="flex items-center gap-2">
            <button
              onClick={() => cloudApi.openCloudArtifact(projectId, selectedJob.internalJobId)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold transition-colors"
              title="Open video in default player"
            >
              <ExternalLink className="w-3.5 h-3.5" />
              <span>Open Video</span>
            </button>
            <button
              onClick={() => cloudApi.openCloudArtifactFolder(projectId, selectedJob.internalJobId)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold transition-colors"
              title="Open containing folder"
            >
              <FolderOpen className="w-3.5 h-3.5" />
              <span>Folder</span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
};
