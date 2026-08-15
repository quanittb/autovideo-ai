import React from 'react';
import { Layout } from './Layout';
import { HomeView } from '../features/home/HomeView';
import { StepUpload } from '../features/project/StepUpload';
import { StepTransform } from '../features/transform/StepTransform';
import { StepExport } from '../features/export/StepExport';
import { ResultView } from '../features/result/ResultView';
import { SettingsView } from '../features/settings/SettingsView';
import { ModelsView } from '../features/models/ModelsView';
import { useUiStore } from '../stores/uiStore';

export const App: React.FC = () => {
  const { activeTab, currentStep } = useUiStore();

  const renderFeatureContent = () => {
    switch (activeTab) {
      case 'home':
        return <HomeView />;
      case 'settings':
        return <SettingsView />;
      case 'models':
        return <ModelsView />;
      case 'projects':
      case 'tools':
        switch (currentStep) {
          case 'upload':
            return <StepUpload />;
          case 'transform':
            return <StepTransform />;
          case 'preview':
            return <ResultView />;
          case 'export':
            return <StepExport />;
          default:
            return <StepUpload />;
        }
      default:
        return <HomeView />;
    }
  };

  return <Layout>{renderFeatureContent()}</Layout>;
};

export default App;
