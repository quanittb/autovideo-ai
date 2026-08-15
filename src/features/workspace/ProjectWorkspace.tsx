import React from 'react';
import { Folder, FileVideo, Clock } from 'lucide-react';
import { VideoPreview } from '../../components/ui/VideoPreview';
import { SceneStrip } from '../../components/ui/SceneStrip';
import { TransformPanel } from '../transform/TransformPanel';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';

export const ProjectWorkspace: React.FC = () => {
  const { activeProject } = useProjectStore();
  const { setCurrentStep } = useUiStore();

  const scenes = activeProject?.scenes || [
    {
      id: 'scene-1',
      index: 1,
      name: 'Woodland Overview',
      startTimeFormatted: '00:00',
      endTimeFormatted: '00:24',
      startFrame: 0,
      endFrame: 720,
      thumbnailEmoji: '🌲',
      status: 'ready',
    },
    {
      id: 'scene-2',
      index: 2,
      name: 'Fox Subject Close-up',
      startTimeFormatted: '00:24',
      endTimeFormatted: '00:48',
      startFrame: 720,
      endFrame: 1440,
      thumbnailEmoji: '🦊',
      status: 'ready',
    },
    {
      id: 'scene-3',
      index: 3,
      name: 'Snow Clearing Run',
      startTimeFormatted: '00:48',
      endTimeFormatted: '01:02',
      startFrame: 1440,
      endFrame: 1860,
      thumbnailEmoji: '❄️',
      status: 'ready',
    },
  ];

  const selectedScene = scenes.find((s) => s.id === activeProject?.selectedSceneId) || scenes[1];

  return (
    <div className="flex-1 flex flex-col h-full bg-slate-950 text-slate-100 p-6 overflow-hidden space-y-4">
      {/* 3-Column Center Workspace */}
      <div className="flex-1 grid grid-cols-1 lg:grid-cols-12 gap-6 min-h-0">
        {/* Left Column: Project Context (2 cols) */}
        <div className="lg:col-span-3 bg-slate-900/60 border border-slate-800/80 rounded-2xl p-5 flex flex-col justify-between overflow-y-auto">
          <div className="space-y-4">
            <div className="space-y-1">
              <span className="text-[10px] uppercase font-mono tracking-wider text-indigo-400 font-bold">
                Active Project
              </span>
              <h3 className="text-base font-bold text-slate-100 truncate">
                {activeProject?.name || 'Fox to Rabbit'}
              </h3>
            </div>

            {/* Ingested Source Info */}
            <div className="p-3.5 rounded-xl bg-slate-950 border border-slate-800 space-y-2 text-xs">
              <div className="flex items-center gap-2 text-slate-300 font-semibold">
                <FileVideo className="w-4 h-4 text-indigo-400" />
                <span className="truncate">{activeProject?.sourceMedia?.originalFileName || activeProject?.sourceAsset?.fileName || 'input_video.mp4'}</span>
              </div>
              <div className="grid grid-cols-2 gap-1 text-[11px] text-slate-400 font-mono">
                <span>1080p • 30 FPS</span>
                <span>01:02 • 45.2 MB</span>
              </div>
            </div>

            {/* Detected Scene Context */}
            <div className="p-3.5 rounded-xl bg-slate-950 border border-slate-800 space-y-1.5 text-xs">
              <div className="flex items-center justify-between text-slate-400">
                <span className="font-semibold text-slate-300">Selected Scene</span>
                <span className="text-[10px] font-mono font-bold text-indigo-400">#{selectedScene.index}</span>
              </div>
              <div className="text-slate-200 font-medium">{selectedScene.name}</div>
              <div className="flex items-center gap-1.5 text-[10px] text-slate-500 font-mono">
                <Clock className="w-3 h-3" />
                <span>{selectedScene.startTimeFormatted} - {selectedScene.endTimeFormatted}</span>
              </div>
            </div>
          </div>

          {/* Quick Actions */}
          <div className="space-y-2 pt-4 border-t border-slate-800/80">
            <button
              onClick={() => setCurrentStep('upload')}
              className="w-full py-2 px-3 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-semibold transition-all flex items-center justify-center gap-1.5"
            >
              <Folder className="w-3.5 h-3.5" />
              <span>Replace Input Video</span>
            </button>
          </div>
        </div>

        {/* Center Column: Large Video Preview (5 cols) */}
        <div className="lg:col-span-5 flex flex-col justify-center min-h-0">
          <VideoPreview
            title={`Preview: ${selectedScene.name}`}
            thumbnailEmoji={selectedScene.thumbnailEmoji}
            durationFormatted="01:02"
            isFixture={true}
            badgeLabel="WORKSPACE PREVIEW FIXTURE"
          />
        </div>

        {/* Right Column: AI Transform Panel (4 cols) */}
        <div className="lg:col-span-4 min-h-0">
          <TransformPanel />
        </div>
      </div>

      {/* Bottom Row: Scene Navigation Strip */}
      <SceneStrip
        scenes={scenes}
        selectedSceneId={selectedScene.id}
        onSelectScene={() => {}}
      />
    </div>
  );
};
