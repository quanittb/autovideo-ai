import React, { useState } from 'react';
import { 
  Sparkles, 
  Wand2, 
  UserRound, 
  Layers, 
  Palette, 
  Box, 
  Sliders, 
  CheckSquare, 
  Square,
  ArrowRight
} from 'lucide-react';
import { ReferenceUploader } from '../../components/ui/ReferenceUploader';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useJobStore } from '../../stores/jobStore';
import { TransformationRequest } from '../../types/contracts';

interface TransformPanelProps {
  className?: string;
}

export const TransformPanel: React.FC<TransformPanelProps> = ({ className = '' }) => {
  const { activeProject, updateTransformationRequest } = useProjectStore();
  const { setActiveTab } = useUiStore();
  const { createJob, startJob } = useJobStore();

  const [activeCategory, setActiveCategory] = useState<TransformationRequest['category']>('character');

  const categories: { id: TransformationRequest['category']; label: string; icon: React.ReactNode; isMvp?: boolean }[] = [
    { id: 'character', label: 'Character', icon: <UserRound className="w-3.5 h-3.5" />, isMvp: true },
    { id: 'background', label: 'Background', icon: <Layers className="w-3.5 h-3.5" /> },
    { id: 'environment', label: 'Environment', icon: <Wand2 className="w-3.5 h-3.5" /> },
    { id: 'style', label: 'Style', icon: <Palette className="w-3.5 h-3.5" /> },
    { id: 'object', label: 'Object', icon: <Box className="w-3.5 h-3.5" /> },
    { id: 'custom', label: 'Custom', icon: <Sliders className="w-3.5 h-3.5" /> },
  ];

  const transformation = activeProject?.transformationConfig || activeProject?.transformationRequest || {
    category: 'character',
    detectedCharacter: 'Fox',
    originalCharacter: 'Fox',
    replacementCharacter: 'White Rabbit',
    prompt: 'A cute white rabbit wearing a warm knitted scarf',
    preservation: {
      preserveMotion: true,
      preserveCamera: true,
      preserveComposition: true,
      preserveOriginalAudio: true,
    },
  };

  const togglePreservation = (key: keyof typeof transformation.preservation) => {
    updateTransformationRequest({
      preservation: {
        ...transformation.preservation,
        [key]: !transformation.preservation[key],
      },
    });
  };

  const handleGenerate = async () => {
    if (!activeProject) return;
    try {
      const created = await createJob(activeProject.id, 'video_pipeline');
      await startJob(created.id);
      setActiveTab('jobs');
    } catch (err) {
      console.error('Failed to create and start pipeline job:', err);
    }
  };

  return (
    <div className={`flex flex-col h-full bg-slate-900/60 border border-slate-800/80 rounded-2xl p-5 overflow-y-auto space-y-5 ${className}`}>
      {/* Category Tabs */}
      <div className="space-y-1.5">
        <label className="text-xs font-semibold text-slate-300">Transformation Mode</label>
        <div className="grid grid-cols-3 gap-1.5 p-1 rounded-xl bg-slate-950 border border-slate-800">
          {categories.map((cat) => {
            const isActive = activeCategory === cat.id;
            return (
              <button
                key={cat.id}
                onClick={() => {
                  setActiveCategory(cat.id);
                  updateTransformationRequest({ category: cat.id });
                }}
                className={`flex items-center justify-center gap-1.5 py-2 px-1 rounded-lg text-[11px] font-semibold transition-all ${
                  isActive
                    ? 'bg-indigo-600 text-white shadow-md shadow-indigo-900/30'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900/60'
                }`}
              >
                {cat.icon}
                <span>{cat.label}</span>
                {cat.isMvp && <span className="text-[9px] text-amber-300 font-bold">★</span>}
              </button>
            );
          })}
        </div>
      </div>

      {/* Main Character Transformation Panel */}
      <div className="space-y-4">
        {/* Detected vs Target Character Cards */}
        <div className="space-y-1.5">
          <label className="text-xs font-semibold text-slate-300">Character Replacement</label>
          <div className="grid grid-cols-2 gap-3 items-center">
            {/* Detected Card */}
            <div className="p-3 rounded-xl bg-slate-950 border border-slate-800 space-y-1 text-center">
              <span className="text-[10px] text-slate-500 block uppercase font-mono">Detected Subject</span>
              <span className="text-3xl block">🦊</span>
              <span className="text-xs font-bold text-amber-200">
                {transformation.detectedCharacter || 'Fox'}
              </span>
            </div>

            {/* Target Card */}
            <div className="p-3 rounded-xl bg-gradient-to-br from-purple-950/60 to-slate-950 border border-purple-500/40 space-y-1 text-center">
              <span className="text-[10px] text-purple-300 block uppercase font-mono">Target Subject</span>
              <span className="text-3xl block">🐰</span>
              <span className="text-xs font-bold text-purple-200">
                {transformation.replacementCharacter || 'White Rabbit'}
              </span>
            </div>
          </div>
        </div>

        {/* Reference Image Uploader */}
        <ReferenceUploader
          label="Target Character Reference"
          onImageSelected={(img) => updateTransformationRequest({ referenceImageUri: img })}
        />

        {/* Prompt Input Area */}
        <div className="space-y-1.5">
          <label className="text-xs font-semibold text-slate-300">
            Prompt / AI Directive
          </label>
          <textarea
            value={transformation.prompt}
            onChange={(e) => updateTransformationRequest({ prompt: e.target.value })}
            rows={3}
            placeholder="Describe the target character and style..."
            className="w-full p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-indigo-500 transition-colors resize-none leading-relaxed"
          />
        </div>

        {/* Preservation Options */}
        <div className="space-y-2 pt-1 border-t border-slate-800/80">
          <label className="text-xs font-semibold text-slate-300">Preservation Rules</label>
          <div className="grid grid-cols-2 gap-2 text-xs">
            <button
              onClick={() => togglePreservation('preserveMotion')}
              className={`flex items-center gap-2 p-2 rounded-lg border transition-all text-left ${
                transformation.preservation.preserveMotion
                  ? 'bg-indigo-950/40 border-indigo-500/60 text-indigo-200'
                  : 'bg-slate-950 border-slate-800 text-slate-400'
              }`}
            >
              {transformation.preservation.preserveMotion ? (
                <CheckSquare className="w-4 h-4 text-indigo-400 shrink-0" />
              ) : (
                <Square className="w-4 h-4 text-slate-600 shrink-0" />
              )}
              <span>Motion & Pose</span>
            </button>

            <button
              onClick={() => togglePreservation('preserveCamera')}
              className={`flex items-center gap-2 p-2 rounded-lg border transition-all text-left ${
                transformation.preservation.preserveCamera
                  ? 'bg-indigo-950/40 border-indigo-500/60 text-indigo-200'
                  : 'bg-slate-950 border-slate-800 text-slate-400'
              }`}
            >
              {transformation.preservation.preserveCamera ? (
                <CheckSquare className="w-4 h-4 text-indigo-400 shrink-0" />
              ) : (
                <Square className="w-4 h-4 text-slate-600 shrink-0" />
              )}
              <span>Camera Movement</span>
            </button>

            <button
              onClick={() => togglePreservation('preserveComposition')}
              className={`flex items-center gap-2 p-2 rounded-lg border transition-all text-left ${
                transformation.preservation.preserveComposition
                  ? 'bg-indigo-950/40 border-indigo-500/60 text-indigo-200'
                  : 'bg-slate-950 border-slate-800 text-slate-400'
              }`}
            >
              {transformation.preservation.preserveComposition ? (
                <CheckSquare className="w-4 h-4 text-indigo-400 shrink-0" />
              ) : (
                <Square className="w-4 h-4 text-slate-600 shrink-0" />
              )}
              <span>Composition</span>
            </button>

            <button
              onClick={() => togglePreservation('preserveOriginalAudio')}
              className={`flex items-center gap-2 p-2 rounded-lg border transition-all text-left ${
                transformation.preservation.preserveOriginalAudio
                  ? 'bg-indigo-950/40 border-indigo-500/60 text-indigo-200'
                  : 'bg-slate-950 border-slate-800 text-slate-400'
              }`}
            >
              {transformation.preservation.preserveOriginalAudio ? (
                <CheckSquare className="w-4 h-4 text-indigo-400 shrink-0" />
              ) : (
                <Square className="w-4 h-4 text-slate-600 shrink-0" />
              )}
              <span>Original Audio</span>
            </button>
          </div>
        </div>
      </div>

      {/* Primary Action Button */}
      <div className="pt-2">
        <button
          onClick={handleGenerate}
          className="w-full py-3.5 px-4 rounded-xl bg-gradient-to-r from-purple-600 via-indigo-600 to-indigo-700 hover:from-purple-500 hover:to-indigo-600 text-white text-sm font-bold shadow-xl shadow-purple-900/40 transition-all flex items-center justify-center gap-2 group"
        >
          <Sparkles className="w-4 h-4 group-hover:rotate-12 transition-transform" />
          <span>Generate Transformed Video</span>
          <ArrowRight className="w-4 h-4 ml-1 group-hover:translate-x-0.5 transition-transform" />
        </button>
      </div>
    </div>
  );
};
