import { create } from 'zustand';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import {
  Job,
  Artifact,
  JobCreatedEvent,
  JobQueuedEvent,
  JobStartedEvent,
  JobStageStartedEvent,
  JobStageProgressEvent,
  JobStageCompletedEvent,
  JobStageCancelledEvent,
  JobProgressEvent,
  JobLogEvent,
  JobArtifactEvent,
  JobCompletedEvent,
  JobFailedEvent,
  JobCancelRequestedEvent,
  JobCancelledEvent,
  JobRetryingEvent,
  JobInterruptedEvent,
  AiJobConfig,
  AiFrameProgressEvent,
  AiReconstructionProgressEvent,
} from '../types/contracts';
import { jobApi, aiApi } from '../lib/ipc';

interface JobStoreState {
  jobs: Job[];
  activeJob: Job | null;
  selectedJobId: string | null;
  jobLogs: Record<string, string[]>;
  jobArtifacts: Record<string, Artifact[]>;
  preflightReport: import('../types/contracts').AiJobPreflightReport | null;
  isLoading: boolean;
  error: string | null;

  fetchJobs: (projectId?: string) => Promise<void>;
  createJob: (projectId: string, jobType?: string, inputFiles?: string[]) => Promise<Job>;
  createAiJob: (projectId: string, inputFiles: string[], aiConfig: AiJobConfig) => Promise<Job>;
  runPreflight: (sourcePath: string, aiConfig: AiJobConfig) => Promise<import('../types/contracts').AiJobPreflightReport>;
  startJob: (jobId: string) => Promise<Job>;
  cancelJob: (jobId: string) => Promise<Job>;
  retryJob: (jobId: string) => Promise<Job>;
  deleteJob: (jobId: string) => Promise<void>;
  fetchJobLogs: (jobId: string) => Promise<string[]>;
  fetchJobArtifacts: (jobId: string) => Promise<Artifact[]>;
  selectJob: (jobId: string | null) => void;
  setActiveJob: (job: Job | null) => void;
  updateJobInStore: (job: Job) => void;
  updateStageProgressInStore: (jobId: string, stageId: string, stageProgress: number, overallProgress: number) => void;
  appendJobLog: (jobId: string, logLine: string) => void;
  upsertArtifact: (jobId: string, artifact: Artifact) => void;
  initEventListeners: () => Promise<UnlistenFn>;
}

let activeUnlistenGlobal: (() => void) | null = null;

