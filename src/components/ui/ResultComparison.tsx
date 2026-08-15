import React, { useState } from 'react';
import { Columns, SplitSquareVertical, Play, Volume2, Maximize2 } from 'lucide-react';
import { MockBadge } from '../common/MockBadge';

interface ResultComparisonProps {
  originalEmoji?: string;
  generatedEmoji?: string;
  originalLabel?: string;
  generatedLabel?: string;
  className?: string;
}

export const ResultComparison: React.FC<ResultComparisonProps> = ({
  originalEmoji = '🦊',
  generatedEmoji = '🐰',
  originalLabel = 'Original (Fox)',
  generatedLabel = 'Transformed AI Output (Rabbit)',
  className = '',
}) => {
  const [mode, setMode] = useState<'split' | 'side-by-side' | 'toggle'>('split');
  const [splitPos] = useState(50);
  const [toggleState, setToggleState] = useState<'before' | 'after'>('after');

  return (
    <div className={`space-y-4 ${className}`}>
      {/* Comparison Mode Selectors */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-1.5 p-1 rounded-xl bg-slate-900 border border-slate-800">
          <button
            onClick={() => setMode('split')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all ${
              mode === 'split' ? 'bg-indigo-600 text-white' : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <SplitSquareVertical className="w-3.5 h-3.5" />
            <span>Interactive Split</span>
          </button>

          <button
            onClick={() => setMode('side-by-side')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all ${
              mode === 'side-by-side' ? 'bg-indigo-600 text-white' : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <Columns className="w-3.5 h-3.5" />
            <span>Side-by-Side</span>
          </button>

          <button
            onClick={() => setMode('toggle')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all ${
              mode === 'toggle' ? 'bg-indigo-600 text-white' : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <span>Before/After Toggle</span>
          </button>
        </div>

        <MockBadge label="COMPARISON PREVIEW FIXTURE" />
      </div>

      {/* Mode 1: Interactive Split */}
      {mode === 'split' && (
        <div className="relative rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden shadow-2xl aspect-video select-none group">
          {/* Left Layer: Original */}
          <div className="absolute inset-0 bg-gradient-to-br from-amber-950 via-slate-900 to-slate-950 flex flex-col items-center justify-center p-6">
            <div className="absolute top-3 left-3 px-2 py-0.5 rounded text-[10px] font-bold bg-slate-950/80 text-slate-300 border border-slate-700">
              {originalLabel}
            </div>
            <span className="text-7xl">{originalEmoji}</span>
            <p className="text-xs font-semibold text-amber-200 mt-2">Original Frame</p>
          </div>

          {/* Right Layer: Transformed (Clipped) */}
          <div
            className="absolute inset-0 bg-gradient-to-br from-amber-900 via-orange-950 to-slate-950 flex flex-col items-center justify-center p-6 border-l border-indigo-500/50"
            style={{ clipPath: `polygon(${splitPos}% 0, 100% 0, 100% 100%, ${splitPos}% 100%)` }}
          >
            <div className="absolute top-3 right-3 px-2 py-0.5 rounded text-[10px] font-bold bg-purple-950/80 text-purple-200 border border-purple-700">
              {generatedLabel}
            </div>
            <span className="text-7xl">{generatedEmoji}</span>
            <p className="text-xs font-semibold text-amber-100 mt-2">Transformed Video</p>
          </div>

          {/* Draggable Divider Handle */}
          <div
            className="absolute top-0 bottom-0 w-1 bg-indigo-500 cursor-ew-resize z-20 flex items-center justify-center"
            style={{ left: `${splitPos}%` }}
          >
            <div className="w-7 h-7 rounded-full bg-indigo-600 border-2 border-white text-white flex items-center justify-center text-[10px] font-bold shadow-lg shadow-indigo-900/80">
              ⚡
            </div>
          </div>

          {/* Transport Bar */}
          <div className="absolute bottom-0 left-0 right-0 p-3 bg-slate-950/85 backdrop-blur-sm border-t border-slate-800 flex items-center justify-between text-xs text-slate-300 z-10">
            <div className="flex items-center gap-3">
              <button className="p-1 text-slate-300 hover:text-white">
                <Play className="w-4 h-4 fill-current" />
              </button>
              <span className="font-mono text-[11px]">00:14 / 01:02</span>
            </div>
            <div className="flex items-center gap-2">
              <button className="p-1 text-slate-400 hover:text-white">
                <Volume2 className="w-4 h-4" />
              </button>
              <button className="p-1 text-slate-400 hover:text-white">
                <Maximize2 className="w-4 h-4" />
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Mode 2: Side-by-Side */}
      {mode === 'side-by-side' && (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="relative rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden aspect-video flex flex-col items-center justify-center p-4">
            <div className="absolute top-3 left-3 px-2 py-0.5 rounded text-[10px] font-bold bg-slate-950/80 text-slate-300 border border-slate-700">
              {originalLabel}
            </div>
            <span className="text-5xl">{originalEmoji}</span>
            <p className="text-xs text-slate-400 mt-2">Original Sequence</p>
          </div>

          <div className="relative rounded-2xl border border-purple-500/40 bg-slate-900 overflow-hidden aspect-video flex flex-col items-center justify-center p-4">
            <div className="absolute top-3 right-3 px-2 py-0.5 rounded text-[10px] font-bold bg-purple-950/80 text-purple-200 border border-purple-700">
              {generatedLabel}
            </div>
            <span className="text-5xl">{generatedEmoji}</span>
            <p className="text-xs text-purple-200 mt-2">AI Transformed Output</p>
          </div>
        </div>
      )}

      {/* Mode 3: Before/After Toggle */}
      {mode === 'toggle' && (
        <div className="relative rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden aspect-video flex flex-col items-center justify-center p-6">
          <div className="absolute top-3 right-3 z-10 flex items-center gap-2">
            <button
              onClick={() => setToggleState(toggleState === 'before' ? 'after' : 'before')}
              className="px-3 py-1 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold shadow-md transition-all"
            >
              Viewing: {toggleState === 'before' ? 'Original (Before)' : 'AI Output (After)'} (Click to toggle)
            </button>
          </div>

          <span className="text-7xl">{toggleState === 'before' ? originalEmoji : generatedEmoji}</span>
          <p className="text-sm font-semibold text-slate-200 mt-2">
            {toggleState === 'before' ? originalLabel : generatedLabel}
          </p>
        </div>
      )}
    </div>
  );
};
