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
