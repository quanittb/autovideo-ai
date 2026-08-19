import React, { useEffect } from 'react';
import { Folder, Sliders, Activity } from 'lucide-react';
import { VideoPreview } from '../editor/components/VideoPreview';
import { PlaybackControls } from '../editor/components/PlaybackControls';
import { Timeline } from '../editor/components/Timeline';
import { MediaInspector } from '../editor/components/MediaInspector';
import { useEditorStore } from '../editor/stores/editorStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';

export const ProjectWorkspace: React.FC = () => {
  const { activeProject } = useProjectStore();
  const { setCurrentStep, setActiveTab } = useUiStore();
  const { loadProjectMedia, reset } = useEditorStore();

  useEffect(() => {
    if (activeProject) {
      loadProjectMedia(activeProject.id);
    }
    return () => {
      reset();
    };
  }, [activeProject?.id, loadProjectMedia, reset]);

  return (
    <div className="flex-1 flex flex-col h-full bg-slate-950 text-slate-100 p-6 overflow-y-auto space-y-4">
      {/* Project Header Banner */}
      <div className="flex items-center justify-between pb-3 border-b border-slate-800/80">
        <div className="flex items-center gap-3">
          <div>
            <span className="text-[10px] uppercase font-mono tracking-wider text-indigo-400 font-bold block">
              Active Project • MVP Character Pipeline
            </span>
            <h2 className="text-xl font-bold text-white tracking-tight">
              {activeProject?.name || 'Untitled Transformation'}
            </h2>
          </div>
        </div>

        <div className="flex items-center gap-3">
          <button
            onClick={() => setActiveTab('jobs')}
            className="px-3.5 py-1.5 rounded-xl bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white text-xs font-bold shadow-md shadow-purple-900/30 flex items-center gap-1.5 transition-all cursor-pointer"
          >
            <Activity className="w-3.5 h-3.5" />
            <span>Jobs & Pipeline</span>
          </button>

          <button
            onClick={() => setActiveTab('verification')}
            className="px-3 py-1.5 rounded-xl bg-purple-500/10 hover:bg-purple-500/20 text-purple-300 border border-purple-500/30 text-xs font-semibold flex items-center gap-1.5 transition-colors"
          >
            <span>Media Diagnostics</span>
          </button>

          <button
            onClick={() => setCurrentStep('upload')}
            className="px-3 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-semibold flex items-center gap-1.5 transition-colors"
          >
            <Folder className="w-3.5 h-3.5" />
            <span>Replace Input Video</span>
          </button>
        </div>
      </div>

      {/* Main Workspace Area (2 Columns) */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-5 items-start">
        {/* Left Column (8 cols): Real Video Preview + Playback Controls */}
        <div className="lg:col-span-8 space-y-3">
          <VideoPreview className="w-full" />
          <PlaybackControls />
        </div>

        {/* Right Column (4 cols): Media Inspector & Context */}
        <div className="lg:col-span-4 space-y-4">
          <MediaInspector />

          {/* Pipeline Info Card */}
          <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800 text-xs space-y-2 select-none">
            <span className="font-bold text-slate-200 block">Workspace Mode</span>
            <p className="text-[11px] text-slate-400 leading-relaxed">
              Real media preview & timeline active. Scrub, inspect frames, and zoom the timeline.
            </p>
            <div className="pt-2 flex items-center justify-between text-[10px] font-mono text-slate-500 border-t border-slate-800/60">
              <span>Shortcuts: Space (Play/Pause)</span>
              <span>← / → (Seek 1s)</span>
            </div>
          </div>
        </div>
      </div>

      {/* Bottom Area: Real Media & Timeline Track */}
      <div className="space-y-2 pt-2">
        <div className="flex items-center justify-between px-1">
          <span className="text-xs font-bold text-slate-300 flex items-center gap-1.5">
            <Sliders className="w-3.5 h-3.5 text-indigo-400" />
            <span>Media Preparation Timeline</span>
          </span>
          <span className="text-[10px] font-mono text-slate-500">
            Ctrl + Mouse Wheel to Zoom
          </span>
        </div>
        <Timeline />
      </div>
    </div>
  );
};
