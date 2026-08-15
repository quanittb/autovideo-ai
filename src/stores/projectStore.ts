import { create } from 'zustand';
import { Project, ProjectSummary, TransformationRequest, SceneInfo } from '../types/contracts';
import { invokeCommand } from '../lib/ipc';

interface ProjectState {
  activeProject: Project | null;
  projects: ProjectSummary[];
  isLoading: boolean;
  error: string | null;

  fetchProjects: () => Promise<void>;
  createNewProject: (name: string) => Promise<Project>;
  loadProject: (id: string) => Promise<Project>;
  saveProject: (project: Project) => Promise<void>;
  deleteProject: (id: string) => Promise<void>;
  importMediaToProject: (projectId: string, filePath: string) => Promise<Project>;
  setActiveProject: (project: Project | null) => void;
  setProjects: (projects: ProjectSummary[]) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;
  updateTransformationRequest: (partial: Partial<TransformationRequest>) => void;
  selectScene: (sceneId: string) => void;
}

const defaultTransformationRequest: TransformationRequest = {
  category: 'character',
  detectedCharacter: 'Fox',
  originalCharacter: 'Fox',
  replacementCharacter: 'White Rabbit',
  prompt: 'A cute white rabbit wearing a warm knitted scarf in autumn',
  negativePrompt: undefined,
  preservation: {
    preserveMotion: true,
    preserveCamera: true,
    preserveComposition: true,
    preserveOriginalAudio: true,
  },
  seed: 42,
};

const defaultScenes: SceneInfo[] = [
  {
    id: 'scene-1',
    index: 1,
    name: 'Woodland Overview',
    startTimeFormatted: '00:00',
    endTimeFormatted: '00:24',
    startFrame: 0,
    endFrame: 720,
    thumbnailEmoji: '🌲',
    status: 'ready',
  },
  {
    id: 'scene-2',
    index: 2,
    name: 'Fox Subject Close-up',
    startTimeFormatted: '00:24',
    endTimeFormatted: '00:48',
    startFrame: 720,
    endFrame: 1440,
    thumbnailEmoji: '🦊',
    status: 'ready',
  },
  {
    id: 'scene-3',
    index: 3,
    name: 'Snow Clearing Run',
    startTimeFormatted: '00:48',
    endTimeFormatted: '01:02',
    startFrame: 1440,
    endFrame: 1860,
    thumbnailEmoji: '❄️',
    status: 'ready',
  },
];

export const defaultFoxRabbitProject: Project = {
  schemaVersion: 1,
  id: 'proj-fox-rabbit',
  name: 'Fox to Rabbit',
  createdAt: '1 day ago',
  updatedAt: '1 day ago',
  status: 'READY',
  sourceMedia: {
    mediaId: 'asset-fox-1',
    originalFileName: 'input_video.mp4',
    sourcePath: 'fixtures/videos/input_video.mp4',
    durationMs: 62000,
    width: 1920,
    height: 1080,
    fps: 30,
    fileSizeBytes: 47395840,
    container: 'mp4',
    videoCodec: 'h264',
    audioCodec: 'aac',
    hasAudio: true,
  },
  transformationConfig: defaultTransformationRequest,
  outputs: [],
  scenes: defaultScenes,
  selectedSceneId: 'scene-2',
  qualityMetrics: {
    temporalConsistencyScore: 98.4,
    identityPreservationScore: 96.2,
    audioSyncOffsetMs: 0,
    warnings: ['High-contrast lighting detected in Scene #2; deflicker filter applied.'],
  },
  isFixture: true,
};

