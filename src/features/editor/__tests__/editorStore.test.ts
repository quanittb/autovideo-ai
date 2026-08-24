import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useEditorStore } from '../stores/editorStore';
import { applyPendingSeekIfExists } from '../stores/editorStore';
import { editorApi } from '../../../lib/ipc';

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: vi.fn((filePath: string) => `http://asset.localhost/${filePath}`),
}));

vi.mock('../../../lib/ipc', () => ({
  editorApi: {
    resolveProjectMedia: vi.fn(),
    persistEditorState: vi.fn(),
  },
}));

describe('truthful editorStore media loading states', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useEditorStore.getState().reset();
    useEditorStore.getState().registerVideoElement(null);
  });

  it('loadProjectMedia sets MEDIA_URL_READY and does not prematurely set READY/PLAYABLE', async () => {
    vi.mocked(editorApi.resolveProjectMedia).mockResolvedValueOnce({
      mediaId: 'media_123',
      originalFileName: 'flow_acceptance_01.mp4',
      sourcePath: 'D:/projects/proj-1/media/flow_acceptance_01.mp4',
      durationSeconds: 9.682,
      durationMs: 9682,
      width: 1080,
      height: 1920,
      fps: 30.0,
      fileSizeBytes: 6743281,
      container: 'mp4',
      videoCodec: 'h264',
      audioCodec: 'aac',
      hasAudio: true,
      framesDir: undefined,
      frameFiles: [],
      audioPath: undefined,
      isCacheAvailable: false,
    });

    await useEditorStore.getState().loadProjectMedia('proj-1');

    const state = useEditorStore.getState();
    expect(state.loadStatus).toBe('MEDIA_URL_READY');
    expect(state.mediaUrl).toBe('http://asset.localhost/D:/projects/proj-1/media/flow_acceptance_01.mp4');
    expect(state.playback.duration).toBe(9.682);
    expect(state.errorMessage).toBeNull();
  });

  it('setMediaPlayable transitions state from MEDIA_URL_READY to READY', () => {
    useEditorStore.setState({ loadStatus: 'MEDIA_URL_READY' });
    useEditorStore.getState().setMediaPlayable(9.682);

    const state = useEditorStore.getState();
    expect(state.loadStatus).toBe('READY');
    expect(state.playback.duration).toBe(9.682);
    expect(state.errorMessage).toBeNull();
  });

  it('setMediaError transitions state to ERROR with sanitized message', () => {
    useEditorStore.setState({ loadStatus: 'MEDIA_URL_READY' });
    useEditorStore.getState().setMediaError('MEDIA_DECODE_ERROR: The video playback could not be decoded by the desktop preview runtime.');

    const state = useEditorStore.getState();
    expect(state.loadStatus).toBe('ERROR');
    expect(state.errorMessage).toContain('MEDIA_DECODE_ERROR');
    expect(state.playback.isPlaying).toBe(false);
  });

  it('loadProjectMedia handles IPC resolution errors gracefully', async () => {
    vi.mocked(editorApi.resolveProjectMedia).mockRejectedValueOnce(
      new Error('PROJECT_HAS_NO_SOURCE_MEDIA: Project does not have imported source media')
    );

    await useEditorStore.getState().loadProjectMedia('proj-empty');

    const state = useEditorStore.getState();
    expect(state.loadStatus).toBe('ERROR');
    expect(state.errorMessage).toBe('PROJECT_HAS_NO_SOURCE_MEDIA: Project does not have imported source media');
  });
});

