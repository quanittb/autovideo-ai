import React from 'react';
import { ArrowLeft, ArrowRight, Cpu } from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { useProjectStore } from '../../stores/projectStore';
import { useHardwareProfile } from '../../hooks/useHardwareProfile';
import { WizardStep } from '../../types';

export const Header: React.FC = () => {
  const { activeTab, currentStep, setCurrentStep, setActiveTab } = useUiStore();
  const { activeProject } = useProjectStore();
  const { hardware } = useHardwareProfile();

  const steps: { id: WizardStep; number: number; label: string }[] = [
    { id: 'upload', number: 1, label: 'Upload' },
    { id: 'transform', number: 2, label: 'Transform' },
    { id: 'processing', number: 3, label: 'Processing' },
    { id: 'result', number: 4, label: 'Result' },
    { id: 'export', number: 5, label: 'Export' },
  ];

  const handleNext = () => {
    if (currentStep === 'upload') setCurrentStep('transform');
    else if (currentStep === 'transform') setCurrentStep('processing');
    else if (currentStep === 'processing') setCurrentStep('result');
    else if (currentStep === 'result') setCurrentStep('export');
  };

  const handlePrev = () => {
    if (currentStep === 'transform') setCurrentStep('upload');
    else if (currentStep === 'processing') setCurrentStep('transform');
    else if (currentStep === 'result') setCurrentStep('processing');
    else if (currentStep === 'export') setCurrentStep('result');
    else setActiveTab('home');
  };

  const isWorkflow = activeTab === 'workspace' || activeTab === 'projects';

  return (
    <header className="h-16 border-b border-slate-800/80 bg-slate-950/70 backdrop-blur-md px-6 flex items-center justify-between shrink-0 select-none">
      {/* Left: Back / Title */}
      <div className="flex items-center gap-3">
        {isWorkflow && (
          <button
            onClick={handlePrev}
            className="p-1.5 rounded-lg text-slate-400 hover:text-slate-100 hover:bg-slate-800 transition-colors"
            aria-label="Back"
          >
            <ArrowLeft className="w-4 h-4" />
          </button>
        )}

        <div className="flex flex-col">
          <span className="text-xs font-bold text-slate-100 truncate max-w-xs">
            {isWorkflow ? (activeProject?.name || 'Fox to Rabbit Transformation') : 'AutoVideo AI Studio'}
          </span>
          <span className="text-[10px] text-slate-500 font-mono">
            {isWorkflow ? 'MVP Character Pipeline' : 'Local-First AI Video Engine'}
          </span>
        </div>
      </div>

      {/* Center: Step Wizard Tracker if in workflow mode */}
      {isWorkflow && (
        <div className="hidden md:flex items-center gap-1.5">
          {steps.map((step, idx) => {
            const stepOrder: Record<WizardStep, number> = {
              upload: 1,
              transform: 2,
              processing: 3,
              result: 4,
              export: 5,
            };
            const currentNum = stepOrder[currentStep];
            const isActive = currentStep === step.id;
            const isPassed = currentNum > step.number;

            return (
              <React.Fragment key={step.id}>
                {idx > 0 && (
                  <div
                    className={`w-4 h-0.5 rounded-full ${
                      isPassed ? 'bg-indigo-600' : 'bg-slate-800'
                    }`}
                  />
                )}
                <button
                  onClick={() => setCurrentStep(step.id)}
                  className={`flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium transition-all ${
                    isActive
                      ? 'bg-indigo-600/20 text-indigo-300 border border-indigo-500/40'
                      : isPassed
                      ? 'text-indigo-400 hover:bg-slate-900'
                      : 'text-slate-500 hover:text-slate-300'
                  }`}
                >
                  <span
                    className={`w-4 h-4 rounded-full flex items-center justify-center text-[9px] font-bold ${
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
      )}

      {/* Right: GPU / Hardware Telemetry Status Pill */}
      <div className="flex items-center gap-3">
        <div className="hidden sm:flex items-center gap-2 px-3 py-1.5 rounded-xl bg-slate-900 border border-slate-800 text-[11px] font-mono text-slate-300">
          <Cpu className="w-3.5 h-3.5 text-indigo-400 shrink-0" />
          <span className="truncate">{hardware?.gpuName || 'DirectML GPU'}</span>
          <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
        </div>

        {isWorkflow && currentStep !== 'export' && (
          <button
            onClick={handleNext}
            className="px-3.5 py-1.5 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold shadow-md shadow-indigo-900/30 transition-all flex items-center gap-1"
          >
            <span>Next</span>
            <ArrowRight className="w-3.5 h-3.5" />
          </button>
        )}
      </div>
    </header>
  );
};
