import React, { useEffect, useState } from 'react';
import { HardDrive, Download, AlertCircle, CheckCircle2 } from 'lucide-react';
import { api } from '../../lib/ipc';
import { ModelDescriptor } from '../../types/contracts';
import { MockBadge } from '../../components/common/MockBadge';

export const ModelsView: React.FC = () => {
  const [models, setModels] = useState<ModelDescriptor[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    api.listModels().then((res) => {
      setModels(res);
      setIsLoading(false);
    });
  }, []);

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-slate-100 tracking-tight">AI Model Manager</h2>
          <p className="text-sm text-slate-400 mt-1">Manage local neural weights and runtime status</p>
        </div>
        <MockBadge label="STRICT AVAILABILITY PROTOCOL" />
      </div>

      <div className="p-4 rounded-xl bg-amber-500/10 border border-amber-500/20 text-xs text-amber-300 flex items-start gap-3">
        <AlertCircle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
        <p className="leading-relaxed">
          <strong>NEVER FAKE AI Policy Active:</strong> When local model weights are absent, transformation pipelines will reject execution with structured <code className="text-amber-200 bg-amber-950/60 px-1 py-0.5 rounded">MODEL_NOT_AVAILABLE</code> error codes instead of producing fake simulated results.
        </p>
      </div>

      <div className="space-y-4">
        {isLoading ? (
          <div className="p-8 text-center text-slate-500 text-xs">Loading model catalog...</div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {models.map((model) => (
              <div key={model.id} className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4">
                <div className="flex items-start justify-between">
                  <div>
                    <h4 className="text-sm font-semibold text-slate-200">{model.name}</h4>
                    <span className="text-[11px] text-slate-500 font-mono block mt-0.5">Task: {model.task}</span>
                  </div>
                  <span className={`px-2 py-0.5 rounded text-[10px] font-bold ${model.isDownloaded ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/40' : 'bg-slate-800 text-slate-400'}`}>
                    {model.isDownloaded ? 'READY' : 'NOT INSTALLED'}
                  </span>
                </div>

                <div className="flex items-center justify-between text-xs text-slate-400 border-t border-slate-800 pt-3">
                  <div className="flex items-center gap-1.5">
                    <HardDrive className="w-3.5 h-3.5 text-slate-500" />
                    <span>{(model.fileSizeBytes / (1024 * 1024 * 1024)).toFixed(1)} GB</span>
                  </div>

                  <button
                    disabled={model.isDownloaded}
                    className="px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 disabled:opacity-40 text-white text-xs font-semibold flex items-center gap-1.5 transition-all"
                  >
                    {model.isDownloaded ? (
                      <>
                        <CheckCircle2 className="w-3.5 h-3.5" />
                        <span>Installed</span>
                      </>
                    ) : (
                      <>
                        <Download className="w-3.5 h-3.5" />
                        <span>Download (Phase 3)</span>
                      </>
                    )}
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};
