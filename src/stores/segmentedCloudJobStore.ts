import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  cloudApi,
  type SegmentedCloudJobManifest,
  type SegmentedCloudSubmissionPreflight,
  type CloudJobRequest,
  type AuthorizedAssetPreview,
} from '../lib/ipc';
import { mergeSegmentedCloudJobSnapshot } from './segmentedCloudJobHelpers';

interface SegmentedCloudJobState {
  segmentedJobsById: Record<string, SegmentedCloudJobManifest>;
  selectedParentId: string | null;

  preflight: SegmentedCloudSubmissionPreflight | null;
  isPreflightLoading: boolean;
  preflightError: string | null;

  isSubmitting: boolean;
  isCancelling: boolean;
  isApproving: boolean;
  actionError: string | null;

  authorizedPreview: AuthorizedAssetPreview | null;
  isSubscribed: boolean;

  subscribeToEvents: () => Promise<UnlistenFn>;
  loadProjectSegmentedJobs: (projectId: string) => Promise<void>;
  runPreflight: (
    request: CloudJobRequest,
    maxCost?: number
  ) => Promise<SegmentedCloudSubmissionPreflight | null>;
  startTransformation: (
    request: CloudJobRequest,
    maxCost?: number
  ) => Promise<SegmentedCloudJobManifest | null>;
  cancelJob: (projectId: string, parentId: string) => Promise<boolean>;
  approveBudget: (
    projectId: string,
    parentId: string,
    maxCost: number
  ) => Promise<SegmentedCloudJobManifest | null>;
  authorizePreview: (
    projectId: string,
    parentId: string
  ) => Promise<AuthorizedAssetPreview | null>;
  revokePreview: (projectId: string, parentId: string) => Promise<void>;
  selectJob: (parentId: string | null) => void;
  clearErrors: () => void;
  resetPreflight: () => void;
}

let unlistenSegmentedJobGlobal: UnlistenFn | null = null;

export const useSegmentedCloudJobStore = create<SegmentedCloudJobState>((set) => ({
  segmentedJobsById: {},
  selectedParentId: null,

  preflight: null,
  isPreflightLoading: false,
  preflightError: null,

  isSubmitting: false,
  isCancelling: false,
  isApproving: false,
  actionError: null,

  authorizedPreview: null,
  isSubscribed: false,

  subscribeToEvents: async () => {
    if (unlistenSegmentedJobGlobal) {
      return unlistenSegmentedJobGlobal;
    }

    const unlisten = await listen<SegmentedCloudJobManifest>(
      'segmented-cloud-job://updated',
      (event) => {
        const incoming = event.payload;
        if (!incoming || !incoming.parentId) return;

        set((state) => {
          const existing = state.segmentedJobsById[incoming.parentId];
          const merged = mergeSegmentedCloudJobSnapshot(existing, incoming);

          if (existing && merged === existing) {
            return state;
          }

          return {
            segmentedJobsById: {
              ...state.segmentedJobsById,
              [incoming.parentId]: merged,
            },
          };
        });
      }
    );

    unlistenSegmentedJobGlobal = () => {
      unlisten();
      unlistenSegmentedJobGlobal = null;
      set({ isSubscribed: false });
    };

    set({ isSubscribed: true });
    return unlistenSegmentedJobGlobal;
  },

  loadProjectSegmentedJobs: async (projectId: string) => {
    try {
      const manifests = await cloudApi.listSegmentedJobs(projectId);
      set((state) => {
        const nextJobs = { ...state.segmentedJobsById };
        for (const manifest of manifests) {
          const existing = nextJobs[manifest.parentId];
          nextJobs[manifest.parentId] = mergeSegmentedCloudJobSnapshot(existing, manifest);
        }
        return { segmentedJobsById: nextJobs };
      });
    } catch (err: any) {
      set({ actionError: err?.message || String(err) });
    }
  },

  runPreflight: async (request: CloudJobRequest, maxCost?: number) => {
    set({ isPreflightLoading: true, preflightError: null });
    try {
      const preflight = await cloudApi.preflightSegmentedTransformation(request, maxCost);
      set({ preflight, isPreflightLoading: false });
      return preflight;
    } catch (err: any) {
      const errMsg = err?.message || String(err);
      set({ preflightError: errMsg, isPreflightLoading: false });
      return null;
    }
  },

  startTransformation: async (request: CloudJobRequest, maxCost?: number) => {
    set({ isSubmitting: true, actionError: null });
    try {
      const manifest = await cloudApi.startSegmentedTransformation(request, maxCost);
      set((state) => ({
        isSubmitting: false,
        selectedParentId: manifest.parentId,
        segmentedJobsById: {
          ...state.segmentedJobsById,
          [manifest.parentId]: manifest,
        },
      }));
      return manifest;
    } catch (err: any) {
      const errMsg = err?.message || String(err);
      set({ isSubmitting: false, actionError: errMsg });
      return null;
    }
  },

  cancelJob: async (projectId: string, parentId: string) => {
    set({ isCancelling: true, actionError: null });
    try {
      const manifest = await cloudApi.cancelSegmentedJob(projectId, parentId);
      set((state) => ({
        isCancelling: false,
        segmentedJobsById: {
          ...state.segmentedJobsById,
          [manifest.parentId]: manifest,
        },
      }));
      return true;
    } catch (err: any) {
      const errMsg = err?.message || String(err);
      set({ isCancelling: false, actionError: errMsg });
      return false;
    }
  },

  approveBudget: async (projectId: string, parentId: string, maxCost: number) => {
    set({ isApproving: true, actionError: null });
    try {
      const manifest = await cloudApi.approveSegmentedBudget(projectId, parentId, maxCost);
      set((state) => ({
        isApproving: false,
        segmentedJobsById: {
          ...state.segmentedJobsById,
          [manifest.parentId]: manifest,
        },
      }));
      return manifest;
    } catch (err: any) {
      const errMsg = err?.message || String(err);
      set({ isApproving: false, actionError: errMsg });
      return null;
    }
  },

  authorizePreview: async (projectId: string, parentId: string) => {
    try {
      const preview = await cloudApi.authorizeSegmentedPreviewAsset(projectId, parentId);
      set({ authorizedPreview: preview });
      return preview;
    } catch (err: any) {
      set({ actionError: err?.message || String(err) });
      return null;
    }
  },

  revokePreview: async (projectId: string, parentId: string) => {
    try {
      await cloudApi.revokeSegmentedPreviewAsset(projectId, parentId);
      set({ authorizedPreview: null });
    } catch (err: any) {
      set({ actionError: err?.message || String(err) });
    }
  },

  selectJob: (parentId: string | null) => {
    set({ selectedParentId: parentId });
  },

  clearErrors: () => {
    set({ preflightError: null, actionError: null });
  },

  resetPreflight: () => {
    set({ preflight: null, preflightError: null, isPreflightLoading: false });
  },
}));
