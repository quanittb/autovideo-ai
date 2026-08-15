import React from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Film } from 'lucide-react';
import { useEditorStore } from '../stores/editorStore';

export const VideoTrack: React.FC = () => {
  const { mediaAsset, playback } = useEditorStore();

  const frames = mediaAsset?.frameFiles || [];
  const framesDir = mediaAsset?.framesDir;

  return (
    <div className="relative h-14 bg-slate-900/90 rounded-xl border border-slate-800/80 overflow-hidden flex items-center shadow-inner group/track">
      {/* Track Label Badge */}
      <div className="absolute left-2 top-2 z-20 px-2 py-0.5 rounded bg-slate-950/80 backdrop-blur-md border border-slate-800 text-[10px] font-mono font-semibold text-slate-300 flex items-center gap-1.5 pointer-events-none shadow-md">
        <Film className="w-3 h-3 text-purple-400" />
        <span>V1 • {mediaAsset?.originalFileName || 'Source Video'}</span>
      </div>

      {/* Frame Strip or Media Block */}
      {frames.length > 0 && framesDir ? (
        <div className="w-full h-full flex overflow-hidden opacity-80 group-hover/track:opacity-100 transition-opacity">
          {frames.map((frameFile, index) => {
            const framePath = `${framesDir}/${frameFile}`;
            const src = convertFileSrc(framePath);
            return (
              <div
                key={index}
                className="flex-1 h-full border-r border-slate-950/60 overflow-hidden relative min-w-[36px] max-w-[90px] bg-slate-950"
              >
                <img
                  src={src}
                  alt={`Frame ${frameFile}`}
                  loading="lazy"
                  className="w-full h-full object-cover select-none pointer-events-none"
                />
              </div>
            );
          })}
        </div>
      ) : (
        /* Clean Media Strip Fallback */
        <div className="w-full h-full bg-gradient-to-r from-purple-950/40 via-indigo-950/30 to-purple-950/40 flex items-center justify-center">
          <div className="text-[11px] font-mono text-indigo-300/80 flex items-center gap-2">
            <span>Source Footage</span>
            <span className="text-slate-500">•</span>
            <span>{(playback.duration || 0).toFixed(1)}s</span>
          </div>
        </div>
      )}
    </div>
  );
};
