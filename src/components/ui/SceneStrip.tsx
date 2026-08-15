import React from 'react';
import { Film, CheckCircle2, Loader2, Clock } from 'lucide-react';
import { SceneInfo } from '../../types/contracts';

interface SceneStripProps {
  scenes: SceneInfo[];
  selectedSceneId: string;
  onSelectScene: (id: string) => void;
  className?: string;
}

export const SceneStrip: React.FC<SceneStripProps> = ({
  scenes,
  selectedSceneId,
  onSelectScene,
  className = '',
}) => {
  return (
    <div className={`p-4 rounded-2xl bg-slate-950/80 border border-slate-800/80 backdrop-blur-md space-y-2.5 ${className}`}>
      <div className="flex items-center justify-between px-1">
        <div className="flex items-center gap-2">
          <Film className="w-4 h-4 text-indigo-400" />
          <span className="text-xs font-semibold text-slate-200">Scene Sequence ({scenes.length} Scenes)</span>
        </div>
        <span className="text-[11px] text-slate-500">Auto-detected shots for targeted AI transformation</span>
      </div>

      <div className="flex items-center gap-3 overflow-x-auto pb-1 scrollbar-thin">
        {scenes.map((scene) => {
          const isSelected = scene.id === selectedSceneId;
          return (
            <button
              key={scene.id}
              onClick={() => onSelectScene(scene.id)}
              className={`flex-shrink-0 w-44 p-2.5 rounded-xl border text-left transition-all ${
                isSelected
                  ? 'bg-indigo-950/40 border-indigo-500 shadow-md shadow-indigo-900/30'
                  : 'bg-slate-900/60 border-slate-800 hover:border-slate-700 hover:bg-slate-900'
              }`}
            >
              {/* Scene Thumbnail Banner */}
              <div className="h-16 rounded-lg bg-gradient-to-tr from-slate-950 to-slate-900 border border-slate-800 flex items-center justify-center relative overflow-hidden mb-2">
                <span className="text-2xl">{scene.thumbnailEmoji}</span>
                <span className="absolute top-1 left-1 px-1.5 py-0.5 rounded text-[9px] font-mono font-bold bg-slate-950/90 text-slate-300">
                  #{scene.index}
                </span>

                {scene.status === 'completed' && (
                  <div className="absolute top-1 right-1">
                    <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
                  </div>
                )}
                {scene.status === 'processing' && (
                  <div className="absolute top-1 right-1">
                    <Loader2 className="w-3.5 h-3.5 text-indigo-400 animate-spin" />
                  </div>
                )}
              </div>

              {/* Scene Details */}
              <div className="space-y-0.5">
                <div className="text-xs font-semibold text-slate-200 truncate">{scene.name}</div>
                <div className="flex items-center gap-1 text-[10px] text-slate-500 font-mono">
                  <Clock className="w-2.5 h-2.5" />
                  <span>{scene.startTimeFormatted} - {scene.endTimeFormatted}</span>
                </div>
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
};
