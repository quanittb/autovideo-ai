import React from 'react';
import { 
  FileVideo, 
  CheckCircle2 
} from 'lucide-react';
import { useEditorStore } from '../stores/editorStore';

export const MediaInspector: React.FC = () => {
  const { mediaAsset } = useEditorStore();

  if (!mediaAsset) {
    return (
      <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800 text-xs text-slate-500 text-center">
        No active media asset selected
      </div>
    );
  }

  const fileSizeMb = (mediaAsset.fileSizeBytes / (1024 * 1024)).toFixed(2);

  return (
    <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4 text-xs select-none">
      <div className="flex items-center justify-between border-b border-slate-800/80 pb-3">
        <span className="font-bold text-slate-200">Media Inspector</span>
        <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 flex items-center gap-1">
          <CheckCircle2 className="w-3 h-3" /> READY
        </span>
      </div>

      <div className="space-y-3 font-mono">
        {/* File Name */}
        <div className="space-y-1">
          <span className="text-[10px] text-slate-500 uppercase tracking-wider block">File Name</span>
          <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 text-xs font-semibold truncate flex items-center gap-2">
            <FileVideo className="w-3.5 h-3.5 text-indigo-400 shrink-0" />
            <span className="truncate">{mediaAsset.originalFileName}</span>
          </div>
        </div>

        {/* 2x2 Stats Grid */}
        <div className="grid grid-cols-2 gap-2 text-[11px]">
          <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-0.5">
            <span className="text-[10px] text-slate-500 block">Duration</span>
            <span className="text-slate-200 font-bold">{mediaAsset.durationSeconds.toFixed(2)}s</span>
          </div>

          <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-0.5">
            <span className="text-[10px] text-slate-500 block">Resolution</span>
            <span className="text-slate-200 font-bold">{mediaAsset.width} × {mediaAsset.height}</span>
          </div>

          <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-0.5">
            <span className="text-[10px] text-slate-500 block">Framerate</span>
            <span className="text-slate-200 font-bold">{mediaAsset.fps} FPS</span>
          </div>

          <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-0.5">
            <span className="text-[10px] text-slate-500 block">File Size</span>
            <span className="text-slate-200 font-bold">{fileSizeMb} MB</span>
          </div>
        </div>

        {/* Codecs */}
        <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 flex justify-between items-center text-[11px]">
          <span className="text-slate-500">Video / Audio Codec:</span>
          <span className="text-slate-200 font-semibold uppercase">
            {mediaAsset.videoCodec} / {mediaAsset.audioCodec || 'None'}
          </span>
        </div>

        {/* Cache Status */}
        <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 flex justify-between items-center text-[11px]">
          <span className="text-slate-500">Timeline Cache:</span>
          {mediaAsset.frameFiles.length > 0 ? (
            <span className="text-purple-400 font-semibold">
              {mediaAsset.frameFiles.length} Frames Available
            </span>
          ) : (
            <span className="text-slate-500">Uncached (Clean Strip)</span>
          )}
        </div>
      </div>
    </div>
  );
};
