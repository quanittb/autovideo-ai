import React, { useState } from 'react';
import {
  CheckCircle2,
  Loader2,
  AlertTriangle,
  Video,
  Layers,
  Square,
  FolderOpen,
  ExternalLink,
  PlusCircle,
  RotateCcw,
} from 'lucide-react';
import { FlowJobSnapshot, FlowJobState } from '../../lib/ipc';
import { useFlowJobStore } from '../../stores/flowJobStore';
import { useProjectStore } from '../../stores/projectStore';

interface FlowJobProgressProps {
  job: FlowJobSnapshot;
}

export const FlowJobProgress: React.FC<FlowJobProgressProps> = ({ job }) => {
  const { cancelFlowJob, resumeFlowJob, openOutputArtifact, revealOutputInFolder, useOutputInProject } =
    useFlowJobStore();
  const { activeProject, setActiveProject } = useProjectStore();
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [isActing, setIsActing] = useState(false);

  const isAlreadyAdded = Boolean(
    activeProject?.derivedMediaAssets?.some(
      (d) => d.provenance.provider === 'FLOW' && d.provenance.providerJobId === job.parentId
    )
  );

  const getFriendlyStateLabel = (state: FlowJobState, currentIdx: number, total: number) => {
    switch (state) {
      case 'PLANNING':
        return 'Preparing video';
      case 'SPLITTING':
        return 'Splitting video into segments';
      case 'READY':
        return 'Ready to process';
      case 'WAITING_FOR_BROWSER':
        return 'Opening Flow browser';
      case 'UPLOADING':
        return 'Uploading video to Flow';
      case 'READY_TO_SUBMIT':
        return 'Configuring transformation settings';
      case 'SUBMITTING':
        return 'Submitting generation';
      case 'GENERATING':
        return total > 1
          ? `Generating segment ${currentIdx + 1} / ${total}`
          : 'Generating video in Google Flow';
      case 'DOWNLOADING':
        return 'Downloading segment output';
      case 'VALIDATING_SEGMENT':
        return 'Validating segment artifact';
      case 'STITCHING':
        return 'Stitching segments & muxing original audio';
      case 'VALIDATING_FINAL':
        return 'Validating final output video';
      case 'COMPLETED':
        return 'Completed';
      case 'CANCELLED':
        return 'Cancelled';
      case 'LOGIN_REQUIRED':
        return 'Sign-in required in Flow profile';
      case 'CREDITS_REQUIRED':
        return 'Flow credits required';
      case 'FLOW_UI_CHANGED':
        return 'Flow UI changed';
      case 'GENERATION_AMBIGUOUS':
        return 'Generation state unconfirmed (No auto-retry)';
      case 'BLOCKED':
        return 'Generation blocked';
      case 'FAILED':
        return 'Generation failed';
      default:
        return state;
    }
  };

  const getStateColor = (state: FlowJobState) => {
    switch (state) {
      case 'COMPLETED':
        return 'text-emerald-400 bg-emerald-950/60 border-emerald-500/40';
      case 'FAILED':
      case 'BLOCKED':
        return 'text-rose-400 bg-rose-950/60 border-rose-500/40';
      case 'CREDITS_REQUIRED':
      case 'LOGIN_REQUIRED':
      case 'GENERATION_AMBIGUOUS':
      case 'FLOW_UI_CHANGED':
      case 'USER_ACTION_REQUIRED':
        return 'text-amber-400 bg-amber-950/60 border-amber-500/40';
      case 'CANCELLED':
        return 'text-slate-400 bg-slate-900 border-slate-700';
      default:
        return 'text-indigo-400 bg-indigo-950/60 border-indigo-500/40';
    }
  };

  const isRunning =
    job.state !== 'COMPLETED' &&
    job.state !== 'FAILED' &&
    job.state !== 'CANCELLED' &&
    job.state !== 'BLOCKED' &&
    job.state !== 'CREDITS_REQUIRED' &&
    job.state !== 'LOGIN_REQUIRED' &&
    job.state !== 'FLOW_UI_CHANGED';

  const progressPct =
    job.totalSegments > 0
      ? Math.round((job.completedGenerations / job.totalSegments) * 100)
      : 0;

  const handleCancel = async () => {
    await cancelFlowJob(job.projectId, job.parentId);
  };

  const handleOpenOutput = async () => {
    setIsActing(true);
    setActionMessage(null);
    try {
      await openOutputArtifact(job.projectId, job.parentId);
    } catch (err: any) {
      setActionMessage(typeof err === 'string' ? err : err?.message || 'Failed to open video');
    } finally {
      setIsActing(false);
    }
  };

  const handleRevealInFolder = async () => {
    setIsActing(true);
    setActionMessage(null);
    try {
      await revealOutputInFolder(job.projectId, job.parentId);
    } catch (err: any) {
      setActionMessage(typeof err === 'string' ? err : err?.message || 'Failed to reveal folder');
    } finally {
      setIsActing(false);
    }
  };

  const handleUseInProject = async () => {
    setIsActing(true);
    setActionMessage(null);
    try {
      const result = await useOutputInProject(job.projectId, job.parentId);
      setActiveProject(result.project);
      setActionMessage(`Added to project as working media: ${result.derivedAsset.media.originalFileName}`);
    } catch (err: any) {
      setActionMessage(typeof err === 'string' ? err : err?.message || 'Failed to import in project');
    } finally {
      setIsActing(false);
    }
  };

  const handleResume = async () => {
    setIsActing(true);
    setActionMessage(null);
    try {
      await resumeFlowJob(job.projectId, job.parentId);
      setActionMessage('Resuming Flow generation...');
    } catch (err: any) {
      setActionMessage(typeof err === 'string' ? err : err?.message || 'Failed to resume generation');
    } finally {
      setIsActing(false);
    }
  };

  const canResume =
    job.state === 'FAILED' ||
    job.state === 'GENERATION_AMBIGUOUS' ||
    job.state === 'BLOCKED';

  return (
    <div className="flex flex-col gap-3 p-4 bg-slate-900/80 border border-slate-800 rounded-xl">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <div className="flex items-center gap-2">
          <Layers className="w-4 h-4 text-indigo-400" />
          <span className="text-sm font-semibold text-slate-200">Flow Job: {job.parentId}</span>
        </div>

        <div className="flex items-center gap-2">
          <span
            className={`text-xs px-2.5 py-0.5 rounded-full border font-medium flex items-center gap-1.5 ${getStateColor(
              job.state
            )}`}
          >
            {isRunning && <Loader2 className="w-3 h-3 animate-spin" />}
            {job.state === 'COMPLETED' && <CheckCircle2 className="w-3 h-3" />}
            {getFriendlyStateLabel(job.state, job.activeSegmentIndex, job.totalSegments)}
          </span>

          {isRunning && (
            <button
              onClick={handleCancel}
              className="px-2 py-0.5 rounded bg-rose-900/40 hover:bg-rose-900/80 border border-rose-700/50 text-rose-300 text-xs flex items-center gap-1 transition cursor-pointer"
            >
              <Square className="w-3 h-3" />
              <span>Cancel</span>
            </button>
          )}

          {canResume && (
            <button
              onClick={handleResume}
              disabled={isActing}
              className="px-2.5 py-0.5 rounded bg-indigo-900/60 hover:bg-indigo-800/80 border border-indigo-600/60 text-indigo-200 text-xs flex items-center gap-1 transition cursor-pointer"
            >
              <RotateCcw className="w-3 h-3" />
              <span>Resume</span>
            </button>
          )}
        </div>
      </div>

      {/* Progress bar */}
      <div className="w-full bg-slate-950 rounded-full h-2 overflow-hidden border border-slate-800">
        <div
          className="bg-gradient-to-r from-indigo-500 to-emerald-500 h-2 transition-all duration-300"
          style={{ width: `${Math.min(100, Math.max(0, progressPct))}%` }}
        />
      </div>

      <div className="grid grid-cols-3 gap-2 text-xs">
        <div className="p-2 bg-slate-950/60 border border-slate-800/80 rounded-lg">
          <span className="text-slate-400 block text-[11px]">Segment Progress</span>
          <span className="font-medium text-slate-200">
            {job.completedGenerations} / {job.totalSegments} completed
          </span>
        </div>

        <div className="p-2 bg-slate-950/60 border border-slate-800/80 rounded-lg">
          <span className="text-slate-400 block text-[11px]">Estimated Credits</span>
          <span className="font-medium text-indigo-300">
            {job.estimatedCredits} Flow Credits
          </span>
        </div>

        <div className="p-2 bg-slate-950/60 border border-slate-800/80 rounded-lg">
          <span className="text-slate-400 block text-[11px]">Active Segment</span>
          <span className="font-medium text-slate-200">
            Index #{job.activeSegmentIndex}
          </span>
        </div>
      </div>

      {job.errorMessage && (
        <div className="flex flex-col gap-2 p-3 bg-rose-950/40 border border-rose-800/50 rounded-lg text-xs text-rose-300">
          <div className="flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 text-rose-400 shrink-0 mt-0.5" />
            <span>{job.errorMessage}</span>
          </div>
          {canResume && (
            <div className="flex items-center justify-end pt-1">
              <button
                onClick={handleResume}
                disabled={isActing}
                className="px-3 py-1 rounded bg-indigo-700 hover:bg-indigo-600 disabled:opacity-50 text-white text-xs flex items-center gap-1.5 transition cursor-pointer font-medium"
              >
                <RotateCcw className="w-3.5 h-3.5" />
                <span>Resume Generation</span>
              </button>
            </div>
          )}
        </div>
      )}

      {job.finalOutputReady && (
        <div className="flex flex-col gap-2 p-3 bg-emerald-950/40 border border-emerald-800/50 rounded-lg text-xs text-emerald-300">
          <div className="flex items-center justify-between flex-wrap gap-2">
            <div className="flex items-center gap-2">
              <Video className="w-4 h-4 text-emerald-400 shrink-0" />
              <span className="font-semibold">Final Video Ready (Original Audio Preserved)</span>
            </div>

            <div className="flex items-center gap-2 flex-wrap">
              <button
                onClick={handleOpenOutput}
                disabled={isActing}
                className="px-2.5 py-1 rounded bg-emerald-800/60 hover:bg-emerald-700/80 border border-emerald-600/50 text-white text-xs flex items-center gap-1 transition cursor-pointer"
              >
                <ExternalLink className="w-3 h-3" />
                <span>Open Video</span>
              </button>

              <button
                onClick={handleRevealInFolder}
                disabled={isActing}
                className="px-2.5 py-1 rounded bg-slate-800 hover:bg-slate-700 border border-slate-600 text-slate-200 text-xs flex items-center gap-1 transition cursor-pointer"
              >
                <FolderOpen className="w-3 h-3" />
                <span>Reveal in Folder</span>
              </button>

              <button
                onClick={handleUseInProject}
                disabled={isActing || isAlreadyAdded}
                className="px-2.5 py-1 rounded bg-indigo-700 hover:bg-indigo-600 disabled:bg-indigo-950 disabled:border-indigo-800 disabled:text-indigo-300 border border-indigo-500 text-white text-xs flex items-center gap-1 transition cursor-pointer"
              >
                <PlusCircle className="w-3 h-3" />
                <span>{isAlreadyAdded ? 'Added to Project' : 'Use in Project'}</span>
              </button>
            </div>
          </div>

          {job.finalOutputPath && (
            <div className="text-[11px] font-mono text-emerald-200/70 truncate">
              Path: {job.finalOutputPath}
            </div>
          )}

          {actionMessage && (
            <div className="text-[11px] text-emerald-200 font-sans border-t border-emerald-800/40 pt-1.5 mt-1">
              {actionMessage}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
