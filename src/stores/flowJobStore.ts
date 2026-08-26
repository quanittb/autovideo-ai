import { create } from 'zustand';
import {
  flowApi,
  FlowGenerationRequest,
  FlowJobSnapshot,
  FlowProfileInfo,
  FlowGenerationPreflight,
  FlowProfileCreditStatus,
  FlowModelCapabilitiesSnapshot,
  FlowCapabilityContext,
  GeminiCredentialStatus,
  PromptSource,
  TransformationIntent,
  IdentityMode,
  TargetFaceSelection,
} from '../lib/ipc';
import { UseFlowOutputResult } from '../types/contracts';

interface FlowJobStoreState {
  profiles: FlowProfileInfo[];
  selectedProfileId: string | null;
  geminiStatus: GeminiCredentialStatus | null;
  activeJob: FlowJobSnapshot | null;
  jobs: FlowJobSnapshot[];
  preflight: FlowGenerationPreflight | null;
  isStarting: boolean;
  isPreflighting: boolean;
  isLoadingProfiles: boolean;
  error: string | null;

  creditStatusByProfile: Record<string, FlowProfileCreditStatus>;
  isRefreshingCreditByProfile: Record<string, boolean>;
  capabilitiesByProfileAndContext: Record<string, FlowModelCapabilitiesSnapshot>;

  loadProfiles: () => Promise<void>;
  createProfile: (profileId: string, name: string) => Promise<void>;
  selectProfile: (profileId: string) => void;
  openProfileBrowser: (profileId: string) => Promise<string>;
  closeProfileBrowser: (profileId: string) => Promise<void>;
  verifyProfileLogin: (profileId: string) => Promise<string>;
  refreshProfileStatus: (profileId: string) => Promise<string>;
  refreshCreditBalance: (profileId: string) => Promise<FlowProfileCreditStatus>;
  fetchModelCapabilities: (
    profileId: string,
    context?: FlowCapabilityContext
  ) => Promise<FlowModelCapabilitiesSnapshot>;
  loadGeminiStatus: () => Promise<void>;
  testGeminiApiKey: () => Promise<GeminiCredentialStatus>;
  loadFlowJobs: (projectId: string) => Promise<void>;
  preflightFlowJob: (request: FlowGenerationRequest) => Promise<FlowGenerationPreflight>;
  clearPreflight: () => void;
  invalidatePreflight: () => void;
  startFlowJob: (
    projectId: string,
    profileId: string,
    prompt: string,
    promptSource: PromptSource,
    sourceMediaId: string,
    options?: {
      transformationIntent?: TransformationIntent;
      identityMode?: IdentityMode;
      targetFace?: TargetFaceSelection;
      maxCredits?: number;
      preserveOriginalAudio?: boolean;
      requestedConfig?: import('../types/contracts').FlowRequestedGenerationConfig;
      configurationFingerprint?: string;
      preflightId?: string;
    }
  ) => Promise<void>;
  cancelFlowJob: (projectId: string, parentId: string) => Promise<void>;
  pollJobStatus: (projectId: string, parentId: string) => Promise<void>;
  openOutputArtifact: (projectId: string, parentId: string) => Promise<string>;
  revealOutputInFolder: (projectId: string, parentId: string) => Promise<string>;
  useOutputInProject: (projectId: string, parentId: string) => Promise<UseFlowOutputResult>;
  clearError: () => void;
}

