import type { SegmentedCloudJobManifest } from '../lib/ipc';

/**
 * Pure helper to verify if an incoming revision is strictly newer than the existing revision.
 */
export function isNewerSegmentedRevision(incomingRev: number, existingRev: number): boolean {
  return incomingRev > existingRev;
}

/**
 * Merges an incoming SegmentedCloudJobManifest snapshot with an existing snapshot idempotently.
 *
 * Invariants:
 * - incoming revision <= existing => keep existing / idempotent
 * - incoming revision > existing => apply incoming
 */
export function mergeSegmentedCloudJobSnapshot(
  existing: SegmentedCloudJobManifest | undefined,
  incoming: SegmentedCloudJobManifest
): SegmentedCloudJobManifest {
  if (!existing) {
    return incoming;
  }
  if (incoming.stateRevision <= existing.stateRevision) {
    return existing;
  }
  return incoming;
}

export type SegmentedJobVisualCategory =
  | 'running'
  | 'approval_required'
  | 'success'
  | 'failed'
  | 'cancelled'
  | 'blocked'
  | 'unknown';

/**
 * Pure helper to classify SegmentedJobState into truthful UI visual categories.
 */
export function getSegmentedJobVisualState(
  state: string | undefined | null
): SegmentedJobVisualCategory {
  if (!state) return 'unknown';

  const s = state.toUpperCase().trim();
  switch (s) {
    case 'PLANNING':
    case 'SPLITTING':
    case 'READY':
    case 'RUNNING':
    case 'STITCHING':
    case 'VALIDATING_OUTPUT':
      return 'running';
    case 'COST_APPROVAL_REQUIRED':
      return 'approval_required';
    case 'COMPLETED':
      return 'success';
    case 'FAILED':
      return 'failed';
    case 'CANCELLED':
    case 'CANCELED':
      return 'cancelled';
    case 'BLOCKED':
      return 'blocked';
    default:
      return 'unknown';
  }
}
