import React from 'react';
import { UploadCloud, CheckCircle2, Film, Play, Volume2, Maximize2 } from 'lucide-react';
import { useProjectStore } from '../../stores/projectStore';
import { MockBadge } from '../../components/common/MockBadge';

export const StepUpload: React.FC = () => {
  const { activeProject } = useProjectStore();

  const tips = [
    'Use high quality videos (1080p or higher)',
    'Ensure good lighting in the video',
    'Characters should be clearly visible',
    'Shorter videos (under 3 minutes) work best',
  ];

  const sourceMedia = activeProject?.sourceMedia;
  const sourceAsset = activeProject?.sourceAsset;

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      <div>
        <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Step 1: Upload Your Video</h2>
        <p className="text-sm text-slate-400 mt-1">Upload the video you want to transform</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-8 items-start">
        {/* Left: Dropzone & Tips */}
        <div className="space-y-6">
          <div className="border-2 border-dashed border-slate-700 hover:border-indigo-500/80 rounded-2xl p-10 bg-slate-900/40 hover:bg-slate-900/60 transition-all flex flex-col items-center justify-center text-center group cursor-pointer">
            <div className="w-16 h-16 rounded-2xl bg-indigo-600/10 border border-indigo-500/20 text-indigo-400 flex items-center justify-center mb-4 group-hover:scale-110 transition-transform shadow-lg shadow-indigo-900/20">
              <UploadCloud className="w-8 h-8" />
            </div>
            <h3 className="text-lg font-semibold text-slate-200">
              Drag & drop your video here
            </h3>
            <p className="text-xs text-indigo-400 hover:underline font-medium mt-1">
              or click to browse
            </p>
            <div className="mt-6 pt-4 border-t border-slate-800/80 text-xs text-slate-500 space-y-1">
              <p>Supports: MP4, MOV, AVI, MKV</p>
              <p>Max file size: 2GB, Max duration: 10 minutes</p>
            </div>
          </div>

          <div className="p-6 rounded-2xl bg-slate-900/50 border border-slate-800/80 space-y-4">
            <h4 className="text-sm font-semibold text-slate-200">Tips for better results</h4>
            <ul className="space-y-2.5">
              {tips.map((tip, i) => (
                <li key={i} className="flex items-center gap-2.5 text-xs text-slate-300">
                  <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                  <span>{tip}</span>
                </li>
              ))}
            </ul>
          </div>
        </div>

        {/* Right: Preview & Video Information */}
        <div className="space-y-6">
          <div className="rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden shadow-xl relative">
            <div className="relative aspect-video bg-gradient-to-br from-amber-950 via-slate-900 to-slate-950 flex flex-col items-center justify-center p-6">
              <div className="text-center space-y-2">
                <span className="text-6xl">🦊</span>
                <p className="text-xs font-semibold text-amber-200">Input Sample Video (Fox in Snow)</p>
              </div>

              <div className="absolute top-3 right-3 z-10">
                <MockBadge label="INPUT PREVIEW FIXTURE" />
              </div>

              <div className="absolute bottom-0 left-0 right-0 p-3 bg-slate-950/80 backdrop-blur-sm border-t border-slate-800 flex items-center justify-between text-xs text-slate-300">
                <div className="flex items-center gap-3">
                  <button className="p-1 text-slate-300 hover:text-white">
                    <Play className="w-4 h-4 fill-current" />
                  </button>
                  <span className="font-mono text-[11px]">00:00 / {sourceMedia ? `${(sourceMedia.durationMs / 1000).toFixed(0)}s` : sourceAsset?.metadata.durationFormatted || '01:02'}</span>
                </div>
                <div className="flex items-center gap-3">
                  <button className="p-1 text-slate-400 hover:text-white">
                    <Volume2 className="w-4 h-4" />
                  </button>
                  <button className="p-1 text-slate-400 hover:text-white">
                    <Maximize2 className="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>
          </div>

          <div className="p-6 rounded-2xl bg-slate-900/50 border border-slate-800/80 space-y-4">
            <div className="flex items-center gap-2 text-sm font-semibold text-slate-200">
              <Film className="w-4 h-4 text-indigo-400" />
              <span>Video Information</span>
            </div>

            <div className="grid grid-cols-2 gap-y-3 gap-x-4 text-xs">
              <div>
                <span className="text-slate-500 block">File Name</span>
                <span className="text-slate-200 font-mono font-medium truncate block">
                  {sourceMedia?.originalFileName || sourceAsset?.fileName || 'input_video.mp4'}
                </span>
              </div>
              <div>
                <span className="text-slate-500 block">Duration</span>
                <span className="text-slate-200 font-mono font-medium">
                  {sourceMedia ? `${(sourceMedia.durationMs / 1000).toFixed(0)}s` : sourceAsset?.metadata.durationFormatted || '01:02'}
                </span>
              </div>
              <div>
                <span className="text-slate-500 block">Resolution</span>
                <span className="text-slate-200 font-mono font-medium">
                  {sourceMedia ? `${sourceMedia.width}x${sourceMedia.height}` : sourceAsset?.metadata ? `${sourceAsset.metadata.width}x${sourceAsset.metadata.height}` : '1920x1080'}
                </span>
              </div>
              <div>
                <span className="text-slate-500 block">Size</span>
                <span className="text-slate-200 font-mono font-medium">
                  {sourceMedia ? `${(sourceMedia.fileSizeBytes / (1024 * 1024)).toFixed(1)} MB` : sourceAsset?.metadata.fileSizeFormatted || '45.2 MB'}
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
