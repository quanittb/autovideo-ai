import React from 'react';
import { ShieldCheck, CheckCircle, AlertTriangle, Activity, Sparkles } from 'lucide-react';
import { QualityMetrics } from '../../types/contracts';

interface QualityReportProps {
  metrics?: QualityMetrics;
  className?: string;
}

export const QualityReport: React.FC<QualityReportProps> = ({
  metrics = {
    temporalConsistencyScore: 98.4,
    identityPreservationScore: 96.2,
    audioSyncOffsetMs: 0,
    warnings: ['High-contrast lighting detected in Scene #2; deflicker filter applied.'],
  },
  className = '',
}) => {
  return (
    <div className={`p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4 ${className}`}>
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <ShieldCheck className="w-4 h-4 text-emerald-400" />
          <h4 className="text-xs font-semibold text-slate-200">Transformation Quality & Consistency Report</h4>
        </div>
        <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-500/10 text-emerald-400 border border-emerald-500/30">
          PASSED QC CHECKS
        </span>
      </div>

      {/* Metrics Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-3 text-xs">
        <div className="p-3 rounded-xl bg-slate-950/80 border border-slate-800/80 space-y-1">
          <div className="flex items-center justify-between text-slate-400">
            <span>Temporal Stability</span>
            <Activity className="w-3.5 h-3.5 text-indigo-400" />
          </div>
          <div className="text-lg font-bold font-mono text-indigo-300">
            {metrics.temporalConsistencyScore}%
          </div>
          <span className="text-[10px] text-slate-500 block">Zero optical flicker detected</span>
        </div>

        <div className="p-3 rounded-xl bg-slate-950/80 border border-slate-800/80 space-y-1">
          <div className="flex items-center justify-between text-slate-400">
            <span>Identity Fidelity</span>
            <Sparkles className="w-3.5 h-3.5 text-purple-400" />
          </div>
          <div className="text-lg font-bold font-mono text-purple-300">
            {metrics.identityPreservationScore}%
          </div>
          <span className="text-[10px] text-slate-500 block">Subject pose & motion aligned</span>
        </div>

        <div className="p-3 rounded-xl bg-slate-950/80 border border-slate-800/80 space-y-1">
          <div className="flex items-center justify-between text-slate-400">
            <span>Audio / Video Sync</span>
            <CheckCircle className="w-3.5 h-3.5 text-emerald-400" />
          </div>
          <div className="text-lg font-bold font-mono text-emerald-300">
            {metrics.audioSyncOffsetMs} ms
          </div>
          <span className="text-[10px] text-slate-500 block">Original waveform preserved</span>
        </div>
      </div>

      {/* Warnings & Notes */}
      {metrics.warnings.length > 0 && (
        <div className="space-y-1.5 pt-1">
          {metrics.warnings.map((warn, i) => (
            <div key={i} className="flex items-center gap-2 text-[11px] text-amber-300/90 bg-amber-500/10 px-3 py-1.5 rounded-lg border border-amber-500/20">
              <AlertTriangle className="w-3.5 h-3.5 text-amber-400 shrink-0" />
              <span>{warn}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
