import { create } from 'zustand';
import {
  flowApi,
  FlowJobSnapshot,
  FlowProfileInfo,
  GeminiCredentialStatus,
  PromptSource,
} from '../lib/ipc';

interface FlowJobStoreState {
  profiles: FlowProfileInfo[];
  selectedProfileId: string | null;
  geminiStatus: GeminiCredentialStatus | null;
  activeJob: FlowJobSnapshot | null;
  isStarting: boolean;
  isLoadingProfiles: boolean;
  error: string | null;

  loadProfiles: () => Promise<void>;
  createProfile: (profileId: string, name: string) => Promise<void>;
  selectProfile: (profileId: string) => void;
  openProfileBrowser: (profileId: string) => Promise<string>;
  closeProfileBrowser: (profileId: string) => Promise<void>;
  refreshProfileStatus: (profileId: string) => Promise<string>;
  loadGeminiStatus: () => Promise<void>;
  testGeminiApiKey: () => Promise<GeminiCredentialStatus>;
  startFlowJob: (
    projectId: string,
    profileId: string,
    prompt: string,
    promptSource: PromptSource,
    sourceMediaId: string
  ) => Promise<void>;
  pollJobStatus: (projectId: string, parentId: string) => Promise<void>;
  clearError: () => void;
}

export const useFlowJobStore = create<FlowJobStoreState>((set, get) => ({
  profiles: [],
  selectedProfileId: null,
  geminiStatus: null,
  activeJob: null,
  isStarting: false,
  isLoadingProfiles: false,
  error: null,

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
          ? { ...p, isLocked: true, browserSessionOpen: true }
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
          ? { ...p, isLocked: false, browserSessionOpen: false }
          : p
      ),
    }));
    await get().loadProfiles();
  },

  refreshProfileStatus: async (profileId: string) => {
    const status = await flowApi.refreshProfileStatus(profileId);
    set((state) => ({
      profiles: state.profiles.map((p) =>
        p.profileId === profileId ? { ...p, status } : p
      ),
    }));
    return status;
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

  startFlowJob: async (
    projectId,
    profileId,
    prompt,
    promptSource,
    sourceMediaId
  ) => {
    set({ isStarting: true, error: null });
    try {
      const job = await flowApi.startFlowGeneration(
        projectId,
        profileId,
        prompt,
        promptSource,
        sourceMediaId
      );
      set({ activeJob: job, isStarting: false });
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

  pollJobStatus: async (projectId: string, parentId: string) => {
    try {
      const updated = await flowApi.getFlowJobStatus(projectId, parentId);
      set({ activeJob: updated });
    } catch (err) {
      // Ignore polling hiccups
    }
  },

  clearError: () => set({ error: null }),
}));
