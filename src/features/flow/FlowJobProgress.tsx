import React from 'react';
import { CheckCircle2, Loader2, AlertTriangle, Video, Layers } from 'lucide-react';
import { FlowJobSnapshot, FlowJobState } from '../../lib/ipc';

interface FlowJobProgressProps {
  job: FlowJobSnapshot;
}

export const FlowJobProgress: React.FC<FlowJobProgressProps> = ({ job }) => {
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
      ? Math.round(((job.completedGenerations) / job.totalSegments) * 100)
      : 0;

  return (
    <div className="flex flex-col gap-3 p-4 bg-slate-900/80 border border-slate-800 rounded-xl">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Layers className="w-4 h-4 text-indigo-400" />
          <span className="text-sm font-semibold text-slate-200">Flow Job: {job.parentId}</span>
        </div>

        <span className={`text-xs px-2.5 py-0.5 rounded-full border font-medium flex items-center gap-1.5 ${getStateColor(job.state)}`}>
          {isRunning && <Loader2 className="w-3 h-3 animate-spin" />}
          {job.state === 'COMPLETED' && <CheckCircle2 className="w-3 h-3" />}
          {job.state}
        </span>
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
        <div className="flex items-start gap-2 p-3 bg-rose-950/40 border border-rose-800/50 rounded-lg text-xs text-rose-300">
          <AlertTriangle className="w-4 h-4 text-rose-400 shrink-0 mt-0.5" />
          <span>{job.errorMessage}</span>
        </div>
      )}

      {job.finalOutputPath && (
        <div className="flex items-center gap-2 p-3 bg-emerald-950/40 border border-emerald-800/50 rounded-lg text-xs text-emerald-300">
          <Video className="w-4 h-4 text-emerald-400 shrink-0" />
          <span className="truncate">Final Video: {job.finalOutputPath}</span>
        </div>
      )}
    </div>
  );
};
