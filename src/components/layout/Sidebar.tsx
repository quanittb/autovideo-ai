import React from 'react';
import { 
  Home, 
  FolderKanban, 
  Sparkles, 
  Activity,
  Cpu,
  Settings as SettingsIcon,
  Video,
  HardDrive,
  CheckCircle2,
  History as HistoryIcon
} from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { useHardwareProfile } from '../../hooks/useHardwareProfile';
import { NavTab } from '../../types';

export const Sidebar: React.FC = () => {
  const { activeTab, setActiveTab } = useUiStore();
  const { hardware } = useHardwareProfile();

  const navItems: { id: NavTab; label: string; icon: React.ReactNode }[] = [
    { id: 'home', label: 'Home', icon: <Home className="w-4 h-4" /> },
    { id: 'flow', label: 'Flow Gen', icon: <Sparkles className="w-4 h-4 text-emerald-400" /> },
    { id: 'generation', label: 'Generative Studio', icon: <Sparkles className="w-4 h-4 text-purple-400" /> },
    { id: 'workspace', label: 'Workspace', icon: <Video className="w-4 h-4" /> },
    { id: 'projects', label: 'Projects', icon: <FolderKanban className="w-4 h-4" /> },
    { id: 'jobs', label: 'Jobs & Pipeline', icon: <Activity className="w-4 h-4" /> },
    { id: 'models', label: 'AI Models', icon: <Cpu className="w-4 h-4" /> },
    { id: 'history', label: 'History', icon: <HistoryIcon className="w-4 h-4" /> },
    { id: 'verification', label: 'Media Engine Test', icon: <Activity className="w-4 h-4 text-purple-400" /> },
    { id: 'settings', label: 'Settings', icon: <SettingsIcon className="w-4 h-4" /> },
  ];

  return (
    <aside className="w-64 bg-slate-950 border-r border-slate-800/80 flex flex-col justify-between h-screen p-4 shrink-0 select-none">
      <div className="flex flex-col gap-6">
        {/* App Branding */}
        <div className="flex items-center justify-between px-2 pt-2">
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-xl bg-gradient-to-tr from-indigo-600 to-purple-500 flex items-center justify-center text-white shadow-lg shadow-purple-900/30">
              <Sparkles className="w-4 h-4" />
            </div>
            <div className="flex flex-col">
              <span className="font-bold text-slate-100 text-sm tracking-tight">AutoVideo AI</span>
              <span className="text-[10px] text-slate-500 font-mono">Desktop Studio</span>
            </div>
          </div>
        </div>

        {/* Navigation List */}
        <nav className="flex flex-col gap-1">
          {navItems.map((item) => {
            const isActive = activeTab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => setActiveTab(item.id)}
                className={`flex items-center gap-3 px-3 py-2.5 rounded-xl text-xs font-semibold transition-all text-left ${
                  isActive
                    ? 'bg-indigo-600 text-white shadow-md shadow-indigo-900/30'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900/80'
                }`}
              >
                {item.icon}
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>
      </div>

      {/* Bottom Section: System Status & Hardware Telemetry Widget */}
      <div className="flex flex-col gap-3 pt-4 border-t border-slate-800/60">
        <div className="p-3 rounded-xl bg-slate-900/60 border border-slate-800/80 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-[10px] uppercase font-mono tracking-wider text-slate-400 font-semibold">
              System Engine
            </span>
            <div className="flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
              <span className="text-[10px] text-emerald-400 font-medium">Ready</span>
            </div>
          </div>

          <div className="space-y-1 text-[11px] text-slate-400 font-mono">
            <div className="flex items-center gap-1.5 text-slate-300 truncate">
              <Cpu className="w-3.5 h-3.5 text-indigo-400 shrink-0" />
              <span className="truncate">{hardware?.gpuName || 'DirectML GPU'}</span>
            </div>
            <div className="flex items-center gap-1.5 text-slate-400">
              <HardDrive className="w-3.5 h-3.5 text-slate-500 shrink-0" />
              <span>Local Storage: Active</span>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2.5 px-2 py-1">
          <div className="w-7 h-7 rounded-lg bg-indigo-950/60 border border-indigo-800/50 flex items-center justify-center text-indigo-400">
            <CheckCircle2 className="w-3.5 h-3.5" />
          </div>
          <div className="flex flex-col min-w-0">
            <span className="text-xs font-semibold text-slate-200 truncate">AutoVideo Desktop</span>
            <span className="text-[10px] text-slate-500">Full Access Studio</span>
          </div>
        </div>
      </div>
    </aside>
  );
};
