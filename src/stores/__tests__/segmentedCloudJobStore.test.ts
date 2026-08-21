import { describe, it, expect } from 'vitest';
import {
  mergeSegmentedCloudJobSnapshot,
  isNewerSegmentedRevision,
  getSegmentedJobVisualState,
} from '../segmentedCloudJobHelpers';
import type { SegmentedCloudJobManifest } from '../../lib/ipc';

function makeMockSegmentedManifest(
  overrides: Partial<SegmentedCloudJobManifest> = {}
): SegmentedCloudJobManifest {
  return {
    schemaVersion: 1,
    stateRevision: 10,
    parentId: 'seg-test-1234',
    clientRequestId: 'client_segmented_req_1',
    projectId: 'proj_segment_test',
    taskType: 'BACKGROUND_REMOVAL',
    providerId: 'replicate',
    modelId: 'bria/video-remove-background',
    configurationHash: 'hash_abc123',
    state: 'COMPLETED',
    sourceFacts: {
      durationSec: 140.0,
      width: 1920,
      height: 1080,
      fps: 30.0,
      hasAudio: true,
    },
    timingFacts: {
      rFrameRate: { num: 30, den: 1 },
      avgFrameRate: { num: 30, den: 1 },
      timeBase: { num: 1, den: 30 },
      isVfr: false,
      nbFrames: 4200,
    },
    segmentPlan: {
      planId: 'plan-123',
      sourceFacts: {
        durationSec: 140.0,
        width: 1920,
        height: 1080,
        fps: 30.0,
        hasAudio: true,
      },
      timingFacts: {
        rFrameRate: { num: 30, den: 1 },
        avgFrameRate: { num: 30, den: 1 },
        timeBase: { num: 1, den: 30 },
        isVfr: false,
        nbFrames: 4200,
      },
      boundaries: [
        {
          index: 0,
          startFrame: 0,
          endFrame: 1400,
          startPts: 0,
          endPts: 1400,
          startMs: 0,
          endMs: 46667,
          expectedDurationSec: 46.667,
        },
        {
          index: 1,
          startFrame: 1400,
          endFrame: 2800,
          startPts: 1400,
          endPts: 2800,
          startMs: 46667,
          endMs: 93333,
          expectedDurationSec: 46.667,
        },
        {
          index: 2,
          startFrame: 2800,
          endFrame: 4200,
          startPts: 2800,
          endPts: 4200,
          startMs: 93333,
          endMs: 140000,
          expectedDurationSec: 46.667,
        },
      ],
      policyVersion: 1,
      providerLimitMs: 60000,
      totalSourceDurationSec: 140.0,
    },
    childJobs: [
      {
        segmentIndex: 0,
        clientJobId: 'segjob:seg-test-1234:0:hash_abc123:v1',
        state: 'COMPLETED',
        durationSec: 46.667,
        costUsd: 0.196,
        updatedAt: '2026-08-21T00:00:10Z',
      },
      {
        segmentIndex: 1,
        clientJobId: 'segjob:seg-test-1234:1:hash_abc123:v1',
        state: 'COMPLETED',
        durationSec: 46.667,
        costUsd: 0.196,
        updatedAt: '2026-08-21T00:00:20Z',
      },
      {
        segmentIndex: 2,
        clientJobId: 'segjob:seg-test-1234:2:hash_abc123:v1',
        state: 'COMPLETED',
        durationSec: 46.667,
        costUsd: 0.196,
        updatedAt: '2026-08-21T00:00:30Z',
      },
    ],
    budgetLimit: 5.0,
    provisionalEstimateUsd: 0.588,
    actualBatchBaseEstimateUsd: 0.588,
    finalOutputReady: true,
    finalAudioPolicy: {
      preserveOriginalAudio: true,
      codec: 'opus',
    },
    timestamps: {
      createdAt: '2026-08-21T00:00:00Z',
      updatedAt: '2026-08-21T00:00:35Z',
      submittedAt: '2026-08-21T00:00:02Z',
      completedAt: '2026-08-21T00:00:35Z',
    },
    cancellationRequested: false,
    progressPct: 100.0,
    error: null,
    ...overrides,
  };
}