describe('seek synchronization — authoritative video element path', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useEditorStore.getState().reset();
    useEditorStore.getState().registerVideoElement(null);
    // Set a non-zero duration so seek has room to move
    useEditorStore.setState((state) => ({
      playback: { ...state.playback, duration: 10 },
    }));
  });

  it('seek updates Zustand currentTime', () => {
    useEditorStore.getState().seek(5.0);
    expect(useEditorStore.getState().playback.currentTime).toBe(5.0);
  });

  it('seek drives video.currentTime when element is registered with readyState >= 1', () => {
    const fakeVideo = { currentTime: 0, readyState: 2 } as unknown as HTMLVideoElement;
    useEditorStore.getState().registerVideoElement(fakeVideo);
    useEditorStore.getState().seek(3.5);
    expect(fakeVideo.currentTime).toBe(3.5);
    expect(useEditorStore.getState().playback.currentTime).toBe(3.5);
  });

  it('seek stores pendingSeekTime when video element has readyState < 1 (metadata not loaded)', () => {
    const fakeVideo = { currentTime: 0, readyState: 0 } as unknown as HTMLVideoElement;
    useEditorStore.getState().registerVideoElement(fakeVideo);
    useEditorStore.getState().seek(7.0);
    // video.currentTime should NOT be set yet
    expect(fakeVideo.currentTime).toBe(0);
    // applyPendingSeekIfExists simulates what happens when loadedmetadata fires
    applyPendingSeekIfExists();
    expect(fakeVideo.currentTime).toBe(7.0);
    // calling again must be a no-op (pendingSeekTime cleared)
    fakeVideo.currentTime = 0;
    applyPendingSeekIfExists();
    expect(fakeVideo.currentTime).toBe(0);
  });

  it('stepForward routes through seek and drives video.currentTime', () => {
    const fakeVideo = { currentTime: 0, readyState: 2 } as unknown as HTMLVideoElement;
    useEditorStore.getState().registerVideoElement(fakeVideo);
    useEditorStore.setState((state) => ({
      playback: { ...state.playback, currentTime: 3.0, duration: 10 },
    }));
    useEditorStore.getState().stepForward(1.0);
    expect(useEditorStore.getState().playback.currentTime).toBe(4.0);
    expect(fakeVideo.currentTime).toBe(4.0);
  });

  it('stepBackward routes through seek and drives video.currentTime', () => {
    const fakeVideo = { currentTime: 0, readyState: 2 } as unknown as HTMLVideoElement;
    useEditorStore.getState().registerVideoElement(fakeVideo);
    useEditorStore.setState((state) => ({
      playback: { ...state.playback, currentTime: 5.0, duration: 10 },
    }));
    useEditorStore.getState().stepBackward(1.0);
    expect(useEditorStore.getState().playback.currentTime).toBe(4.0);
    expect(fakeVideo.currentTime).toBe(4.0);
  });

  it('seek clamps to [0, duration] and does not allow exceeding duration', () => {
    const fakeVideo = { currentTime: 0, readyState: 2 } as unknown as HTMLVideoElement;
    useEditorStore.getState().registerVideoElement(fakeVideo);
    useEditorStore.getState().seek(999);
    expect(useEditorStore.getState().playback.currentTime).toBe(10);
    expect(fakeVideo.currentTime).toBe(10);
    useEditorStore.getState().seek(-5);
    expect(useEditorStore.getState().playback.currentTime).toBe(0);
    expect(fakeVideo.currentTime).toBe(0);
  });

  it('seek is a no-op for non-finite values', () => {
    const fakeVideo = { currentTime: 0, readyState: 2 } as unknown as HTMLVideoElement;
    useEditorStore.getState().registerVideoElement(fakeVideo);
    useEditorStore.getState().seek(NaN);
    expect(useEditorStore.getState().playback.currentTime).toBe(0);
    expect(fakeVideo.currentTime).toBe(0);
    useEditorStore.getState().seek(Infinity);
    expect(useEditorStore.getState().playback.currentTime).toBe(0);
  });

  it('registerVideoElement null clears element and no-ops seek video drive', () => {
    const fakeVideo = { currentTime: 0, readyState: 2 } as unknown as HTMLVideoElement;
    useEditorStore.getState().registerVideoElement(fakeVideo);
    useEditorStore.getState().registerVideoElement(null);
    useEditorStore.getState().seek(5.0);
    // currentTime must NOT be touched (fakeVideo was unregistered)
    expect(fakeVideo.currentTime).toBe(0);
    expect(useEditorStore.getState().playback.currentTime).toBe(5.0);
  });
});

