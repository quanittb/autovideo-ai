import React, { useEffect, useState } from 'react';
import { 
  Sparkles, 
  Wand2, 
  UserRoundCheck, 
  Palette, 
  Video, 
  ArrowRight, 
  MoreVertical, 
  Clock,
  Plus,
  UploadCloud,
  FolderOpen
} from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { useProjectStore } from '../../stores/projectStore';
import { MockBadge } from '../../components/common/MockBadge';
import { EmptyState } from '../../components/ui/EmptyState';
import { LoadingState } from '../../components/ui/LoadingState';
import { ErrorState } from '../../components/ui/ErrorState';
import { ProjectSummary } from '../../types/contracts';

export const HomeView: React.FC = () => {
  const { setActiveTab, setCurrentStep } = useUiStore();
  const { projects, fetchProjects, createNewProject, loadProject, isLoading, error } = useProjectStore();
  const [viewState, setViewState] = useState<'auto' | 'empty' | 'loading' | 'error'>('auto');

  useEffect(() => {
    fetchProjects();
  }, [fetchProjects]);

  const quickTools = [
    {
      id: 'character',
      title: 'Character Replacement',
      desc: 'Replace character subject with AI (MVP)',
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
      desc: 'Apply anime, 3D render, or visual styles',
      icon: <Palette className="w-5 h-5 text-indigo-400" />,
      color: 'bg-indigo-500/10 border-indigo-500/20 text-indigo-400',
    },
    {
      id: 'enhancer',
      title: 'Video Enhancer',
      desc: 'Improve quality, resolution, and fps',
      icon: <Video className="w-5 h-5 text-emerald-400" />,
      color: 'bg-emerald-500/10 border-emerald-500/20 text-emerald-400',
    },
  ];

  const recentOutputs = [
    {
      id: 'out-1',
      title: 'Fox → Rabbit (Autumn Transformation)',
      date: '1 day ago',
      resolution: '1080p',
      size: '85.2 MB',
      emoji: '🐰',
    },
    {
      id: 'out-2',
      title: 'Winter Woodland → Golden Autumn',
      date: '2 hours ago',
      resolution: '1080p',
      size: '92.4 MB',
      emoji: '🍂',
    },
  ];

  const handleStartNewProject = async (customName?: string) => {
    try {
      await createNewProject(customName || 'Untitled Transformation');
      setCurrentStep('upload');
      setActiveTab('workspace');
    } catch (err) {
      console.error('Failed to create project:', err);
    }
  };

  const handleOpenProject = async (proj: ProjectSummary) => {
    try {
      await loadProject(proj.id);
      setCurrentStep('transform');
      setActiveTab('workspace');
    } catch (err) {
      console.error('Failed to load project:', err);
    }
  };

  const isActuallyEmpty = viewState === 'empty' || (viewState === 'auto' && projects.length === 0 && !isLoading);

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-8 bg-slate-950 text-slate-100">
      {/* Dev Mode UI State Filter Bar */}
      <div className="flex items-center justify-between p-3 rounded-xl bg-slate-900/60 border border-slate-800 text-xs">
        <div className="flex items-center gap-2">
          <MockBadge label="REAL PROJECT PERSISTENCE ACTIVE" />
          <span className="text-slate-400">Desktop Ergonomics & Local-First Rust Persistence</span>
        </div>
        <div className="flex items-center gap-1.5">
          <span className="text-[11px] text-slate-500 mr-1">Preview UI State:</span>
          {(['auto', 'empty', 'loading', 'error'] as const).map((st) => (
            <button
              key={st}
              onClick={() => setViewState(st)}
              className={`px-2 py-1 rounded text-[10px] font-mono capitalize transition-all ${
                viewState === st
                  ? 'bg-indigo-600 text-white font-bold'
                  : 'text-slate-400 hover:bg-slate-800'
              }`}
            >
              {st}
            </button>
          ))}
        </div>
      </div>

      {(viewState === 'loading' || (viewState === 'auto' && isLoading)) ? (
        <LoadingState message="Loading projects from local filesystem..." />
      ) : (viewState === 'error' || (viewState === 'auto' && error && projects.length === 0)) ? (
        <ErrorState
          title="Failed to Load Projects"
          message={error || "Could not read project manifests from local application directory."}
          code="STORAGE_READ_ERROR"
          onRetry={() => fetchProjects()}
        />
      ) : (
        <>
          {/* Main Hero & Obvious Main CTA Section */}
          <div className="relative rounded-2xl bg-gradient-to-r from-slate-900 via-indigo-950/60 to-purple-950/40 border border-slate-800/80 p-8 overflow-hidden flex flex-col md:flex-row items-center justify-between gap-8">
            <div className="max-w-md space-y-4 z-10">
              <div className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-semibold bg-indigo-500/10 text-indigo-300 border border-indigo-500/30">
                <Sparkles className="w-3.5 h-3.5" />
                <span>Next-Gen Video AI</span>
              </div>
              <h1 className="text-3xl font-extrabold tracking-tight text-white leading-tight">
                AI Video Transformation
              </h1>
              <p className="text-sm text-slate-300 leading-relaxed">
                Describe the changes you want. AutoVideo AI decides and executes the frame processing pipeline.
              </p>
              <div className="pt-2 flex items-center gap-3">
                <button
                  onClick={() => handleStartNewProject()}
                  className="px-6 py-3 rounded-xl bg-gradient-to-r from-purple-600 via-indigo-600 to-indigo-700 hover:from-purple-500 hover:to-indigo-600 text-white text-sm font-bold shadow-xl shadow-purple-900/40 transition-all flex items-center gap-2"
                >
                  <Plus className="w-4 h-4" />
                  <span>Create Project</span>
                  <ArrowRight className="w-4 h-4" />
                </button>
              </div>
            </div>

            {/* Hero Visual Before/After Mockup */}
            <div className="relative rounded-xl border border-slate-700/60 bg-slate-900/90 p-2 shadow-2xl shrink-0 flex items-center gap-3 z-10">
              <div className="relative w-44 h-32 rounded-lg overflow-hidden border border-slate-700">
                <div className="absolute top-2 left-2 z-10">
                  <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-slate-950/80 text-slate-300 border border-slate-700">
                    Original Fox
                  </span>
                </div>
                <div className="w-full h-full bg-gradient-to-br from-amber-900 to-slate-900 flex items-center justify-center p-4">
                  <span className="text-4xl">🦊</span>
                </div>
              </div>

              <div className="w-8 h-8 rounded-full bg-indigo-600 text-white flex items-center justify-center shadow-lg shadow-indigo-900/50 z-20">
                <ArrowRight className="w-4 h-4" />
              </div>

              <div className="relative w-44 h-32 rounded-lg overflow-hidden border border-purple-500/50">
                <div className="absolute top-2 left-2 z-10">
                  <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-purple-950/80 text-purple-200 border border-purple-700">
                    Transformed Rabbit
                  </span>
                </div>
                <div className="w-full h-full bg-gradient-to-br from-amber-800 to-amber-950 flex items-center justify-center p-4">
                  <span className="text-4xl">🐰</span>
                </div>
              </div>
            </div>
          </div>

          {/* Quick Tools Grid */}
          <div className="space-y-4">
            <h3 className="text-base font-bold text-slate-200">Quick Tools</h3>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
              {quickTools.map((tool) => (
                <div
                  key={tool.id}
                  onClick={() => handleStartNewProject(tool.title)}
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

          {/* Recent Projects & Ingestion Dropzone Grid */}
          <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-start">
            {/* Left 8 cols: Recent Projects */}
            <div className="lg:col-span-8 space-y-4">
              <div className="flex items-center justify-between">
                <h3 className="text-base font-bold text-slate-200">Recent Projects</h3>
                <button 
                  onClick={() => setActiveTab('projects')}
                  className="text-xs text-indigo-400 hover:text-indigo-300 font-medium"
                >
                  View All ({projects.length})
                </button>
              </div>

              {isActuallyEmpty ? (
                <EmptyState
                  icon={FolderOpen}
                  title="No Projects Yet"
                  description="You haven't created any video transformations yet. Click below to start your first project."
                  actionLabel="Create Project"
                  onAction={() => handleStartNewProject()}
                />
              ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                  {projects.map((proj) => (
                    <div
                      key={proj.id}
                      onClick={() => handleOpenProject(proj)}
                      className="rounded-xl bg-slate-900/60 border border-slate-800/80 hover:border-slate-700 overflow-hidden group cursor-pointer transition-all flex flex-col justify-between"
                    >
                      <div className="h-32 bg-slate-950 relative overflow-hidden flex items-center justify-center">
                        <Video className="w-8 h-8 text-slate-600" />
                        {proj.isFixture && (
                          <div className="absolute top-2 right-2">
                            <MockBadge label="DEMO" />
                          </div>
                        )}
                      </div>

                      <div className="p-3.5 flex items-center justify-between border-t border-slate-800/60 bg-slate-900/40">
                        <div className="min-w-0 pr-2">
                          <h4 className="text-xs font-semibold text-slate-200 group-hover:text-indigo-300 transition-colors truncate">
                            {proj.name}
                          </h4>
                          <div className="flex items-center gap-1.5 text-[10px] text-slate-500 mt-0.5 font-mono">
                            <Clock className="w-3 h-3 shrink-0" />
                            <span className="truncate">{proj.createdAt.slice(0, 10)}</span>
                          </div>
                        </div>
                        <MoreVertical className="w-4 h-4 text-slate-500 shrink-0" />
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {/* Right 4 cols: Fast Import Video Dropzone */}
            <div className="lg:col-span-4 space-y-4">
              <h3 className="text-base font-bold text-slate-200">Import Video</h3>
              <div
                onClick={() => handleStartNewProject()}
                className="p-8 rounded-2xl border-2 border-dashed border-slate-800 hover:border-indigo-500/80 bg-slate-900/40 hover:bg-slate-900/60 transition-all flex flex-col items-center justify-center text-center cursor-pointer group space-y-3"
              >
                <div className="w-12 h-12 rounded-xl bg-indigo-600/10 border border-indigo-500/20 text-indigo-400 flex items-center justify-center group-hover:scale-110 transition-transform">
                  <UploadCloud className="w-6 h-6" />
                </div>
                <div>
                  <h4 className="text-xs font-semibold text-slate-200">Drag video here</h4>
                  <p className="text-[11px] text-indigo-400 mt-0.5">or click to browse from disk</p>
                </div>
                <span className="text-[10px] text-slate-500">MP4, MOV, MKV up to 2GB</span>
              </div>
            </div>
          </div>

          {/* Recent Outputs Gallery */}
          <div className="space-y-4">
            <h3 className="text-base font-bold text-slate-200">Recent Outputs</h3>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {recentOutputs.map((out) => (
                <div
                  key={out.id}
                  onClick={() => {
                    setCurrentStep('result');
                    setActiveTab('workspace');
                  }}
                  className="p-4 rounded-xl bg-slate-900/50 border border-slate-800 hover:border-indigo-500/40 transition-all flex items-center justify-between cursor-pointer group"
                >
                  <div className="flex items-center gap-3">
                    <div className="w-12 h-12 rounded-lg bg-purple-950/60 border border-purple-800/60 flex items-center justify-center text-2xl group-hover:scale-105 transition-transform">
                      {out.emoji}
                    </div>
                    <div>
                      <h4 className="text-xs font-semibold text-slate-200 group-hover:text-indigo-300 transition-colors">
                        {out.title}
                      </h4>
                      <div className="flex items-center gap-2 text-[10px] text-slate-500 font-mono mt-0.5">
                        <span>{out.date}</span>
                        <span>•</span>
                        <span>{out.resolution}</span>
                        <span>•</span>
                        <span>{out.size}</span>
                      </div>
                    </div>
                  </div>

                  <ArrowRight className="w-4 h-4 text-slate-500 group-hover:text-indigo-400 group-hover:translate-x-1 transition-all" />
                </div>
              ))}
            </div>
          </div>
        </>
      )}
    </div>
  );
};
