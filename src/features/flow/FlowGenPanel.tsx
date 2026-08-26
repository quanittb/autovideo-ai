import React, { useEffect, useState } from 'react';
import { Play, Sparkles, AlertCircle, Info, RefreshCw, Folder, LogIn, ExternalLink, Film } from 'lucide-react';
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
    preflight,
    isStarting,
    isPreflighting,
    isLoadingProfiles,
    error: storeError,
    creditStatusByProfile,
    isRefreshingCreditByProfile,
    loadProfiles,
    createProfile,
    selectProfile,
    openProfileBrowser,
    refreshCreditBalance,
    fetchModelCapabilities,
    loadGeminiStatus,
    loadFlowJobs,
    preflightFlowJob,
    invalidatePreflight,
    startFlowJob,
    pollJobStatus,
  } = useFlowJobStore();

  const { activeProject } = useProjectStore();

  const projectId = activeProject?.id || '';
  const [selectedMediaId, setSelectedMediaId] = useState<string>('');
  const [transformationIntent, setTransformationIntent] =
    useState<TransformationIntent>('FACE_REPLACE');
  const [maxCreditsInput, setMaxCreditsInput] = useState<string>('');

  // Generation Settings State
  const [selectedModel, setSelectedModel] = useState<string>('Omni Flash');
  const [selectedResolution, setSelectedResolution] = useState<string>('720p');
  const [selectedDuration, setSelectedDuration] = useState<number>(10);
  const [selectedOrientation, setSelectedOrientation] = useState<string>('9:16');
  const [selectedOutputCount, setSelectedOutputCount] = useState<number>(1);

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
    initialPrompt: '',
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
    if (selectedProfileId) {
      refreshCreditBalance(selectedProfileId).catch(() => {});
      fetchModelCapabilities(selectedProfileId, 'UPLOADED_VIDEO_EDIT').catch(() => {});
    }
  }, [selectedProfileId, refreshCreditBalance, fetchModelCapabilities]);

  useEffect(() => {
    if (projectId) {
      loadFlowJobs(projectId);
    }
  }, [projectId, loadFlowJobs]);

  const availableMediaList: { id: string; label: string; isDerived: boolean; durationSec: number }[] = [];
  if (activeProject?.sourceMedia) {
    availableMediaList.push({
      id: activeProject.sourceMedia.mediaId,
      label: `Original: ${activeProject.sourceMedia.originalFileName}`,
      isDerived: false,
      durationSec: Math.round(activeProject.sourceMedia.durationMs / 1000),
    });
  }
  if (activeProject?.derivedMediaAssets) {
    activeProject.derivedMediaAssets.forEach((d, idx) => {
      availableMediaList.push({
        id: d.media.mediaId,
        label: `Flow Derived #${idx + 1}: ${d.media.originalFileName}`,
        isDerived: true,
        durationSec: Math.round(d.media.durationMs / 1000),
      });
    });
  }

  useEffect(() => {
    if (activeProject?.editorState?.activeMediaId) {
      setSelectedMediaId(activeProject.editorState.activeMediaId);
    } else if (activeProject?.sourceMedia?.mediaId) {
      setSelectedMediaId(activeProject.sourceMedia.mediaId);
    } else if (availableMediaList.length > 0) {
      setSelectedMediaId(availableMediaList[0].id);
    } else {
      setSelectedMediaId('');
    }
  }, [activeProject?.id, activeProject?.editorState?.activeMediaId, activeProject?.sourceMedia?.mediaId]);

  // Polling active job
  useEffect(() => {
    if (!activeJob || !projectId) return;
    if (
      activeJob.state === 'COMPLETED' ||
      activeJob.state === 'FAILED' ||
      activeJob.state === 'CANCELLED' ||
      activeJob.state === 'BLOCKED' ||
      activeJob.state === 'LOGIN_REQUIRED' ||
      activeJob.state === 'CREDITS_REQUIRED' ||
      activeJob.state === 'FLOW_UI_CHANGED' ||
      activeJob.state === 'GENERATION_AMBIGUOUS' ||
      activeJob.state === 'USER_ACTION_REQUIRED'
    ) {
      return;
    }

    const timer = setInterval(() => {
      pollJobStatus(projectId, activeJob.parentId);
    }, 2000);

    return () => clearInterval(timer);
  }, [activeJob, projectId, pollJobStatus]);

  // Invalidate preflight on any parameter change
  useEffect(() => {
    invalidatePreflight();
  }, [
    projectId,
    selectedProfileId,
    selectedMediaId,
    transformationIntent,
    prompt,
    selectedModel,
    selectedResolution,
    selectedDuration,
    selectedOrientation,
    selectedOutputCount,
    invalidatePreflight,
  ]);

  const isPromptValid =
    transformationIntent === 'FACE_REPLACE' ? true : prompt.trim().length > 0;

  const currentRequestedConfig = {
    modelId: selectedModel,
    resolution: selectedResolution,
    durationSec: selectedDuration,
    orientation: selectedOrientation,
    outputCount: selectedOutputCount,
  };

  const handleCheckCost = async () => {
    if (!projectId || !selectedProfileId || !selectedMediaId.trim() || !isPromptValid) return;
    try {
      await preflightFlowJob({
        projectId,
        profileId: selectedProfileId,
        sourceMediaId: selectedMediaId.trim(),
        transformationIntent,
        identityMode: 'GENERATED',
        prompt: prompt.trim(),
        promptSource,
        preserveOriginalAudio: true,
        requestedConfig: currentRequestedConfig,
      });
    } catch {
      // Handled by store
    }
  };

  const handleStartGeneration = async () => {
    if (!projectId || !selectedProfileId || !selectedMediaId.trim() || !isPromptValid) return;

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
        requestedConfig: currentRequestedConfig,
        configurationFingerprint: preflight?.configurationFingerprint,
        preflightId: preflight?.preflightId,
      }
    );
  };

  const selectedProfile = profiles.find((p) => p.profileId === selectedProfileId);
  const isProfileLocked = selectedProfile?.isLocked ?? false;
  const isManualBrowserOpen =
    selectedProfile?.manualBrowserOpen || selectedProfile?.browserSessionOpen || false;
  const isProfileReady = selectedProfile?.status === 'READY';
  const isLoginRequired = selectedProfile?.status === 'LOGIN_REQUIRED';

  const currentProfileCredit = selectedProfileId ? creditStatusByProfile[selectedProfileId] : undefined;
  const isRefreshingCredit = selectedProfileId ? !!isRefreshingCreditByProfile[selectedProfileId] : false;

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
            if (projectId) loadFlowJobs(projectId);
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

      {/* Profile Selector & Credit Balance Banner */}
      <div className="flex flex-col gap-3">
        <FlowProfileSelector
          profiles={profiles}
          selectedProfileId={selectedProfileId}
          isLoading={isLoadingProfiles}
          onSelectProfile={selectProfile}
          onCreateProfile={createProfile}
          onRefreshProfiles={loadProfiles}
        />

        {selectedProfile && (
          <div className="flex items-center justify-between px-4 py-2.5 bg-slate-900/80 border border-slate-800 rounded-xl text-xs">
            <div className="flex items-center gap-3">
              <span className="text-slate-400">Profile Balance ({selectedProfile.name}):</span>
              {currentProfileCredit?.balance !== undefined ? (
                <span className="font-bold text-emerald-400 text-sm">
                  {currentProfileCredit.balance.toLocaleString()} credits
                </span>
              ) : currentProfileCredit?.status === 'LOGIN_REQUIRED' ? (
                <span className="text-amber-400 font-medium">Login Required</span>
              ) : currentProfileCredit?.status === 'PROFILE_BUSY' ? (
                <span className="text-amber-400 font-medium">Profile Busy / In Use</span>
              ) : (
                <span className="text-slate-400 italic">Unknown (Not yet read from Flow)</span>
              )}
              {currentProfileCredit?.checkedAt && (
                <span className="text-[10px] text-slate-500">
                  (checked {new Date(currentProfileCredit.checkedAt).toLocaleTimeString()})
                </span>
              )}
            </div>

            <button
              type="button"
              onClick={() => selectedProfileId && refreshCreditBalance(selectedProfileId)}
              disabled={isRefreshingCredit || isProfileLocked || isManualBrowserOpen}
              className="flex items-center gap-1 px-2.5 py-1 text-xs text-indigo-300 hover:text-white bg-indigo-950/50 hover:bg-indigo-900 border border-indigo-700/40 rounded-lg transition disabled:opacity-40 cursor-pointer"
              title="Refresh live credit balance for this profile"
            >
              <RefreshCw className={`w-3 h-3 ${isRefreshingCredit ? 'animate-spin' : ''}`} />
              {isRefreshingCredit ? 'Reading...' : 'Refresh Balance'}
            </button>
          </div>
        )}
      </div>

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

      {/* Project Source Media, Transformation Intent & Generation Settings */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="flex flex-col gap-2 p-4 bg-slate-900/60 border border-slate-800 rounded-xl">
          <div className="flex items-center gap-2">
            <Folder className="w-4 h-4 text-indigo-400" />
            <span className="text-sm font-semibold text-slate-200">Project Working Media</span>
          </div>

          {!activeProject ? (
            <div className="p-3 bg-amber-500/10 border border-amber-500/20 text-amber-300 text-xs rounded-lg">
              No active project loaded. Please open or create a project to select source video.
            </div>
          ) : availableMediaList.length === 0 ? (
            <div className="p-3 bg-amber-500/10 border border-amber-500/20 text-amber-300 text-xs rounded-lg">
              Please import a video into this project before starting Google Flow generation.
            </div>
          ) : (
            <div className="flex flex-col gap-2 text-xs">
              <div className="flex items-center justify-between text-slate-400">
                <span>Active Project:</span>
                <strong className="text-slate-200">{activeProject.name}</strong>
              </div>

              <div className="flex flex-col gap-1">
                <label className="text-[11px] text-slate-400 font-medium flex items-center gap-1">
                  <Film className="w-3.5 h-3.5 text-indigo-400" />
                  Select Working Media (Original or Derived):
                </label>
                <select
                  value={selectedMediaId}
                  onChange={(e) => setSelectedMediaId(e.target.value)}
                  className="w-full px-2.5 py-1.5 bg-slate-950 border border-slate-700 rounded-lg text-slate-200 text-xs focus:outline-none focus:border-indigo-500 font-sans"
                >
                  {availableMediaList.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.label} ({m.durationSec}s)
                    </option>
                  ))}
                </select>
              </div>

              <div className="text-[11px] text-slate-500 font-mono">
                mediaId: {selectedMediaId || 'none'}
              </div>
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

      {/* Model & Quality Configuration */}
      <div className="p-4 bg-slate-900/60 border border-slate-800 rounded-xl flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-purple-400" />
            <span className="text-sm font-semibold text-slate-200">Model & Quality Settings</span>
          </div>
          <span className="text-[11px] text-slate-400">Production Defaults Applied</span>
        </div>

        <div className="grid grid-cols-2 sm:grid-cols-5 gap-3 text-xs">
          <div className="flex flex-col gap-1">
            <label className="text-[11px] text-slate-400 font-medium">Model</label>
            <select
              value={selectedModel}
              onChange={(e) => setSelectedModel(e.target.value)}
              className="px-2.5 py-1.5 bg-slate-950 border border-slate-700 rounded-lg text-slate-200 text-xs focus:outline-none focus:border-indigo-500"
            >
              <option value="Omni Flash">Omni Flash (Default)</option>
            </select>
          </div>

          <div className="flex flex-col gap-1">
            <label className="text-[11px] text-slate-400 font-medium">Resolution</label>
            <select
              value={selectedResolution}
              onChange={(e) => setSelectedResolution(e.target.value)}
              className="px-2.5 py-1.5 bg-slate-950 border border-slate-700 rounded-lg text-slate-200 text-xs focus:outline-none focus:border-indigo-500"
            >
              <option value="720p">720p (Lowest Cost)</option>
              <option value="1080p">1080p (HD)</option>
            </select>
          </div>

          <div className="flex flex-col gap-1">
            <label className="text-[11px] text-slate-400 font-medium">Duration</label>
            <select
              value={selectedDuration}
              onChange={(e) => setSelectedDuration(parseInt(e.target.value, 10))}
              className="px-2.5 py-1.5 bg-slate-950 border border-slate-700 rounded-lg text-slate-200 text-xs focus:outline-none focus:border-indigo-500"
            >
              <option value={10}>10s (Edit Standard)</option>
            </select>
          </div>

          <div className="flex flex-col gap-1">
            <label className="text-[11px] text-slate-400 font-medium">Orientation</label>
            <select
              value={selectedOrientation}
              onChange={(e) => setSelectedOrientation(e.target.value)}
              className="px-2.5 py-1.5 bg-slate-950 border border-slate-700 rounded-lg text-slate-200 text-xs focus:outline-none focus:border-indigo-500"
            >
              <option value="9:16">9:16 (Portrait)</option>
              <option value="16:9">16:9 (Landscape)</option>
            </select>
          </div>

          <div className="flex flex-col gap-1">
            <label className="text-[11px] text-slate-400 font-medium">Outputs</label>
            <select
              value={selectedOutputCount}
              onChange={(e) => setSelectedOutputCount(parseInt(e.target.value, 10))}
              className="px-2.5 py-1.5 bg-slate-950 border border-slate-700 rounded-lg text-slate-200 text-xs focus:outline-none focus:border-indigo-500"
            >
              <option value={1}>1 Output</option>
            </select>
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

      {/* Preflight Inspection Banner */}
      {preflight && (
        <div className={`p-4 rounded-xl border text-xs flex flex-col gap-2 ${
          preflight.readyForPaidSubmission
            ? 'bg-indigo-950/40 border-indigo-500/30'
            : 'bg-amber-950/40 border-amber-500/30 text-amber-200'
        }`}>
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 font-medium">
              <Sparkles className="w-4 h-4 text-indigo-400" />
              <span>
                Discovered Flow Cost:{' '}
                <strong>{preflight.liveDisplayedCreditCost ? `${preflight.liveDisplayedCreditCost} credits` : 'Unknown'}</strong>
                {preflight.costProvenance && (
                  <span className="text-slate-400 ml-2 font-mono text-[10px] uppercase bg-slate-800/80 px-1.5 py-0.5 rounded">
                    {preflight.costProvenance}
                  </span>
                )}
                {preflight.liveCreditBalance !== undefined && (
                  <span className="text-slate-300 ml-2">
                    (Account Balance: {preflight.liveCreditBalance} credits)
                  </span>
                )}
              </span>
            </div>
            <span className="text-slate-400">
              Verified at {new Date(preflight.checkedAt).toLocaleTimeString()}
            </span>
          </div>

          <div className="flex items-center gap-4 text-slate-300 flex-wrap">
            <span>Video Attached: {preflight.videoAttached ? 'YES' : 'NO'}</span>
            <span>Edit Mode: {preflight.videoEditActive ? 'ACTIVE (/edit/)' : 'INACTIVE'}</span>
            <span>Config Verified: {preflight.configurationVerified ? 'YES' : 'NO'}</span>
            {preflight.observedModel && (
              <span>Model: {preflight.observedModel}</span>
            )}
            {preflight.observedResolution && (
              <span>Resolution: {preflight.observedResolution}</span>
            )}
            {preflight.configuredOrientation && (
              <span>Orientation: {preflight.configuredOrientation}</span>
            )}
            <span>Outputs: x{preflight.outputCount}</span>
            {preflight.configurationFingerprint && (
              <span className="font-mono text-[10px] text-slate-500">
                sig:{preflight.configurationFingerprint.substring(0, 8)}
              </span>
            )}
          </div>

          {preflight.blockingCode && (
            <div className="flex items-center gap-1.5 text-amber-400 font-semibold mt-1">
              <AlertCircle className="w-3.5 h-3.5" />
              <span>Blocking Code: {preflight.blockingCode}</span>
            </div>
          )}
        </div>
      )}

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

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={handleCheckCost}
            disabled={
              !projectId ||
              !selectedProfileId ||
              !isProfileReady ||
              isManualBrowserOpen ||
              isProfileLocked ||
              !isPromptValid ||
              !selectedMediaId ||
              isStarting ||
              isPreflighting ||
              (activeJob !== null && activeJob.state !== 'COMPLETED' && activeJob.state !== 'FAILED' && activeJob.state !== 'CANCELLED')
            }
            className="flex items-center gap-2 px-4 py-2.5 text-xs font-semibold text-slate-200 bg-slate-800 hover:bg-slate-700 border border-slate-700 hover:border-slate-600 disabled:opacity-40 disabled:cursor-not-allowed rounded-xl transition cursor-pointer"
          >
            <Sparkles className={`w-3.5 h-3.5 text-indigo-400 ${isPreflighting ? 'animate-spin' : ''}`} />
            {isPreflighting ? 'Checking Flow Cost...' : 'Check Flow Cost'}
          </button>

          <button
            type="button"
            onClick={handleStartGeneration}
            disabled={
              !projectId ||
              !selectedProfileId ||
              !isProfileReady ||
              isManualBrowserOpen ||
              isProfileLocked ||
              !isPromptValid ||
              !selectedMediaId ||
              isStarting ||
              isPreflighting ||
              (activeJob !== null && activeJob.state !== 'COMPLETED' && activeJob.state !== 'FAILED' && activeJob.state !== 'CANCELLED')
            }
            className="flex items-center gap-2 px-5 py-2.5 text-sm font-semibold text-white bg-gradient-to-r from-indigo-600 to-purple-600 hover:from-indigo-500 hover:to-purple-500 disabled:opacity-40 disabled:cursor-not-allowed rounded-xl shadow-lg transition cursor-pointer"
          >
            <Play className="w-4 h-4 fill-white" />
            {isStarting ? 'Initiating Pipeline...' : 'Generate with Google Flow'}
          </button>
        </div>
      </div>

      {/* Active Job Progress */}
      {activeJob && <FlowJobProgress job={activeJob} />}
    </div>
  );
};
