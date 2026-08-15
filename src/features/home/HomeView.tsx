import React from 'react';
import { 
  Sparkles, 
  Wand2, 
  UserRoundCheck, 
  Palette, 
  Video, 
  ArrowRight, 
  MoreVertical, 
  Clock 
} from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { useProjectStore, defaultFoxRabbitProject } from '../../stores/projectStore';
import { MockBadge } from '../../components/common/MockBadge';
import { ProjectSummary } from '../../types/contracts';

export const HomeView: React.FC = () => {
  const { setActiveTab, setCurrentStep } = useUiStore();
  const { projects, setActiveProject } = useProjectStore();

  const quickTools = [
    {
      id: 'character',
      title: 'Character Replacement',
      desc: 'Replace characters with AI (MVP)',
      icon: <UserRoundCheck className="w-5 h-5 text-purple-400" />,
      color: 'bg-purple-500/10 border-purple-500/20 text-purple-400',
    },
    {
      id: 'scene',
      title: 'Scene Transformation',
      desc: 'Change scene, season, location with AI',
      icon: <Wand2 className="w-5 h-5 text-sky-400" />,
      color: 'bg-sky-500/10 border-sky-500/20 text-sky-400',
    },
    {
      id: 'style',
      title: 'Style Transfer',
      desc: 'Apply different visual styles',
      icon: <Palette className="w-5 h-5 text-indigo-400" />,
      color: 'bg-indigo-500/10 border-indigo-500/20 text-indigo-400',
    },
    {
      id: 'enhancer',
      title: 'Video Enhancer',
      desc: 'Improve quality and resolution',
      icon: <Video className="w-5 h-5 text-emerald-400" />,
      color: 'bg-emerald-500/10 border-emerald-500/20 text-emerald-400',
    },
  ];

  const handleStartNewProject = () => {
    setActiveProject({
      ...defaultFoxRabbitProject,
      id: `proj-${Date.now()}`,
      name: 'Untitled Transformation',
      createdAt: 'Just now',
      updatedAt: 'Just now',
    });
    setCurrentStep('upload');
    setActiveTab('projects');
  };

  const handleOpenProject = (proj: ProjectSummary) => {
    setActiveProject({
      ...defaultFoxRabbitProject,
      id: proj.id,
      name: proj.name,
    });
    setCurrentStep('transform');
    setActiveTab('projects');
  };

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-8 bg-slate-950 text-slate-100">
      {/* Honesty Status Banner */}
      <div className="p-4 rounded-xl bg-amber-500/10 border border-amber-500/20 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <MockBadge label="PHASE 1 ARCHITECTURE MODE" />
          <p className="text-xs text-amber-200/90">
            AutoVideo AI Phase 1 Foundation active. Real AI inference is cleanly abstracted; UI displays verified fixture assets.
          </p>
        </div>
      </div>

      {/* Hero Banner Section */}
      <div className="relative rounded-2xl bg-gradient-to-r from-slate-900 via-indigo-950/60 to-purple-950/40 border border-slate-800/80 p-8 overflow-hidden flex flex-col md:flex-row items-center justify-between gap-8">
        <div className="max-w-md space-y-4 z-10">
          <div className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-semibold bg-indigo-500/10 text-indigo-300 border border-indigo-500/30">
            <Sparkles className="w-3.5 h-3.5" />
            <span>Character Transformation MVP</span>
          </div>
          <h1 className="text-3xl font-extrabold tracking-tight text-white leading-tight">
            AI Video Transformation
          </h1>
          <p className="text-sm text-slate-300 leading-relaxed">
            Change characters, scenes, seasons, locations and more with AI. The user describes the result; AutoVideo AI executes the pipeline.
          </p>
          <div className="pt-2">
            <button
              onClick={handleStartNewProject}
              className="px-6 py-2.5 rounded-xl bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white text-sm font-semibold shadow-lg shadow-purple-900/40 transition-all flex items-center gap-2"
            >
              <span>Try Now</span>
              <ArrowRight className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* Hero Visual Before/After Mockup */}
        <div className="relative rounded-xl border border-slate-700/60 bg-slate-900/90 p-2 shadow-2xl shrink-0 flex items-center gap-3 z-10">
          {/* Before Fox */}
          <div className="relative w-44 h-32 rounded-lg overflow-hidden border border-slate-700">
            <div className="absolute top-2 left-2 z-10">
              <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-slate-950/80 text-slate-300 border border-slate-700">
                Original Fox
              </span>
            </div>
            <div className="w-full h-full bg-gradient-to-br from-amber-900 to-slate-900 flex items-center justify-center p-4">
              <div className="text-center">
                <span className="text-3xl">🦊</span>
                <p className="text-[11px] text-amber-200 font-medium mt-1">Snowy Forest</p>
              </div>
            </div>
          </div>

          {/* Arrow */}
          <div className="w-8 h-8 rounded-full bg-indigo-600 text-white flex items-center justify-center shadow-lg shadow-indigo-900/50 z-20">
            <ArrowRight className="w-4 h-4" />
          </div>

          {/* After Rabbit */}
          <div className="relative w-44 h-32 rounded-lg overflow-hidden border border-purple-500/50">
            <div className="absolute top-2 left-2 z-10">
              <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-purple-950/80 text-purple-200 border border-purple-700">
                Transformed Rabbit
              </span>
            </div>
            <div className="w-full h-full bg-gradient-to-br from-amber-800 to-amber-950 flex items-center justify-center p-4">
              <div className="text-center">
                <span className="text-3xl">🐰</span>
                <p className="text-[11px] text-amber-100 font-medium mt-1">Autumn Forest</p>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Quick Tools */}
      <div className="space-y-4">
        <h3 className="text-base font-bold text-slate-200">Quick Tools</h3>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          {quickTools.map((tool) => (
            <div
              key={tool.id}
              onClick={handleStartNewProject}
              className="p-5 rounded-xl bg-slate-900/60 border border-slate-800/80 hover:border-indigo-500/50 hover:bg-slate-900 transition-all cursor-pointer group space-y-3"
            >
              <div className={`w-10 h-10 rounded-lg flex items-center justify-center border ${tool.color}`}>
                {tool.icon}
              </div>
              <div>
                <h4 className="text-sm font-semibold text-slate-100 group-hover:text-indigo-300 transition-colors">
                  {tool.title}
                </h4>
                <p className="text-xs text-slate-400 mt-1 leading-relaxed">{tool.desc}</p>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Recent Projects */}
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <h3 className="text-base font-bold text-slate-200">Recent Projects</h3>
          <button 
            onClick={() => setActiveTab('projects')}
            className="text-xs text-indigo-400 hover:text-indigo-300 font-medium"
          >
            View All
          </button>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          {projects.map((proj) => (
            <div
              key={proj.id}
              onClick={() => handleOpenProject(proj)}
              className="rounded-xl bg-slate-900/60 border border-slate-800/80 hover:border-slate-700 overflow-hidden group cursor-pointer transition-all flex flex-col justify-between"
            >
              <div className="h-36 bg-slate-950 relative overflow-hidden flex items-center justify-center">
                {proj.id === 'proj-fox-rabbit' ? (
                  <div className="w-full h-full bg-gradient-to-tr from-amber-900 via-orange-950 to-slate-950 p-4 flex items-center justify-center">
                    <span className="text-4xl">🦊 ➔ 🐰</span>
                  </div>
                ) : proj.id === 'proj-winter' ? (
                  <div className="w-full h-full bg-gradient-to-tr from-blue-950 via-slate-900 to-amber-950 p-4 flex items-center justify-center">
                    <span className="text-4xl">❄️ ➔ 🍂</span>
                  </div>
                ) : (
                  <div className="w-full h-full bg-slate-900/80 flex items-center justify-center">
                    <Video className="w-8 h-8 text-slate-600" />
                  </div>
                )}
                {proj.isFixture && (
                  <div className="absolute top-2 right-2">
                    <MockBadge label="DEMO" />
                  </div>
                )}
              </div>

              <div className="p-3.5 flex items-center justify-between border-t border-slate-800/60 bg-slate-900/40">
                <div>
                  <h4 className="text-xs font-semibold text-slate-200 group-hover:text-indigo-300 transition-colors">
                    {proj.name}
                  </h4>
                  <div className="flex items-center gap-1.5 text-[11px] text-slate-500 mt-0.5">
                    <Clock className="w-3 h-3" />
                    <span>{proj.createdAt}</span>
                  </div>
                </div>
                <button className="text-slate-500 hover:text-slate-300 p-1">
                  <MoreVertical className="w-4 h-4" />
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
