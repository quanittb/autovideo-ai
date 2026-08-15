import { create } from 'zustand';
import { Job, JobProgress, JobState } from '../types/contracts';

interface JobStoreState {
  activeJob: Job | null;
  setActiveJob: (job: Job | null) => void;
  updateJobState: (state: JobState) => void;
  updateJobProgress: (progress: JobProgress) => void;
}

export const useJobStore = create<JobStoreState>((set) => ({
  activeJob: null,

  setActiveJob: (job) => set({ activeJob: job }),
  updateJobState: (state) =>
    set((s) => (s.activeJob ? { activeJob: { ...s.activeJob, state } } : s)),
  updateJobProgress: (progress) =>
    set((s) => (s.activeJob ? { activeJob: { ...s.activeJob, progress } } : s)),
}));
