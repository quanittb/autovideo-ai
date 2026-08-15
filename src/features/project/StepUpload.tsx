import React, { useState } from 'react';
import { 
  UploadCloud, 
  CheckCircle2, 
  Film, 
  Play, 
  Volume2, 
  Maximize2, 
  AlertTriangle,
  AlertCircle
} from 'lucide-react';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { MockBadge } from '../../components/common/MockBadge';

export const StepUpload: React.FC = () => {
  const { activeProject, importMediaToProject, isLoading, error } = useProjectStore();
  const { setCurrentStep } = useUiStore();
  const [inputPath, setInputPath] = useState('');
  const [showManualPathInput, setShowManualPathInput] = useState(false);

  const tips = [
    'Use high quality videos (1080p or higher)',
    'Ensure good lighting in the video',
    'Characters should be clearly visible',
    'Shorter videos (30–90 seconds) work best',
  ];

  const sourceMedia = activeProject?.sourceMedia;

  const handleSimulateImport = async (samplePath: string) => {
    if (!activeProject) return;
    try {
      await importMediaToProject(activeProject.id, samplePath);
    } catch (err) {
      console.error('Import failed:', err);
    }
  };

  const isWebviewPlayable = sourceMedia
    ? ['mp4', 'mov'].includes(sourceMedia.container.toLowerCase()) && ['h264', 'avc1', 'vp8', 'vp9', 'av1'].includes(sourceMedia.videoCodec.toLowerCase())
    : true;

  const isLongVideo = sourceMedia ? sourceMedia.durationMs > 90_000 : false;

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Step 1: Upload Your Video</h2>
          <p className="text-sm text-slate-400 mt-1">Import a source video file for AI video transformation</p>
        </div>
        {activeProject?.isFixture && <MockBadge label="DEMO FIXTURE ASSET" />}
      </div>

      {error && (
        <div className="p-4 rounded-xl bg-rose-500/10 border border-rose-500/20 text-xs text-rose-300 flex items-start gap-3">
          <AlertCircle className="w-4 h-4 text-rose-400 shrink-0 mt-0.5" />
          <div className="space-y-1">
            <span className="font-semibold block">Media Validation / Import Failed</span>
            <p className="leading-relaxed">{error}</p>
          </div>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-8 items-start">
        {/* Left: Dropzone & Tips */}
        <div className="space-y-6">
          <div 
            onClick={() => setShowManualPathInput(!showManualPathInput)}
            className="border-2 border-dashed border-slate-700 hover:border-indigo-500/80 rounded-2xl p-10 bg-slate-900/40 hover:bg-slate-900/60 transition-all flex flex-col items-center justify-center text-center group cursor-pointer"
          >
            <div className="w-16 h-16 rounded-2xl bg-indigo-600/10 border border-indigo-500/20 text-indigo-400 flex items-center justify-center mb-4 group-hover:scale-110 transition-transform shadow-lg shadow-indigo-900/20">
              <UploadCloud className="w-8 h-8" />
            </div>
            <h3 className="text-lg font-semibold text-slate-200">
              {sourceMedia ? 'Replace Video File' : 'Drag & drop your video here'}
            </h3>
            <p className="text-xs text-indigo-400 font-medium mt-1">
              or click to specify file path
            </p>
            <div className="mt-6 pt-4 border-t border-slate-800/80 text-xs text-slate-500 space-y-1">
              <p>Accepted formats: <strong>MP4, MOV, AVI, MKV</strong></p>
              <p>Max file size: <strong>2 GB</strong> • Recommended: <strong>30–90 seconds</strong></p>
            </div>
          </div>

          {/* Quick File Path Importer for Desktop Testing */}
          {showManualPathInput && (
            <div className="p-4 rounded-xl bg-slate-900/80 border border-slate-800 space-y-3">
              <span className="text-xs font-semibold text-slate-200 block">Import From File Path</span>
              <div className="flex items-center gap-2">
                <input
                  type="text"
                  value={inputPath}
                  onChange={(e) => setInputPath(e.target.value)}
                  placeholder="e.g. C:/Videos/source_clip.mp4"
                  className="flex-1 p-2 rounded-lg bg-slate-950 border border-slate-800 text-xs font-mono text-slate-200 focus:outline-none focus:border-indigo-500"
                />
                <button
                  onClick={() => inputPath && handleSimulateImport(inputPath)}
                  disabled={!inputPath || isLoading}
                  className="px-3 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold disabled:opacity-50"
                >
                  Import
                </button>
              </div>
            </div>
          )}

          <div className="p-6 rounded-2xl bg-slate-900/50 border border-slate-800/80 space-y-4">
            <h4 className="text-sm font-semibold text-slate-200">Tips for best AI results</h4>
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
          {/* Video Preview Card */}
          <div className="rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden shadow-xl relative">
            <div className="relative aspect-video bg-gradient-to-br from-amber-950 via-slate-900 to-slate-950 flex flex-col items-center justify-center p-6 select-none">
              {!isWebviewPlayable ? (
                <div className="text-center space-y-2 p-4 rounded-xl bg-slate-950/80 border border-amber-500/30 max-w-xs">
                  <AlertTriangle className="w-8 h-8 text-amber-400 mx-auto" />
                  <span className="text-xs font-bold text-amber-300 font-mono block">PREVIEW_UNAVAILABLE</span>
                  <p className="text-[11px] text-slate-400 leading-relaxed">
                    Source format ({sourceMedia?.container.toUpperCase()} / {sourceMedia?.videoCodec}) is not natively decoded by the OS webview. Processing will still succeed via backend pipeline.
                  </p>
                </div>
              ) : (
                <div className="text-center space-y-2">
                  <span className="text-6xl drop-shadow-2xl">🦊</span>
                  <p className="text-xs font-semibold text-amber-200">
                    {sourceMedia?.originalFileName || 'Input Video Stream'}
                  </p>
                </div>
              )}

              <div className="absolute bottom-0 left-0 right-0 p-3 bg-slate-950/80 backdrop-blur-sm border-t border-slate-800 flex items-center justify-between text-xs text-slate-300">
                <div className="flex items-center gap-3">
                  <button className="p-1 text-slate-300 hover:text-white" aria-label="Play">
                    <Play className="w-4 h-4 fill-current" />
                  </button>
                  <span className="font-mono text-[11px]">
                    00:00 / {sourceMedia ? `${Math.floor(sourceMedia.durationMs / 60000).toString().padStart(2, '0')}:${Math.floor((sourceMedia.durationMs % 60000) / 1000).toString().padStart(2, '0')}` : '01:02'}
                  </span>
                </div>
                <div className="flex items-center gap-3">
                  <button className="p-1 text-slate-400 hover:text-white" aria-label="Mute">
                    <Volume2 className="w-4 h-4" />
                  </button>
                  <button className="p-1 text-slate-400 hover:text-white" aria-label="Fullscreen">
                    <Maximize2 className="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>
          </div>

          {/* Long Video Duration Warning Notice */}
          {isLongVideo && (
            <div className="p-3.5 rounded-xl bg-amber-500/10 border border-amber-500/20 text-xs text-amber-300 flex items-center gap-2.5">
              <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0" />
              <span>
                Duration exceeds recommended 90s. AI video transformation may require significant memory and processing time.
              </span>
            </div>
          )}

          {/* Video Metadata Cards */}
          <div className="p-6 rounded-2xl bg-slate-900/50 border border-slate-800/80 space-y-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2 text-sm font-semibold text-slate-200">
                <Film className="w-4 h-4 text-indigo-400" />
                <span>Source Media Metadata</span>
              </div>
              <span className="text-[10px] font-mono text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/20">
                {activeProject?.status || 'EMPTY'}
              </span>
            </div>

            <div className="grid grid-cols-2 md:grid-cols-3 gap-y-3 gap-x-4 text-xs">
              <div>
                <span className="text-slate-500 block">File Name</span>
                <span className="text-slate-200 font-mono font-medium truncate block">
                  {sourceMedia?.originalFileName || 'input_video.mp4'}
                </span>
              </div>

              <div>
                <span className="text-slate-500 block">Duration</span>
                <span className="text-slate-200 font-mono font-medium">
                  {sourceMedia ? `${(sourceMedia.durationMs / 1000).toFixed(1)}s` : '62.0s'}
                </span>
              </div>

              <div>
                <span className="text-slate-500 block">Resolution</span>
                <span className="text-slate-200 font-mono font-medium">
                  {sourceMedia ? `${sourceMedia.width}x${sourceMedia.height}` : '1920x1080'}
                </span>
              </div>

              <div>
                <span className="text-slate-500 block">Frame Rate</span>
                <span className="text-slate-200 font-mono font-medium">
                  {sourceMedia ? `${sourceMedia.fps.toFixed(0)} FPS` : '30 FPS'}
                </span>
              </div>

              <div>
                <span className="text-slate-500 block">File Size</span>
                <span className="text-slate-200 font-mono font-medium">
                  {sourceMedia ? `${(sourceMedia.fileSizeBytes / (1024 * 1024)).toFixed(1)} MB` : '45.2 MB'}
                </span>
              </div>

              <div>
                <span className="text-slate-500 block">Video / Audio Codec</span>
                <span className="text-slate-200 font-mono font-medium uppercase truncate block">
                  {sourceMedia ? `${sourceMedia.videoCodec} / ${sourceMedia.audioCodec || 'None'}` : 'H264 / AAC'}
                </span>
              </div>
            </div>

            {/* Next Step CTA */}
            <div className="pt-3 border-t border-slate-800/80 flex justify-end">
              <button
                onClick={() => setCurrentStep('transform')}
                className="px-5 py-2 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold shadow-md shadow-indigo-900/30 transition-all"
              >
                Proceed to Transformation
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
