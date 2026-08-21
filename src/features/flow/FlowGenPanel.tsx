import React, { useEffect, useState } from 'react';
import { Play, Sparkles, AlertCircle, Info, RefreshCw, Folder } from 'lucide-react';
import { useFlowJobStore } from '../../stores/flowJobStore';
import { useProjectStore } from '../../stores/projectStore';
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

  const { activeProject } = useProjectStore();

  const projectId = activeProject?.id || '';
  const [selectedMediaId, setSelectedMediaId] = useState<string>('');

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

  useEffect(() => {
    if (activeProject?.sourceAsset?.id) {
      setSelectedMediaId(activeProject.sourceAsset.id);
    } else if (activeProject?.sourceMedia?.sourcePath) {
      setSelectedMediaId(activeProject.sourceMedia.sourcePath);
    }
  }, [activeProject]);

  // Polling active job
  useEffect(() => {
    if (!activeJob || !projectId) return;
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
    if (!projectId || !selectedProfileId || !prompt.trim() || !selectedMediaId.trim()) return;

    await startFlowJob(
      projectId,
      selectedProfileId,
      prompt.trim(),
      promptSource,
      selectedMediaId.trim()
    );
  };

  const selectedProfile = profiles.find((p) => p.profileId === selectedProfileId);
  const isProfileLocked = selectedProfile?.isLocked ?? false;

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
          className="flex items-center gap-1.5 px-3 py-1.5 text-xs text-slate-400 hover:text-slate-200 bg-slate-900 border border-slate-800 rounded-lg hover:border-slate-700 transition cursor-pointer"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${isLoadingProfiles ? 'animate-spin' : ''}`} />
          Refresh Status
        </button>
      </div>

      {storeError && (
        <div className="flex items-center gap-2 p-3 text-xs bg-red-500/10 border border-red-500/20 text-red-300 rounded-xl">
          <AlertCircle className="w-4 h-4 shrink-0" />
          <span>{storeError}</span>
        </div>
      )}

      {/* Profile Selector */}
      <FlowProfileSelector
        profiles={profiles}
        selectedProfileId={selectedProfileId}
        isLoading={isLoadingProfiles}
        onSelectProfile={selectProfile}
        onCreateProfile={createProfile}
        onRefreshProfiles={loadProfiles}
      />

      {/* Project Source Media Confinement Section */}
      <div className="flex flex-col gap-2 p-4 bg-slate-900/60 border border-slate-800 rounded-xl">
        <div className="flex items-center gap-2">
          <Folder className="w-4 h-4 text-indigo-400" />
          <span className="text-sm font-semibold text-slate-200">Project Source Media</span>
        </div>

        {!activeProject ? (
          <div className="p-3 bg-amber-500/10 border border-amber-500/20 text-amber-300 text-xs rounded-lg">
            No active project loaded. Please open or create a project to select source video.
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between text-xs text-slate-400">
              <span>Active Project: <strong className="text-slate-200">{activeProject.name}</strong> ({activeProject.id})</span>
              <span>Source Media: <strong className="text-slate-200">{selectedMediaId || 'No media imported'}</strong></span>
            </div>
            {!selectedMediaId && (
              <span className="text-xs text-amber-400">
                Please import a video into this project before starting Google Flow generation.
              </span>
            )}
          </div>
        )}
      </div>

      {/* Prompt Editor */}
      <FlowPromptEditor
        prompt={prompt}
        promptSource={promptSource}
        isOptimizing={isOptimizing}
        geminiConfigured={geminiStatus?.isConfigured ?? false}
        optimizationError={optimizationError}
        canUndo={canUndo}
        disabled={isStarting || (activeJob !== null && activeJob.state !== 'COMPLETED' && activeJob.state !== 'FAILED')}
        onPromptChange={handlePromptChange}
        onGenPrompt={handleGenPrompt}
        onUndo={handleUndo}
      />

      {/* Execution Guard & Action */}
      <div className="flex items-center justify-between p-4 bg-slate-900/40 border border-slate-800 rounded-xl">
        <div className="flex items-center gap-2 text-xs text-slate-400">
          <Info className="w-4 h-4 text-indigo-400 shrink-0" />
          <span>
            Contract: <strong>OmniEditUploadedVideo (40 credits / segment generation)</strong>. Sequential execution with zero automatic retries.
          </span>
        </div>

        <button
          type="button"
          onClick={handleStartGeneration}
          disabled={
            !projectId ||
            !selectedProfileId ||
            isProfileLocked ||
            !prompt.trim() ||
            !selectedMediaId ||
            isStarting ||
            (activeJob !== null && activeJob.state !== 'COMPLETED' && activeJob.state !== 'FAILED')
          }
          className="flex items-center gap-2 px-5 py-2.5 text-sm font-semibold text-white bg-gradient-to-r from-indigo-600 to-purple-600 hover:from-indigo-500 hover:to-purple-500 disabled:opacity-40 disabled:cursor-not-allowed rounded-xl shadow-lg transition cursor-pointer"
        >
          <Play className="w-4 h-4 fill-white" />
          {isStarting ? 'Initiating Pipeline...' : 'Start Google Flow Generation'}
        </button>
      </div>

      {/* Active Job Progress */}
      {activeJob && <FlowJobProgress job={activeJob} />}
    </div>
  );
};
