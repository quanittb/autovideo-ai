import React, { useState } from 'react';
import { 
  Cpu, 
  Folder, 
  Sliders, 
  Lock, 
  Info, 
  Sparkles, 
  Zap, 
  CheckCircle2
} from 'lucide-react';
import { useHardwareProfile } from '../../hooks/useHardwareProfile';
import { useAppInfo } from '../../hooks/useAppInfo';

export const SettingsView: React.FC = () => {
  const { hardware, storage } = useHardwareProfile();
  const { appInfo } = useAppInfo();
  const [activeTab, setActiveTab] = useState<'general' | 'models' | 'gpu' | 'storage' | 'performance' | 'privacy' | 'about'>('gpu');

  const tabs: { id: typeof activeTab; label: string; icon: React.ReactNode }[] = [
    { id: 'general', label: 'General', icon: <Sliders className="w-4 h-4" /> },
    { id: 'models', label: 'AI Models', icon: <Sparkles className="w-4 h-4" /> },
    { id: 'gpu', label: 'GPU / Runtime', icon: <Cpu className="w-4 h-4" /> },
    { id: 'storage', label: 'Storage', icon: <Folder className="w-4 h-4" /> },
    { id: 'performance', label: 'Performance', icon: <Zap className="w-4 h-4" /> },
    { id: 'privacy', label: 'Privacy', icon: <Lock className="w-4 h-4" /> },
    { id: 'about', label: 'About', icon: <Info className="w-4 h-4" /> },
  ];

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      <div>
        <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Application Settings</h2>
        <p className="text-sm text-slate-400 mt-1">Configure hardware acceleration, local storage, and AI runtimes</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
        {/* Left Settings Sidebar (3 cols) */}
        <div className="lg:col-span-3 bg-slate-900/60 border border-slate-800/80 rounded-2xl p-2 space-y-1">
          {tabs.map((tab) => {
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-semibold transition-all text-left ${
                  isActive
                    ? 'bg-indigo-600 text-white shadow-md shadow-indigo-900/30'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900'
                }`}
              >
                {tab.icon}
                <span>{tab.label}</span>
              </button>
            );
          })}
        </div>

        {/* Right Settings Content (9 cols) */}
        <div className="lg:col-span-9 bg-slate-900/60 border border-slate-800/80 rounded-2xl p-6 space-y-6">
          {/* Tab 1: General */}
          {activeTab === 'general' && (
            <div className="space-y-5">
              <h3 className="text-base font-bold text-slate-200">General Preferences</h3>
              <div className="space-y-4 text-xs">
                <div className="flex items-center justify-between p-3.5 rounded-xl bg-slate-950 border border-slate-800">
                  <div>
                    <span className="font-semibold text-slate-200 block">Auto-save transformation sessions</span>
                    <span className="text-slate-500">Automatically cache intermediate plans and keyframe indices</span>
                  </div>
                  <input type="checkbox" defaultChecked className="w-4 h-4 accent-indigo-600 cursor-pointer" />
                </div>

                <div className="flex items-center justify-between p-3.5 rounded-xl bg-slate-950 border border-slate-800">
                  <div>
                    <span className="font-semibold text-slate-200 block">Default Export Quality</span>
                    <span className="text-slate-500">Preset quality for newly initiated projects</span>
                  </div>
                  <select className="p-2 rounded-lg bg-slate-900 border border-slate-700 text-slate-200 text-xs">
                    <option>High Quality (1080p)</option>
                    <option>Standard</option>
                    <option>4K Ultra HD</option>
                  </select>
                </div>
              </div>
            </div>
          )}

          {/* Tab 2: AI Models */}
          {activeTab === 'models' && (
            <div className="space-y-4">
              <h3 className="text-base font-bold text-slate-200">AI Model Directory & Provider</h3>
              <p className="text-xs text-slate-400 leading-relaxed">
                AutoVideo AI maintains local neural model weights on disk. You can configure download servers and verify checksum integrity.
              </p>
              <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-2 text-xs">
                <span className="text-slate-400 block font-semibold">Model Provider Strategy:</span>
                <div className="flex items-center gap-2">
                  <span className="px-2.5 py-1 rounded-lg bg-indigo-950/60 border border-indigo-500/40 text-indigo-300 font-mono">
                    Local-First ONNX / DirectML Provider
                  </span>
                  <span className="px-2.5 py-1 rounded-lg bg-slate-900 border border-slate-800 text-slate-500 font-mono">
                    Future Cloud Adapter (Ready)
                  </span>
                </div>
              </div>
            </div>
          )}

          {/* Tab 3: GPU / Runtime */}
          {activeTab === 'gpu' && (
            <div className="space-y-5">
              <h3 className="text-base font-bold text-slate-200">Hardware Acceleration & Compute Runtime</h3>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs">
                <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-1">
                  <span className="text-slate-500 block">Primary GPU Device:</span>
                  <span className="font-mono font-semibold text-slate-200 text-sm">{hardware?.gpuName || 'DirectX 12 Primary GPU'}</span>
                </div>
                <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-1">
                  <span className="text-slate-500 block">Operating System:</span>
                  <span className="font-mono font-semibold text-slate-200 text-sm uppercase">{hardware?.os} ({hardware?.arch})</span>
                </div>
                <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-1">
                  <span className="text-slate-500 block">DirectML Hardware Support:</span>
                  <span className={`font-bold ${hardware?.isDirectmlSupported ? 'text-emerald-400' : 'text-slate-500'}`}>
                    {hardware?.isDirectmlSupported ? 'Active & Supported' : 'Unavailable'}
                  </span>
                </div>
                <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-1">
                  <span className="text-slate-500 block">Available Logical CPU Threads:</span>
                  <span className="font-mono font-semibold text-slate-200 text-sm">{hardware?.cpuCores || 8} Threads</span>
                </div>
              </div>
            </div>
          )}

          {/* Tab 4: Storage */}
          {activeTab === 'storage' && (
            <div className="space-y-5">
              <h3 className="text-base font-bold text-slate-200">Storage Locations</h3>
              <div className="space-y-3 text-xs">
                <div>
                  <span className="text-slate-400 block mb-1 font-semibold">Projects Data Directory:</span>
                  <input
                    type="text"
                    readOnly
                    value={storage?.projectsDir || './.autovideo_data/projects'}
                    className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-300"
                  />
                </div>

                <div>
                  <span className="text-slate-400 block mb-1 font-semibold">Model Weights Directory:</span>
                  <input
                    type="text"
                    readOnly
                    value={storage?.modelsDir || './.autovideo_data/models'}
                    className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-300"
                  />
                </div>

                <div>
                  <span className="text-slate-400 block mb-1 font-semibold">Temporary Video Frame Buffer:</span>
                  <input
                    type="text"
                    readOnly
                    value={storage?.tempDir || './.autovideo_data/temp'}
                    className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-300"
                  />
                </div>
              </div>
            </div>
          )}

          {/* Tab 5: Performance */}
          {activeTab === 'performance' && (
            <div className="space-y-4">
              <h3 className="text-base font-bold text-slate-200">Performance & VRAM Management</h3>
              <div className="space-y-3 text-xs">
                <div className="flex items-center justify-between p-3.5 rounded-xl bg-slate-950 border border-slate-800">
                  <div>
                    <span className="font-semibold text-slate-200 block">VRAM Auto-Unload</span>
                    <span className="text-slate-500">Unload diffusion weights when idle for 5 minutes</span>
                  </div>
                  <input type="checkbox" defaultChecked className="w-4 h-4 accent-indigo-600 cursor-pointer" />
                </div>
                <div className="flex items-center justify-between p-3.5 rounded-xl bg-slate-950 border border-slate-800">
                  <div>
                    <span className="font-semibold text-slate-200 block">FFmpeg Hardware Decoding</span>
                    <span className="text-slate-500">Use D3D11VA / VideoToolbox acceleration</span>
                  </div>
                  <input type="checkbox" defaultChecked className="w-4 h-4 accent-indigo-600 cursor-pointer" />
                </div>
              </div>
            </div>
          )}

          {/* Tab 6: Privacy */}
          {activeTab === 'privacy' && (
            <div className="space-y-4">
              <h3 className="text-base font-bold text-slate-200">Privacy & Security</h3>
              <p className="text-xs text-slate-400 leading-relaxed">
                AutoVideo AI is built local-first. Video frames, prompts, and audio streams never leave your device unless you explicitly enable a Cloud Rendering Provider.
              </p>
              <div className="p-4 rounded-xl bg-emerald-500/10 border border-emerald-500/20 text-xs text-emerald-300 flex items-center gap-2.5">
                <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                <span>100% Local Inference & Media Processing Enabled</span>
              </div>
            </div>
          )}

          {/* Tab 7: About */}
          {activeTab === 'about' && (
            <div className="space-y-4">
              <h3 className="text-base font-bold text-slate-200">About AutoVideo AI</h3>
              <div className="space-y-2 text-xs text-slate-400">
                <p>
                  <strong>AutoVideo AI</strong> — AI-powered desktop video transformation studio.
                </p>
                <div className="p-3.5 rounded-xl bg-slate-950 border border-slate-800 font-mono space-y-1">
                  <div>App Version: <span className="text-slate-200 font-bold">{appInfo?.version || '0.1.0'}</span></div>
                  <div>Build Environment: <span className="text-slate-200">{appInfo?.environment || 'development'}</span></div>
                  <div>Architecture: <span className="text-indigo-400 font-bold">Phase 2 Desktop UI Foundation</span></div>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
