import { useEffect } from 'react';
import { Sidebar } from './components/layout/Sidebar';
import { Header } from './components/layout/Header';
import { HomeView } from './components/dashboard/HomeView';
import { StepUpload } from './components/wizard/StepUpload';
import { StepTransform } from './components/wizard/StepTransform';
import { StepExport } from './components/wizard/StepExport';
import { useAppStore } from './store/useAppStore';
import { invoke } from '@tauri-apps/api/core';

function App() {
  const { activeTab, currentStep, setAiStatus } = useAppStore();

  useEffect(() => {
    // Attempt invoking Tauri command to fetch system AI availability status
    invoke('get_ai_status')
      .then((status: any) => {
        setAiStatus(status);
      })
      .catch((err) => {
        console.log('Tauri IPC offline or dev mode fallback:', err);
      });
  }, [setAiStatus]);

  const renderContent = () => {
    if (activeTab === 'home') {
      return <HomeView />;
    }

    if (activeTab === 'projects' || activeTab === 'tools') {
      switch (currentStep) {
        case 'upload':
          return <StepUpload />;
        case 'transform':
          return <StepTransform />;
        case 'preview':
        case 'export':
          return <StepExport />;
        default:
          return <StepUpload />;
      }
    }

    // Other placeholder tabs render HomeView by default
    return <HomeView />;
  };

  return (
    <div className="flex h-screen w-screen bg-slate-950 text-slate-100 font-sans overflow-hidden antialiased select-none">
      {/* Navigation Sidebar */}
      <Sidebar />

      {/* Main Workspace Area */}
      <div className="flex-1 flex flex-col min-w-0 h-full overflow-hidden">
        <Header />
        <main className="flex-1 flex flex-col min-h-0 overflow-hidden">
          {renderContent()}
        </main>
      </div>
    </div>
  );
}

export default App;
