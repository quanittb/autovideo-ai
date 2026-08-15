import React, { useState } from 'react';
import { 
  Download, 
  Sparkles, 
  Film, 
  Clock, 
  Monitor, 
  HardDrive, 
  Play, 
  Volume2, 
  Maximize2, 
  CheckCircle, 
  FolderOpen 
} from 'lucide-react';
import { MockBadge } from '../../components/common/MockBadge';
import { ExportSettings } from '../../types/contracts';

export const StepExport: React.FC = () => {
  const [settings, setSettings] = useState<ExportSettings>({
    resolution: '1080p (1920x1080)',
    fps: 30,
    codec: 'H.264 (AVC)',
    quality: 'High Quality',
    audioOption: 'Preserve Original Audio',
    removeWatermark: true,
    outputDirectory: 'C:/Users/User/Videos/AutoVideo',
  });

  const [isExporting, setIsExporting] = useState(false);
  const [exportComplete, setExportComplete] = useState(false);

  const handleExport = () => {
    setIsExporting(true);
    setTimeout(() => {
      setIsExporting(false);
      setExportComplete(true);
    }, 2000);
  };

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      <div>
        <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Step 4: Export Your Video</h2>
        <p className="text-sm text-slate-400 mt-1">Configure export rendering parameters and output destination</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
        {/* Left Export Settings (5 cols) */}
        <div className="lg:col-span-5 space-y-6">
          <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-4">
            <h4 className="text-sm font-semibold text-slate-100">Render Parameters</h4>

            {/* Resolution */}
            <div className="space-y-1.5">
              <label className="text-xs font-semibold text-slate-300">Resolution</label>
              <select
                value={settings.resolution}
                onChange={(e) => setSettings({ ...settings, resolution: e.target.value as any })}
                className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
              >
                <option value="1080p (1920x1080)">1080p Full HD (1920x1080)</option>
                <option value="4K (3840x2160)">4K Ultra HD (3840x2160)</option>
                <option value="720p (1280x720)">720p HD (1280x720)</option>
              </select>
            </div>

            {/* Codec & FPS */}
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1.5">
                <label className="text-xs font-semibold text-slate-300">Video Codec</label>
                <select
                  value={settings.codec}
                  onChange={(e) => setSettings({ ...settings, codec: e.target.value as any })}
                  className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
                >
                  <option value="H.264 (AVC)">H.264 (AVC Universal)</option>
                  <option value="HEVC (H.265)">HEVC (H.265 High Efficiency)</option>
                  <option value="Apple ProRes">Apple ProRes (Master)</option>
                </select>
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-semibold text-slate-300">Frame Rate</label>
                <select
                  value={settings.fps}
                  onChange={(e) => setSettings({ ...settings, fps: parseInt(e.target.value) as any })}
                  className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
                >
                  <option value={30}>30 FPS</option>
                  <option value={60}>60 FPS</option>
                  <option value={24}>24 FPS (Cinematic)</option>
                </select>
              </div>
            </div>

            {/* Quality & Audio */}
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1.5">
                <label className="text-xs font-semibold text-slate-300">Quality Preset</label>
                <select
                  value={settings.quality}
                  onChange={(e) => setSettings({ ...settings, quality: e.target.value as any })}
                  className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
                >
                  <option value="High Quality">High Quality</option>
                  <option value="Standard">Standard</option>
                  <option value="Lossless (Pro)">Lossless (Pro)</option>
                </select>
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-semibold text-slate-300">Audio Track</label>
                <select
                  value={settings.audioOption}
                  onChange={(e) => setSettings({ ...settings, audioOption: e.target.value as any })}
                  className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
                >
                  <option value="Preserve Original Audio">Preserve Original Audio</option>
                  <option value="AI Enhanced Audio">AI Enhanced Audio</option>
                </select>
              </div>
            </div>

            {/* Output Destination Folder */}
            <div className="space-y-1.5">
              <label className="text-xs font-semibold text-slate-300">Output Folder</label>
              <div className="flex items-center gap-2">
                <input
                  type="text"
                  readOnly
                  value={settings.outputDirectory}
                  className="flex-1 p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-400 focus:outline-none"
                />
                <button
                  className="p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200"
                  title="Choose destination directory"
                >
                  <FolderOpen className="w-4 h-4" />
                </button>
              </div>
            </div>

            {/* Remove Watermark Toggle */}
            <div className="flex items-center justify-between p-3 rounded-xl bg-slate-950 border border-slate-800">
              <div className="flex items-center gap-2">
                <span className="text-xs font-medium text-slate-200">Remove Watermark</span>
                <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-purple-600 text-white">Pro</span>
              </div>
              <input
                type="checkbox"
                checked={settings.removeWatermark}
                onChange={(e) => setSettings({ ...settings, removeWatermark: e.target.checked })}
                className="w-4 h-4 accent-indigo-600 rounded cursor-pointer"
              />
            </div>

            {/* Export Action Button */}
            <div className="space-y-2 pt-2">
              <button
                onClick={handleExport}
                disabled={isExporting}
                className="w-full py-3.5 px-4 rounded-xl bg-gradient-to-r from-purple-600 via-indigo-600 to-indigo-700 hover:from-purple-500 hover:to-indigo-600 text-white text-sm font-bold shadow-xl shadow-purple-900/40 transition-all flex items-center justify-center gap-2 disabled:opacity-50"
              >
                {isExporting ? (
                  <>
                    <Sparkles className="w-4 h-4 animate-spin" />
                    <span>Rendering Output Video...</span>
                  </>
                ) : exportComplete ? (
                  <>
                    <CheckCircle className="w-4 h-4" />
                    <span>Download Transformed Video</span>
                  </>
                ) : (
                  <>
                    <Download className="w-4 h-4" />
                    <span>Export Video File</span>
                  </>
                )}
              </button>
              <p className="text-[11px] text-slate-500 text-center">Estimated render time: ~2 minutes</p>
            </div>
          </div>
        </div>

        {/* Right Preview & Details (7 cols) */}
        <div className="lg:col-span-7 space-y-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-slate-200">Final Export Preview</h3>
            <MockBadge label="EXPORT PREVIEW FIXTURE" />
          </div>

          <div className="relative rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden shadow-2xl aspect-video flex flex-col items-center justify-center">
            <div className="w-full h-full bg-gradient-to-br from-amber-900 via-orange-950 to-slate-950 flex flex-col items-center justify-center p-6">
              <span className="text-7xl">🐰</span>
              <p className="text-sm font-bold text-amber-100 mt-2">Transformed Video Output (Rabbit in Autumn)</p>
            </div>

            <div className="absolute bottom-0 left-0 right-0 p-3 bg-slate-950/85 backdrop-blur-sm border-t border-slate-800 flex items-center justify-between text-xs text-slate-300">
              <div className="flex items-center gap-3">
                <button className="p-1 text-slate-300 hover:text-white">
                  <Play className="w-4 h-4 fill-current" />
                </button>
                <span className="font-mono text-[11px]">00:00 / 01:02</span>
              </div>
              <div className="flex items-center gap-2">
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
            <h4 className="text-xs font-semibold text-slate-300">File Output Summary</h4>
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
                  <span className="text-xs font-mono font-semibold text-slate-200">{settings.resolution.split(' ')[0]}</span>
                </div>
              </div>

              <div className="p-3.5 rounded-xl bg-slate-900/60 border border-slate-800/80 flex items-center gap-3">
                <Film className="w-4 h-4 text-sky-400 shrink-0" />
                <div>
                  <span className="text-[10px] text-slate-500 block">Format / FPS</span>
                  <span className="text-xs font-mono font-semibold text-slate-200">{settings.fps} FPS MP4</span>
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
