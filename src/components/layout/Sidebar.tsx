import React from 'react';
import { 
  Home, 
  FolderKanban, 
  Sparkles, 
  User,
  Activity,
  Cpu,
  Settings as SettingsIcon,
  Video
} from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { NavTab } from '../../types';

export const Sidebar: React.FC = () => {
  const { activeTab, setActiveTab } = useUiStore();

  const navItems: { id: NavTab; label: string; icon: React.ReactNode }[] = [
    { id: 'home', label: 'Home', icon: <Home className="w-4 h-4" /> },
    { id: 'workspace', label: 'Workspace', icon: <Video className="w-4 h-4" /> },
    { id: 'projects', label: 'Projects', icon: <FolderKanban className="w-4 h-4" /> },
    { id: 'jobs', label: 'Jobs & Pipeline', icon: <Activity className="w-4 h-4" /> },
    { id: 'models', label: 'AI Models', icon: <Cpu className="w-4 h-4" /> },
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

      {/* Bottom Section: Pro Upgrade Banner & User Profile */}
      <div className="flex flex-col gap-4 pt-4 border-t border-slate-800/60">
        <div className="p-3.5 rounded-xl bg-gradient-to-b from-indigo-950/40 to-slate-900/80 border border-indigo-900/40 relative overflow-hidden">
          <div className="absolute top-0 right-0 w-20 h-20 bg-indigo-500/10 rounded-full blur-xl pointer-events-none" />
          <h4 className="text-xs font-bold text-slate-100 mb-0.5">Upgrade to Pro</h4>
          <p className="text-[11px] text-slate-400 mb-2.5 leading-relaxed">
            Unlock 4K rendering & cloud acceleration.
          </p>
          <button className="w-full py-2 px-3 rounded-lg bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white text-[11px] font-bold shadow-md shadow-purple-900/30 transition-all flex items-center justify-center gap-1.5">
            <Sparkles className="w-3 h-3" />
            <span>Upgrade Now</span>
          </button>
        </div>

        <div className="flex items-center gap-3 px-2 py-1">
          <div className="w-8 h-8 rounded-full bg-slate-800 border border-slate-700 flex items-center justify-center text-slate-300">
            <User className="w-4 h-4" />
          </div>
          <div className="flex flex-col min-w-0">
            <span className="text-xs font-semibold text-slate-200 truncate">Creator Workspace</span>
            <span className="text-[10px] text-slate-500">Free Tier</span>
          </div>
        </div>
      </div>
    </aside>
  );
};
