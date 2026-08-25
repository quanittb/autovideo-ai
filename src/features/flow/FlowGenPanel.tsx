import React, { useEffect, useState } from 'react';
import { Play, Sparkles, AlertCircle, Info, RefreshCw, Folder, LogIn, ExternalLink } from 'lucide-react';
import { useFlowJobStore } from '../../stores/flowJobStore';
import { useProjectStore } from '../../stores/projectStore';
import { usePromptOptimization } from './usePromptOptimization';
import { FlowPromptEditor } from './FlowPromptEditor';
import { FlowProfileSelector } from './FlowProfileSelector';
import { FlowJobProgress } from './FlowJobProgress';
import { TransformationIntent } from '../../lib/ipc';

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
    openProfileBrowser,
    loadGeminiStatus,
    startFlowJob,
    pollJobStatus,
  } = useFlowJobStore();

  const { activeProject } = useProjectStore();

  const projectId = activeProject?.id || '';
  const [selectedMediaId, setSelectedMediaId] = useState<string>('');
  const [transformationIntent, setTransformationIntent] =
    useState<TransformationIntent>('FACE_REPLACE');
  const [maxCreditsInput, setMaxCreditsInput] = useState<string>('');

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
    transformationIntent,
    identityMode: 'GENERATED',
    preserveBackground: true,
    preserveBody: true,
    preserveClothing: true,
    preserveNonTargetFaces: true,
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

    const maxCredits = maxCreditsInput.trim() ? parseInt(maxCreditsInput.trim(), 10) : undefined;

    await startFlowJob(
      projectId,
      selectedProfileId,
      prompt.trim(),
      promptSource,
      selectedMediaId.trim(),
      {
        transformationIntent,
        identityMode: 'GENERATED',
        maxCredits: isNaN(maxCredits as number) ? undefined : maxCredits,
        preserveOriginalAudio: true,
      }
    );
  };

  const selectedProfile = profiles.find((p) => p.profileId === selectedProfileId);
  const isProfileLocked = selectedProfile?.isLocked ?? false;
  const isManualBrowserOpen =
    selectedProfile?.manualBrowserOpen || selectedProfile?.browserSessionOpen || false;
  const isProfileReady = selectedProfile?.status === 'READY';
  const isLoginRequired = selectedProfile?.status === 'LOGIN_REQUIRED';

  return (
    <div className="flex flex-col gap-6 max-w-5xl mx-auto p-6 text-slate-100">
      <div className="flex items-center justify-between border-b border-slate-800 pb-4">
        <div className="flex items-center gap-3">
          <div className="p-2.5 bg-gradient-to-br from-indigo-500/20 to-purple-500/20 border border-indigo-500/30 rounded-xl">
            <Sparkles className="w-6 h-6 text-indigo-400" />
          </div>
          <div>
            <h1 className="text-xl font-bold text-slate-100">Google Flow Video Transformation</h1>
            <p className="text-xs text-slate-400">
              Deterministic browser-driven generative editing with true video edit mode & preservation-first semantics
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

      {/* Login Required Notice */}
      {isLoginRequired && selectedProfileId && (
        <div className="flex items-center justify-between p-4 bg-amber-500/10 border border-amber-500/30 rounded-xl text-xs text-amber-200">
          <div className="flex items-center gap-2">
            <LogIn className="w-4 h-4 text-amber-400 shrink-0" />
            <span>Profile <strong>{selectedProfile?.name}</strong> requires Google account sign-in before generating.</span>
          </div>
          <button
            onClick={() => openProfileBrowser(selectedProfileId)}
            className="px-3 py-1.5 bg-amber-600 hover:bg-amber-500 text-slate-950 font-semibold rounded-lg flex items-center gap-1.5 transition cursor-pointer"
          >
            <ExternalLink className="w-3.5 h-3.5" />
            <span>Connect / Sign in to Flow</span>
          </button>
        </div>
      )}

      {/* Project Source Media & Transformation Intent Section */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
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
            <div className="flex flex-col gap-1.5 text-xs text-slate-400">
              <div className="flex items-center justify-between">
                <span>Active Project:</span>
                <strong className="text-slate-200">{activeProject.name}</strong>
              </div>
              <div className="flex items-center justify-between">
                <span>Source Media:</span>
                <strong className="text-slate-200 truncate max-w-[200px]">{selectedMediaId || 'No media imported'}</strong>
              </div>
              {!selectedMediaId && (
                <span className="text-xs text-amber-400 mt-1">
                  Please import a video into this project before starting Google Flow generation.
                </span>
              )}
            </div>
          )}
        </div>

        <div className="flex flex-col gap-2 p-4 bg-slate-900/60 border border-slate-800 rounded-xl">
          <div className="flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-indigo-400" />
            <span className="text-sm font-semibold text-slate-200">Transformation Intent</span>
          </div>

          <div className="grid grid-cols-2 gap-2 text-xs">
            <label className="flex items-center gap-2 p-2 bg-slate-950/60 border border-slate-800 rounded-lg cursor-pointer">
              <input
                type="radio"
                name="intent"
                checked={transformationIntent === 'FACE_REPLACE'}
                onChange={() => setTransformationIntent('FACE_REPLACE')}
                className="accent-indigo-600"
              />
              <span className="font-medium text-slate-200">Change Face (Default)</span>
            </label>

            <label className="flex items-center gap-2 p-2 bg-slate-950/60 border border-slate-800 rounded-lg cursor-pointer">
              <input
                type="radio"
                name="intent"
                checked={transformationIntent === 'STYLE_EDIT'}
                onChange={() => setTransformationIntent('STYLE_EDIT')}
                className="accent-indigo-600"
              />
              <span className="font-medium text-slate-200">Style Edit</span>
            </label>

            <label className="flex items-center gap-2 p-2 bg-slate-950/60 border border-slate-800 rounded-lg cursor-pointer">
              <input
                type="radio"
                name="intent"
                checked={transformationIntent === 'BACKGROUND_REPLACE'}
                onChange={() => setTransformationIntent('BACKGROUND_REPLACE')}
                className="accent-indigo-600"
              />
              <span className="font-medium text-slate-200">Change Background</span>
            </label>

            <label className="flex items-center gap-2 p-2 bg-slate-950/60 border border-slate-800 rounded-lg cursor-pointer">
              <input
                type="radio"
                name="intent"
                checked={transformationIntent === 'GENERIC_PROMPT_EDIT'}
                onChange={() => setTransformationIntent('GENERIC_PROMPT_EDIT')}
                className="accent-indigo-600"
              />
              <span className="font-medium text-slate-200">Custom Edit</span>
            </label>
          </div>
        </div>
      </div>

      {/* Prompt Editor */}
      <FlowPromptEditor
        prompt={prompt}
        promptSource={promptSource}
        isOptimizing={isOptimizing}
        geminiConfigured={geminiStatus?.isConfigured ?? geminiStatus?.stored ?? false}
        optimizationError={optimizationError}
        canUndo={canUndo}
        disabled={isStarting || (activeJob !== null && activeJob.state !== 'COMPLETED' && activeJob.state !== 'FAILED' && activeJob.state !== 'CANCELLED')}
        onPromptChange={handlePromptChange}
        onGenPrompt={handleGenPrompt}
        onUndo={handleUndo}
      />

      {/* Execution Guard & Action */}
      <div className="flex items-center justify-between p-4 bg-slate-900/40 border border-slate-800 rounded-xl flex-wrap gap-3">
        <div className="flex items-center gap-4 text-xs text-slate-400">
          <div className="flex items-center gap-1.5">
            <Info className="w-4 h-4 text-indigo-400 shrink-0" />
            <span>Mode: <strong>OmniEditUploadedVideo</strong></span>
          </div>

          <div className="flex items-center gap-2">
            <span>Budget Limit:</span>
            <input
              type="number"
              placeholder="e.g. 40 (Optional)"
              value={maxCreditsInput}
              onChange={(e) => setMaxCreditsInput(e.target.value)}
              className="w-28 px-2 py-1 bg-slate-950 border border-slate-700 rounded text-slate-200 font-mono text-xs focus:outline-none focus:border-indigo-500"
            />
          </div>
        </div>

        <button
          type="button"
          onClick={handleStartGeneration}
          disabled={
            !projectId ||
            !selectedProfileId ||
            !isProfileReady ||
            isManualBrowserOpen ||
            isProfileLocked ||
            !prompt.trim() ||
            !selectedMediaId ||
            isStarting ||
            (activeJob !== null && activeJob.state !== 'COMPLETED' && activeJob.state !== 'FAILED' && activeJob.state !== 'CANCELLED')
          }
          className="flex items-center gap-2 px-5 py-2.5 text-sm font-semibold text-white bg-gradient-to-r from-indigo-600 to-purple-600 hover:from-indigo-500 hover:to-purple-500 disabled:opacity-40 disabled:cursor-not-allowed rounded-xl shadow-lg transition cursor-pointer"
        >
          <Play className="w-4 h-4 fill-white" />
          {isStarting ? 'Initiating Pipeline...' : 'Generate with Google Flow'}
        </button>
      </div>

      {/* Active Job Progress */}
      {activeJob && <FlowJobProgress job={activeJob} />}
    </div>
  );
};
