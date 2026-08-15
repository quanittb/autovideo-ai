import React, { useState } from 'react';
import { RefreshCw, ArrowRight, Play, Volume2, Maximize2 } from 'lucide-react';
import { useAppStore } from '../../store/useAppStore';
import { MockBadge } from '../common/MockBadge';

export const StepTransform: React.FC = () => {
  const { activeProject, updateTransformationConfig } = useAppStore();
  const [splitPos] = useState(50); // percentage split for before/after player
  const [activeSubTab, setActiveSubTab] = useState<'scene' | 'character' | 'style' | 'advanced'>('character');

  const transformation = activeProject?.transformation || {
    category: 'character',
    originalCharacter: 'Fox',
    replacementCharacter: 'Rabbit',
    prompt: 'A cute white rabbit wearing a scarf',
    resolution: '1080p (1920x1080)',
    quality: 'High Quality',
    format: 'MP4',
    fps: 30,
    removeWatermark: true,
  };

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      {/* Header */}
      <div>
        <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Step 2: Transform Your Video</h2>
        <p className="text-sm text-slate-400 mt-1">Describe the changes you want to make</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
        {/* Left Control Panel (5 cols) */}
        <div className="lg:col-span-5 space-y-6">
          {/* Category Tabs */}
          <div className="flex items-center gap-1.5 p-1 rounded-xl bg-slate-900 border border-slate-800">
            {(['scene', 'character', 'style', 'advanced'] as const).map((tab) => (
              <button
                key={tab}
                onClick={() => {
                  setActiveSubTab(tab);
                  updateTransformationConfig({ category: tab });
                }}
                className={`flex-1 py-2 text-xs font-semibold rounded-lg capitalize transition-all ${
                  activeSubTab === tab
                    ? 'bg-indigo-600 text-white shadow-md shadow-indigo-900/30'
                    : 'text-slate-400 hover:text-slate-200'
                }`}
              >
                {tab}
              </button>
            ))}
          </div>

          {/* Character Replacement Card */}
          <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-5">
            <div>
              <h4 className="text-sm font-semibold text-slate-100">Character Replacement</h4>
              <p className="text-xs text-slate-400 mt-0.5">Replace characters in your video with new ones</p>
            </div>

            {/* Character Comparison Cards */}
            <div className="grid grid-cols-2 gap-3 items-center">
              {/* Original Character */}
              <div className="space-y-1.5">
                <span className="text-[11px] font-medium text-slate-400 block">Original Character</span>
                <div className="h-28 rounded-xl bg-slate-950 border border-slate-800 overflow-hidden flex flex-col items-center justify-center p-3">
                  <span className="text-3xl">🦊</span>
                  <span className="text-xs font-semibold text-amber-200 mt-1">Fox</span>
                </div>
              </div>

              {/* Arrow Indicator */}
              <div className="absolute left-1/2 -translate-x-1/2 hidden">
                <ArrowRight className="w-4 h-4 text-slate-500" />
              </div>

              {/* New Character */}
              <div className="space-y-1.5">
                <span className="text-[11px] font-medium text-slate-400 block">New Character</span>
                <div className="h-28 rounded-xl bg-gradient-to-br from-purple-950 to-slate-950 border border-purple-500/40 overflow-hidden flex flex-col items-center justify-center p-3">
                  <span className="text-3xl">🐰</span>
                  <span className="text-xs font-semibold text-purple-200 mt-1">Rabbit</span>
                </div>
              </div>
            </div>

            <button className="w-full py-2.5 px-4 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold shadow-md shadow-indigo-900/30 transition-all flex items-center justify-center gap-2">
              <RefreshCw className="w-3.5 h-3.5" />
              <span>Change Character</span>
            </button>

            {/* Character Description Prompt Textarea */}
            <div className="space-y-2 pt-2">
              <label className="text-xs font-semibold text-slate-300 block">
                Character Description (Optional)
              </label>
              <textarea
                value={transformation.prompt}
                onChange={(e) => updateTransformationConfig({ prompt: e.target.value })}
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

        {/* Right Preview Side-by-Side Player (7 cols) */}
        <div className="lg:col-span-7 space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-slate-200">Preview Transformation</h3>
            <MockBadge label="BEFORE / AFTER SPLIT PREVIEW" />
          </div>

          {/* Interactive Split Before/After Player */}
          <div className="relative rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden shadow-2xl aspect-video select-none group">
            {/* Original Left Layer */}
            <div className="absolute inset-0 bg-gradient-to-br from-amber-950 via-slate-900 to-slate-950 flex flex-col items-center justify-center p-6">
              <div className="absolute top-3 left-3 px-2 py-0.5 rounded text-[10px] font-bold bg-slate-950/80 text-slate-300 border border-slate-700">
                Original
              </div>
              <span className="text-6xl">🦊</span>
              <p className="text-xs font-semibold text-amber-200 mt-2">Fox in Winter</p>
            </div>

            {/* Transformed Right Layer (Clipped by splitPos) */}
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

            {/* Split Slider Handle */}
            <div
              className="absolute top-0 bottom-0 w-1 bg-indigo-500 cursor-ew-resize z-20 flex items-center justify-center"
              style={{ left: `${splitPos}%` }}
            >
              <div className="w-7 h-7 rounded-full bg-indigo-600 border-2 border-white text-white flex items-center justify-center text-[10px] font-bold shadow-lg shadow-indigo-900/80">
                ⚡
              </div>
            </div>

            {/* Transport Bar */}
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
