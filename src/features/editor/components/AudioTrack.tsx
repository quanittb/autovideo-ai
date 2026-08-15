import React from 'react';
import { Volume2, VolumeX } from 'lucide-react';
import { useEditorStore } from '../stores/editorStore';

export const AudioTrack: React.FC = () => {
  const { mediaAsset } = useEditorStore();
  const hasAudio = mediaAsset?.hasAudio ?? false;

  return (
    <div className="relative h-10 bg-slate-900/70 rounded-xl border border-slate-800/80 overflow-hidden flex items-center shadow-inner">
      {/* Track Label Badge */}
      <div className="absolute left-2 top-2 z-20 px-2 py-0.5 rounded bg-slate-950/80 backdrop-blur-md border border-slate-800 text-[10px] font-mono font-semibold text-slate-300 flex items-center gap-1.5 pointer-events-none shadow-md">
        {hasAudio ? (
          <>
            <Volume2 className="w-3 h-3 text-emerald-400" />
            <span>A1 • {mediaAsset?.audioCodec ? mediaAsset.audioCodec.toUpperCase() : 'Stereo Audio'}</span>
          </>
        ) : (
          <>
            <VolumeX className="w-3 h-3 text-slate-500" />
            <span className="text-slate-500">NO AUDIO STREAM</span>
          </>
        )}
      </div>

      {/* Audio Pattern Display */}
      {hasAudio ? (
        <div className="w-full h-full bg-gradient-to-r from-emerald-950/20 via-teal-950/30 to-emerald-950/20 flex items-center justify-around px-24 opacity-60">
          {/* Subtle audio waveform visual bars */}
          {Array.from({ length: 48 }).map((_, idx) => {
            const height = 20 + Math.sin(idx * 0.4) * 12 + ((idx % 3) * 4);
            return (
              <div
                key={idx}
                className="w-1 bg-emerald-400/40 rounded-full"
                style={{ height: `${height}%` }}
              />
            );
          })}
        </div>
      ) : (
        <div className="w-full h-full bg-slate-950/40 flex items-center justify-center text-[10px] font-mono text-slate-600">
          Source media contains no audio stream
        </div>
      )}
    </div>
  );
};
