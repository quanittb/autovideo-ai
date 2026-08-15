import React from 'react';
import { Sparkles, XCircle, PauseCircle, PlayCircle, RotateCcw } from 'lucide-react';
import { useJobStore } from '../../stores/jobStore';
import { MockBadge } from '../../components/common/MockBadge';

export const JobMonitor: React.FC = () => {
  const { activeJob, updateJobState } = useJobStore();

  if (!activeJob) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center p-8 text-center text-slate-400">
        <Sparkles className="w-12 h-12 text-slate-600 mb-3" />
        <h3 className="text-base font-semibold text-slate-300">No Active Transformation Job</h3>
        <p className="text-xs text-slate-500 max-w-sm mt-1">
          Start a transformation from the Transform or Export steps to monitor progress.
        </p>
      </div>
    );
  }

  const { state, stage, progress } = activeJob;

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Job Processing Monitor</h2>
          <p className="text-sm text-slate-400 mt-1">Real-time status of current video transformation</p>
        </div>
        <MockBadge label="ASYNC JOB ENGINE CONTRACT" />
      </div>

      <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-6">
        {/* Status Header */}
        <div className="flex items-center justify-between border-b border-slate-800 pb-4">
          <div>
            <span className="text-xs text-slate-500 block">Current State</span>
            <span className="text-sm font-bold font-mono text-indigo-400">{state}</span>
          </div>
          <div>
            <span className="text-xs text-slate-500 block">Stage {progress.stageIndex} of {progress.totalStages}</span>
            <span className="text-sm font-semibold text-slate-200">{stage}</span>
          </div>
          <div>
            <span className="text-xs text-slate-500 block">Estimated Time Remaining</span>
            <span className="text-sm font-mono font-medium text-slate-300">
              {progress.estimatedSecondsRemaining}s
            </span>
          </div>
        </div>

        {/* Progress Bar */}
        <div className="space-y-2">
          <div className="flex justify-between text-xs text-slate-400">
            <span>Overall Progress</span>
            <span className="font-mono font-bold text-slate-200">{progress.percentage.toFixed(1)}%</span>
          </div>
          <div className="w-full h-3 bg-slate-950 rounded-full overflow-hidden border border-slate-800">
            <div
              className="h-full bg-gradient-to-r from-purple-600 to-indigo-500 transition-all duration-300"
              style={{ width: `${progress.percentage}%` }}
            />
          </div>
          <div className="flex justify-between text-[11px] text-slate-500">
            <span>Frame {progress.currentFrame} / {progress.totalFrames}</span>
            <span>Zero fake progress enforcement</span>
          </div>
        </div>

        {/* Job Control Buttons */}
        <div className="flex items-center gap-3 pt-2">
          {state === 'RUNNING' && (
            <button
              onClick={() => updateJobState('PAUSED')}
              className="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold flex items-center gap-1.5"
            >
              <PauseCircle className="w-4 h-4" />
              <span>Pause Job</span>
            </button>
          )}

          {state === 'PAUSED' && (
            <button
              onClick={() => updateJobState('RUNNING')}
              className="px-4 py-2 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold flex items-center gap-1.5"
            >
              <PlayCircle className="w-4 h-4" />
              <span>Resume Job</span>
            </button>
          )}

          {(state === 'RUNNING' || state === 'PAUSED' || state === 'QUEUED') && (
            <button
              onClick={() => updateJobState('CANCELLED')}
              className="px-4 py-2 rounded-xl bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 border border-rose-500/30 text-xs font-semibold flex items-center gap-1.5"
            >
              <XCircle className="w-4 h-4" />
              <span>Cancel Job</span>
            </button>
          )}

          {(state === 'FAILED' || state === 'CANCELLED') && (
            <button
              onClick={() => updateJobState('RUNNING')}
              className="px-4 py-2 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold flex items-center gap-1.5"
            >
              <RotateCcw className="w-4 h-4" />
              <span>Retry Pipeline</span>
            </button>
          )}
        </div>
      </div>
    </div>
  );
};
