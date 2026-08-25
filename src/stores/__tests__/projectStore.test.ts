import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useProjectStore } from '../projectStore';
import { invokeCommand } from '../../lib/ipc';

vi.mock('../../lib/ipc', () => ({
  invokeCommand: vi.fn(),
  projectApi: {
    createProject: vi.fn(),
    getProject: vi.fn(),
    listProjects: vi.fn(),
    updateProject: vi.fn(),
    deleteProject: vi.fn(),
    importMediaToProject: vi.fn(),
  },
}));

describe('projectStore (FLOW-P2)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useProjectStore.setState({
      activeProject: null,
      projects: [],
      isLoading: false,
      error: null,
    });
  });

  it('1. activeProject is null by default without injecting fake fixtures', () => {
    const state = useProjectStore.getState();
    expect(state.activeProject).toBeNull();
  });

  it('2. createNewProject creates a clean project without fake scenes', async () => {
    const mockProject = {
      schemaVersion: 2,
      id: 'proj_new_123',
      name: 'Clean User Project',
      createdAt: '2026-08-25T12:00:00Z',
      updatedAt: '2026-08-25T12:00:00Z',
      status: 'DRAFT' as const,
      scenes: [],
      outputs: [],
      derivedMediaAssets: [],
      editorState: {
        currentTime: 0.0,
        timelineZoom: 1.0,
        activeMediaId: undefined,
      },
    };

    vi.mocked(invokeCommand).mockResolvedValueOnce(mockProject);

    const created = await useProjectStore.getState().createNewProject('Clean User Project');
    expect(created.id).toBe('proj_new_123');
    expect(created.scenes).toEqual([]);
    expect(created.derivedMediaAssets).toEqual([]);

    const state = useProjectStore.getState();
    expect(state.activeProject?.id).toBe('proj_new_123');
    expect(state.activeProject?.scenes).toHaveLength(0);
  });

  it('3. loadProject supports schema v2 projects with derivedMediaAssets', async () => {
    const mockV2Project = {
      schemaVersion: 2,
      id: 'proj_v2_123',
      name: 'Project with Derived Flow Assets',
      createdAt: '2026-08-25T12:00:00Z',
      updatedAt: '2026-08-25T12:00:00Z',
      status: 'IMPORTED' as const,
      scenes: [],
      outputs: [],
      sourceMedia: {
        mediaId: 'media_orig',
        originalFileName: 'orig.mp4',
        sourcePath: 'media/orig.mp4',
        durationMs: 10000,
        width: 1920,
        height: 1080,
        fps: 30.0,
        fileSizeBytes: 1024000,
        container: 'mp4',
        videoCodec: 'h264',
        hasAudio: true,
      },
      derivedMediaAssets: [
        {
          media: {
            mediaId: 'media_flow_derived_1',
            originalFileName: 'flow_derived.mp4',
            sourcePath: 'media/derived/flow_derived.mp4',
            durationMs: 10000,
            width: 1920,
            height: 1080,
            fps: 30.0,
            fileSizeBytes: 2048000,
            container: 'mp4',
            videoCodec: 'h264',
            hasAudio: true,
          },
          provenance: {
            provider: 'FLOW',
            providerJobId: 'flow_parent_1',
            sourceMediaId: 'media_orig',
            transformationIntent: 'FACE_REPLACE',
            identityMode: 'GENERATED',
            promptHash: 'sha_123',
            createdAt: '2026-08-25T12:05:00Z',
          },
        },
      ],
      editorState: {
        currentTime: 0.0,
        timelineZoom: 1.0,
        activeMediaId: 'media_flow_derived_1',
      },
    };

    vi.mocked(invokeCommand).mockResolvedValueOnce(mockV2Project);

    await useProjectStore.getState().loadProject('proj_v2_123');

    const state = useProjectStore.getState();
    expect(state.activeProject?.id).toBe('proj_v2_123');
    expect(state.activeProject?.derivedMediaAssets).toHaveLength(1);
    expect(state.activeProject?.derivedMediaAssets?.[0].media.mediaId).toBe('media_flow_derived_1');
    expect(state.activeProject?.editorState?.activeMediaId).toBe('media_flow_derived_1');
  });
});
