import React from 'react';
import { Sparkles, RotateCcw, AlertTriangle, Bot } from 'lucide-react';
import { PromptSource } from '../../lib/ipc';

interface FlowPromptEditorProps {
  prompt: string;
  promptSource: PromptSource;
  isOptimizing: boolean;
  canUndo: boolean;
  optimizationError: string | null;
  geminiConfigured?: boolean;
  disabled?: boolean;
  onPromptChange: (text: string) => void;
  onGenPrompt: () => void;
  onUndo: () => void;
}

export const FlowPromptEditor: React.FC<FlowPromptEditorProps> = ({
  prompt,
  promptSource,
  isOptimizing,
  canUndo,
  optimizationError,
  geminiConfigured = false,
  disabled = false,
  onPromptChange,
  onGenPrompt,
  onUndo,
}) => {
  const getBadgeStyle = () => {
    switch (promptSource) {
      case 'GEMINI_OPTIMIZED':
        return 'bg-emerald-950/80 border-emerald-500/30 text-emerald-400';
      case 'GEMINI_OPTIMIZED_THEN_EDITED':
        return 'bg-amber-950/80 border-amber-500/30 text-amber-400';
      case 'USER':
      default:
        return 'bg-slate-800/80 border-slate-700 text-slate-300';
    }
  };

  const getBadgeLabel = () => {
    switch (promptSource) {
      case 'GEMINI_OPTIMIZED':
        return 'Gemini Optimized';
      case 'GEMINI_OPTIMIZED_THEN_EDITED':
        return 'Optimized & Edited';
      case 'USER':
      default:
        return 'Direct User Prompt';
    }
  };

  return (
    <div className="flex flex-col gap-2 p-4 bg-slate-900/60 border border-slate-800 rounded-xl">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <label className="text-sm font-semibold text-slate-200">Google Flow Prompt</label>
          <span
            className={`text-xs px-2.5 py-0.5 rounded-full border flex items-center gap-1 font-medium ${getBadgeStyle()}`}
          >
            {promptSource === 'GEMINI_OPTIMIZED' && <Sparkles className="w-3 h-3" />}
            {getBadgeLabel()}
          </span>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onUndo}
            disabled={disabled || !canUndo || isOptimizing}
            title="Undo prompt change"
            className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium text-slate-400 hover:text-slate-200 bg-slate-800 hover:bg-slate-700 disabled:opacity-40 disabled:cursor-not-allowed rounded-lg transition"
          >
            <RotateCcw className="w-3 h-3" />
            Undo
          </button>

          <button
            type="button"
            onClick={onGenPrompt}
            disabled={disabled || isOptimizing || !prompt.trim()}
            title={geminiConfigured ? 'Optimize prompt with Gemini' : 'Optional Gemini API Key configured via Settings'}
            className="flex items-center gap-1.5 px-3 py-1 text-xs font-semibold text-white bg-gradient-to-r from-violet-600 to-indigo-600 hover:from-violet-500 hover:to-indigo-500 disabled:opacity-40 disabled:cursor-not-allowed rounded-lg shadow-sm transition"
          >
            {isOptimizing ? (
              <div className="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin" />
            ) : (
              <Sparkles className="w-3.5 h-3.5" />
            )}
            Gen Prompt
          </button>
        </div>
      </div>

      <textarea
        value={prompt}
        onChange={(e) => onPromptChange(e.target.value)}
        disabled={disabled}
        placeholder="Enter transformation prompt for Google Flow..."
        rows={4}
        className="w-full px-3.5 py-2.5 text-sm text-slate-100 bg-slate-950/80 border border-slate-700 rounded-lg focus:outline-none focus:border-indigo-500 disabled:opacity-60 transition resize-none font-sans"
      />

      {optimizationError && (
        <div className="flex items-center gap-2 px-3 py-2 text-xs text-rose-300 bg-rose-950/40 border border-rose-800/40 rounded-lg">
          <AlertTriangle className="w-4 h-4 text-rose-400 shrink-0" />
          <span>Prompt optimization error: {optimizationError}. Existing prompt was preserved.</span>
        </div>
      )}

      <div className="flex items-center justify-between text-[11px] text-slate-400">
        <span>Prompt is sent verbatim to Google Flow unless explicitly optimized.</span>
        <span className="flex items-center gap-1">
          <Bot className="w-3 h-3 text-slate-400" />
          Gemini: {geminiConfigured ? 'Configured' : 'Not Configured (Optional)'}
        </span>
      </div>
    </div>
  );
};
