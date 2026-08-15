import React from 'react';
import { 
  Play, 
  Pause, 
  SkipBack, 
  SkipForward, 
  ChevronLeft, 
  ChevronRight, 
  Volume2, 
  VolumeX, 
  ZoomIn, 
  ZoomOut, 
  Maximize
} from 'lucide-react';
import { useEditorStore } from '../stores/editorStore';

export const PlaybackControls: React.FC = () => {
  const {
    playback,
    timelineZoom,
    setIsPlaying,
    setVolume,
    setMuted,
    setTimelineZoom,
    stepForward,
    stepBackward,
    seek,
  } = useEditorStore();

  const formatTime = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    const ms = Math.floor((secs % 1) * 10);
    return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}.${ms}`;
  };

  const handleZoomIn = () => setTimelineZoom(timelineZoom + 0.25);
  const handleZoomOut = () => setTimelineZoom(timelineZoom - 0.25);
  const handleZoomFit = () => setTimelineZoom(1.0);

  return (
    <div className="p-3 bg-slate-900/80 border-y border-slate-800/80 flex items-center justify-between text-xs text-slate-300 select-none">
      {/* Left: Transport Buttons */}
      <div className="flex items-center gap-2">
        <button
          onClick={() => seek(0)}
          className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
          title="Jump to Start (Home)"
        >
          <SkipBack className="w-4 h-4" />
        </button>

        <button
          onClick={() => stepBackward(1.0)}
          className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
          title="Step Back 1s (Left Arrow)"
        >
          <ChevronLeft className="w-4 h-4" />
        </button>

        <button
          onClick={() => setIsPlaying(!playback.isPlaying)}
          className="p-2 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white shadow-md shadow-indigo-900/30 transition-all font-bold"
          title="Play / Pause (Space)"
        >
          {playback.isPlaying ? (
            <Pause className="w-4 h-4 fill-current" />
          ) : (
            <Play className="w-4 h-4 fill-current ml-0.5" />
          )}
        </button>

        <button
          onClick={() => stepForward(1.0)}
          className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
          title="Step Forward 1s (Right Arrow)"
        >
          <ChevronRight className="w-4 h-4" />
        </button>

        <button
          onClick={() => seek(playback.duration)}
          className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
          title="Jump to End (End)"
        >
          <SkipForward className="w-4 h-4" />
        </button>

        {/* Time Counter */}
        <div className="ml-3 px-3 py-1 rounded-lg bg-slate-950 border border-slate-800 font-mono text-[11px] text-slate-200">
          <span className="text-indigo-400 font-semibold">{formatTime(playback.currentTime)}</span>
          <span className="text-slate-500 mx-1">/</span>
          <span className="text-slate-400">{formatTime(playback.duration)}</span>
        </div>
      </div>

      {/* Right: Volume & Timeline Scale Controls */}
      <div className="flex items-center gap-4">
        {/* Volume Scrubber */}
        <div className="flex items-center gap-2">
          <button
            onClick={() => setMuted(!playback.muted)}
            className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
            title="Mute / Unmute (M)"
          >
            {playback.muted || playback.volume === 0 ? (
              <VolumeX className="w-4 h-4 text-rose-400" />
            ) : (
              <Volume2 className="w-4 h-4" />
            )}
          </button>
          <input
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={playback.muted ? 0 : playback.volume}
            onChange={(e) => setVolume(parseFloat(e.target.value))}
            className="w-16 h-1 bg-slate-800 rounded-lg appearance-none cursor-pointer accent-indigo-500"
          />
        </div>

        {/* Timeline Zoom Controls */}
        <div className="flex items-center gap-1 bg-slate-950 px-2 py-1 rounded-xl border border-slate-800 text-xs">
          <span className="text-[10px] font-mono text-slate-500 mr-1 uppercase">Zoom</span>
          <button
            onClick={handleZoomOut}
            disabled={timelineZoom <= 0.5}
            className="p-1 rounded text-slate-400 hover:text-white disabled:opacity-30 transition-colors"
            title="Zoom Out"
          >
            <ZoomOut className="w-3.5 h-3.5" />
          </button>
          <span className="font-mono text-[10px] text-slate-300 w-9 text-center">
            {Math.round(timelineZoom * 100)}%
          </span>
          <button
            onClick={handleZoomIn}
            disabled={timelineZoom >= 3.0}
            className="p-1 rounded text-slate-400 hover:text-white disabled:opacity-30 transition-colors"
            title="Zoom In"
          >
            <ZoomIn className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={handleZoomFit}
            className="p-1 rounded text-slate-400 hover:text-indigo-400 ml-1 transition-colors"
            title="Fit Timeline (100%)"
          >
            <Maximize className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
};
