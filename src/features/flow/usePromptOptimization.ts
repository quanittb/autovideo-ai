import { useState, useCallback, useRef } from 'react';
import { flowApi, PromptSource, OptimizePromptResponse } from '../../lib/ipc';

export interface UsePromptOptimizationOptions {
  initialPrompt?: string;
  taskType?: string;
  videoDurationSec?: number;
  fps?: number;
  resolution?: [number, number];
}

export interface PromptHistoryEntry {
  prompt: string;
  source: PromptSource;
}

export function usePromptOptimization(options: UsePromptOptimizationOptions = {}) {
  const {
    initialPrompt = 'Turn character into a cyber hero in futuristic city with neon lights',
    taskType = 'VIDEO_TRANSFORMATION',
    videoDurationSec,
    fps,
    resolution,
  } = options;

  const [prompt, setPrompt] = useState<string>(initialPrompt);
  const [promptSource, setPromptSource] = useState<PromptSource>('USER');
  const [history, setHistory] = useState<PromptHistoryEntry[]>([]);
  const [isOptimizing, setIsOptimizing] = useState<boolean>(false);
  const [optimizationError, setOptimizationError] = useState<string | null>(null);

  // Track active in-flight request to handle stale responses and race conditions
  const inFlightSnapshotRef = useRef<{ prompt: string; reqId: number } | null>(null);
  const reqCounterRef = useRef<number>(0);

  const handlePromptChange = useCallback((newText: string) => {
    setPrompt(newText);
    setOptimizationError(null);

    // If text was optimized by Gemini, user manual edit transitions provenance
    if (promptSource === 'GEMINI_OPTIMIZED') {
      setPromptSource('GEMINI_OPTIMIZED_THEN_EDITED');
    }
  }, [promptSource]);

  const handleGenPrompt = useCallback(async () => {
    const rawPrompt = prompt.trim();
    if (!rawPrompt || isOptimizing) return;

    setIsOptimizing(true);
    setOptimizationError(null);

    reqCounterRef.current += 1;
    const currentReqId = reqCounterRef.current;
    const snapshotPrompt = prompt;
    inFlightSnapshotRef.current = { prompt: snapshotPrompt, reqId: currentReqId };

    try {
      const resp: OptimizePromptResponse = await flowApi.optimizePrompt({
        prompt: snapshotPrompt,
        taskType,
        videoDurationSec,
        fps,
        resolution,
      });

      // Stale response check: Only apply if user hasn't modified text while in-flight
      // and this is still the active request ID
      if (
        inFlightSnapshotRef.current?.reqId === currentReqId &&
        prompt === snapshotPrompt
      ) {
        // Save previous prompt and source to undo stack
        setHistory((prev) => [...prev, { prompt: snapshotPrompt, source: promptSource }]);
        setPrompt(resp.optimizedPrompt);
        setPromptSource('GEMINI_OPTIMIZED');
      }
    } catch (err: any) {
      // Failure leaves prompt text untouched and shows error alert
      setOptimizationError(
        typeof err === 'string' ? err : err?.message || 'PROMPT_OPTIMIZATION_FAILED'
      );
    } finally {
      if (inFlightSnapshotRef.current?.reqId === currentReqId) {
        setIsOptimizing(false);
        inFlightSnapshotRef.current = null;
      }
    }
  }, [prompt, promptSource, isOptimizing, taskType, videoDurationSec, fps, resolution]);

  const handleUndo = useCallback(() => {
    if (history.length === 0) return;

    setHistory((prev) => {
      const next = [...prev];
      const previousEntry = next.pop();
      if (previousEntry) {
        setPrompt(previousEntry.prompt);
        setPromptSource(previousEntry.source);
      }
      return next;
    });
    setOptimizationError(null);
  }, [history]);

  return {
    prompt,
    promptSource,
    isOptimizing,
    optimizationError,
    canUndo: history.length > 0,
    handlePromptChange,
    handleGenPrompt,
    handleUndo,
    setPrompt,
    setPromptSource,
  };
}
