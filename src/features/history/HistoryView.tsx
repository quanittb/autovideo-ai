import React, { useEffect, useState, useMemo } from 'react';
import { 
  History, 
  Search, 
  RotateCw, 
  CheckCircle2, 
  XCircle, 
  Clock, 
  AlertTriangle, 
  Play, 
  FolderOpen, 
  FileVideo, 
  Sparkles, 
  Cpu,
  Layers,
  ArrowRight
} from 'lucide-react';
import { jobApi, mediaApi } from '../../lib/ipc';
import { Job, JobStatus } from '../../types/contracts';
import { useUiStore } from '../../stores/uiStore';
import { useJobStore } from '../../stores/jobStore';
import { EmptyState } from '../../components/ui/EmptyState';
import { LoadingState } from '../../components/ui/LoadingState';
import { ErrorState } from '../../components/ui/ErrorState';

type StatusFilter = 'ALL' | 'COMPLETED' | 'RUNNING' | 'FAILED' | 'INTERRUPTED' | 'CANCELLED';

export const HistoryView: React.FC = () => {
  const { setActiveTab } = useUiStore();
  const { selectJob } = useJobStore();
  const [jobs, setJobs] = useState<Job[]>([]);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('ALL');

  const fetchHistory = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const allJobs = await jobApi.getAllJobHistory().catch(async () => {
        return await jobApi.listJobs();
      });
      // Sort newest first
      allJobs.sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime());
      setJobs(allJobs);
    } catch (err: any) {
      setError(err?.message || 'Failed to fetch job history');
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    fetchHistory();
  }, []);

  const counts = useMemo(() => {
    return {
      ALL: jobs.length,
      COMPLETED: jobs.filter(j => j.status === 'COMPLETED').length,
      RUNNING: jobs.filter(j => ['QUEUED', 'PREPARING', 'RUNNING'].includes(j.status)).length,
      FAILED: jobs.filter(j => j.status === 'FAILED').length,
      INTERRUPTED: jobs.filter(j => j.status === 'INTERRUPTED').length,
      CANCELLED: jobs.filter(j => j.status === 'CANCELLED').length,
    };
  }, [jobs]);

  const filteredJobs = useMemo(() => {
    return jobs.filter((job) => {
      // Status filter
      if (statusFilter === 'COMPLETED' && job.status !== 'COMPLETED') return false;
      if (statusFilter === 'RUNNING' && !['QUEUED', 'PREPARING', 'RUNNING'].includes(job.status)) return false;
      if (statusFilter === 'FAILED' && job.status !== 'FAILED') return false;
      if (statusFilter === 'INTERRUPTED' && job.status !== 'INTERRUPTED') return false;
      if (statusFilter === 'CANCELLED' && job.status !== 'CANCELLED') return false;

      // Text search
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchId = job.id.toLowerCase().includes(q);
        const matchProj = job.projectId.toLowerCase().includes(q);
        const matchInput = job.inputFiles.some(f => f.toLowerCase().includes(q));
        const matchModel = job.aiConfig?.modelId.toLowerCase().includes(q);
        if (!matchId && !matchProj && !matchInput && !matchModel) return false;
      }

      return true;
    });
  }, [jobs, statusFilter, searchQuery]);

  const handleOpenVideo = async (filePath: string) => {
    try {
      await mediaApi.openFilePath(filePath);
    } catch (err) {
      console.error('Failed to open video file:', err);
    }
  };

  const handleOpenFolder = async (filePath: string) => {
    try {
      const dir = filePath.substring(0, Math.max(filePath.lastIndexOf('/'), filePath.lastIndexOf('\\')));
      await mediaApi.openDirectory(dir || filePath);
    } catch (err) {
      console.error('Failed to open directory:', err);
    }
  };

  const handleInspectJob = (job: Job) => {
    selectJob(job.id);
    setActiveTab('jobs');
  };

  const getStatusBadge = (status: JobStatus) => {
    switch (status) {
      case 'COMPLETED':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold bg-emerald-500/10 text-emerald-300 border border-emerald-500/30">
            <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
            <span>Completed</span>
          </span>
        );
      case 'RUNNING':
      case 'PREPARING':
      case 'QUEUED':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold bg-indigo-500/10 text-indigo-300 border border-indigo-500/30 animate-pulse">
            <RotateCw className="w-3.5 h-3.5 text-indigo-400 animate-spin" />
            <span>Processing</span>
          </span>
        );
      case 'INTERRUPTED':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold bg-amber-500/10 text-amber-300 border border-amber-500/30">
            <AlertTriangle className="w-3.5 h-3.5 text-amber-400" />
            <span>Interrupted</span>
          </span>
        );
      case 'FAILED':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold bg-rose-500/10 text-rose-300 border border-rose-500/30">
            <XCircle className="w-3.5 h-3.5 text-rose-400" />
            <span>Failed</span>
          </span>
        );
      case 'CANCELLED':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold bg-slate-800 text-slate-400 border border-slate-700">
            <Clock className="w-3.5 h-3.5 text-slate-500" />
            <span>Cancelled</span>
          </span>
        );
      default:
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] bg-slate-800 text-slate-400 font-mono">
            {status}
          </span>
        );
    }
  };

  const formatDate = (iso: string) => {
    try {
      const d = new Date(iso);
      return d.toLocaleDateString(undefined, {
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
      });
    } catch {
      return iso;
    }
  };

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100 font-sans">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-xl bg-gradient-to-tr from-indigo-600 to-purple-600 flex items-center justify-center shadow-lg shadow-purple-900/30">
              <History className="w-4 h-4 text-white" />
            </div>
            <h2 className="text-2xl font-bold tracking-tight text-white">Pipeline Execution History</h2>
          </div>
          <p className="text-sm text-slate-400 mt-1">
            Browse and inspect all historical AI video transformations, execution logs, and output deliverables
          </p>
        </div>

        <button
          onClick={fetchHistory}
          className="px-3.5 py-2 rounded-xl bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-300 text-xs font-semibold flex items-center gap-2 transition-colors cursor-pointer self-start md:self-auto"
        >
          <RotateCw className={`w-3.5 h-3.5 text-indigo-400 ${isLoading ? 'animate-spin' : ''}`} />
          <span>Refresh History</span>
        </button>
      </div>

      {/* Filters & Search Bar */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3 p-2 bg-slate-900/60 border border-slate-800/80 rounded-2xl">
        {/* Search */}
        <div className="relative flex-1">
          <Search className="w-4 h-4 text-slate-500 absolute left-3 top-1/2 -translate-y-1/2" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search by project, video filename, or model ID..."
            className="w-full pl-9 pr-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-500/60"
          />
        </div>

        {/* Status Filter Tabs */}
        <div className="flex items-center gap-1 overflow-x-auto pb-1 sm:pb-0">
          {(['ALL', 'COMPLETED', 'RUNNING', 'INTERRUPTED', 'FAILED', 'CANCELLED'] as const).map((st) => {
            const isActive = statusFilter === st;
            return (
              <button
                key={st}
                onClick={() => setStatusFilter(st)}
                className={`px-3 py-1.5 rounded-xl text-xs font-semibold transition-all whitespace-nowrap cursor-pointer ${
                  isActive
                    ? 'bg-indigo-600 text-white shadow-md shadow-indigo-900/30'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800'
                }`}
              >
                <span>{st.charAt(0) + st.slice(1).toLowerCase()}</span>
                <span className="ml-1.5 text-[10px] font-mono opacity-80">({counts[st] || 0})</span>
              </button>
            );
          })}
        </div>
      </div>

      {/* Main Content Area */}
      {isLoading ? (
        <LoadingState message="Loading transformation history from local records..." />
      ) : error ? (
        <ErrorState
          title="Failed to Load History"
          message={error}
          code="HISTORY_LOAD_ERROR"
          onRetry={fetchHistory}
        />
      ) : filteredJobs.length === 0 ? (
        <EmptyState
          icon={History}
          title="No Transformation Jobs Found"
          description={
            searchQuery.trim() || statusFilter !== 'ALL'
              ? 'No jobs match your current filter and search criteria.'
              : 'You haven\'t executed any AI video transformation jobs yet. Go to the Models tab to launch your first job.'
          }
          actionLabel="Create AI Job"
          onAction={() => setActiveTab('models')}
        />
      ) : (
        <div className="space-y-3.5">
          {filteredJobs.map((job) => {
            const hasFinalVideo = job.outputFiles && job.outputFiles.length > 0;
            const primaryOutput = hasFinalVideo ? job.outputFiles[0] : null;
            const isAiJob = !!job.aiConfig?.enabled;
            const inputFileName = job.inputFiles.length > 0
              ? job.inputFiles[0].replace(/^.*[\\/]/, '')
              : 'Unknown Source';

            return (
              <div
                key={job.id}
                className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 hover:border-slate-700/80 transition-all space-y-4"
              >
                {/* Top Row: Project info, status, timestamps */}
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                  <div className="flex items-center gap-3">
                    <div className="w-10 h-10 rounded-xl bg-slate-950 border border-slate-800 flex items-center justify-center text-slate-400">
                      <FileVideo className="w-5 h-5 text-indigo-400" />
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="font-bold text-slate-200 text-sm">{job.projectId}</span>
                        <span className="text-[10px] text-slate-500 font-mono">#{job.id.slice(0, 8)}</span>
                      </div>
                      <div className="flex items-center gap-2 text-xs text-slate-400 font-mono mt-0.5">
                        <span>Source: <strong className="text-slate-300">{inputFileName}</strong></span>
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center gap-2.5">
                    {getStatusBadge(job.status)}
                    <span className="text-xs text-slate-500 font-mono">{formatDate(job.createdAt)}</span>
                  </div>
                </div>

                {/* Middle Row: AI Model, Hardware, Stages */}
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 p-3 rounded-xl bg-slate-950 border border-slate-800/80 text-xs">
                  <div className="flex items-center gap-2 text-slate-300">
                    <Cpu className="w-4 h-4 text-purple-400 shrink-0" />
                    <div className="min-w-0">
                      <span className="text-[10px] text-slate-500 block uppercase font-semibold">AI Model</span>
                      <span className="font-mono truncate block">
                        {isAiJob ? `${job.aiConfig?.modelId} (v${job.aiConfig?.modelVersion || '1.0.0'})` : 'Standard Non-AI Pipeline'}
                      </span>
                    </div>
                  </div>

                  <div className="flex items-center gap-2 text-slate-300">
                    <Layers className="w-4 h-4 text-indigo-400 shrink-0" />
                    <div className="min-w-0">
                      <span className="text-[10px] text-slate-500 block uppercase font-semibold">Sampling Mode</span>
                      <span className="font-mono truncate block">
                        {isAiJob ? (job.aiConfig?.frameSampling?.mode === 'every_nth' ? `Every ${job.aiConfig?.frameSampling?.nth} frames` : '100% All Frames') : 'Standard'}
                      </span>
                    </div>
                  </div>

                  <div className="flex items-center gap-2 text-slate-300">
                    <Sparkles className="w-4 h-4 text-emerald-400 shrink-0" />
                    <div className="min-w-0">
                      <span className="text-[10px] text-slate-500 block uppercase font-semibold">Provider</span>
                      <span className="font-mono truncate block">
                        {job.aiConfig?.provider || 'CPU (Universal)'}
                      </span>
                    </div>
                  </div>
                </div>

                {/* Bottom Row: Actions */}
                <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3 pt-1">
                  <div className="text-xs text-slate-400 font-mono truncate">
                    {job.message || 'Pipeline completed successfully.'}
                  </div>

                  <div className="flex items-center gap-2 shrink-0">
                    {hasFinalVideo && primaryOutput && (
                      <>
                        <button
                          onClick={() => handleOpenVideo(primaryOutput)}
                          className="px-3 py-1.5 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold flex items-center gap-1.5 shadow-md shadow-indigo-900/30 transition-all cursor-pointer"
                        >
                          <Play className="w-3.5 h-3.5 fill-white" />
                          <span>Open Video</span>
                        </button>

                        <button
                          onClick={() => handleOpenFolder(primaryOutput)}
                          className="px-3 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold flex items-center gap-1.5 transition-colors cursor-pointer"
                        >
                          <FolderOpen className="w-3.5 h-3.5 text-purple-400" />
                          <span>Open Folder</span>
                        </button>
                      </>
                    )}

                    <button
                      onClick={() => handleInspectJob(job)}
                      className="px-3 py-1.5 rounded-xl bg-slate-900 hover:bg-slate-800 border border-slate-700/80 text-slate-300 hover:text-white text-xs font-semibold flex items-center gap-1.5 transition-colors cursor-pointer"
                    >
                      <span>Inspect Details</span>
                      <ArrowRight className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
