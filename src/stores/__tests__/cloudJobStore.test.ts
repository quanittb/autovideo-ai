import { describe, it, expect } from 'vitest';
import { mergeCloudJobSnapshot, isNewerRevision } from '../cloudJobHelpers';
import type { CloudJobEventPayload } from '../../lib/ipc';

function makeMockPayload(overrides: Partial<CloudJobEventPayload> = {}): CloudJobEventPayload {
  return {
    jobId: 'client_job_1',
    internalJobId: 'cloud_job_abc_123',
    projectId: 'proj_xyz',
    providerId: 'replicate',
    modelId: 'bria/video-remove-background',
    taskType: 'BACKGROUND_REMOVAL',
    executionClass: 'UTILITY_CLOUD',
    state: 'COMPLETED',
    submissionState: 'ACKNOWLEDGED',
    budgetLimit: 3.0,
    retryCounters: { submitAttempts: 0, pollAttempts: 0, downloadAttempts: 0 },
    createdAt: '2026-08-21T00:00:00Z',
    updatedAt: '2026-08-21T00:00:10Z',
    cancellationRequested: false,
    stateRevision: 10,
    ...overrides,
  };
}

describe('Phase 18: Monotonic Revision Merging & Invariants', () => {
  it('accepts incoming snapshot when incoming.stateRevision > existing.stateRevision', () => {
    const existing = makeMockPayload({ stateRevision: 5, state: 'SUBMITTED' });
    const incoming = makeMockPayload({ stateRevision: 6, state: 'PROCESSING' });

    const result = mergeCloudJobSnapshot(existing, incoming);
    expect(result).toBe(incoming);
    expect(result.stateRevision).toBe(6);
    expect(result.state).toBe('PROCESSING');
  });

  it('rejects incoming snapshot when incoming.stateRevision < existing.stateRevision', () => {
    const existing = makeMockPayload({ stateRevision: 10, state: 'COMPLETED' });
    const incoming = makeMockPayload({ stateRevision: 8, state: 'SUBMITTED' });

    const result = mergeCloudJobSnapshot(existing, incoming);
    expect(result).toBe(existing);
    expect(result.stateRevision).toBe(10);
    expect(result.state).toBe('COMPLETED');
  });

  it('preserves existing snapshot reference when incoming.stateRevision == existing.stateRevision (idempotency)', () => {
    const existing = makeMockPayload({ stateRevision: 12, state: 'COMPLETED', progressPct: 100 });
    // Incoming has altered stale payload with same revision 12
    const incoming = makeMockPayload({ stateRevision: 12, state: 'PROCESSING', progressPct: 50 });

    const result = mergeCloudJobSnapshot(existing, incoming);
    // Strict reference equality
    expect(result).toBe(existing);
    expect(result.state).toBe('COMPLETED');
    expect(result.progressPct).toBe(100);
  });

  it('returns incoming directly if existing is undefined', () => {
    const incoming = makeMockPayload({ stateRevision: 1 });
    const result = mergeCloudJobSnapshot(undefined, incoming);
    expect(result).toBe(incoming);
  });

  it('isNewerRevision helper strictly evaluates greater than', () => {
    expect(isNewerRevision(5, 4)).toBe(true);
    expect(isNewerRevision(5, 5)).toBe(false);
    expect(isNewerRevision(4, 5)).toBe(false);
  });
});

describe('Phase 18: Identity & Preview Authorization Invariants', () => {
  it('stores snapshots keyed strictly by internalJobId without outputUrl in payload', () => {
    const payload = makeMockPayload({
      jobId: 'client_req_99',
      internalJobId: 'internal_job_canonical_88',
      stateRevision: 1,
    });

    const store: Record<string, CloudJobEventPayload> = {};
    store[payload.internalJobId] = payload;

    expect(store['internal_job_canonical_88']).toBeDefined();
    expect(store['client_req_99']).toBeUndefined();
    // Verify remote outputUrl is not part of event contract
    expect((payload as unknown as Record<string, unknown>).outputUrl).toBeUndefined();
  });

  it('format-aware descriptors correctly carry alpha requirements', () => {
    const bgPayload = makeMockPayload({
      artifactDescriptor: {
        container: 'webm',
        videoCodec: 'vp9',
        requireAlpha: true,
        requireAudio: false,
      },
    });

    expect(bgPayload.artifactDescriptor?.container).toBe('webm');
    expect(bgPayload.artifactDescriptor?.videoCodec).toBe('vp9');
    expect(bgPayload.artifactDescriptor?.requireAlpha).toBe(true);
  });
});
