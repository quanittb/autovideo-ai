import React from 'react';
import { Download, RefreshCw, ArrowRight } from 'lucide-react';
import { ResultComparison } from '../../components/ui/ResultComparison';
import { QualityReport } from '../../components/ui/QualityReport';
import { SceneStrip } from '../../components/ui/SceneStrip';
import { useUiStore } from '../../stores/uiStore';
import { useProjectStore } from '../../stores/projectStore';

export const ResultView: React.FC = () => {
  const { setCurrentStep } = useUiStore();
  const { activeProject } = useProjectStore();

  const scenes = activeProject?.scenes || [
    {
      id: 'scene-1',
      index: 1,
      name: 'Woodland Overview',
      startTimeFormatted: '00:00',
      endTimeFormatted: '00:24',
      startFrame: 0,
      endFrame: 720,
      thumbnailEmoji: '🌲',
      status: 'completed',
    },
    {
      id: 'scene-2',
      index: 2,
      name: 'Fox Subject Close-up',
      startTimeFormatted: '00:24',
      endTimeFormatted: '00:48',
      startFrame: 720,
      endFrame: 1440,
      thumbnailEmoji: '🐰',
      status: 'completed',
    },
    {
      id: 'scene-3',
      index: 3,
      name: 'Snow Clearing Run',
      startTimeFormatted: '00:48',
      endTimeFormatted: '01:02',
      startFrame: 1440,
      endFrame: 1860,
      thumbnailEmoji: '🍂',
      status: 'completed',
    },
  ];

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      {/* Title & Primary CTA */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Transformation Results & Inspection</h2>
          <p className="text-sm text-slate-400 mt-1">Review side-by-side Before/After output and quality verification</p>
        </div>

        <button
          onClick={() => setCurrentStep('export')}
          className="px-6 py-2.5 rounded-xl bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white text-sm font-bold shadow-lg shadow-purple-900/40 transition-all flex items-center gap-2"
        >
          <Download className="w-4 h-4" />
          <span>Proceed to Export</span>
          <ArrowRight className="w-4 h-4" />
        </button>
      </div>

      {/* Main Comparison Component */}
      <ResultComparison
        originalEmoji="🦊"
        generatedEmoji="🐰"
        originalLabel="Original Source (Fox)"
        generatedLabel="Transformed AI Subject (Rabbit)"
      />

      {/* Scene Navigation Strip */}
      <SceneStrip
        scenes={scenes}
        selectedSceneId="scene-2"
        onSelectScene={() => {}}
      />

      {/* Quality Report and Actions Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-start">
        <div className="lg:col-span-8">
          <QualityReport />
        </div>

        <div className="lg:col-span-4 p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3">
          <h4 className="text-xs font-semibold text-slate-300">Quick Iteration</h4>
          <p className="text-xs text-slate-500 leading-relaxed">
            Need to refine the prompt, modify target character traits, or change preservation rules?
          </p>
          <button
            onClick={() => setCurrentStep('transform')}
            className="w-full py-2.5 px-3 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold flex items-center justify-center gap-1.5 transition-colors"
          >
            <RefreshCw className="w-3.5 h-3.5" />
            <span>Refine Prompt & Re-generate</span>
          </button>
        </div>
      </div>
    </div>
  );
};
