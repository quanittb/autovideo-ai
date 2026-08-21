import type { CloudJobEventPayload } from '../lib/ipc';

/**
 * Pure helper to verify if an incoming revision is strictly newer than the existing revision.
 */
export function isNewerRevision(incomingRev: number, existingRev: number): boolean {
  return incomingRev > existingRev;
}

/**
 * Merges an incoming CloudJobEventPayload snapshot with an existing snapshot idempotently.
 *
 * Invariants:
 * - incoming revision < existing => reject (keep existing)
 * - incoming revision == existing => keep existing / idempotent
 * - incoming revision > existing => apply incoming
 */
export function mergeCloudJobSnapshot(
  existing: CloudJobEventPayload | undefined,
  incoming: CloudJobEventPayload
): CloudJobEventPayload {
  if (!existing) {
    return incoming;
  }
  if (incoming.stateRevision <= existing.stateRevision) {
    return existing;
  }
  return incoming;
}

export type CloudJobVisualCategory =
  | 'running'
  | 'approval_required'
  | 'success'
  | 'failed'
  | 'cancelled'
  | 'blocked'
  | 'unknown';

/**
 * Pure helper to classify canonical and legacy CloudJobState into truthful UI visual categories.
 *
 * Categorization rules:
 * - running: CREATED, VALIDATING, UPLOADING, SUBMITTED, PROCESSING, DOWNLOADING, VALIDATING_OUTPUT, QUEUED, POLLING, DOWNLOADING_OUTPUT
 * - approval_required: COST_APPROVAL_REQUIRED
 * - success: COMPLETED
 * - failed: FAILED
 * - cancelled: CANCELLED
 * - blocked: BLOCKED
 * - unknown: any other unrecognized or null state (NEVER auto-classified as failed)
 */
export function getCloudJobVisualState(state: string | undefined | null): CloudJobVisualCategory {
  if (!state) return 'unknown';

  const s = state.toUpperCase().trim();
  switch (s) {
    case 'CREATED':
    case 'VALIDATING':
    case 'UPLOADING':
    case 'SUBMITTING':
    case 'SUBMITTED':
    case 'PROCESSING':
    case 'POLLING':
    case 'DOWNLOADING':
    case 'DOWNLOADING_OUTPUT':
    case 'VALIDATING_OUTPUT':
    case 'QUEUED':
      return 'running';

    case 'COST_APPROVAL_REQUIRED':
      return 'approval_required';

    case 'COMPLETED':
      return 'success';

    case 'FAILED':
      return 'failed';

    case 'CANCELLED':
      return 'cancelled';

    case 'BLOCKED':
      return 'blocked';

    default:
      return 'unknown';
  }
}
