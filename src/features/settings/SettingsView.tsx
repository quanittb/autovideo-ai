import React from 'react';
import { Cpu, ShieldCheck, Folder } from 'lucide-react';
import { useHardwareProfile } from '../../hooks/useHardwareProfile';
import { useAppInfo } from '../../hooks/useAppInfo';

export const SettingsView: React.FC = () => {
  const { hardware, storage } = useHardwareProfile();
  const { appInfo } = useAppInfo();

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      <div>
        <h2 className="text-2xl font-bold text-slate-100 tracking-tight">System Settings & Hardware Profile</h2>
        <p className="text-sm text-slate-400 mt-1">Platform architecture and local runtime capabilities</p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Hardware & Acceleration Card */}
        <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4">
          <div className="flex items-center gap-2.5 text-sm font-semibold text-slate-200">
            <Cpu className="w-5 h-5 text-indigo-400" />
            <span>Hardware & AI Acceleration</span>
          </div>

          <div className="space-y-3 text-xs">
            <div className="flex justify-between py-1.5 border-b border-slate-800/60">
              <span className="text-slate-400">Operating System:</span>
              <span className="font-mono text-slate-200 uppercase">{hardware?.os || 'Windows'} ({hardware?.arch || 'x86_64'})</span>
            </div>
            <div className="flex justify-between py-1.5 border-b border-slate-800/60">
              <span className="text-slate-400">CPU Parallelism:</span>
              <span className="font-mono text-slate-200">{hardware?.cpuCores || 8} Logical Cores</span>
            </div>
            <div className="flex justify-between py-1.5 border-b border-slate-800/60">
              <span className="text-slate-400">Primary GPU:</span>
              <span className="font-mono text-slate-200">{hardware?.gpuName || 'DirectX 12 GPU'}</span>
            </div>
            <div className="flex justify-between py-1.5 border-b border-slate-800/60">
              <span className="text-slate-400">DirectML Acceleration:</span>
              <span className={`font-semibold ${hardware?.isDirectmlSupported ? 'text-emerald-400' : 'text-slate-500'}`}>
                {hardware?.isDirectmlSupported ? 'Supported (Active)' : 'Unavailable'}
              </span>
            </div>
            <div className="flex justify-between py-1.5">
              <span className="text-slate-400">Apple Metal Acceleration:</span>
              <span className={`font-semibold ${hardware?.isMetalSupported ? 'text-emerald-400' : 'text-slate-500'}`}>
                {hardware?.isMetalSupported ? 'Supported' : 'Unavailable on this OS'}
              </span>
            </div>
          </div>
        </div>

        {/* Storage Paths Card */}
        <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4">
          <div className="flex items-center gap-2.5 text-sm font-semibold text-slate-200">
            <Folder className="w-5 h-5 text-purple-400" />
            <span>Storage Strategy</span>
          </div>

          <div className="space-y-3 text-xs">
            <div>
              <span className="text-slate-500 block mb-0.5">Projects Directory:</span>
              <span className="font-mono text-[11px] text-slate-300 block truncate bg-slate-950 p-2 rounded-lg border border-slate-800">
                {storage?.projectsDir || './.autovideo_data/projects'}
              </span>
            </div>
            <div>
              <span className="text-slate-500 block mb-0.5">Model Weights Directory:</span>
              <span className="font-mono text-[11px] text-slate-300 block truncate bg-slate-950 p-2 rounded-lg border border-slate-800">
                {storage?.modelsDir || './.autovideo_data/models'}
              </span>
            </div>
            <div>
              <span className="text-slate-500 block mb-0.5">Temp Cache Workspace:</span>
              <span className="font-mono text-[11px] text-slate-300 block truncate bg-slate-950 p-2 rounded-lg border border-slate-800">
                {storage?.tempDir || './.autovideo_data/temp'}
              </span>
            </div>
          </div>
        </div>

        {/* Application Info */}
        <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4 md:col-span-2">
          <div className="flex items-center gap-2.5 text-sm font-semibold text-slate-200">
            <ShieldCheck className="w-5 h-5 text-sky-400" />
            <span>Architecture & Security Boundary</span>
          </div>
          <p className="text-xs text-slate-400 leading-relaxed">
            AutoVideo AI enforces strict sandboxing: React UI communicates exclusively through typed Tauri commands and events. No direct shell or arbitrary binary execution is exposed to the frontend context.
          </p>
          <div className="flex items-center gap-4 text-xs text-slate-500 pt-2 border-t border-slate-800">
            <span>App Version: <strong className="text-slate-300 font-mono">{appInfo?.version || '0.1.0'}</strong></span>
            <span>Environment: <strong className="text-slate-300 font-mono">{appInfo?.environment || 'development'}</strong></span>
            <span>Architecture Phase: <strong className="text-indigo-400 font-mono">Phase 1 Foundation</strong></span>
          </div>
        </div>
      </div>
    </div>
  );
};
