import React from 'react';
import { Layout } from './Layout';
import { HomeView } from '../features/home/HomeView';
import { ProjectWorkspace } from '../features/workspace/ProjectWorkspace';
import { StepUpload } from '../features/project/StepUpload';
import { JobMonitor } from '../features/processing/JobMonitor';
import { ResultView } from '../features/result/ResultView';
import { StepExport } from '../features/export/StepExport';
import { SettingsView } from '../features/settings/SettingsView';
import { ModelsView } from '../features/models/ModelsView';
import { HistoryView } from '../features/history/HistoryView';
import { MediaVerificationRunner } from '../features/media/MediaVerificationRunner';
import { GenerativeStudioView } from '../features/generation/GenerativeStudioView';
import { useUiStore } from '../stores/uiStore';

export const App: React.FC = () => {
  const { activeTab, currentStep } = useUiStore();

  const renderFeatureContent = () => {
    switch (activeTab) {
      case 'home':
        return <HomeView />;
      case 'generation':
        return <GenerativeStudioView />;
      case 'verification':
        return <MediaVerificationRunner />;
      case 'settings':
        return <SettingsView />;
      case 'models':
        return <ModelsView />;
      case 'history':
        return <HistoryView />;
      case 'jobs':
        return <JobMonitor />;
      case 'workspace':
      case 'projects':
        switch (currentStep) {
          case 'upload':
            return <StepUpload />;
          case 'transform':
            return <ProjectWorkspace />;
          case 'processing':
            return <JobMonitor />;
          case 'result':
            return <ResultView />;
          case 'export':
            return <StepExport />;
          default:
            return <ProjectWorkspace />;
        }
      default:
        return <HomeView />;
    }
  };

  return <Layout>{renderFeatureContent()}</Layout>;
};

export default App;
