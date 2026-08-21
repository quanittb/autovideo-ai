import React, { useState, useEffect, useCallback } from 'react';
import {
  UserRound,
  Layers,
  Sparkles,
  AlertCircle,
  CheckCircle2,
  Loader2,
  XCircle,
  ShieldAlert,
} from 'lucide-react';
import { ReferenceUploader } from '../../components/ui/ReferenceUploader';
import { useProjectStore } from '../../stores/projectStore';
import { useCloudJobStore } from '../../stores/cloudJobStore';
import type { CloudJobRequest } from '../../lib/ipc';

interface TransformPanelProps {
  className?: string;
}

export const TransformPanel: React.FC<TransformPanelProps> = ({ className = '' }) => {
  const { activeProject } = useProjectStore();
  const {
    preflight,
    isPreflightLoading,
    preflightError,
    isSubmitting,
    isCancelling,
    actionError,
    cloudJobsById,
    selectedInternalJobId,
    runPreflight,
    startTransformation,
    cancelJob,
    clearErrors,
  } = useCloudJobStore();

  const [taskType, setTaskType] = useState<'CHARACTER_REPLACEMENT' | 'BACKGROUND_REMOVAL'>('CHARACTER_REPLACEMENT');
  const [prompt, setPrompt] = useState('A charismatic cyber hero in futuristic jacket');
  const [referenceImages, setReferenceImages] = useState<string[]>([]);
  const [budgetLimit, setBudgetLimit] = useState<number>(3.0);

  const selectedJob = selectedInternalJobId ? cloudJobsById[selectedInternalJobId] : null;

  // Build current CloudJobRequest
  const buildRequest = useCallback((): CloudJobRequest | null => {
    if (!activeProject) return null;

    const sourceVideoPath = activeProject.sourceMedia?.sourcePath;
    const durationSeconds = activeProject.sourceMedia?.durationMs
      ? activeProject.sourceMedia.durationMs / 1000
      : 10.0;

    return {
      jobId: `req_${Date.now()}`,
      projectId: activeProject.id,
      prompt: taskType === 'BACKGROUND_REMOVAL' ? '' : prompt,
      sourceVideo: sourceVideoPath,
      referenceImages: taskType === 'BACKGROUND_REMOVAL' ? undefined : (referenceImages.length > 0 ? referenceImages : undefined),
      durationSeconds,
      fps: activeProject.sourceMedia?.fps || 30.0,
      resolution: [
        activeProject.sourceMedia?.width || 1920,
        activeProject.sourceMedia?.height || 1080,
      ],
      taskType,
    };
  }, [activeProject, taskType, prompt, referenceImages]);

  // Run preflight whenever configuration changes
  useEffect(() => {
    const req = buildRequest();
    if (req) {
      runPreflight(req, budgetLimit);
    }
  }, [buildRequest, budgetLimit, runPreflight]);

  const handleTaskTypeChange = (type: 'CHARACTER_REPLACEMENT' | 'BACKGROUND_REMOVAL') => {
    setTaskType(type);
    clearErrors();
    if (type === 'BACKGROUND_REMOVAL') {
      setReferenceImages([]);
    }
  };

  const handleStartGeneration = async () => {
    const req = buildRequest();
    if (!req) return;
    await startTransformation(req, budgetLimit);
  };

  const handleCancelCurrentJob = async () => {
    if (!activeProject || !selectedJob) return;
    await cancelJob(activeProject.id, selectedJob.internalJobId);
  };

  const isJobRunning =
    selectedJob &&
    ['queued', 'submitting', 'polling', 'downloading_output', 'validating_output'].includes(
      selectedJob.state
    );

  const canGenerate =
    !isSubmitting &&
    !isJobRunning &&
    preflight?.submittable === true &&
    (taskType === 'BACKGROUND_REMOVAL' || referenceImages.length > 0);

  return (
    <div
      className={`flex flex-col h-full bg-slate-900/60 border border-slate-800/80 rounded-2xl p-5 overflow-y-auto space-y-5 text-slate-100 ${className}`}
    >
      {/* Transformation Mode Selector */}
      <div className="space-y-2">
        <label className="text-xs font-semibold text-slate-300">Transformation Mode</label>
        <div className="grid grid-cols-2 gap-2 p-1 rounded-xl bg-slate-950 border border-slate-800">
          <button
            onClick={() => handleTaskTypeChange('CHARACTER_REPLACEMENT')}
            className={`flex items-center justify-center gap-2 py-2.5 px-3 rounded-lg text-xs font-semibold transition-all ${
              taskType === 'CHARACTER_REPLACEMENT'
                ? 'bg-indigo-600 text-white shadow-md shadow-indigo-900/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <UserRound className="w-4 h-4" />
            <span>Character Replacement</span>
          </button>
          <button
            onClick={() => handleTaskTypeChange('BACKGROUND_REMOVAL')}
            className={`flex items-center justify-center gap-2 py-2.5 px-3 rounded-lg text-xs font-semibold transition-all ${
              taskType === 'BACKGROUND_REMOVAL'
                ? 'bg-indigo-600 text-white shadow-md shadow-indigo-900/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <Layers className="w-4 h-4" />
            <span>Background Removal</span>
          </button>
        </div>
      </div>

      {/* Task-Specific Inputs */}
      {taskType === 'CHARACTER_REPLACEMENT' ? (
        <div className="space-y-4">
          <div className="space-y-1.5">
            <label className="text-xs font-semibold text-slate-300">Character Description</label>
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={2}
              placeholder="Describe the target character appearance, outfit, and style..."
              className="w-full p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-indigo-500 transition-colors resize-none leading-relaxed"
            />
          </div>

          <ReferenceUploader
            label="Target Character Reference (1-3 images)"
            onImageSelected={(img) => {
              if (img && !referenceImages.includes(img)) {
                setReferenceImages([...referenceImages.slice(0, 2), img]);
              }
            }}
          />
          {referenceImages.length > 0 && (
            <div className="flex items-center justify-between text-[11px] text-slate-400 font-mono">
              <span>References attached: {referenceImages.length}/3</span>
              <button
                onClick={() => setReferenceImages([])}
                className="text-rose-400 hover:underline"
              >
                Clear all
              </button>
            </div>
          )}
        </div>
      ) : (
        <div className="p-4 rounded-xl bg-slate-950/80 border border-slate-800 space-y-2">
          <div className="flex items-center gap-2 text-indigo-400 font-semibold text-xs">
            <Sparkles className="w-4 h-4" />
            <span>Automatic Alpha Matting (BRIA AI)</span>
          </div>
          <p className="text-xs text-slate-400 leading-relaxed">
            Extracts foreground subjects and produces a transparent WebM (VP9 + Alpha channel) video.
            No reference images required.
          </p>
        </div>
      )}

      {/* Authoritative Preflight & Cost Summary */}
      <div className="p-4 rounded-xl bg-slate-950/90 border border-slate-800/90 space-y-3">
        <div className="flex items-center justify-between">
          <span className="text-xs font-semibold text-slate-300">Execution & Cost Estimate</span>
          {isPreflightLoading ? (
            <Loader2 className="w-3.5 h-3.5 text-indigo-400 animate-spin" />
          ) : preflight?.submittable ? (
            <span className="flex items-center gap-1 text-[11px] font-semibold text-emerald-400">
              <CheckCircle2 className="w-3.5 h-3.5" />
              <span>Ready to submit</span>
            </span>
          ) : (
            <span className="flex items-center gap-1 text-[11px] font-semibold text-amber-400">
              <AlertCircle className="w-3.5 h-3.5" />
              <span>Blocked ({preflight?.blockingCode || 'Pending inputs'})</span>
            </span>
          )}
        </div>

        {preflight && (
          <div className="grid grid-cols-2 gap-2 text-[11px] font-mono">
            <div className="p-2.5 rounded-lg bg-slate-900 border border-slate-800/80">
              <span className="text-slate-500 block text-[10px]">ROUTED PROVIDER</span>
              <span className="text-slate-200 font-semibold truncate block">
                {preflight.routingDecision.providerId} / {preflight.routingDecision.modelId}
              </span>
            </div>
            <div className="p-2.5 rounded-lg bg-slate-900 border border-slate-800/80">
              <span className="text-slate-500 block text-[10px]">ESTIMATED COST</span>
              <span className="text-emerald-400 font-semibold">
                ${preflight.routingDecision.estimatedCost.estimatedUsd?.toFixed(3) || '0.000'}{' '}
                {preflight.routingDecision.estimatedCost.currency}
              </span>
            </div>
          </div>
        )}

        {/* Budget Limit Config */}
        <div className="flex items-center justify-between pt-1 text-xs">
          <span className="text-slate-400">Budget Limit:</span>
          <div className="flex items-center gap-1 font-mono text-slate-200">
            <span>$</span>
            <input
              type="number"
              min={0.01}
              max={100}
              step={0.5}
              value={budgetLimit}
              onChange={(e) => setBudgetLimit(parseFloat(e.target.value) || 3.0)}
              className="w-16 p-1 rounded bg-slate-900 border border-slate-800 text-xs text-right focus:outline-none focus:border-indigo-500"
            />
          </div>
        </div>
      </div>

      {/* Error / Blocking Banner */}
      {(actionError || preflightError) && (
        <div className="p-3.5 rounded-xl bg-rose-950/60 border border-rose-800/80 flex items-start gap-2.5 text-xs text-rose-200">
          <ShieldAlert className="w-4 h-4 text-rose-400 shrink-0 mt-0.5" />
          <div className="space-y-1">
            <span className="font-semibold block">Execution Guard Notice</span>
            <p className="text-rose-300/90 font-mono text-[11px]">
              {actionError || preflightError}
            </p>
          </div>
        </div>
      )}

      {/* Action Buttons */}
      <div className="pt-2 space-y-2">
        {isJobRunning ? (
          <button
            onClick={handleCancelCurrentJob}
            disabled={isCancelling}
            className="w-full py-3 px-4 rounded-xl bg-rose-600 hover:bg-rose-500 disabled:opacity-50 text-white text-xs font-semibold shadow-md shadow-rose-900/30 transition-all flex items-center justify-center gap-2"
          >
            {isCancelling ? <Loader2 className="w-4 h-4 animate-spin" /> : <XCircle className="w-4 h-4" />}
            <span>Cancel Transformation</span>
          </button>
        ) : (
          <button
            onClick={handleStartGeneration}
            disabled={!canGenerate}
            className="w-full py-3 px-4 rounded-xl bg-indigo-600 hover:bg-indigo-500 disabled:bg-slate-800 disabled:text-slate-500 disabled:cursor-not-allowed text-white text-xs font-semibold shadow-md shadow-indigo-900/30 transition-all flex items-center justify-center gap-2"
          >
            {isSubmitting ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Sparkles className="w-4 h-4" />
            )}
            <span>
              {isSubmitting
                ? 'Submitting...'
                : taskType === 'BACKGROUND_REMOVAL'
                ? 'Remove Background'
                : 'Replace Character'}
            </span>
          </button>
        )}
      </div>
    </div>
  );
};
