import { create } from 'zustand';
import { Project, ProjectSummary, TransformationRequest } from '../types/contracts';

interface ProjectState {
  activeProject: Project | null;
  projects: ProjectSummary[];
  isLoading: boolean;
  error: string | null;

  setActiveProject: (project: Project | null) => void;
  setProjects: (projects: ProjectSummary[]) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;
  updateTransformationRequest: (partial: Partial<TransformationRequest>) => void;
}

const defaultTransformationRequest: TransformationRequest = {
  category: 'character',
  originalCharacter: 'Fox',
  replacementCharacter: 'Rabbit',
  prompt: 'A cute white rabbit wearing a scarf',
  negativePrompt: undefined,
  seed: 42,
};

export const defaultFoxRabbitProject: Project = {
  id: 'proj-fox-rabbit',
  name: 'Fox to Rabbit',
  createdAt: '1 day ago',
  updatedAt: '1 day ago',
  sourceAsset: {
    id: 'asset-1',
    fileName: 'input_video.mp4',
    filePath: 'fixtures/videos/input_video.mp4',
    metadata: {
      width: 1920,
      height: 1080,
      durationSeconds: 62,
      durationFormatted: '01:02',
      fps: 30,
      totalFrames: 1860,
      codec: 'h264',
      bitrateKbps: 6000,
      fileSizeBytes: 47395840,
      fileSizeFormatted: '45.2 MB',
    },
    isFixture: true,
  },
  transformationRequest: defaultTransformationRequest,
  isFixture: true,
};

export const useProjectStore = create<ProjectState>((set) => ({
  activeProject: defaultFoxRabbitProject,
  projects: [
    {
      id: 'proj-fox-rabbit',
      name: 'Fox to Rabbit',
      createdAt: '1 day ago',
      updatedAt: '1 day ago',
      hasOutput: false,
      isFixture: true,
    },
    {
      id: 'proj-winter',
      name: 'Winter to Autumn',
      createdAt: '2 hours ago',
      updatedAt: '2 hours ago',
      hasOutput: false,
      isFixture: true,
    },
    {
      id: 'proj-beach',
      name: 'Beach Vacation',
      createdAt: '2 days ago',
      updatedAt: '2 days ago',
      hasOutput: false,
      isFixture: true,
    },
    {
      id: 'proj-market',
      name: 'Home to Market',
      createdAt: '3 days ago',
      updatedAt: '3 days ago',
      hasOutput: false,
      isFixture: true,
    },
  ],
  isLoading: false,
  error: null,

  setActiveProject: (project) => set({ activeProject: project }),
  setProjects: (projects) => set({ projects }),
  setLoading: (loading) => set({ isLoading: loading }),
  setError: (error) => set({ error }),
  updateTransformationRequest: (partial) =>
    set((state) => {
      if (!state.activeProject) return state;
      return {
        activeProject: {
          ...state.activeProject,
          transformationRequest: {
            ...state.activeProject.transformationRequest,
            ...partial,
          },
        },
      };
    }),
}));
