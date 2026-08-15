import React, { useState } from 'react';
import { RefreshCw, Play, Volume2, Maximize2 } from 'lucide-react';
import { useProjectStore } from '../../stores/projectStore';
import { MockBadge } from '../../components/common/MockBadge';

export const StepTransform: React.FC = () => {
  const { activeProject, updateTransformationRequest } = useProjectStore();
  const [splitPos] = useState(50);
  const [activeSubTab, setActiveSubTab] = useState<'character' | 'background' | 'environment' | 'style' | 'object' | 'custom'>('character');

  const transformation = activeProject?.transformationRequest || {
    category: 'character',
    originalCharacter: 'Fox',
    replacementCharacter: 'Rabbit',
    prompt: 'A cute white rabbit wearing a scarf',
  };

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      <div>
        <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Step 2: Transform Your Video</h2>
        <p className="text-sm text-slate-400 mt-1">Configure your AI transformation pipeline</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
        {/* Left: Transform Controls */}
        <div className="lg:col-span-5 space-y-6">
          <div className="flex items-center gap-1.5 p-1 rounded-xl bg-slate-900 border border-slate-800">
            {(['character', 'background', 'environment', 'style'] as const).map((tab) => (
              <button
                key={tab}
                onClick={() => {
                  setActiveSubTab(tab);
                  updateTransformationRequest({ category: tab });
                }}
                className={`flex-1 py-2 text-xs font-semibold rounded-lg capitalize transition-all ${
                  activeSubTab === tab
                    ? 'bg-indigo-600 text-white shadow-md shadow-indigo-900/30'
                    : 'text-slate-400 hover:text-slate-200'
                }`}
              >
                {tab} {tab === 'character' && '(MVP)'}
              </button>
            ))}
          </div>

          <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-5">
            <div>
              <h4 className="text-sm font-semibold text-slate-100">Character Replacement</h4>
              <p className="text-xs text-slate-400 mt-0.5">Replace characters in your video with new ones</p>
            </div>

            <div className="grid grid-cols-2 gap-3 items-center">
              <div className="space-y-1.5">
                <span className="text-[11px] font-medium text-slate-400 block">Original Character</span>
                <div className="h-28 rounded-xl bg-slate-950 border border-slate-800 overflow-hidden flex flex-col items-center justify-center p-3">
                  <span className="text-3xl">🦊</span>
                  <span className="text-xs font-semibold text-amber-200 mt-1">{transformation.originalCharacter || 'Fox'}</span>
                </div>
              </div>

              <div className="space-y-1.5">
                <span className="text-[11px] font-medium text-slate-400 block">New Character</span>
                <div className="h-28 rounded-xl bg-gradient-to-br from-purple-950 to-slate-950 border border-purple-500/40 overflow-hidden flex flex-col items-center justify-center p-3">
                  <span className="text-3xl">🐰</span>
                  <span className="text-xs font-semibold text-purple-200 mt-1">{transformation.replacementCharacter || 'Rabbit'}</span>
                </div>
              </div>
            </div>

            <button className="w-full py-2.5 px-4 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold shadow-md shadow-indigo-900/30 transition-all flex items-center justify-center gap-2">
              <RefreshCw className="w-3.5 h-3.5" />
              <span>Change Target Character</span>
            </button>

            <div className="space-y-2 pt-2">
              <label className="text-xs font-semibold text-slate-300 block">
                Character Description & Prompt
              </label>
              <textarea
                value={transformation.prompt}
                onChange={(e) => updateTransformationRequest({ prompt: e.target.value })}
                rows={3}
                placeholder="A cute white rabbit wearing a scarf..."
                className="w-full p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-indigo-500 transition-colors resize-none"
              />
              <div className="flex justify-end">
                <span className="text-[10px] text-slate-500 font-mono">
                  {transformation.prompt.length}/500
                </span>
              </div>
            </div>
          </div>
        </div>

        {/* Right: Side-by-Side Comparison */}
        <div className="lg:col-span-7 space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-slate-200">Preview Transformation</h3>
            <MockBadge label="BEFORE / AFTER SPLIT PREVIEW" />
          </div>

          <div className="relative rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden shadow-2xl aspect-video select-none group">
            <div className="absolute inset-0 bg-gradient-to-br from-amber-950 via-slate-900 to-slate-950 flex flex-col items-center justify-center p-6">
              <div className="absolute top-3 left-3 px-2 py-0.5 rounded text-[10px] font-bold bg-slate-950/80 text-slate-300 border border-slate-700">
                Original
              </div>
              <span className="text-6xl">🦊</span>
              <p className="text-xs font-semibold text-amber-200 mt-2">Fox in Winter</p>
            </div>

            <div
              className="absolute inset-0 bg-gradient-to-br from-amber-900 via-orange-950 to-slate-950 flex flex-col items-center justify-center p-6 border-l border-indigo-500/50"
              style={{ clipPath: `polygon(${splitPos}% 0, 100% 0, 100% 100%, ${splitPos}% 100%)` }}
            >
              <div className="absolute top-3 right-3 px-2 py-0.5 rounded text-[10px] font-bold bg-purple-950/80 text-purple-200 border border-purple-700">
                Preview
              </div>
              <span className="text-6xl">🐰</span>
              <p className="text-xs font-semibold text-amber-100 mt-2">Rabbit in Autumn</p>
            </div>

            <div
              className="absolute top-0 bottom-0 w-1 bg-indigo-500 cursor-ew-resize z-20 flex items-center justify-center"
              style={{ left: `${splitPos}%` }}
            >
              <div className="w-7 h-7 rounded-full bg-indigo-600 border-2 border-white text-white flex items-center justify-center text-[10px] font-bold shadow-lg shadow-indigo-900/80">
                ⚡
              </div>
            </div>

            <div className="absolute bottom-0 left-0 right-0 p-3 bg-slate-950/80 backdrop-blur-sm border-t border-slate-800 flex items-center justify-between text-xs text-slate-300 z-10">
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

          <p className="text-[11px] text-slate-500 text-center italic">
            ⓘ This is a preview. The final result may vary depending on local AI model inference.
          </p>
        </div>
      </div>
    </div>
  );
};