export const useJobStore = create<JobStoreState>((set, get) => ({
  jobs: [],
  activeJob: null,
  selectedJobId: null,
  jobLogs: {},
  jobArtifacts: {},
  preflightReport: null,
  isLoading: false,
  error: null,

  fetchJobs: async (projectId?: string) => {
    set({ isLoading: true, error: null });
    try {
      const jobs = await jobApi.listJobs(projectId);
      set({ jobs, isLoading: false });
      if (jobs.length > 0 && !get().selectedJobId) {
        set({ selectedJobId: jobs[0].id, activeJob: jobs[0] });
      }
    } catch (err: any) {
      set({ error: err?.message || 'Failed to fetch jobs', isLoading: false });
    }
  },

  createJob: async (projectId: string, jobType?: string, inputFiles?: string[]) => {
    set({ isLoading: true, error: null });
    try {
      const created = await jobApi.createPipelineJob(projectId, jobType, inputFiles);
      set((state) => ({
        jobs: [created, ...state.jobs.filter((j) => j.id !== created.id)],
        activeJob: created,
        selectedJobId: created.id,
        isLoading: false,
      }));
      return created;
    } catch (err: any) {
      set({ error: err?.message || 'Failed to create job', isLoading: false });
      throw err;
    }
  },

  createAiJob: async (projectId: string, inputFiles: string[], aiConfig: AiJobConfig) => {
    set({ isLoading: true, error: null });
    try {
      const created = await aiApi.createProductionAiJob(projectId, inputFiles, aiConfig);
      set((state) => ({
        jobs: [created, ...state.jobs.filter((j) => j.id !== created.id)],
        activeJob: created,
        selectedJobId: created.id,
        isLoading: false,
      }));
      return created;
    } catch (err: any) {
      set({ error: err?.message || 'Failed to create production AI job', isLoading: false });
      throw err;
    }
  },

  runPreflight: async (sourcePath: string, aiConfig: AiJobConfig) => {
    set({ isLoading: true, error: null });
    try {
      const report = await aiApi.validateJobPreflight(sourcePath, aiConfig);
      set({ preflightReport: report, isLoading: false });
      return report;
    } catch (err: any) {
      set({ error: err?.message || 'Failed to execute preflight validation', isLoading: false });
      throw err;
    }
  },

  startJob: async (jobId: string) => {
    try {
      const started = await jobApi.startJob(jobId);
      get().updateJobInStore(started);
      return started;
    } catch (err: any) {
      set({ error: err?.message || 'Failed to start job' });
      throw err;
    }
  },

  cancelJob: async (jobId: string) => {
    try {
      const cancelled = await jobApi.cancelJob(jobId);
      get().updateJobInStore(cancelled);
      return cancelled;
    } catch (err: any) {
      set({ error: err?.message || 'Failed to cancel job' });
      throw err;
    }
  },

  retryJob: async (jobId: string) => {
    try {
      const retried = await jobApi.retryJob(jobId);
      get().updateJobInStore(retried);
      return retried;
    } catch (err: any) {
      set({ error: err?.message || 'Failed to retry job' });
      throw err;
    }
  },

  deleteJob: async (jobId: string) => {
    try {
      await jobApi.deleteJob(jobId);
      set((state) => {
        const remaining = state.jobs.filter((j) => j.id !== jobId);
        const newSelected = state.selectedJobId === jobId ? (remaining[0]?.id || null) : state.selectedJobId;
        const newActive = remaining.find((j) => j.id === newSelected) || null;
        return {
          jobs: remaining,
          selectedJobId: newSelected,
          activeJob: newActive,
        };
      });
    } catch (err: any) {
      set({ error: err?.message || 'Failed to delete job' });
      throw err;
    }
  },

  fetchJobLogs: async (jobId: string) => {
    try {
      const logs = await jobApi.getJobLogs(jobId);
      set((state) => ({
        jobLogs: { ...state.jobLogs, [jobId]: logs },
      }));
      return logs;
    } catch (err) {
      return [];
    }
  },

  fetchJobArtifacts: async (jobId: string) => {
    try {
      const artifacts = await jobApi.getJobArtifacts(jobId);
      set((state) => ({
        jobArtifacts: { ...state.jobArtifacts, [jobId]: artifacts },
      }));
      return artifacts;
    } catch (err) {
      return [];
    }
  },

  selectJob: (jobId: string | null) => {
    const job = get().jobs.find((j) => j.id === jobId) || null;
    set({ selectedJobId: jobId, activeJob: job });
    if (jobId) {
      get().fetchJobLogs(jobId);
      get().fetchJobArtifacts(jobId);
    }
  },

  setActiveJob: (job) => set({ activeJob: job }),

  updateJobInStore: (job: Job) => {
    set((state) => {
      const exists = state.jobs.some((j) => j.id === job.id);
      const updatedList = exists
        ? state.jobs.map((j) => (j.id === job.id ? job : j))
        : [job, ...state.jobs];
      return {
        jobs: updatedList,
        activeJob: state.activeJob?.id === job.id ? job : state.activeJob,
      };
    });
  },

  updateStageProgressInStore: (jobId: string, stageId: string, stageProgress: number, overallProgress: number) => {
    set((state) => {
      const updateJob = (j: Job): Job => {
        if (j.id !== jobId) return j;
        const stages = j.stages.map((s) => (s.id === stageId ? { ...s, progress: stageProgress, status: 'RUNNING' as const } : s));
        return {
          ...j,
          progress: overallProgress,
          stages,
        };
      };

      return {
        jobs: state.jobs.map(updateJob),
        activeJob: state.activeJob?.id === jobId ? updateJob(state.activeJob) : state.activeJob,
      };
    });
  },

  appendJobLog: (jobId: string, logLine: string) => {
    set((state) => {
      const existing = state.jobLogs[jobId] || [];
      return {
        jobLogs: {
          ...state.jobLogs,
          [jobId]: [...existing, logLine],
        },
      };
    });
  },

  upsertArtifact: (jobId: string, artifact: Artifact) => {
    set((state) => {
      const existing = state.jobArtifacts[jobId] || [];
      const updated = existing.some((a) => a.id === artifact.id)
        ? existing.map((a) => (a.id === artifact.id ? artifact : a))
        : [...existing, artifact];
      return {
        jobArtifacts: {
          ...state.jobArtifacts,
          [jobId]: updated,
        },
      };
    });
  },

  initEventListeners: async () => {
    if (activeUnlistenGlobal) {
      activeUnlistenGlobal();
      activeUnlistenGlobal = null;
    }

    const unlistenCreated = await listen<JobCreatedEvent>('job:created', (event) => {
      get().updateJobInStore(event.payload.job);
    });

    const unlistenQueued = await listen<JobQueuedEvent>('job:queued', (event) => {
      get().updateJobInStore(event.payload.job);
    });

    const unlistenStarted = await listen<JobStartedEvent>('job:started', (event) => {
      get().updateJobInStore(event.payload.job);
    });

    const unlistenStageStarted = await listen<JobStageStartedEvent>('job:stage_started', (event) => {
      set((state) => {
        const updateJob = (j: Job): Job => {
          if (j.id !== event.payload.jobId) return j;
          const stages = j.stages.map((s, idx) =>
            idx === event.payload.stageIndex
              ? { ...s, status: event.payload.stageStatus, progress: 0 }
              : s
          );
          return {
            ...j,
            currentStage: event.payload.stageId,
            currentStageIndex: event.payload.stageIndex,
            stages,
          };
        };

        return {
          jobs: state.jobs.map(updateJob),
          activeJob: state.activeJob?.id === event.payload.jobId ? updateJob(state.activeJob) : state.activeJob,
        };
      });
    });

    const unlistenStageProgress = await listen<JobStageProgressEvent>('job:stage_progress', (event) => {
      get().updateStageProgressInStore(
        event.payload.jobId,
        event.payload.stageId,
        event.payload.stageProgress,
        event.payload.overallProgress
      );
    });

    const unlistenStageCompleted = await listen<JobStageCompletedEvent>('job:stage_completed', (event) => {
      set((state) => {
        const updateJob = (j: Job): Job => {
          if (j.id !== event.payload.jobId) return j;
          const stages = j.stages.map((s, idx) =>
            idx === event.payload.stageIndex
              ? { ...s, status: event.payload.stageStatus, progress: 100, message: event.payload.message }
              : s
          );
          return { ...j, stages };
        };

        return {
          jobs: state.jobs.map(updateJob),
          activeJob: state.activeJob?.id === event.payload.jobId ? updateJob(state.activeJob) : state.activeJob,
        };
      });
    });

    const unlistenStageCancelled = await listen<JobStageCancelledEvent>('job:stage_cancelled', (event) => {
      set((state) => {
        const updateJob = (j: Job): Job => {
          if (j.id !== event.payload.jobId) return j;
          const stages = j.stages.map((s, idx) =>
            idx === event.payload.stageIndex
              ? { ...s, status: 'CANCELLED' as const, message: 'Stage cancelled by user' }
              : s
          );
          return { ...j, stages };
        };

        return {
          jobs: state.jobs.map(updateJob),
          activeJob: state.activeJob?.id === event.payload.jobId ? updateJob(state.activeJob) : state.activeJob,
        };
      });
    });

    const unlistenProgress = await listen<JobProgressEvent>('job:progress', (event) => {
      get().updateJobInStore(event.payload.job);
    });

    const unlistenLog = await listen<JobLogEvent>('job:log', (event) => {
      const formatted = `[${event.payload.timestamp}] [${event.payload.level}] [${event.payload.stageId}] ${event.payload.message}`;
      get().appendJobLog(event.payload.jobId, formatted);
    });

    const unlistenArtifact = await listen<JobArtifactEvent>('job:artifact', (event) => {
      get().upsertArtifact(event.payload.jobId, event.payload.artifact);
    });

    const unlistenCompleted = await listen<JobCompletedEvent>('job:completed', (event) => {
      get().updateJobInStore(event.payload.job);
    });

    const unlistenFailed = await listen<JobFailedEvent>('job:failed', (event) => {
      get().updateJobInStore(event.payload.job);
    });

    const unlistenCancelRequested = await listen<JobCancelRequestedEvent>('job:cancel_requested', (event) => {
      get().updateJobInStore(event.payload.job);
    });

    const unlistenCancelled = await listen<JobCancelledEvent>('job:cancelled', (event) => {
      get().updateJobInStore(event.payload.job);
    });

    const unlistenRetrying = await listen<JobRetryingEvent>('job:retrying', (event) => {
      get().updateJobInStore(event.payload.job);
    });

    const unlistenInterrupted = await listen<JobInterruptedEvent>('job:interrupted', (event) => {
      get().updateJobInStore(event.payload.job);
    });

    const unlistenAiFrameProgress = await listen<AiFrameProgressEvent>('ai:frame_progress', (event) => {
      set((state) => {
        const updateJob = (j: Job): Job => {
          if (j.id !== event.payload.jobId) return j;
          return { ...j, aiMetrics: event.payload.metrics };
        };

        return {
          jobs: state.jobs.map(updateJob),
          activeJob: state.activeJob?.id === event.payload.jobId ? updateJob(state.activeJob) : state.activeJob,
        };
      });
    });

    const unlistenAiReconstructionProgress = await listen<AiReconstructionProgressEvent>(
      'ai:reconstruction_progress',
      (event) => {
        set((state) => {
          const updateJob = (j: Job): Job => {
            if (j.id !== event.payload.jobId) return j;
            return { ...j, progress: event.payload.overallProgress, message: event.payload.message };
          };

          return {
            jobs: state.jobs.map(updateJob),
            activeJob: state.activeJob?.id === event.payload.jobId ? updateJob(state.activeJob) : state.activeJob,
          };
        });
      }
    );

    const cleanup = () => {
      unlistenCreated();
      unlistenQueued();
      unlistenStarted();
      unlistenStageStarted();
      unlistenStageProgress();
      unlistenStageCompleted();
      unlistenStageCancelled();
      unlistenProgress();
      unlistenLog();
      unlistenArtifact();
      unlistenCompleted();
      unlistenFailed();
      unlistenCancelRequested();
      unlistenCancelled();
      unlistenRetrying();
      unlistenInterrupted();
      unlistenAiFrameProgress();
      unlistenAiReconstructionProgress();
      activeUnlistenGlobal = null;
    };

    activeUnlistenGlobal = cleanup;
    return cleanup;
  },
}));

