import React from 'react';
import { 
  Sparkles, 
  XCircle, 
  PauseCircle, 
  PlayCircle, 
  CheckCircle2, 
  Cpu, 
  Clock, 
  ArrowRight
} from 'lucide-react';
import { useJobStore } from '../../stores/jobStore';
import { useUiStore } from '../../stores/uiStore';
import { MockBadge } from '../../components/common/MockBadge';
import { JobStage } from '../../types/contracts';

export const JobMonitor: React.FC = () => {
  const { activeJob, updateJobState, updateJobProgress } = useJobStore();
  const { setCurrentStep } = useUiStore();

  const stages: { id: JobStage; label: string; number: number }[] = [
    { id: 'ANALYSIS', label: 'Analysis & Scene Detect', number: 1 },
    { id: 'PLANNING', label: 'Transformation Planning', number: 2 },
    { id: 'PREPARATION', label: 'Keyframe Mask Extraction', number: 3 },
    { id: 'TRANSFORMATION', label: 'Subject Inpainting Diffusion', number: 4 },
    { id: 'TEMPORAL_REFINEMENT', label: 'Temporal Consistency Smoothing', number: 5 },
    { id: 'AUDIO', label: 'Audio Re-alignment & Demux', number: 6 },
    { id: 'QUALITY_CHECK', label: 'Quality Assessment (QC)', number: 7 },
    { id: 'EXPORT', label: 'Encoding Final Artifact', number: 8 },
  ];

  // Default fixture job if none is in store
  const job = activeJob || {
    id: 'job-demo-fox',
    projectId: 'proj-fox-rabbit',
    state: 'RUNNING' as const,
    stage: 'TRANSFORMATION' as JobStage,
    progress: {
      stage: 'TRANSFORMATION' as JobStage,
      stageIndex: 4,
      totalStages: 8,
      currentFrame: 840,
      totalFrames: 1860,
      percentage: 48.5,
      estimatedSecondsRemaining: 68,
      currentSceneName: 'Scene #2 - Fox Subject Close-up',
      gpuDevice: 'DirectX 12 Compatible GPU (DirectML)',
      vramUsageMB: 4920,
    },
    createdAt: 'Just now',
    updatedAt: 'Just now',
    isFixture: true,
  };

  const handleSimulateCompletion = () => {
    updateJobState('COMPLETED');
    updateJobProgress({
      ...job.progress,
      percentage: 100,
      currentFrame: job.progress.totalFrames || 1860,
      stage: 'EXPORT',
      stageIndex: 8,
      estimatedSecondsRemaining: 0,
    });
  };

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      {/* Title Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-slate-100 tracking-tight">AI Transformation Pipeline</h2>
          <p className="text-sm text-slate-400 mt-1">Real-time asynchronous job orchestration monitor</p>
        </div>
        <MockBadge label="DEMO JOB ORCHESTRATION" />
      </div>

      {/* Main Status & Telemetry Card */}
      <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-6">
        {/* Top Info Bar */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 pb-4 border-b border-slate-800/80">
          <div>
            <span className="text-[10px] text-slate-500 font-mono uppercase block">Pipeline State</span>
            <span className="text-sm font-bold font-mono text-indigo-400">{job.state}</span>
          </div>

          <div>
            <span className="text-[10px] text-slate-500 font-mono uppercase block">Current Scene</span>
            <span className="text-xs font-semibold text-slate-200 truncate block">
              {job.progress.currentSceneName || 'Scene #2 - Fox Close-up'}
            </span>
          </div>

          <div>
            <span className="text-[10px] text-slate-500 font-mono uppercase block">Estimated Remaining</span>
            <div className="flex items-center gap-1.5 text-xs font-mono font-medium text-slate-200">
              <Clock className="w-3.5 h-3.5 text-slate-400" />
              <span>{job.progress.estimatedSecondsRemaining}s</span>
            </div>
          </div>

          <div>
            <span className="text-[10px] text-slate-500 font-mono uppercase block">Active Hardware</span>
            <div className="flex items-center gap-1.5 text-xs text-slate-200">
              <Cpu className="w-3.5 h-3.5 text-purple-400 shrink-0" />
              <span className="truncate">{job.progress.gpuDevice || 'DirectML GPU'}</span>
            </div>
          </div>
        </div>

        {/* Big Progress Display */}
        <div className="space-y-3">
          <div className="flex items-center justify-between text-xs">
            <span className="font-semibold text-slate-300">
              Overall Job Completion ({job.progress.currentFrame} / {job.progress.totalFrames || 1860} frames)
            </span>
            <span className="font-mono text-base font-extrabold text-indigo-400">
              {job.progress.percentage.toFixed(1)}%
            </span>
          </div>

          <div className="w-full h-3.5 bg-slate-950 rounded-full overflow-hidden border border-slate-800 p-0.5">
            <div
              className="h-full bg-gradient-to-r from-purple-600 via-indigo-600 to-indigo-400 rounded-full transition-all duration-500"
              style={{ width: `${job.progress.percentage}%` }}
            />
          </div>
        </div>

        {/* 8-Stage Progress List */}
        <div className="space-y-2 pt-2">
          <span className="text-xs font-semibold text-slate-300 block">Pipeline Execution Stages</span>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-2.5">
            {stages.map((stg) => {
              const isPassed = job.progress.stageIndex > stg.number;
              const isCurrent = job.progress.stageIndex === stg.number;
              return (
                <div
                  key={stg.id}
                  className={`p-3 rounded-xl border transition-all ${
                    isCurrent
                      ? 'bg-indigo-950/40 border-indigo-500/80 shadow-md shadow-indigo-900/20'
                      : isPassed
                      ? 'bg-slate-950/60 border-slate-800 text-slate-400'
                      : 'bg-slate-950/20 border-slate-900 text-slate-600'
                  }`}
                >
                  <div className="flex items-center justify-between mb-1">
                    <span className="text-[10px] font-mono font-bold">Stage {stg.number}</span>
                    {isPassed ? (
                      <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
                    ) : isCurrent ? (
                      <span className="w-2 h-2 rounded-full bg-indigo-400 animate-ping" />
                    ) : null}
                  </div>
                  <div className={`text-xs font-semibold truncate ${isCurrent ? 'text-indigo-200' : isPassed ? 'text-slate-300' : 'text-slate-600'}`}>
                    {stg.label}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Action Buttons */}
        <div className="flex items-center justify-between pt-4 border-t border-slate-800">
          <div className="flex items-center gap-3">
            {job.state === 'RUNNING' && (
              <button
                onClick={() => updateJobState('PAUSED')}
                className="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold flex items-center gap-1.5"
              >
                <PauseCircle className="w-4 h-4" />
                <span>Pause</span>
              </button>
            )}

            {job.state === 'PAUSED' && (
              <button
                onClick={() => updateJobState('RUNNING')}
                className="px-4 py-2 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold flex items-center gap-1.5"
              >
                <PlayCircle className="w-4 h-4" />
                <span>Resume</span>
              </button>
            )}

            {(job.state === 'RUNNING' || job.state === 'PAUSED') && (
              <button
                onClick={() => updateJobState('CANCELLED')}
                className="px-4 py-2 rounded-xl bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 border border-rose-500/20 text-xs font-semibold flex items-center gap-1.5"
              >
                <XCircle className="w-4 h-4" />
                <span>Cancel</span>
              </button>
            )}

            {job.state === 'RUNNING' && (
              <button
                onClick={handleSimulateCompletion}
                className="px-3 py-1.5 rounded-lg bg-slate-800/80 hover:bg-slate-800 text-[11px] text-slate-400"
              >
                Simulate Completion (Fixture)
              </button>
            )}
          </div>

          {/* If job completed, show Proceed CTA */}
          {job.state === 'COMPLETED' && (
            <button
              onClick={() => setCurrentStep('result')}
              className="px-5 py-2.5 rounded-xl bg-gradient-to-r from-emerald-600 to-teal-600 hover:from-emerald-500 hover:to-teal-500 text-white text-xs font-bold shadow-lg shadow-emerald-900/40 transition-all flex items-center gap-2 animate-bounce"
            >
              <Sparkles className="w-4 h-4" />
              <span>Review Transformed Video</span>
              <ArrowRight className="w-4 h-4" />
            </button>
          )}
        </div>
      </div>
    </div>
  );
};
