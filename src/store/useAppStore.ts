import { create } from 'zustand';
import { NavTab, WizardStep, Project, TransformationConfig, AiAvailabilityStatus } from '../types';

interface AppState {
  activeTab: NavTab;
  currentStep: WizardStep;
  activeProject: Project | null;
  projects: Project[];
  aiStatus: AiAvailabilityStatus;
  
  // Actions
  setActiveTab: (tab: NavTab) => void;
  setCurrentStep: (step: WizardStep) => void;
  setActiveProject: (project: Project | null) => void;
  setProjects: (projects: Project[]) => void;
  setAiStatus: (status: AiAvailabilityStatus) => void;
  updateTransformationConfig: (configPartial: Partial<TransformationConfig>) => void;
  startNewProject: () => void;
}

const defaultTransformation: TransformationConfig = {
  category: 'character',
  originalCharacter: 'Fox',
  replacementCharacter: 'Rabbit',
  prompt: 'A cute white rabbit wearing a scarf',
  resolution: '1080p (1920x1080)',
  quality: 'High Quality',
  format: 'MP4',
  fps: 30,
  removeWatermark: true,
};

const sampleProjects: Project[] = [
  {
    id: 'proj-1',
    name: 'Winter to Autumn',
    createdAt: '2 hours ago',
    updatedAt: '2 hours ago',
    sourceVideoPath: 'fixtures/videos/winter_sample.mp4',
    mediaInfo: {
      fileName: 'winter_sample.mp4',
      filePath: 'fixtures/videos/winter_sample.mp4',
      durationSeconds: 62,
      durationFormatted: '01:02',
      resolution: '1920x1080',
      width: 1920,
      height: 1080,
      fileSizeBytes: 47395840,
      fileSizeFormatted: '45.2 MB',
      fps: 30,
      codec: 'h264',
    },
    transformation: {
      ...defaultTransformation,
      category: 'scene',
      prompt: 'Transform snowy winter landscape into warm golden autumn',
    },
    isMockDemo: true,
  },
  {
    id: 'proj-2',
    name: 'Fox to Rabbit',
    createdAt: '1 day ago',
    updatedAt: '1 day ago',
    sourceVideoPath: 'fixtures/videos/input_video.mp4',
    mediaInfo: {
      fileName: 'input_video.mp4',
      filePath: 'fixtures/videos/input_video.mp4',
      durationSeconds: 62,
      durationFormatted: '01:02',
      resolution: '1920x1080',
      width: 1920,
      height: 1080,
      fileSizeBytes: 47395840,
      fileSizeFormatted: '45.2 MB',
      fps: 30,
      codec: 'h264',
    },
    transformation: {
      ...defaultTransformation,
      category: 'character',
      originalCharacter: 'Fox',
      replacementCharacter: 'Rabbit',
      prompt: 'A cute white rabbit wearing a scarf',
    },
    isMockDemo: true,
  },
  {
    id: 'proj-3',
    name: 'Beach Vacation',
    createdAt: '2 days ago',
    updatedAt: '2 days ago',
    transformation: defaultTransformation,
    isMockDemo: true,
  },
  {
    id: 'proj-4',
    name: 'Home to Market',
    createdAt: '3 days ago',
    updatedAt: '3 days ago',
    transformation: defaultTransformation,
    isMockDemo: true,
  },
];

export const useAppStore = create<AppState>((set) => ({
  activeTab: 'home',
  currentStep: 'upload',
  activeProject: sampleProjects[1], // Default to Fox to Rabbit for demonstration
  projects: sampleProjects,
  aiStatus: {
    type: 'MODEL_NOT_AVAILABLE',
    model_name: 'AutoVideo Diffusion v1.0',
    guidance: 'Local model weights not installed. Using verified fixture demo mode.',
  },

  setActiveTab: (tab) => set({ activeTab: tab }),
  setCurrentStep: (step) => set({ currentStep: step }),
  setActiveProject: (project) => set({ activeProject: project }),
  setProjects: (projects) => set({ projects }),
  setAiStatus: (status) => set({ aiStatus: status }),
  updateTransformationConfig: (configPartial) =>
    set((state) => {
      if (!state.activeProject) return state;
      return {
        activeProject: {
          ...state.activeProject,
          transformation: {
            ...state.activeProject.transformation,
            ...configPartial,
          },
        },
      };
    }),
  startNewProject: () => {
    const newProj: Project = {
      id: `proj-${Date.now()}`,
      name: 'Untitled Transformation',
      createdAt: 'Just now',
      updatedAt: 'Just now',
      transformation: defaultTransformation,
      isMockDemo: true,
    };
    set((state) => ({
      projects: [newProj, ...state.projects],
      activeProject: newProj,
      currentStep: 'upload',
      activeTab: 'projects',
    }));
  },
}));
