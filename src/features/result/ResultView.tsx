import React from 'react';
import { Play, Volume2, Maximize2, Download, RefreshCw } from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { MockBadge } from '../../components/common/MockBadge';

export const ResultView: React.FC = () => {
  const { setCurrentStep } = useUiStore();

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Transformation Result</h2>
          <p className="text-sm text-slate-400 mt-1">Review your AI-transformed video output</p>
        </div>
        <MockBadge label="OUTPUT VERIFICATION FIXTURE" />
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
        <div className="lg:col-span-8 space-y-4">
          <div className="relative rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden shadow-2xl aspect-video flex flex-col items-center justify-center">
            <div className="w-full h-full bg-gradient-to-br from-amber-900 via-orange-950 to-slate-950 flex flex-col items-center justify-center p-6">
              <span className="text-6xl">🐰</span>
              <p className="text-sm font-bold text-amber-100 mt-2">Transformed Video (Fox → Rabbit)</p>
            </div>

            <div className="absolute bottom-0 left-0 right-0 p-3 bg-slate-950/80 backdrop-blur-sm border-t border-slate-800 flex items-center justify-between text-xs text-slate-300">
              <div className="flex items-center gap-3">
                <button className="p-1 text-slate-300 hover:text-white">
                  <Play className="w-4 h-4 fill-current" />
                </button>
                <span className="font-mono text-[11px]">00:00 / 01:02</span>
              </div>
              <div className="flex items-center gap-3">
                <button className="p-1 text-slate-400 hover:text-white">
                  <Volume2 className="w-4 h-4" />
                </button>
                <button className="p-1 text-slate-400 hover:text-white">
                  <Maximize2 className="w-4 h-4" />
                </button>
              </div>
            </div>
          </div>
        </div>

        <div className="lg:col-span-4 space-y-4">
          <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-4">
            <h4 className="text-sm font-semibold text-slate-100">Actions</h4>
            <button
              onClick={() => setCurrentStep('export')}
              className="w-full py-3 px-4 rounded-xl bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white text-xs font-semibold shadow-md shadow-purple-900/30 transition-all flex items-center justify-center gap-2"
            >
              <Download className="w-4 h-4" />
              <span>Proceed to Export</span>
            </button>
            <button
              onClick={() => setCurrentStep('transform')}
              className="w-full py-2.5 px-4 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold transition-all flex items-center justify-center gap-2"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              <span>Modify Transformation Prompt</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
