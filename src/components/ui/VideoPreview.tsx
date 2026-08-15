import React, { useState } from 'react';
import { Play, Pause, Volume2, VolumeX, Maximize2 } from 'lucide-react';
import { MockBadge } from '../common/MockBadge';

interface VideoPreviewProps {
  title?: string;
  thumbnailEmoji?: string;
  durationFormatted?: string;
  isFixture?: boolean;
  className?: string;
  badgeLabel?: string;
}

export const VideoPreview: React.FC<VideoPreviewProps> = ({
  title = 'Input Footage Preview',
  thumbnailEmoji = '🦊',
  durationFormatted = '01:02',
  isFixture = true,
  className = '',
  badgeLabel = 'FIXTURE PREVIEW',
}) => {
  const [isPlaying, setIsPlaying] = useState(false);
  const [isMuted, setIsMuted] = useState(false);
  const [currentTimeFormatted] = useState('00:14');

  return (
    <div className={`relative rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden shadow-2xl flex flex-col justify-between aspect-video group ${className}`}>
      {/* Video Content Canvas Area */}
      <div className="relative w-full h-full bg-gradient-to-br from-amber-950 via-slate-900 to-slate-950 flex flex-col items-center justify-center p-6 select-none">
        <div className="text-center space-y-2 group-hover:scale-105 transition-transform">
          <span className="text-7xl drop-shadow-2xl">{thumbnailEmoji}</span>
          <p className="text-xs font-semibold text-amber-200/90">{title}</p>
        </div>

        {/* Top Badges */}
        <div className="absolute top-3 right-3 flex items-center gap-2 z-10">
          {isFixture && <MockBadge label={badgeLabel} />}
        </div>

        {/* Center Big Play Button Overlay when paused */}
        {!isPlaying && (
          <button
            onClick={() => setIsPlaying(true)}
            className="absolute inset-0 m-auto w-14 h-14 rounded-full bg-indigo-600/80 hover:bg-indigo-600 text-white flex items-center justify-center shadow-2xl backdrop-blur-md hover:scale-110 transition-all z-10"
            aria-label="Play video"
          >
            <Play className="w-6 h-6 fill-current ml-0.5" />
          </button>
        )}
      </div>

      {/* Video Transport Controls Bar */}
      <div className="p-3.5 bg-slate-950/85 backdrop-blur-md border-t border-slate-800 flex items-center justify-between text-xs text-slate-300 z-20">
        <div className="flex items-center gap-3">
          <button
            onClick={() => setIsPlaying(!isPlaying)}
            className="p-1 rounded-lg text-slate-300 hover:text-white hover:bg-slate-800 transition-colors"
            aria-label={isPlaying ? 'Pause' : 'Play'}
          >
            {isPlaying ? (
              <Pause className="w-4 h-4 fill-current" />
            ) : (
              <Play className="w-4 h-4 fill-current" />
            )}
          </button>

          {/* Time Scrubber Bar */}
          <div className="w-48 md:w-64 h-1.5 bg-slate-800 rounded-full overflow-hidden cursor-pointer relative group/bar">
            <div className="h-full w-1/4 bg-indigo-500 rounded-full group-hover/bar:bg-indigo-400" />
          </div>

          <span className="font-mono text-[11px] text-slate-400">
            {currentTimeFormatted} / {durationFormatted}
          </span>
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={() => setIsMuted(!isMuted)}
            className="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
            aria-label={isMuted ? 'Unmute' : 'Mute'}
          >
            {isMuted ? <VolumeX className="w-4 h-4" /> : <Volume2 className="w-4 h-4" />}
          </button>

          <button
            className="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
            aria-label="Fullscreen"
          >
            <Maximize2 className="w-4 h-4" />
          </button>
        </div>
      </div>
    </div>
  );
};