describe('Phase 19: Segmented Cloud Job Monotonic Revision Merging', () => {
  it('accepts incoming snapshot when incoming.stateRevision > existing.stateRevision', () => {
    const existing = makeMockSegmentedManifest({ stateRevision: 5, state: 'RUNNING' });
    const incoming = makeMockSegmentedManifest({ stateRevision: 6, state: 'STITCHING' });

    const result = mergeSegmentedCloudJobSnapshot(existing, incoming);
    expect(result).toBe(incoming);
    expect(result.stateRevision).toBe(6);
    expect(result.state).toBe('STITCHING');
  });

  it('rejects incoming snapshot when incoming.stateRevision < existing.stateRevision', () => {
    const existing = makeMockSegmentedManifest({ stateRevision: 10, state: 'COMPLETED' });
    const incoming = makeMockSegmentedManifest({ stateRevision: 8, state: 'RUNNING' });

    const result = mergeSegmentedCloudJobSnapshot(existing, incoming);
    expect(result).toBe(existing);
    expect(result.stateRevision).toBe(10);
    expect(result.state).toBe('COMPLETED');
  });

  it('preserves existing snapshot reference when incoming.stateRevision == existing.stateRevision (idempotency)', () => {
    const existing = makeMockSegmentedManifest({ stateRevision: 12, state: 'COMPLETED', progressPct: 100 });
    const incoming = makeMockSegmentedManifest({ stateRevision: 12, state: 'RUNNING', progressPct: 50 });

    const result = mergeSegmentedCloudJobSnapshot(existing, incoming);
    expect(result).toBe(existing);
    expect(result.state).toBe('COMPLETED');
    expect(result.progressPct).toBe(100);
  });

  it('returns incoming directly if existing is undefined', () => {
    const incoming = makeMockSegmentedManifest({ stateRevision: 1 });
    const result = mergeSegmentedCloudJobSnapshot(undefined, incoming);
    expect(result).toBe(incoming);
  });

  it('handles stale hydration race condition where event arrives before stale list hydration', () => {
    let storeState: Record<string, SegmentedCloudJobManifest> = {};

    // 1. Event with revision 12 arrives from WebSocket / Tauri event
    const eventSnapshot = makeMockSegmentedManifest({
      parentId: 'seg-race-1',
      stateRevision: 12,
      state: 'STITCHING',
    });
    storeState['seg-race-1'] = mergeSegmentedCloudJobSnapshot(storeState['seg-race-1'], eventSnapshot);

    expect(storeState['seg-race-1'].stateRevision).toBe(12);
    expect(storeState['seg-race-1'].state).toBe('STITCHING');

    // 2. Slower list/hydrate call with stale revision 10 completes later
    const staleHydratedSnapshot = makeMockSegmentedManifest({
      parentId: 'seg-race-1',
      stateRevision: 10,
      state: 'RUNNING',
    });
    storeState['seg-race-1'] = mergeSegmentedCloudJobSnapshot(storeState['seg-race-1'], staleHydratedSnapshot);

    // 3. Invariant: Revision 12 must remain intact
    expect(storeState['seg-race-1'].stateRevision).toBe(12);
    expect(storeState['seg-race-1'].state).toBe('STITCHING');
  });

  it('isNewerSegmentedRevision helper strictly evaluates greater than', () => {
    expect(isNewerSegmentedRevision(5, 4)).toBe(true);
    expect(isNewerSegmentedRevision(5, 5)).toBe(false);
    expect(isNewerSegmentedRevision(4, 5)).toBe(false);
  });
});

describe('Phase 19: Segmented Job Visual State Categorization', () => {
  it('correctly maps active running states', () => {
    expect(getSegmentedJobVisualState('PLANNING')).toBe('running');
    expect(getSegmentedJobVisualState('SPLITTING')).toBe('running');
    expect(getSegmentedJobVisualState('READY')).toBe('running');
    expect(getSegmentedJobVisualState('RUNNING')).toBe('running');
    expect(getSegmentedJobVisualState('STITCHING')).toBe('running');
    expect(getSegmentedJobVisualState('VALIDATING_OUTPUT')).toBe('running');
  });

  it('correctly maps terminal and approval states', () => {
    expect(getSegmentedJobVisualState('COST_APPROVAL_REQUIRED')).toBe('approval_required');
    expect(getSegmentedJobVisualState('COMPLETED')).toBe('success');
    expect(getSegmentedJobVisualState('FAILED')).toBe('failed');
    expect(getSegmentedJobVisualState('CANCELLED')).toBe('cancelled');
    expect(getSegmentedJobVisualState('BLOCKED')).toBe('blocked');
    expect(getSegmentedJobVisualState(null)).toBe('unknown');
    expect(getSegmentedJobVisualState('SOMETHING_ELSE')).toBe('unknown');
  });
});
