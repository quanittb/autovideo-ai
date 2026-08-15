import React from 'react';
import { ArrowLeft, ArrowRight, Sparkles } from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { useProjectStore, defaultFoxRabbitProject } from '../../stores/projectStore';
import { WizardStep } from '../../types';

export const Header: React.FC = () => {
  const { activeTab, currentStep, setCurrentStep, setActiveTab } = useUiStore();
  const { activeProject, setActiveProject } = useProjectStore();

  const steps: { id: WizardStep; number: number; label: string }[] = [
    { id: 'upload', number: 1, label: 'Upload' },
    { id: 'transform', number: 2, label: 'Transform' },
    { id: 'preview', number: 3, label: 'Preview' },
    { id: 'export', number: 4, label: 'Export' },
  ];

  const handleNext = () => {
    if (currentStep === 'upload') setCurrentStep('transform');
    else if (currentStep === 'transform') setCurrentStep('preview');
    else if (currentStep === 'preview') setCurrentStep('export');
  };

  const handlePrev = () => {
    if (currentStep === 'transform') setCurrentStep('upload');
    else if (currentStep === 'preview') setCurrentStep('transform');
    else if (currentStep === 'export') setCurrentStep('preview');
    else setActiveTab('home');
  };

  const handleStartNew = () => {
    setActiveProject({
      ...defaultFoxRabbitProject,
      id: `proj-${Date.now()}`,
      name: 'Untitled Transformation',
      createdAt: 'Just now',
      updatedAt: 'Just now',
    });
    setCurrentStep('upload');
    setActiveTab('projects');
  };

  const isWizardView = activeTab === 'projects' || activeTab === 'tools';

  return (
    <header className="h-16 border-b border-slate-800/80 bg-slate-950/60 backdrop-blur-md px-6 flex items-center justify-between shrink-0">
      {isWizardView ? (
        <>
          <div className="flex items-center gap-3">
            <button
              onClick={handlePrev}
              className="p-1.5 rounded-lg text-slate-400 hover:text-slate-100 hover:bg-slate-800 transition-colors"
            >
              <ArrowLeft className="w-5 h-5" />
            </button>
            <span className="font-semibold text-slate-200 text-sm">
              {activeProject?.name || 'New Project'}
            </span>
          </div>

          <div className="flex items-center gap-2">
            {steps.map((step, idx) => {
              const stepIndexMap: Record<WizardStep, number> = { upload: 1, transform: 2, preview: 3, export: 4 };
              const currentNum = stepIndexMap[currentStep];
              const isActive = currentStep === step.id;
              const isPassed = currentNum > step.number;

              return (
                <React.Fragment key={step.id}>
                  {idx > 0 && <div className={`w-6 h-0.5 rounded-full ${isPassed ? 'bg-indigo-600' : 'bg-slate-800'}`} />}
                  <button
                    onClick={() => setCurrentStep(step.id)}
                    className={`flex items-center gap-2 px-3 py-1.5 rounded-full text-xs font-medium transition-all ${
                      isActive
                        ? 'bg-indigo-600/20 text-indigo-300 border border-indigo-500/40'
                        : isPassed
                        ? 'text-indigo-400 hover:bg-slate-900'
                        : 'text-slate-500 hover:text-slate-300'
                    }`}
                  >
                    <span
                      className={`w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold ${
                        isActive
                          ? 'bg-indigo-600 text-white'
                          : isPassed
                          ? 'bg-indigo-950 border border-indigo-600 text-indigo-400'
                          : 'bg-slate-800 text-slate-400'
                      }`}
                    >
                      {step.number}
                    </span>
                    <span>{step.label}</span>
                  </button>
                </React.Fragment>
              );
            })}
          </div>

          <div className="flex items-center gap-3">
            {currentStep !== 'export' && (
              <button
                onClick={handleNext}
                className="px-4 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold shadow-md shadow-indigo-900/30 transition-all flex items-center gap-1.5"
              >
                <span>Next Step</span>
                <ArrowRight className="w-3.5 h-3.5" />
              </button>
            )}
          </div>
        </>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <h2 className="text-base font-semibold text-slate-200">
              AutoVideo AI Studio
            </h2>
            <span className="text-xs text-slate-400">Next-gen prompt-driven video transformation</span>
          </div>

          <div className="flex items-center gap-3">
            <button
              onClick={handleStartNew}
              className="px-4 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold shadow-md shadow-indigo-900/30 transition-all flex items-center gap-1.5"
            >
              <Sparkles className="w-3.5 h-3.5" />
              <span>+ New Project</span>
            </button>
          </div>
        </>
      )}
    </header>
  );
};
