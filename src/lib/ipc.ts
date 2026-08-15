import { invoke } from '@tauri-apps/api/core';
import { HardwareProfile, ModelDescriptor, Project, ProjectSummary, StoragePaths } from '../types/contracts';

export interface AppInfo {
  name: string;
  version: string;
  environment: string;
}

export async function invokeCommand<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return await invoke<T>(cmd, args);
}

export const api = {
  getAppInfo: async (): Promise<AppInfo> => {
    try {
      return await invoke<AppInfo>('get_app_info');
    } catch {
      return { name: 'AutoVideo AI', version: '0.1.0', environment: 'web-fallback' };
    }
  },

  getHardwareProfile: async (): Promise<HardwareProfile> => {
    try {
      return await invoke<HardwareProfile>('get_hardware_profile');
    } catch {
      return {
        os: 'windows',
        arch: 'x86_64',
        cpuCores: 8,
        totalMemoryBytes: 16 * 1024 * 1024 * 1024,
        gpuName: 'DirectX 12 Compatible GPU',
        vramBytes: 8 * 1024 * 1024 * 1024,
        isDirectmlSupported: true,
        isMetalSupported: false,
        isCudaSupported: false,
      };
    }
  },

  getStoragePaths: async (): Promise<StoragePaths> => {
    try {
      return await invoke<StoragePaths>('get_storage_paths');
    } catch {
      return {
        appDataDir: './.autovideo_data',
        projectsDir: './.autovideo_data/projects',
        modelsDir: './.autovideo_data/models',
        cacheDir: './.autovideo_data/cache',
        logsDir: './.autovideo_data/logs',
        tempDir: './.autovideo_data/temp',
      };
    }
  },

  listProjects: async (): Promise<ProjectSummary[]> => {
    try {
      return await invoke<ProjectSummary[]>('list_projects');
    } catch {
      return [];
    }
  },

  getProject: async (id: string): Promise<Project> => {
    return await invoke<Project>('get_project', { id });
  },

  createProject: async (name: string): Promise<Project> => {
    return await invoke<Project>('create_project', { name });
  },

  updateProject: async (project: Project): Promise<Project> => {
    return await invoke<Project>('update_project', { project });
  },

  deleteProject: async (id: string): Promise<void> => {
    return await invoke<void>('delete_project', { id });
  },

  listModels: async (): Promise<ModelDescriptor[]> => {
    try {
      return await invoke<ModelDescriptor[]>('list_models');
    } catch {
      return [];
    }
  },

  getAiStatus: async (): Promise<string> => {
    return await invoke<string>('get_ai_status');
  },
};
