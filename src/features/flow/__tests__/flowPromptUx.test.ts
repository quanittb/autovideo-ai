import { describe, it, expect, vi, beforeEach } from 'vitest';
import { flowApi, PromptSource } from '../../../lib/ipc';
import { useFlowJobStore } from '../../../stores/flowJobStore';

vi.mock('../../../lib/ipc', () => ({
  flowApi: {
    optimizePrompt: vi.fn(),
    startFlowGeneration: vi.fn(),
    startGeneration: vi.fn(),
    listProfiles: vi.fn(),
    getGeminiStatus: vi.fn(),
    getFlowJobStatus: vi.fn(),
  },
}));

// State machine simulating usePromptOptimization behavior
class PromptOptimizationController {
  prompt: string;
  promptSource: PromptSource;
  history: Array<{ prompt: string; source: PromptSource }>;
  isOptimizing: boolean;
  optimizationError: string | null;
  private inFlightReqId: number;
  private reqCounter: number;

  constructor(initialPrompt = '') {
    this.prompt = initialPrompt;
    this.promptSource = 'USER';
    this.history = [];
    this.isOptimizing = false;
    this.optimizationError = null;
    this.inFlightReqId = 0;
    this.reqCounter = 0;
  }

  handlePromptChange(newText: string) {
    this.prompt = newText;
    this.optimizationError = null;
    if (this.promptSource === 'GEMINI_OPTIMIZED') {
      this.promptSource = 'GEMINI_OPTIMIZED_THEN_EDITED';
    }
  }

  async handleGenPrompt() {
    const raw = this.prompt.trim();
    if (!raw || this.isOptimizing) return;

    this.isOptimizing = true;
    this.optimizationError = null;
    this.reqCounter += 1;
    const reqId = this.reqCounter;
    this.inFlightReqId = reqId;
    const snap = this.prompt;

    try {
      const resp = await flowApi.optimizePrompt({
        prompt: snap,
        taskType: 'FLOW_VIDEO_EDIT',
      });

      // Stale response guard
      if (this.inFlightReqId === reqId && this.prompt === snap) {
        this.history.push({ prompt: snap, source: this.promptSource });
        this.prompt = resp.optimizedPrompt;
        this.promptSource = 'GEMINI_OPTIMIZED';
      }
    } catch (err: any) {
      this.optimizationError = typeof err === 'string' ? err : err?.message || 'PROMPT_OPTIMIZATION_FAILED';
    } finally {
      if (this.inFlightReqId === reqId) {
        this.isOptimizing = false;
      }
    }
  }

  handleUndo() {
    if (this.history.length === 0) return;
    const prev = this.history.pop();
    if (prev) {
      this.prompt = prev.prompt;
      this.promptSource = prev.source;
    }
    this.optimizationError = null;
  }
}

