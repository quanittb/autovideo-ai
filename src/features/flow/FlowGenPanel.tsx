import React, { useEffect, useState } from 'react';
import { Film, Play, Sparkles, AlertCircle, Info, RefreshCw } from 'lucide-react';
import { useFlowJobStore } from '../../stores/flowJobStore';
import { usePromptOptimization } from './usePromptOptimization';
import { FlowPromptEditor } from './FlowPromptEditor';
import { FlowProfileSelector } from './FlowProfileSelector';
import { FlowJobProgress } from './FlowJobProgress';

export const FlowGenPanel: React.FC = () => {
  const {
    profiles,
    selectedProfileId,
    geminiStatus,
    activeJob,
    isStarting,
    isLoadingProfiles,
    error: storeError,
    loadProfiles,
    createProfile,
    selectProfile,
    loadGeminiStatus,
    startFlowJob,
    pollJobStatus,
  } = useFlowJobStore();

  const [sourceVideoPath, setSourceVideoPath] = useState<string>('');
  const [projectId] = useState<string>('default_project');

  const {
    prompt,
    promptSource,
    isOptimizing,
    optimizationError,
    canUndo,
    handlePromptChange,
    handleGenPrompt,
    handleUndo,
  } = usePromptOptimization({
    initialPrompt: 'Transform character into a neon-lit cyber guardian while preserving realistic background motion',
    taskType: 'FLOW_VIDEO_EDIT',
    videoDurationSec: 10.0,
  });

  useEffect(() => {
    loadProfiles();
    loadGeminiStatus();
  }, [loadProfiles, loadGeminiStatus]);

  // Polling active job
  useEffect(() => {
    if (!activeJob) return;
    if (
      activeJob.state === 'COMPLETED' ||
      activeJob.state === 'FAILED' ||
      activeJob.state === 'CANCELLED' ||
      activeJob.state === 'BLOCKED'
    ) {
      return;
    }

    const timer = setInterval(() => {
      pollJobStatus(projectId, activeJob.parentId);
    }, 2000);

    return () => clearInterval(timer);
  }, [activeJob, projectId, pollJobStatus]);

  const handleStartGeneration = async () => {
    if (!selectedProfileId || !prompt.trim() || !sourceVideoPath.trim()) return;

    await startFlowJob(
      projectId,
      selectedProfileId,
      prompt.trim(),
      promptSource,
      sourceVideoPath.trim()
    );
  };

  return (
    <div className="flex flex-col gap-6 max-w-5xl mx-auto p-6 text-slate-100">
      <div className="flex items-center justify-between border-b border-slate-800 pb-4">
        <div className="flex items-center gap-3">
          <div className="p-2.5 bg-gradient-to-br from-indigo-500/20 to-purple-500/20 border border-indigo-500/30 rounded-xl">
            <Sparkles className="w-6 h-6 text-indigo-400" />
          </div>
          <div>
            <h1 className="text-xl font-bold text-slate-100">Google Flow Gen (Phase 20A)</h1>
            <p className="text-xs text-slate-400">
              Browser-driven generative video transformation with optional Gemini prompt refinement
            </p>
          </div>
        </div>

        <button
          type="button"
          onClick={() => {
            loadProfiles();
            loadGeminiStatus();
          }}
          className="flex items-center gap-1.5 px-3 py-1.5 text-xs text-slate-400 hover:text-slate-200 bg-slate-900 border border-slate-800 rounded-lg hover:border-slate-700 transition"
        >
          <RefreshCw className="w-3.5 h-3.5" />
          Refresh
        </button>
      </div>

      {storeError && (
        <div className="flex items-center gap-2 p-3 bg-rose-950/40 border border-rose-800/50 rounded-lg text-xs text-rose-300">
          <AlertCircle className="w-4 h-4 text-rose-400 shrink-0" />
          <span>{storeError}</span>
        </div>
      )}

      {/* Grid configuration */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2 flex flex-col gap-4">
          {/* Source video selection */}
          <div className="flex flex-col gap-2 p-4 bg-slate-900/60 border border-slate-800 rounded-xl">
            <div className="flex items-center gap-2">
              <Film className="w-4 h-4 text-indigo-400" />
              <label className="text-sm font-semibold text-slate-200">Source Video Path</label>
            </div>
            <input
              type="text"
              placeholder="e.g. C:\Users\quant\Dropbox\PC\Downloads\video_test.mp4"
              value={sourceVideoPath}
              onChange={(e) => setSourceVideoPath(e.target.value)}
              className="w-full px-3 py-2 text-sm text-slate-100 bg-slate-950/70 border border-slate-700 rounded-lg focus:outline-none focus:border-indigo-500 font-mono"
            />
            <span className="text-[11px] text-slate-500">
              Source video audio is preserved and muxed once into final output during stitch.
            </span>
          </div>

          {/* Prompt Editor */}
          <FlowPromptEditor
            prompt={prompt}
            promptSource={promptSource}
            isOptimizing={isOptimizing}
            canUndo={canUndo}
            optimizationError={optimizationError}
            geminiConfigured={geminiStatus?.isConfigured}
            onPromptChange={handlePromptChange}
            onGenPrompt={handleGenPrompt}
            onUndo={handleUndo}
          />

          {/* Generation Action */}
          <button
            type="button"
            onClick={handleStartGeneration}
            disabled={
              isStarting ||
              !selectedProfileId ||
              !prompt.trim() ||
              !sourceVideoPath.trim()
            }
            className="flex items-center justify-center gap-2 py-3 px-6 text-sm font-semibold text-white bg-gradient-to-r from-indigo-600 to-violet-600 hover:from-indigo-500 hover:to-violet-500 disabled:opacity-40 disabled:cursor-not-allowed rounded-xl shadow-lg shadow-indigo-950/50 transition"
          >
            {isStarting ? (
              <div className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
            ) : (
              <Play className="w-4 h-4 fill-current" />
            )}
            Start Google Flow Generation
          </button>
        </div>

        {/* Right Sidebar: Profile & Capabilities */}
        <div className="flex flex-col gap-4">
          <FlowProfileSelector
            profiles={profiles}
            selectedProfileId={selectedProfileId}
            isLoading={isLoadingProfiles}
            onSelectProfile={selectProfile}
            onCreateProfile={createProfile}
          />

          <div className="flex flex-col gap-2 p-4 bg-slate-900/60 border border-slate-800 rounded-xl text-xs">
            <div className="flex items-center gap-1.5 font-semibold text-slate-200">
              <Info className="w-4 h-4 text-indigo-400" />
              Capability & Policy (v1)
            </div>
            <ul className="flex flex-col gap-1 text-slate-400">
              <li>• Max Segment Duration: 10.0s</li>
              <li>• Estimated Credits / Seg: 40 Flow Credits (Omni Edit)</li>
              <li>• Output Format: MP4 (H.264 + AAC)</li>
              <li>• Crash Guard: Pre-click persistence active</li>
            </ul>
          </div>
        </div>
      </div>

      {/* Active Job Progress Display */}
      {activeJob && (
        <div className="mt-2">
          <FlowJobProgress job={activeJob} />
        </div>
      )}
    </div>
  );
};