export const useFlowJobStore = create<FlowJobStoreState>((set, get) => ({
  profiles: [],
  selectedProfileId: null,
  geminiStatus: null,
  activeJob: null,
  jobs: [],
  preflight: null,
  isStarting: false,
  isPreflighting: false,
  isLoadingProfiles: false,
  error: null,
  creditStatusByProfile: {},
  isRefreshingCreditByProfile: {},
  capabilitiesByProfileAndContext: {},

  loadProfiles: async () => {
    set({ isLoadingProfiles: true, error: null });
    try {
      const list = await flowApi.listProfiles();
      set({
        profiles: list,
        selectedProfileId:
          list.length > 0 && !get().selectedProfileId
            ? list[0].profileId
            : get().selectedProfileId,
        isLoadingProfiles: false,
      });
    } catch (err: any) {
      set({
        error:
          typeof err === 'string'
            ? err
            : err?.message || 'Failed to load profiles',
        isLoadingProfiles: false,
      });
    }
  },

  createProfile: async (profileId: string, name: string) => {
    try {
      const created = await flowApi.createProfile(profileId, name);
      set((state) => ({
        profiles: [
          ...state.profiles.filter((p) => p.profileId !== created.profileId),
          created,
        ],
        selectedProfileId: created.profileId,
      }));
    } catch (err: any) {
      set({
        error:
          typeof err === 'string'
            ? err
            : err?.message || 'Failed to create profile',
      });
      throw err;
    }
  },

  selectProfile: (profileId: string) => {
    set({ selectedProfileId: profileId });
  },

  openProfileBrowser: async (profileId: string) => {
    const res = await flowApi.openProfileBrowser(profileId);
    set((state) => ({
      profiles: state.profiles.map((p) =>
        p.profileId === profileId
          ? {
              ...p,
              isLocked: true,
              manualBrowserOpen: true,
              browserSessionOpen: true,
              status: 'UNKNOWN',
            }
          : p
      ),
    }));
    return res;
  },

  closeProfileBrowser: async (profileId: string) => {
    await flowApi.closeProfileBrowser(profileId);
    set((state) => ({
      profiles: state.profiles.map((p) =>
        p.profileId === profileId
          ? {
              ...p,
              isLocked: false,
              manualBrowserOpen: false,
              browserSessionOpen: false,
              status: 'UNKNOWN',
            }
          : p
      ),
    }));
    await get().loadProfiles();
  },

  verifyProfileLogin: async (profileId: string) => {
    const status = await flowApi.verifyProfileLogin(profileId);
    set((state) => ({
      profiles: state.profiles.map((p) =>
        p.profileId === profileId ? { ...p, status } : p
      ),
    }));
    return status;
  },

  refreshProfileStatus: async (profileId: string) => {
    return await get().verifyProfileLogin(profileId);
  },

  refreshCreditBalance: async (profileId: string) => {
    if (get().isRefreshingCreditByProfile[profileId]) {
      const existing = get().creditStatusByProfile[profileId];
      if (existing) return existing;
    }

    set((state) => ({
      isRefreshingCreditByProfile: {
        ...state.isRefreshingCreditByProfile,
        [profileId]: true,
      },
    }));

    try {
      const status = await flowApi.refreshCreditBalance(profileId);
      set((state) => ({
        creditStatusByProfile: {
          ...state.creditStatusByProfile,
          [profileId]: status,
        },
        isRefreshingCreditByProfile: {
          ...state.isRefreshingCreditByProfile,
          [profileId]: false,
        },
      }));
      return status;
    } catch (err: any) {
      const fallback: FlowProfileCreditStatus = {
        profileId,
        balance: undefined,
        status: 'ERROR',
        checkedAt: new Date().toISOString(),
        source: 'UNKNOWN',
      };
      set((state) => ({
        creditStatusByProfile: {
          ...state.creditStatusByProfile,
          [profileId]: fallback,
        },
        isRefreshingCreditByProfile: {
          ...state.isRefreshingCreditByProfile,
          [profileId]: false,
        },
      }));
      return fallback;
    }
  },

  fetchModelCapabilities: async (profileId: string, context: FlowCapabilityContext = 'UPLOADED_VIDEO_EDIT') => {
    const key = `${profileId}:${context}`;
    try {
      const snapshot = await flowApi.getModelCapabilities(profileId, context);
      set((state) => ({
        capabilitiesByProfileAndContext: {
          ...state.capabilitiesByProfileAndContext,
          [key]: snapshot,
        },
      }));
      return snapshot;
    } catch (err: any) {
      const emptySnapshot: FlowModelCapabilitiesSnapshot = {
        profileId,
        operationContext: context,
        models: [],
        source: 'UNKNOWN',
        observedAt: new Date().toISOString(),
        status: 'ERROR',
      };
      return emptySnapshot;
    }
  },

  loadGeminiStatus: async () => {
    try {
      const status = await flowApi.getGeminiStatus();
      set({ geminiStatus: status });
    } catch (err) {
      // Ignored non-blocking
    }
  },

  testGeminiApiKey: async () => {
    const status = await flowApi.testGeminiApiKey();
    set({ geminiStatus: status });
    return status;
  },

  loadFlowJobs: async (projectId: string) => {
    if (!projectId) return;
    try {
      const jobs = await flowApi.listFlowJobs(projectId);
      set({ jobs });
    } catch (err) {
      // Ignored non-blocking
    }
  },

  preflightFlowJob: async (request: FlowGenerationRequest) => {
    set({ isPreflighting: true, error: null });
    try {
      const preflight = await flowApi.preflightGeneration(request);
      set({ preflight, isPreflighting: false });
      return preflight;
    } catch (err: any) {
      const errMsg =
        typeof err === 'string'
          ? err
          : err?.message || 'Preflight inspection failed';
      set({ error: errMsg, isPreflighting: false });
      throw err;
    }
  },

  clearPreflight: () => {
    set({ preflight: null });
  },

  invalidatePreflight: () => {
    set({ preflight: null });
  },

  startFlowJob: async (
    projectId,
    profileId,
    prompt,
    promptSource,
    sourceMediaId,
    options
  ) => {
    set({ isStarting: true, error: null });
    try {
      const req: FlowGenerationRequest = {
        projectId,
        profileId,
        prompt,
        promptSource,
        sourceMediaId,
        transformationIntent: options?.transformationIntent,
        identityMode: options?.identityMode,
        targetFace: options?.targetFace,
        maxCredits: options?.maxCredits,
        preserveOriginalAudio: options?.preserveOriginalAudio,
        requestedConfig: options?.requestedConfig,
        configurationFingerprint: options?.configurationFingerprint,
        preflightId: options?.preflightId,
      };
      const job = await flowApi.startGeneration(req);
      set((state) => ({
        activeJob: job,
        jobs: [job, ...state.jobs.filter((j) => j.parentId !== job.parentId)],
        isStarting: false,
      }));
    } catch (err: any) {
      set({
        error:
          typeof err === 'string'
            ? err
            : err?.message || 'Failed to start Flow job',
        isStarting: false,
      });
    }
  },

  cancelFlowJob: async (projectId: string, parentId: string) => {
    try {
      const cancelled = await flowApi.cancelFlowJob(projectId, parentId);
      set((state) => ({
        activeJob: cancelled,
        jobs: state.jobs.map((j) => (j.parentId === parentId ? cancelled : j)),
      }));
    } catch (err: any) {
      set({
        error:
          typeof err === 'string'
            ? err
            : err?.message || 'Failed to cancel Flow job',
      });
    }
  },

  pollJobStatus: async (projectId: string, parentId: string) => {
    try {
      const updated = await flowApi.getFlowJobStatus(projectId, parentId);
      set((state) => ({
        activeJob: updated,
        jobs: state.jobs.map((j) => (j.parentId === parentId ? updated : j)),
      }));
    } catch (err) {
      // Ignore polling hiccups
    }
  },

  openOutputArtifact: async (projectId: string, parentId: string) => {
    return await flowApi.openOutputArtifact(projectId, parentId);
  },

  revealOutputInFolder: async (projectId: string, parentId: string) => {
    return await flowApi.revealOutputInFolder(projectId, parentId);
  },

  useOutputInProject: async (projectId: string, parentId: string): Promise<UseFlowOutputResult> => {
    return await flowApi.useOutputInProject(projectId, parentId);
  },

  clearError: () => set({ error: null }),
}));
