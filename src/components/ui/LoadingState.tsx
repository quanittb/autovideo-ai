import React from 'react';
import { Loader2 } from 'lucide-react';

interface LoadingStateProps {
  message?: string;
  subMessage?: string;
}

export const LoadingState: React.FC<LoadingStateProps> = ({
  message = 'Loading data...',
  subMessage = 'Connecting to local Rust background runtime',
}) => {
  return (
    <div className="flex flex-col items-center justify-center p-12 text-center rounded-2xl bg-slate-900/30 border border-slate-800/80 space-y-3">
      <Loader2 className="w-8 h-8 text-indigo-500 animate-spin" />
      <div className="space-y-1">
        <h4 className="text-sm font-semibold text-slate-200">{message}</h4>
        <p className="text-xs text-slate-500">{subMessage}</p>
      </div>
    </div>
  );
};
