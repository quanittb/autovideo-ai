import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useFlowJobStore } from '../flowJobStore';
import { flowApi } from '../../lib/ipc';

vi.mock('../../lib/ipc', () => ({
  flowApi: {
    listProfiles: vi.fn(),
    createProfile: vi.fn(),
    openProfileBrowser: vi.fn(),
    closeProfileBrowser: vi.fn(),
    refreshProfileStatus: vi.fn(),
    getGeminiStatus: vi.fn(),
    testGeminiApiKey: vi.fn(),
    startFlowGeneration: vi.fn(),
    getFlowJobStatus: vi.fn(),
  },
}));

describe('flowJobStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useFlowJobStore.setState({
      profiles: [],
      selectedProfileId: null,
      geminiStatus: null,
      activeJob: null,
      isStarting: false,
      isLoadingProfiles: false,
      error: null,
    });
  });

  it('loads profiles and selects the first one by default', async () => {
    vi.mocked(flowApi.listProfiles).mockResolvedValueOnce([
      {
        profileId: 'prof_1',
        name: 'Profile 1',
        status: 'READY',
        isLocked: false,
        browserSessionOpen: false,
        createdAt: '2026-08-21T00:00:00Z',
        updatedAt: '2026-08-21T00:00:00Z',
      },
    ]);

    await useFlowJobStore.getState().loadProfiles();

    const state = useFlowJobStore.getState();
    expect(state.profiles).toHaveLength(1);
    expect(state.selectedProfileId).toBe('prof_1');
    expect(state.isLoadingProfiles).toBe(false);
  });

  it('creates profile and automatically selects it', async () => {
    vi.mocked(flowApi.createProfile).mockResolvedValueOnce({
      profileId: 'prof_new',
      name: 'New Profile',
      status: 'UNKNOWN',
      isLocked: false,
      browserSessionOpen: false,
      createdAt: '2026-08-21T00:00:00Z',
      updatedAt: '2026-08-21T00:00:00Z',
    });

    await useFlowJobStore.getState().createProfile('prof_new', 'New Profile');

    const state = useFlowJobStore.getState();
    expect(state.profiles).toHaveLength(1);
    expect(state.selectedProfileId).toBe('prof_new');
  });

  it('handles openProfileBrowser and closeProfileBrowser state transitions', async () => {
    useFlowJobStore.setState({
      profiles: [
        {
          profileId: 'prof_1',
          name: 'Profile 1',
          status: 'UNKNOWN',
          isLocked: false,
          browserSessionOpen: false,
          createdAt: '2026-08-21T00:00:00Z',
          updatedAt: '2026-08-21T00:00:00Z',
        },
      ],
      selectedProfileId: 'prof_1',
    });

    vi.mocked(flowApi.openProfileBrowser).mockResolvedValueOnce('OPEN');
    await useFlowJobStore.getState().openProfileBrowser('prof_1');

    let state = useFlowJobStore.getState();
    expect(state.profiles[0].browserSessionOpen).toBe(true);
    expect(state.profiles[0].isLocked).toBe(true);

    vi.mocked(flowApi.closeProfileBrowser).mockResolvedValueOnce(undefined);
    vi.mocked(flowApi.listProfiles).mockResolvedValueOnce([
      {
        profileId: 'prof_1',
        name: 'Profile 1',
        status: 'READY',
        isLocked: false,
        browserSessionOpen: false,
        createdAt: '2026-08-21T00:00:00Z',
        updatedAt: '2026-08-21T00:00:00Z',
      },
    ]);

    await useFlowJobStore.getState().closeProfileBrowser('prof_1');
    state = useFlowJobStore.getState();
    expect(state.profiles[0].browserSessionOpen).toBe(false);
    expect(state.profiles[0].isLocked).toBe(false);
  });

  it('tests Gemini API key and stores verification status', async () => {
    vi.mocked(flowApi.testGeminiApiKey).mockResolvedValueOnce({
      stored: true,
      verificationStatus: 'VALID',
      model: 'gemini-2.5-flash-lite',
      lastVerifiedAt: '2026-08-21T12:00:00Z',
      sanitizedMessage: null,
    });

    const res = await useFlowJobStore.getState().testGeminiApiKey();
    expect(res.verificationStatus).toBe('VALID');
    expect(useFlowJobStore.getState().geminiStatus?.verificationStatus).toBe('VALID');
  });

  it('starts Flow generation job and sets activeJob', async () => {
    vi.mocked(flowApi.startFlowGeneration).mockResolvedValueOnce({
      parentId: 'flow_parent_test',
      projectId: 'proj_test',
      profileId: 'prof_1',
      submittedPrompt: 'A cybernetic cat jumping across rooftops',
      promptHash: 'hash_test',
      promptSource: 'USER',
      state: 'READY',
      stateRevision: 1,
      activeSegmentIndex: 0,
      totalSegments: 2,
      estimatedCredits: 80,
      completedGenerations: 0,
      finalOutputReady: false,
      timestamps: {
        createdAt: '2026-08-21T00:00:00Z',
        updatedAt: '2026-08-21T00:00:00Z',
      },
    });

    await useFlowJobStore
      .getState()
      .startFlowJob(
        'proj_test',
        'prof_1',
        'A cybernetic cat jumping across rooftops',
        'USER',
        'video_001.mp4'
      );

    const state = useFlowJobStore.getState();
    expect(state.activeJob).not.toBeNull();
    expect(state.activeJob?.parentId).toBe('flow_parent_test');
    expect(state.activeJob?.estimatedCredits).toBe(80);
    expect(state.isStarting).toBe(false);
  });

  it('polls and updates active job status', async () => {
    useFlowJobStore.setState({
      activeJob: {
        parentId: 'flow_parent_test',
        projectId: 'proj_test',
        profileId: 'prof_1',
        submittedPrompt: 'Prompt',
        promptHash: 'hash',
        promptSource: 'USER',
        state: 'GENERATING',
        stateRevision: 3,
        activeSegmentIndex: 0,
        totalSegments: 2,
        estimatedCredits: 80,
        completedGenerations: 0,
        finalOutputReady: false,
        timestamps: {
          createdAt: '2026-08-21T00:00:00Z',
          updatedAt: '2026-08-21T00:00:00Z',
        },
      },
    });

    vi.mocked(flowApi.getFlowJobStatus).mockResolvedValueOnce({
      parentId: 'flow_parent_test',
      projectId: 'proj_test',
      profileId: 'prof_1',
      submittedPrompt: 'Prompt',
      promptHash: 'hash',
      promptSource: 'USER',
      state: 'COMPLETED',
      stateRevision: 5,
      activeSegmentIndex: 1,
      totalSegments: 2,
      estimatedCredits: 80,
      completedGenerations: 2,
      finalOutputReady: true,
      timestamps: {
        createdAt: '2026-08-21T00:00:00Z',
        updatedAt: '2026-08-21T00:00:00Z',
      },
    });

    await useFlowJobStore.getState().pollJobStatus('proj_test', 'flow_parent_test');

    const state = useFlowJobStore.getState();
    expect(state.activeJob?.state).toBe('COMPLETED');
    expect(state.activeJob?.completedGenerations).toBe(2);
    expect(state.activeJob?.finalOutputReady).toBe(true);
  });
});
