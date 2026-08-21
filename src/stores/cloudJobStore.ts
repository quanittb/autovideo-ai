import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  cloudApi,
  type CloudJobEventPayload,
  type CloudJobRequest,
  type CloudSubmissionPreflight,
  type AuthorizedAssetPreview,
} from '../lib/ipc';
import { mergeCloudJobSnapshot } from './cloudJobHelpers';

interface CloudJobState {
  // Canonical job store keyed strictly by internalJobId
  cloudJobsById: Record<string, CloudJobEventPayload>;
  // Secondary lookup map from client jobId -> internalJobId
  clientJobIdToInternalJobId: Record<string, string>;
  selectedInternalJobId: string | null;

  // Preflight state
  preflight: CloudSubmissionPreflight | null;
  isPreflightLoading: boolean;
  preflightError: string | null;

  // Submission & Action state
  isSubmitting: boolean;
  isCancelling: boolean;
  actionError: string | null;

  // Authorized local asset previews
  authorizedSource: AuthorizedAssetPreview | null;
  authorizedArtifact: AuthorizedAssetPreview | null;

  // Subscription state
  isSubscribed: boolean;

  // Store actions
  subscribeToEvents: () => Promise<UnlistenFn>;
  loadProjectCloudJobs: (projectId: string) => Promise<void>;
  runPreflight: (request: CloudJobRequest, maxCost?: number) => Promise<CloudSubmissionPreflight | null>;
  startTransformation: (request: CloudJobRequest, maxCost?: number) => Promise<CloudJobEventPayload | null>;
  cancelJob: (projectId: string, internalJobId: string) => Promise<boolean>;
  authorizeSource: (projectId: string) => Promise<AuthorizedAssetPreview | null>;
  authorizeArtifact: (projectId: string, internalJobId: string) => Promise<AuthorizedAssetPreview | null>;
  revokePreview: (projectId: string) => Promise<void>;
  selectJob: (internalJobId: string | null) => void;
  clearErrors: () => void;
  resetPreflight: () => void;
}

let unlistenCloudJobGlobal: UnlistenFn | null = null;

export const useCloudJobStore = create<CloudJobState>((set, get) => ({
  cloudJobsById: {},
  clientJobIdToInternalJobId: {},
  selectedInternalJobId: null,

  preflight: null,
  isPreflightLoading: false,
  preflightError: null,

  isSubmitting: false,
  isCancelling: false,
  actionError: null,

  authorizedSource: null,
  authorizedArtifact: null,

  isSubscribed: false,

  subscribeToEvents: async () => {
    if (unlistenCloudJobGlobal) {
      return unlistenCloudJobGlobal;
    }

    const unlisten = await listen<CloudJobEventPayload>('cloud-job://updated', (event) => {
      const incoming = event.payload;
      if (!incoming || !incoming.internalJobId) return;

      set((state) => {
        const existing = state.cloudJobsById[incoming.internalJobId];
        const merged = mergeCloudJobSnapshot(existing, incoming);

        // If idempotent (same reference), return unchanged state
        if (existing && merged === existing) {
          return state;
        }

        const newJobs = {
          ...state.cloudJobsById,
          [incoming.internalJobId]: merged,
        };

        const newClientIndex = {
          ...state.clientJobIdToInternalJobId,
          [incoming.jobId]: incoming.internalJobId,
        };

        return {
          cloudJobsById: newJobs,
          clientJobIdToInternalJobId: newClientIndex,
        };
      });
    });

    unlistenCloudJobGlobal = () => {
      unlisten();
      unlistenCloudJobGlobal = null;
      set({ isSubscribed: false });
    };

    set({ isSubscribed: true });
    return unlistenCloudJobGlobal;
  },

  loadProjectCloudJobs: async (projectId: string) => {
    try {
      const jobs = await cloudApi.listCloudJobs(projectId);
      set((state) => {
        const nextJobs = { ...state.cloudJobsById };
        const nextIndex = { ...state.clientJobIdToInternalJobId };

        for (const job of jobs) {
          if (!job.internalJobId) continue;
          const existing = nextJobs[job.internalJobId];
          nextJobs[job.internalJobId] = mergeCloudJobSnapshot(existing, job);
          nextIndex[job.jobId] = job.internalJobId;
        }

        return {
          cloudJobsById: nextJobs,
          clientJobIdToInternalJobId: nextIndex,
        };
      });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      set({ actionError: `Failed to load project cloud jobs: ${msg}` });
    }
  },

  runPreflight: async (request: CloudJobRequest, maxCost?: number) => {
    set({ isPreflightLoading: true, preflightError: null });
    try {
      const preflight = await cloudApi.preflightCloudTransformation(request, maxCost);
      set({ preflight, isPreflightLoading: false, preflightError: null });
      return preflight;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      set({ preflight: null, isPreflightLoading: false, preflightError: msg });
      return null;
    }
  },

  startTransformation: async (request: CloudJobRequest, maxCost?: number) => {
    set({ isSubmitting: true, actionError: null });
    try {
      const payload = await cloudApi.startCloudTransformation(request, maxCost);
      set((state) => {
        const existing = state.cloudJobsById[payload.internalJobId];
        const merged = mergeCloudJobSnapshot(existing, payload);

        return {
          isSubmitting: false,
          actionError: null,
          selectedInternalJobId: payload.internalJobId,
          cloudJobsById: {
            ...state.cloudJobsById,
            [payload.internalJobId]: merged,
          },
          clientJobIdToInternalJobId: {
            ...state.clientJobIdToInternalJobId,
            [payload.jobId]: payload.internalJobId,
          },
        };
      });
      return payload;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      // Invariant: Do NOT fabricate a BLOCKED job in cloudJobsById on submission failure
      set({ isSubmitting: false, actionError: msg });
      return null;
    }
  },

  cancelJob: async (projectId: string, internalJobId: string) => {
    set({ isCancelling: true });
    try {
      await cloudApi.cancelCloudGeneration(internalJobId, projectId, undefined);
      set({ isCancelling: false });
      return true;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      set({ isCancelling: false, actionError: `Cancel failed: ${msg}` });
      return false;
    }
  },

  authorizeSource: async (projectId: string) => {
    try {
      const preview = await cloudApi.authorizePreviewAsset(projectId, 'projectSource');
      set({ authorizedSource: preview });
      return preview;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      set({ actionError: `Source authorization failed: ${msg}` });
      return null;
    }
  },

  authorizeArtifact: async (projectId: string, internalJobId: string) => {
    try {
      const preview = await cloudApi.authorizePreviewAsset(projectId, 'cloudArtifact', internalJobId);
      set({ authorizedArtifact: preview });
      return preview;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      set({ actionError: `Artifact authorization failed: ${msg}` });
      return null;
    }
  },

  revokePreview: async (projectId: string) => {
    try {
      await cloudApi.revokePreviewAsset(projectId, 'projectSource');
      const selected = get().selectedInternalJobId;
      if (selected) {
        await cloudApi.revokePreviewAsset(projectId, 'cloudArtifact', selected);
      }
      set({ authorizedSource: null, authorizedArtifact: null });
    } catch {
      // Ignore cleanup errors
      set({ authorizedSource: null, authorizedArtifact: null });
    }
  },

  selectJob: (internalJobId: string | null) => {
    set({ selectedInternalJobId: internalJobId });
  },

  clearErrors: () => {
    set({ actionError: null, preflightError: null });
  },

  resetPreflight: () => {
    set({ preflight: null, preflightError: null, isPreflightLoading: false });
  },
}));
