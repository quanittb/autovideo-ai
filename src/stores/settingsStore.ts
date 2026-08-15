import { create } from 'zustand';
import { HardwareProfile, StoragePaths, ModelDescriptor } from '../types/contracts';

interface SettingsState {
  hardware: HardwareProfile | null;
  storage: StoragePaths | null;
  models: ModelDescriptor[];
  setHardware: (hardware: HardwareProfile) => void;
  setStorage: (storage: StoragePaths) => void;
  setModels: (models: ModelDescriptor[]) => void;
}

export const useSettingsStore = create<SettingsState>((set) => ({
  hardware: null,
  storage: null,
  models: [],

  setHardware: (hardware) => set({ hardware }),
  setStorage: (storage) => set({ storage }),
  setModels: (models) => set({ models }),
}));
