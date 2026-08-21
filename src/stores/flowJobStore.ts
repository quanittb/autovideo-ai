import { create } from 'zustand';
import { flowApi, FlowJobSnapshot, FlowProfileInfo, GeminiStatusResponse, PromptSource } from '../lib/ipc';

interface FlowJobStoreState {
  profiles: FlowProfileInfo[];
  selectedProfileId: string | null;
  geminiStatus: GeminiStatusResponse | null;
  activeJob: FlowJobSnapshot | null;
  isStarting: boolean;
  isLoadingProfiles: boolean;
  error: string | null;

  loadProfiles: () => Promise<void>;
  createProfile: (profileId: string, name: string) => Promise<void>;
  selectProfile: (profileId: string) => void;
  loadGeminiStatus: () => Promise<void>;
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
        selectedProfileId: list.length > 0 && !get().selectedProfileId ? list[0].profileId : get().selectedProfileId,
        isLoadingProfiles: false,
      });
    } catch (err: any) {
      set({
        error: typeof err === 'string' ? err : err?.message || 'Failed to load profiles',
        isLoadingProfiles: false,
      });
    }
  },

  createProfile: async (profileId: string, name: string) => {
    try {
      const created = await flowApi.createProfile(profileId, name);
      set((state) => ({
        profiles: [...state.profiles.filter((p) => p.profileId !== created.profileId), created],
        selectedProfileId: created.profileId,
      }));
    } catch (err: any) {
      set({ error: typeof err === 'string' ? err : err?.message || 'Failed to create profile' });
    }
  },

  selectProfile: (profileId: string) => {
    set({ selectedProfileId: profileId });
  },

  loadGeminiStatus: async () => {
    try {
      const status = await flowApi.getGeminiStatus();
      set({ geminiStatus: status });
    } catch (err) {
      // Ignored non-blocking
    }
  },

  startFlowJob: async (projectId, profileId, prompt, promptSource, sourceMediaId) => {
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
        error: typeof err === 'string' ? err : err?.message || 'Failed to start Flow job',
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