export const useProjectStore = create<ProjectState>((set, get) => ({
  activeProject: defaultFoxRabbitProject,
  projects: [],
  isLoading: false,
  error: null,

  fetchProjects: async () => {
    set({ isLoading: true, error: null });
    try {
      const realProjects = await invokeCommand<ProjectSummary[]>('list_projects');
      set({ projects: realProjects, isLoading: false });
    } catch (err: any) {
      console.warn('Failed to fetch real projects from Tauri, keeping current list:', err);
      set({ error: err?.message || 'Failed to load projects', isLoading: false });
    }
  },

  createNewProject: async (name: string) => {
    set({ isLoading: true, error: null });
    try {
      const created = await invokeCommand<Project>('create_project', { name });
      const enriched: Project = {
        ...created,
        scenes: defaultScenes,
        selectedSceneId: 'scene-1',
      };
      set((state) => ({
        activeProject: enriched,
        projects: [
          {
            id: created.id,
            name: created.name,
            createdAt: created.createdAt,
            updatedAt: created.updatedAt,
            status: created.status,
            thumbnailPath: undefined,
            hasOutput: false,
            isFixture: false,
          },
          ...state.projects,
        ],
        isLoading: false,
      }));
      return enriched;
    } catch (err: any) {
      set({ error: err?.message || 'Failed to create project', isLoading: false });
      throw err;
    }
  },

  loadProject: async (id: string) => {
    set({ isLoading: true, error: null });
    try {
      const loaded = await invokeCommand<Project>('get_project', { id });
      const enriched: Project = {
        ...loaded,
        scenes: loaded.scenes || defaultScenes,
        selectedSceneId: loaded.selectedSceneId || 'scene-1',
      };
      set({ activeProject: enriched, isLoading: false });
      return enriched;
    } catch (err: any) {
      set({ error: err?.message || 'Failed to load project', isLoading: false });
      throw err;
    }
  },

  saveProject: async (project: Project) => {
    try {
      const saved = await invokeCommand<Project>('update_project', { project });
      set((state) => ({
        activeProject: {
          ...project,
          updatedAt: saved.updatedAt,
        },
        projects: state.projects.map((p) =>
          p.id === project.id
            ? { ...p, name: project.name, updatedAt: saved.updatedAt, status: project.status }
            : p
        ),
      }));
    } catch (err: any) {
      set({ error: err?.message || 'Failed to save project' });
    }
  },

  deleteProject: async (id: string) => {
    set({ isLoading: true, error: null });
    try {
      await invokeCommand('delete_project', { id });
      set((state) => ({
        projects: state.projects.filter((p) => p.id !== id),
        activeProject: state.activeProject?.id === id ? null : state.activeProject,
        isLoading: false,
      }));
    } catch (err: any) {
      set({ error: err?.message || 'Failed to delete project', isLoading: false });
      throw err;
    }
  },

  importMediaToProject: async (projectId: string, filePath: string) => {
    set({ isLoading: true, error: null });
    try {
      const updated = await invokeCommand<Project>('import_media', { projectId, filePath });
      const enriched: Project = {
        ...updated,
        scenes: updated.scenes || defaultScenes,
        selectedSceneId: updated.selectedSceneId || 'scene-1',
      };
      set((state) => ({
        activeProject: enriched,
        projects: state.projects.map((p) =>
          p.id === projectId
            ? { ...p, status: updated.status, updatedAt: updated.updatedAt }
            : p
        ),
        isLoading: false,
      }));
      return enriched;
    } catch (err: any) {
      set({ error: err?.message || 'Failed to import video file', isLoading: false });
      throw err;
    }
  },

  setActiveProject: (project) => set({ activeProject: project }),
  setProjects: (projects) => set({ projects }),
  setLoading: (loading) => set({ isLoading: loading }),
  setError: (error) => set({ error }),

  updateTransformationRequest: (partial) => {
    const current = get().activeProject;
    if (!current) return;
    const updatedProject: Project = {
      ...current,
      transformationConfig: {
        ...current.transformationConfig,
        ...partial,
      },
    };
    set({ activeProject: updatedProject });
    if (!current.isFixture) {
      get().saveProject(updatedProject);
    }
  },

  selectScene: (sceneId) =>
    set((state) => {
      if (!state.activeProject) return state;
      return {
        activeProject: {
          ...state.activeProject,
          selectedSceneId: sceneId,
        },
      };
    }),
}));
