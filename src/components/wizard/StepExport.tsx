import React, { useState } from 'react';
import { Download, Sparkles, Film, Clock, Monitor, HardDrive, Play, Volume2, Maximize2, CheckCircle, AlertTriangle } from 'lucide-react';
import { useAppStore } from '../../store/useAppStore';
import { MockBadge } from '../common/MockBadge';

export const StepExport: React.FC = () => {
  const { activeProject, updateTransformationConfig } = useAppStore();
  const [isExporting, setIsExporting] = useState(false);
  const [exportComplete, setExportComplete] = useState(false);

  const config = activeProject?.transformation || {
    resolution: '1080p (1920x1080)',
    quality: 'High Quality',
    format: 'MP4',
    fps: 30,
    removeWatermark: true,
    category: 'character',
    prompt: '',
  };

  const handleExport = () => {
    setIsExporting(true);
    setTimeout(() => {
      setIsExporting(false);
      setExportComplete(true);
    }, 2500);
  };

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      {/* Title */}
      <div>
        <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Step 4: Export Your Video</h2>
        <p className="text-sm text-slate-400 mt-1">Choose export settings and download your transformed video</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
        {/* Left Export Settings Panel (5 cols) */}
        <div className="lg:col-span-5 space-y-6">
          <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-5">
            <h4 className="text-sm font-semibold text-slate-100">Export Settings</h4>

            {/* Resolution */}
            <div className="space-y-1.5">
              <label className="text-xs font-semibold text-slate-300">Resolution</label>
              <select
                value={config.resolution}
                onChange={(e) => updateTransformationConfig({ resolution: e.target.value })}
                className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
              >
                <option>1080p (1920x1080)</option>
                <option>4K Ultra HD (3840x2160)</option>
                <option>720p HD (1280x720)</option>
              </select>
            </div>

            {/* Quality */}
            <div className="space-y-1.5">
              <label className="text-xs font-semibold text-slate-300">Quality</label>
              <select
                value={config.quality}
                onChange={(e) => updateTransformationConfig({ quality: e.target.value })}
                className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
              >
                <option>High Quality</option>
                <option>Standard</option>
                <option>Lossless (Pro)</option>
              </select>
            </div>

            {/* Format & FPS */}
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1.5">
                <label className="text-xs font-semibold text-slate-300">Format</label>
                <select
                  value={config.format}
                  onChange={(e) => updateTransformationConfig({ format: e.target.value })}
                  className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
                >
                  <option>MP4</option>
                  <option>MOV</option>
                  <option>MKV</option>
                </select>
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-semibold text-slate-300">FPS</label>
                <select
                  value={`${config.fps} fps`}
                  onChange={(e) => updateTransformationConfig({ fps: parseInt(e.target.value) })}
                  className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
                >
                  <option>30 fps</option>
                  <option>60 fps</option>
                  <option>24 fps</option>
                </select>
              </div>
            </div>

            {/* Remove Watermark Toggle */}
            <div className="flex items-center justify-between p-3.5 rounded-xl bg-slate-950 border border-slate-800">
              <div className="flex items-center gap-2">
                <span className="text-xs font-medium text-slate-200">Remove Watermark</span>
                <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-purple-600 text-white">Pro</span>
              </div>
              <input
                type="checkbox"
                checked={config.removeWatermark}
                onChange={(e) => updateTransformationConfig({ removeWatermark: e.target.checked })}
                className="w-4 h-4 accent-indigo-600 rounded cursor-pointer"
              />
            </div>

            {/* Export CTA Button */}
            <div className="space-y-2 pt-2">
              <button
                onClick={handleExport}
                disabled={isExporting}
                className="w-full py-3 px-4 rounded-xl bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white text-sm font-semibold shadow-lg shadow-purple-900/40 transition-all flex items-center justify-center gap-2 disabled:opacity-50"
              >
                {isExporting ? (
                  <>
                    <Sparkles className="w-4 h-4 animate-spin" />
                    <span>Processing FFmpeg Render Pipeline...</span>
                  </>
                ) : exportComplete ? (
                  <>
                    <CheckCircle className="w-4 h-4" />
                    <span>Download Transformed Video</span>
                  </>
                ) : (
                  <>
                    <Download className="w-4 h-4" />
                    <span>Export Video</span>
                  </>
                )}
              </button>
              <p className="text-[11px] text-slate-500 text-center">Estimated time: 2-5 minutes</p>
            </div>
          </div>

          {/* Model Status Warning */}
          <div className="p-4 rounded-xl bg-amber-500/10 border border-amber-500/20 text-xs text-amber-300 space-y-1">
            <div className="flex items-center gap-2 font-semibold text-amber-200">
              <AlertTriangle className="w-4 h-4 text-amber-400" />
              <span>MODEL_NOT_AVAILABLE Protocol Active</span>
            </div>
            <p className="text-[11px] text-slate-400 leading-relaxed">
              Local AI models are not loaded. Export preview uses verified demo media. To enable full GPU rendering, download model weights in Settings.
            </p>
          </div>
        </div>

        {/* Right Export Preview & Information (7 cols) */}
        <div className="lg:col-span-7 space-y-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-slate-200">Export Preview</h3>
            <MockBadge label="EXPORT PREVIEW" />
          </div>

          {/* Video Player */}
          <div className="relative rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden shadow-2xl aspect-video flex flex-col items-center justify-center">
            <div className="w-full h-full bg-gradient-to-br from-amber-900 via-orange-950 to-slate-950 flex flex-col items-center justify-center p-6">
              <span className="text-6xl">🐰</span>
              <p className="text-sm font-bold text-amber-100 mt-2">Transformed Video (Fox → Rabbit)</p>
            </div>

            {/* Transport Bar */}
            <div className="absolute bottom-0 left-0 right-0 p-3 bg-slate-950/80 backdrop-blur-sm border-t border-slate-800 flex items-center justify-between text-xs text-slate-300">
              <div className="flex items-center gap-3">
                <button className="p-1 text-slate-300 hover:text-white">
                  <Play className="w-4 h-4 fill-current" />
                </button>
                <span className="font-mono text-[11px]">00:00 / 01:02</span>
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

          {/* Export Information Cards */}
          <div className="space-y-3">
            <h4 className="text-xs font-semibold text-slate-300">Export Information</h4>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
              <div className="p-3.5 rounded-xl bg-slate-900/60 border border-slate-800/80 flex items-center gap-3">
                <Clock className="w-4 h-4 text-indigo-400 shrink-0" />
                <div>
                  <span className="text-[10px] text-slate-500 block">Duration</span>
                  <span className="text-xs font-mono font-semibold text-slate-200">01:02</span>
                </div>
              </div>

              <div className="p-3.5 rounded-xl bg-slate-900/60 border border-slate-800/80 flex items-center gap-3">
                <Monitor className="w-4 h-4 text-purple-400 shrink-0" />
                <div>
                  <span className="text-[10px] text-slate-500 block">Resolution</span>
                  <span className="text-xs font-mono font-semibold text-slate-200">1920x1080</span>
                </div>
              </div>

              <div className="p-3.5 rounded-xl bg-slate-900/60 border border-slate-800/80 flex items-center gap-3">
                <Film className="w-4 h-4 text-sky-400 shrink-0" />
                <div>
                  <span className="text-[10px] text-slate-500 block">Format</span>
                  <span className="text-xs font-mono font-semibold text-slate-200">MP4</span>
                </div>
              </div>

              <div className="p-3.5 rounded-xl bg-slate-900/60 border border-slate-800/80 flex items-center gap-3">
                <HardDrive className="w-4 h-4 text-emerald-400 shrink-0" />
                <div>
                  <span className="text-[10px] text-slate-500 block">Est. Size</span>
                  <span className="text-xs font-mono font-semibold text-slate-200">85.2 MB</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
