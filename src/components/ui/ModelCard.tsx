import React from 'react';
import { HardDrive, Cpu, Download, Trash2, CheckCircle2, Shield } from 'lucide-react';
import { ModelDescriptor } from '../../types/contracts';

interface ModelCardProps {
  model: ModelDescriptor;
  onDownloadClick?: (id: string) => void;
  onRemoveClick?: (id: string) => void;
}

export const ModelCard: React.FC<ModelCardProps> = ({
  model,
  onDownloadClick,
  onRemoveClick,
}) => {
  return (
    <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 hover:border-slate-700 transition-all space-y-4 flex flex-col justify-between">
      <div className="space-y-3">
        {/* Header */}
        <div className="flex items-start justify-between gap-2">
          <div>
            <div className="flex items-center gap-2">
              <h4 className="text-sm font-semibold text-slate-100">{model.name}</h4>
              <span className="px-1.5 py-0.5 rounded text-[10px] font-mono text-slate-400 bg-slate-800">
                v{model.version}
              </span>
            </div>
            <span className="text-[11px] text-indigo-400 font-medium capitalize block mt-0.5">
              Task: {model.task}
            </span>
          </div>

          <span
            className={`px-2 py-0.5 rounded text-[10px] font-bold tracking-wider ${
              model.isDownloaded
                ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/30'
                : 'bg-slate-800 text-slate-400 border border-slate-700'
            }`}
          >
            {model.isDownloaded ? 'READY' : 'NOT INSTALLED'}
          </span>
        </div>

        {/* Specs Details */}
        <div className="grid grid-cols-2 gap-2 text-[11px] bg-slate-950/60 p-2.5 rounded-xl border border-slate-800/80">
          <div className="flex items-center gap-1.5 text-slate-400">
            <HardDrive className="w-3.5 h-3.5 text-slate-500" />
            <span>{(model.fileSizeBytes / (1024 * 1024 * 1024)).toFixed(1)} GB</span>
          </div>

          <div className="flex items-center gap-1.5 text-slate-400">
            <Cpu className="w-3.5 h-3.5 text-slate-500" />
            <span>{(model.vramRequirementMB / 1024).toFixed(0)} GB VRAM</span>
          </div>

          <div className="flex items-center gap-1.5 text-slate-400">
            <Shield className="w-3.5 h-3.5 text-slate-500" />
            <span>{model.license}</span>
          </div>

          <div className="text-slate-400 truncate">
            <span>{model.runtime}</span>
          </div>
        </div>
      </div>

      {/* Action Footer */}
      <div className="flex items-center justify-between pt-2 border-t border-slate-800/80">
        <span className="text-[10px] text-slate-500 font-mono">
          SHA: {model.sha256Checksum.slice(0, 10)}...
        </span>

        <div className="flex items-center gap-2">
          {model.isDownloaded ? (
            <>
              <button
                onClick={() => onRemoveClick && onRemoveClick(model.id)}
                className="p-1.5 rounded-lg bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 border border-rose-500/20 text-xs transition-colors"
                title="Remove model weights"
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>
              <div className="flex items-center gap-1 text-xs text-emerald-400 font-medium px-2 py-1 bg-emerald-500/10 rounded-lg">
                <CheckCircle2 className="w-3.5 h-3.5" />
                <span>Installed</span>
              </div>
            </>
          ) : (
            <button
              onClick={() => onDownloadClick && onDownloadClick(model.id)}
              className="px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold flex items-center gap-1.5 shadow-md transition-all"
            >
              <Download className="w-3.5 h-3.5" />
              <span>Download Weights</span>
            </button>
          )}
        </div>
      </div>
    </div>
  );
};
