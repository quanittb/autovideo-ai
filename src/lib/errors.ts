import { AppError, ErrorCode } from '../types/contracts';

export function isAppError(err: unknown): err is AppError {
  return typeof err === 'object' && err !== null && 'code' in err && 'message' in err;
}

export function formatErrorMessage(err: unknown): string {
  if (isAppError(err)) {
    return `[${err.code}] ${err.message}${err.details ? `: ${err.details}` : ''}`;
  }
  if (err instanceof Error) {
    return err.message;
  }
  return typeof err === 'string' ? err : 'An unknown error occurred';
}

export function getErrorBadgeColor(code: ErrorCode): string {
  switch (code) {
    case 'MODEL_NOT_AVAILABLE':
    case 'RUNTIME_NOT_AVAILABLE':
      return 'bg-amber-500/10 text-amber-400 border-amber-500/30';
    case 'PROCESS_FAILED':
    case 'INSUFFICIENT_RESOURCES':
      return 'bg-red-500/10 text-red-400 border-red-500/30';
    case 'CANCELLED':
      return 'bg-slate-500/10 text-slate-400 border-slate-500/30';
    default:
      return 'bg-indigo-500/10 text-indigo-400 border-indigo-500/30';
  }
}
