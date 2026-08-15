import { create } from 'zustand';
import { NavTab, WizardStep } from '../types';

interface UiState {
  activeTab: NavTab;
  currentStep: WizardStep;
  isSidebarCollapsed: boolean;
  activeModal: string | null;
  setActiveTab: (tab: NavTab) => void;
  setCurrentStep: (step: WizardStep) => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  openModal: (modal: string) => void;
  closeModal: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  activeTab: 'home',
  currentStep: 'upload',
  isSidebarCollapsed: false,
  activeModal: null,

  setActiveTab: (tab) => set({ activeTab: tab }),
  setCurrentStep: (step) => set({ currentStep: step }),
  setSidebarCollapsed: (collapsed) => set({ isSidebarCollapsed: collapsed }),
  openModal: (modal) => set({ activeModal: modal }),
  closeModal: () => set({ activeModal: null }),
}));