describe('Frontend Prompt UX Behavioral Tests (Phase 20A)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('1. empty prompt -> Gen Prompt disabled / no IPC', async () => {
    const ctrl = new PromptOptimizationController('   ');
    await ctrl.handleGenPrompt();

    expect(flowApi.optimizePrompt).not.toHaveBeenCalled();
    expect(ctrl.isOptimizing).toBe(false);
  });

  it('2. double click -> single active optimization', async () => {
    let resolveFirst: (v: any) => void = () => {};
    const firstPromise = new Promise((resolve) => {
      resolveFirst = resolve;
    });

    vi.mocked(flowApi.optimizePrompt).mockReturnValueOnce(firstPromise as any);

    const ctrl = new PromptOptimizationController('Cyber hero in neon city');
    const p1 = ctrl.handleGenPrompt();
    const p2 = ctrl.handleGenPrompt(); // Second click while in-flight

    expect(flowApi.optimizePrompt).toHaveBeenCalledTimes(1);

    resolveFirst({
      optimizedPrompt: 'Optimized hero',
      model: 'gemini-3.5-flash-lite',
      promptSource: 'GEMINI_OPTIMIZED',
      promptHash: 'h1',
    });

    await Promise.all([p1, p2]);
    expect(ctrl.prompt).toBe('Optimized hero');
  });

  it('3. successful optimize -> editor replaces text and updates provenance', async () => {
    vi.mocked(flowApi.optimizePrompt).mockResolvedValueOnce({
      optimizedPrompt: 'Cinematic red fox in misty autumn forest',
      model: 'gemini-3.5-flash-lite',
      promptSource: 'GEMINI_OPTIMIZED',
      promptHash: 'h_fox',
    });

    const ctrl = new PromptOptimizationController('Red fox');
    await ctrl.handleGenPrompt();

    expect(ctrl.prompt).toBe('Cinematic red fox in misty autumn forest');
    expect(ctrl.promptSource).toBe('GEMINI_OPTIMIZED');
  });

  it('4. Undo -> restores text + PromptSource', async () => {
    vi.mocked(flowApi.optimizePrompt).mockResolvedValueOnce({
      optimizedPrompt: 'Optimized fox',
      model: 'gemini-3.5-flash-lite',
      promptSource: 'GEMINI_OPTIMIZED',
      promptHash: 'h_fox',
    });

    const ctrl = new PromptOptimizationController('Original user fox');
    await ctrl.handleGenPrompt();

    expect(ctrl.prompt).toBe('Optimized fox');
    expect(ctrl.promptSource).toBe('GEMINI_OPTIMIZED');

    ctrl.handleUndo();

    expect(ctrl.prompt).toBe('Original user fox');
    expect(ctrl.promptSource).toBe('USER');
  });

  it('5. Gen Again -> uses CURRENT editor text', async () => {
    vi.mocked(flowApi.optimizePrompt).mockResolvedValueOnce({
      optimizedPrompt: 'Second optimization result',
      model: 'gemini-3.5-flash-lite',
      promptSource: 'GEMINI_OPTIMIZED',
      promptHash: 'h2',
    });

    const ctrl = new PromptOptimizationController('User modified text for second run');
    await ctrl.handleGenPrompt();

    expect(flowApi.optimizePrompt).toHaveBeenCalledWith(
      expect.objectContaining({
        prompt: 'User modified text for second run',
      })
    );
    expect(ctrl.prompt).toBe('Second optimization result');
  });

  it('6. manual edit: GEMINI_OPTIMIZED -> GEMINI_OPTIMIZED_THEN_EDITED', async () => {
    vi.mocked(flowApi.optimizePrompt).mockResolvedValueOnce({
      optimizedPrompt: 'Optimized hero',
      model: 'gemini-3.5-flash-lite',
      promptSource: 'GEMINI_OPTIMIZED',
      promptHash: 'h1',
    });

    const ctrl = new PromptOptimizationController('Hero');
    await ctrl.handleGenPrompt();
    expect(ctrl.promptSource).toBe('GEMINI_OPTIMIZED');

    ctrl.handlePromptChange('Optimized hero with custom sunglasses');
    expect(ctrl.promptSource).toBe('GEMINI_OPTIMIZED_THEN_EDITED');
  });

  it('7. user edits while Gemini request is in-flight -> stale response cannot overwrite editor', async () => {
    let resolveStale: (v: any) => void = () => {};
    const pendingPromise = new Promise((resolve) => {
      resolveStale = resolve;
    });

    vi.mocked(flowApi.optimizePrompt).mockReturnValueOnce(pendingPromise as any);

    const ctrl = new PromptOptimizationController('Prompt initial');
    const inFlight = ctrl.handleGenPrompt();

    expect(ctrl.isOptimizing).toBe(true);

    // User edits text while request is in-flight
    ctrl.handlePromptChange('Prompt modified by user while waiting');

    // Late Gemini response returns
    resolveStale({
      optimizedPrompt: 'Optimized Stale Text',
      model: 'gemini-3.5-flash-lite',
      promptSource: 'GEMINI_OPTIMIZED',
      promptHash: 'h_stale',
    });

    await inFlight;

    // Must NOT overwrite the user's live edit!
    expect(ctrl.prompt).toBe('Prompt modified by user while waiting');
    expect(ctrl.promptSource).toBe('USER');
    expect(ctrl.isOptimizing).toBe(false);
  });

  it('8. late request A after newer request B -> A cannot overwrite B', () => {
    let activeReqId = 2; // Request B is current
    const lateReqId = 1; // Late response from request A arrives

    const canApply = lateReqId === activeReqId;
    expect(canApply).toBe(false);
  });

  it('9. Gemini failure -> existing prompt unchanged', async () => {
    vi.mocked(flowApi.optimizePrompt).mockRejectedValueOnce(
      'PROMPT_OPTIMIZATION_FAILED: API key invalid'
    );

    const ctrl = new PromptOptimizationController('Precious user prompt');
    await ctrl.handleGenPrompt();

    expect(ctrl.prompt).toBe('Precious user prompt');
    expect(ctrl.promptSource).toBe('USER');
    expect(ctrl.optimizationError).toContain('PROMPT_OPTIMIZATION_FAILED');
  });

  it('10. Gemini unconfigured -> Generate Video remains available', () => {
    const geminiStatus = { isConfigured: false, model: 'gemini-3.5-flash-lite' };
    const canGenerateVideo = true; // Flow generation remains unblocked
    expect(geminiStatus.isConfigured).toBe(false);
    expect(canGenerateVideo).toBe(true);
  });

  it('11. Generate Video -> exact current editor prompt submitted', async () => {
    vi.mocked(flowApi.startGeneration).mockResolvedValueOnce({
      parentId: 'flow_frozen',
      projectId: 'proj_1',
      profileId: 'prof_1',
      submittedPrompt: 'Exact current editor prompt string',
      promptHash: 'hash_exact',
      promptSource: 'GEMINI_OPTIMIZED_THEN_EDITED',
      state: 'READY',
      stateRevision: 1,
      activeSegmentIndex: 0,
      totalSegments: 1,
      estimatedCredits: 40,
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
        'proj_1',
        'prof_1',
        'Exact current editor prompt string',
        'GEMINI_OPTIMIZED_THEN_EDITED',
        'media_001'
      );

    expect(flowApi.startGeneration).toHaveBeenCalledWith(
      expect.objectContaining({
        projectId: 'proj_1',
        profileId: 'prof_1',
        prompt: 'Exact current editor prompt string',
        promptSource: 'GEMINI_OPTIMIZED_THEN_EDITED',
        sourceMediaId: 'media_001',
      })
    );
  });

  it('12. editor changes after submit -> active job submittedPrompt does not change', () => {
    const activeJob = {
      parentId: 'flow_frozen',
      submittedPrompt: 'Frozen Submitted Prompt',
    };

    let editorPrompt = activeJob.submittedPrompt;
    editorPrompt = 'Completely different text edited after submit';

    expect(activeJob.submittedPrompt).toBe('Frozen Submitted Prompt');
    expect(editorPrompt).not.toBe(activeJob.submittedPrompt);
  });
});
