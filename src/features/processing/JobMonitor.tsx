import React, { useEffect, useState, useRef, useMemo } from 'react';
import { 
  Play, 
  Square, 
  RotateCw, 
  Trash2, 
  Terminal, 
  CheckCircle2, 
  XCircle, 
  Clock, 
  AlertTriangle,
  Loader2, 
  Sparkles, 
  Layers, 
  FileVideo, 
  FileAudio, 
  FileText, 
  Video, 
  Plus,
  Copy,
  Check,
  FolderOpen,
  Pause,
  ArrowRight,
  Film
} from 'lucide-react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useJobStore } from '../../stores/jobStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { mediaApi } from '../../lib/ipc';
import { JobStatus, StageStatus } from '../../types/contracts';

type FilterType = 'ALL' | 'ACTIVE' | 'COMPLETED' | 'FAILED' | 'INTERRUPTED' | 'CANCELLED';

export const JobMonitor: React.FC = () => {
  const { 
    jobs, 
    activeJob, 
    selectedJobId, 
    jobLogs, 
    jobArtifacts, 
    isLoading,
    fetchJobs, 
    createJob, 
    startJob, 
    cancelJob, 
    retryJob, 
    deleteJob, 
    selectJob,
    initEventListeners 
  } = useJobStore();

  const { activeProject } = useProjectStore();
  const { setActiveTab: setAppNavTab } = useUiStore();
  const [filterTab, setFilterTab] = useState<FilterType>('ALL');
  const [activeBottomTab, setActiveBottomTab] = useState<'LOGS' | 'OUTPUT' | 'ARTIFACTS'>('LOGS');
  const [autoScroll, setAutoScroll] = useState<boolean>(true);
  const [copiedLogs, setCopiedLogs] = useState<boolean>(false);
  const [showErrorDetails, setShowErrorDetails] = useState<boolean>(false);
  const logContainerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    fetchJobs();
    let unlisten: (() => void) | undefined;
    initEventListeners().then((unsub) => {
      unlisten = unsub;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  // Selected job resolution
  const selectedJob = useMemo(() => {
    if (selectedJobId) {
      const found = jobs.find((j) => j.id === selectedJobId);
      if (found) return found;
    }
    return activeJob || (jobs.length > 0 ? jobs[0] : null);
  }, [jobs, selectedJobId, activeJob]);

  // Logs & Artifacts for selected job
  const currentLogs = selectedJob ? (jobLogs[selectedJob.id] || []) : [];
  const currentArtifacts = selectedJob ? (jobArtifacts[selectedJob.id] || []) : [];

  const finalVideoArtifact = useMemo(() => {
    return currentArtifacts.find((a) => a.artifactType === 'final_video' || a.artifactType === 'output_video');
  }, [currentArtifacts]);

  const outputSrc = useMemo(() => {
    return finalVideoArtifact?.path ? convertFileSrc(finalVideoArtifact.path) : null;
  }, [finalVideoArtifact]);

  // Auto-scroll logs handling
  useEffect(() => {
    if (autoScroll && logContainerRef.current) {
      logContainerRef.current.scrollTop = logContainerRef.current.scrollHeight;
    }
  }, [currentLogs, autoScroll, selectedJobId]);

  // Filters calculation
  const counts = useMemo(() => {
    return {
      ALL: jobs.length,
      ACTIVE: jobs.filter((j) => ['QUEUED', 'PREPARING', 'RUNNING', 'CANCELLING'].includes(j.status)).length,
      COMPLETED: jobs.filter((j) => j.status === 'COMPLETED').length,
      FAILED: jobs.filter((j) => j.status === 'FAILED').length,
      INTERRUPTED: jobs.filter((j) => j.status === 'INTERRUPTED').length,
      CANCELLED: jobs.filter((j) => j.status === 'CANCELLED').length,
    };
  }, [jobs]);

  const filteredJobs = useMemo(() => {
    return jobs.filter((j) => {
      if (filterTab === 'ALL') return true;
      if (filterTab === 'ACTIVE') return ['QUEUED', 'PREPARING', 'RUNNING', 'CANCELLING'].includes(j.status);
      if (filterTab === 'COMPLETED') return j.status === 'COMPLETED';
      if (filterTab === 'FAILED') return j.status === 'FAILED';
      if (filterTab === 'INTERRUPTED') return j.status === 'INTERRUPTED';
      if (filterTab === 'CANCELLED') return j.status === 'CANCELLED';
      return true;
    });
  }, [jobs, filterTab]);

  const handleCreateAndStartJob = async () => {
    if (!activeProject) return;
    try {
      const created = await createJob(activeProject.id, 'video_pipeline');
      await startJob(created.id);
    } catch (err) {
      console.error('Failed to create and start pipeline job:', err);
    }
  };

  const handleCopyLogs = async () => {
    if (currentLogs.length === 0) return;
    try {
      await navigator.clipboard.writeText(currentLogs.join('\n'));
      setCopiedLogs(true);
      setTimeout(() => setCopiedLogs(false), 2000);
    } catch (err) {
      console.error('Failed to copy logs:', err);
    }
  };

  const handleOpenOutputFolder = async (filePath: string) => {
    try {
      const dir = filePath.substring(0, Math.max(filePath.lastIndexOf('/'), filePath.lastIndexOf('\\')));
      await mediaApi.openDirectory(dir || filePath);
    } catch (err) {
      console.error('Failed to open directory:', err);
    }
  };

  const handleOpenVideoFile = async (filePath: string) => {
    try {
      await mediaApi.openFilePath(filePath);
    } catch (err) {
      console.error('Failed to open video file:', err);
    }
  };

  const getHumanReadableError = (error?: { code?: string; message?: string; details?: string }) => {
    if (!error) return null;
    switch (error.code) {
      case 'ModelHashMismatch':
        return {
          title: 'Model Integrity Verification Failed',
          description: 'The pinned model file SHA-256 fingerprint on disk does not match the registry manifest.',
          action: 'Reinstall or reactivate the model version in the Models tab.',
        };
      case 'ResourceLimitExceeded':
        return {
          title: 'Resource Limit Exceeded',
          description: 'The video frame resolution or tensor size exceeds safe production limits (4096px / 67.1M elements).',
          action: 'Select a standard resolution or adjust resource profile.',
        };
      case 'DiskQuotaExceeded':
        return {
          title: 'Disk Storage Quota Exceeded',
          description: 'The job exceeded the allowed disk storage budget (50 GB default quota).',
          action: 'Clean up old cached artifacts or free disk space.',
        };
      case 'FrameQualityFailed':
        return {
          title: 'Frame Technical Quality Check Failed',
          description: 'An AI inference frame produced empty, corrupt, or invalid pixel data.',
          action: 'Verify input video frames and preprocessing profile.',
        };
      case 'OutputNotFound':
        return {
          title: 'Output Video File Missing',
          description: 'FFmpeg reconstruction completed but the output video file was not found on disk.',
          action: 'Check FFmpeg logs and output folder permissions.',
        };
      case 'OutputInvalid':
        return {
          title: 'Output Validation Gate Failed',
          description: 'The reconstructed video failed duration matching, rational FPS, or stream integrity checks.',
          action: 'Retry reconstruction or verify source video streams.',
        };
      case 'FileNotFound':
        return {
          title: 'Input Media File Not Found',
          description: 'The source video file path could not be located on disk.',
          action: 'Verify the file path still exists and is accessible.',
        };
      default:
        return {
          title: 'Pipeline Execution Error',
          description: error.message || 'An unexpected error occurred during pipeline execution.',
          action: 'Review the technical log entries below and retry.',
        };
    }
  };

  const formatFileSize = (bytes: number): string => {
    if (!bytes || bytes <= 0) return '0 B';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  };

  const formatDuration = (startedAt?: string | null, completedAt?: string | null): string => {
    if (!startedAt) return '—';
    const start = new Date(startedAt).getTime();
    const end = completedAt ? new Date(completedAt).getTime() : Date.now();
    const diffMs = Math.max(0, end - start);
    const diffSec = diffMs / 1000;
    if (diffSec < 60) return `${diffSec.toFixed(1)}s`;
    const mins = Math.floor(diffSec / 60);
    const secs = Math.floor(diffSec % 60);
    return `${mins}m ${secs}s`;
  };

  const getStatusBadge = (status: JobStatus) => {
    switch (status) {
      case 'RUNNING':
      case 'PREPARING':
        return (
          <span className="px-2.5 py-0.5 rounded-full text-[10px] font-mono font-bold bg-indigo-500/20 text-indigo-300 border border-indigo-500/30 flex items-center gap-1.5 shadow-sm shadow-indigo-950/40">
            <Loader2 className="w-3 h-3 animate-spin text-indigo-400" />
            <span>{status}</span>
          </span>
        );
      case 'QUEUED':
        return (
          <span className="px-2.5 py-0.5 rounded-full text-[10px] font-mono font-bold bg-sky-500/20 text-sky-300 border border-sky-500/30 flex items-center gap-1.5">
            <Clock className="w-3 h-3 text-sky-400" />
            <span>QUEUED</span>
          </span>
        );
      case 'CANCELLING':
        return (
          <span className="px-2.5 py-0.5 rounded-full text-[10px] font-mono font-bold bg-amber-500/20 text-amber-300 border border-amber-500/30 flex items-center gap-1.5">
            <Loader2 className="w-3 h-3 animate-spin text-amber-400" />
            <span>CANCELLING</span>
          </span>
        );
      case 'COMPLETED':
        return (
          <span className="px-2.5 py-0.5 rounded-full text-[10px] font-mono font-bold bg-emerald-500/20 text-emerald-300 border border-emerald-500/30 flex items-center gap-1.5">
            <CheckCircle2 className="w-3 h-3 text-emerald-400" />
            <span>COMPLETED</span>
          </span>
        );
      case 'FAILED':
        return (
          <span className="px-2.5 py-0.5 rounded-full text-[10px] font-mono font-bold bg-rose-500/20 text-rose-300 border border-rose-500/30 flex items-center gap-1.5">
            <XCircle className="w-3 h-3 text-rose-400" />
            <span>FAILED</span>
          </span>
        );
      case 'CANCELLED':
        return (
          <span className="px-2.5 py-0.5 rounded-full text-[10px] font-mono font-bold bg-slate-800 text-slate-400 border border-slate-700 flex items-center gap-1.5">
            <Square className="w-2.5 h-2.5 fill-current text-slate-500" />
            <span>CANCELLED</span>
          </span>
        );
      case 'INTERRUPTED':
        return (
          <span className="px-2.5 py-0.5 rounded-full text-[10px] font-mono font-bold bg-amber-500/20 text-amber-300 border border-amber-500/30 flex items-center gap-1.5">
            <AlertTriangle className="w-3 h-3 text-amber-400" />
            <span>INTERRUPTED</span>
          </span>
        );
      default:
        return (
          <span className="px-2.5 py-0.5 rounded-full text-[10px] font-mono font-bold bg-slate-800 text-slate-300 border border-slate-700">
            {status}
          </span>
        );
    }
  };

  const getStageIcon = (status: StageStatus) => {
    switch (status) {
      case 'COMPLETED':
        return <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />;
      case 'RUNNING':
        return <Loader2 className="w-4 h-4 text-indigo-400 shrink-0 animate-spin" />;
      case 'FAILED':
        return <XCircle className="w-4 h-4 text-rose-400 shrink-0" />;
      case 'CANCELLED':
        return <Square className="w-4 h-4 text-slate-500 shrink-0" />;
      case 'SKIPPED':
        return <ArrowRight className="w-4 h-4 text-slate-400 shrink-0" />;
      case 'PAUSE_UNSUPPORTED':
        return <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0" />;
      default:
        return <Clock className="w-4 h-4 text-slate-600 shrink-0" />;
    }
  };

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden bg-slate-950 text-slate-100 font-sans">
      {/* ------------------------------------------------------------- */}
      {/* Header Bar */}
      {/* ------------------------------------------------------------- */}
      <div className="px-6 py-4 border-b border-slate-800 flex items-center justify-between shrink-0 bg-slate-950/90 backdrop-blur">
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-lg font-bold tracking-tight text-white flex items-center gap-2">
              <Film className="w-5 h-5 text-purple-400" />
              <span>Jobs & Pipeline Orchestrator</span>
            </h1>
            <span className="px-2.5 py-0.5 rounded text-[10px] font-mono font-bold bg-purple-500/20 text-purple-300 border border-purple-500/30">
              DESKTOP STUDIO ENGINE
            </span>
          </div>
          <p className="text-xs text-slate-400 mt-0.5 font-mono">
            {activeProject ? (
              <span>Project: <strong className="text-slate-200">{activeProject.name}</strong> ({activeProject.id})</span>
            ) : (
              <span>Select a project from workspace to manage pipeline executions.</span>
            )}
          </p>
        </div>

        <div className="flex items-center gap-2.5">
          <button
            onClick={() => fetchJobs(activeProject?.id)}
            className="px-3 py-1.5 rounded-xl bg-slate-900 border border-slate-800 hover:border-slate-700 text-slate-300 hover:text-white text-xs font-semibold flex items-center gap-1.5 transition-all cursor-pointer"
            title="Reload jobs and manifests from disk"
          >
            <RotateCw className={`w-3.5 h-3.5 ${isLoading ? 'animate-spin' : ''}`} />
            <span>Refresh</span>
          </button>

          <button
            onClick={handleCreateAndStartJob}
            disabled={!activeProject || !activeProject.sourceMedia}
            className="px-4 py-1.5 rounded-xl bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white text-xs font-bold shadow-lg shadow-purple-950/50 flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed transition-all cursor-pointer"
            title={!activeProject?.sourceMedia ? 'Import source media to run pipeline job' : 'Start pipeline execution'}
          >
            <Plus className="w-4 h-4" />
            <span>Run Pipeline Job</span>
          </button>
        </div>
      </div>

      {/* ------------------------------------------------------------- */}
      {/* Main Grid: Left Column (Job List) | Right Column (Selected Detail) */}
      {/* ------------------------------------------------------------- */}
      <div className="flex-1 grid grid-cols-1 lg:grid-cols-12 overflow-hidden">
        {/* Left Column (5 cols): Job List & Filter Tabs */}
        <div className="lg:col-span-5 border-r border-slate-800/80 flex flex-col h-full overflow-hidden bg-slate-950/60">
          {/* Filter Tab Bar */}
          <div className="p-2.5 border-b border-slate-800/60 flex items-center gap-1 overflow-x-auto text-[11px] font-semibold bg-slate-950/40">
            {(['ALL', 'ACTIVE', 'COMPLETED', 'FAILED', 'INTERRUPTED', 'CANCELLED'] as const).map((tab) => (
              <button
                key={tab}
                onClick={() => setFilterTab(tab)}
                className={`px-2.5 py-1.5 rounded-lg transition-all cursor-pointer flex items-center gap-1.5 shrink-0 ${
                  filterTab === tab
                    ? 'bg-slate-800 text-purple-300 border border-purple-500/30 shadow-sm font-bold'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900/60'
                }`}
              >
                <span>{tab}</span>
                <span className={`px-1.5 py-0.2 rounded-full text-[9px] font-mono ${
                  filterTab === tab ? 'bg-purple-500/20 text-purple-200' : 'bg-slate-800/80 text-slate-500'
                }`}>
                  {counts[tab]}
                </span>
              </button>
            ))}
          </div>

          {/* Job List Container */}
          <div className="flex-1 overflow-y-auto p-3.5 space-y-2.5">
            {isLoading && jobs.length === 0 ? (
              <div className="h-48 flex flex-col items-center justify-center text-center p-6 text-slate-500 text-xs space-y-2">
                <Loader2 className="w-6 h-6 animate-spin text-purple-400" />
                <span>Loading pipeline jobs from disk...</span>
              </div>
            ) : filteredJobs.length === 0 ? (
              <div className="h-48 flex flex-col items-center justify-center text-center p-6 border border-dashed border-slate-800/80 rounded-2xl text-slate-500 text-xs space-y-2">
                <Sparkles className="w-6 h-6 text-slate-600" />
                <span>No jobs matching filter "{filterTab}".</span>
                {filterTab !== 'ALL' && (
                  <button
                    onClick={() => setFilterTab('ALL')}
                    className="px-3 py-1 rounded-lg bg-slate-900 text-purple-300 text-[11px] font-semibold hover:bg-slate-800"
                  >
                    Clear Filter
                  </button>
                )}
              </div>
            ) : (
              filteredJobs.map((job) => {
                const isSelected = selectedJob?.id === job.id;
                return (
                  <div
                    key={job.id}
                    onClick={() => selectJob(job.id)}
                    className={`p-3.5 rounded-2xl border transition-all cursor-pointer space-y-2.5 ${
                      isSelected
                        ? 'bg-purple-950/20 border-purple-500/50 shadow-lg shadow-purple-950/40 ring-1 ring-purple-500/30'
                        : 'bg-slate-900/60 border-slate-800/80 hover:bg-slate-900 hover:border-slate-700/80'
                    }`}
                  >
                    {/* Header Row */}
                    <div className="flex items-center justify-between gap-2">
                      <div className="flex items-center gap-2 min-w-0">
                        <span className="text-xs font-mono font-bold text-slate-200 truncate">
                          {job.id}
                        </span>
                        {job.retryCount > 0 && (
                          <span className="px-1.5 py-0.2 rounded text-[9px] font-mono bg-purple-500/20 text-purple-300 border border-purple-500/30">
                            #{job.retryCount + 1}
                          </span>
                        )}
                      </div>
                      {getStatusBadge(job.status)}
                    </div>

                    {/* Progress & Stage Status */}
                    <div className="space-y-1">
                      <div className="flex items-center justify-between text-[10px] font-mono text-slate-400">
                        <span className="truncate max-w-[240px] text-slate-300">
                          {job.message || 'Queued'}
                        </span>
                        <span className="font-bold text-purple-300 ml-2">
                          {job.progress.toFixed(0)}%
                        </span>
                      </div>
                      <div className="h-1.5 w-full bg-slate-950 rounded-full overflow-hidden border border-slate-800">
                        <div
                          className={`h-full transition-all duration-300 ${
                            job.status === 'FAILED'
                              ? 'bg-rose-500'
                              : job.status === 'COMPLETED'
                              ? 'bg-emerald-500'
                              : job.status === 'INTERRUPTED'
                              ? 'bg-amber-500'
                              : 'bg-gradient-to-r from-purple-500 to-indigo-500'
                          }`}
                          style={{ width: `${Math.min(100, Math.max(0, job.progress))}%` }}
                        />
                      </div>
                    </div>

                    {/* Footer Info & Actions */}
                    <div className="flex items-center justify-between text-[11px] text-slate-400 pt-1.5 border-t border-slate-800/50">
                      <div className="flex items-center gap-2 text-[10px] font-mono text-slate-500">
                        <span>{new Date(job.createdAt).toLocaleTimeString()}</span>
                        <span>•</span>
                        <span>{formatDuration(job.startedAt, job.completedAt)}</span>
                      </div>

                      <div className="flex items-center gap-1.5">
                        {['RUNNING', 'PREPARING', 'QUEUED'].includes(job.status) && (
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              cancelJob(job.id);
                            }}
                            className="px-2 py-0.5 rounded bg-rose-500/20 text-rose-300 hover:bg-rose-500/30 text-[10px] font-bold border border-rose-500/30 cursor-pointer"
                          >
                            Cancel
                          </button>
                        )}

                        {job.status === 'CANCELLING' && (
                          <button
                            disabled
                            className="px-2 py-0.5 rounded bg-amber-500/10 text-amber-300/70 text-[10px] font-bold border border-amber-500/20 flex items-center gap-1 cursor-not-allowed"
                          >
                            <Loader2 className="w-2.5 h-2.5 animate-spin" />
                            <span>Cancelling</span>
                          </button>
                        )}

                        {['FAILED', 'CANCELLED', 'INTERRUPTED'].includes(job.status) && (
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              retryJob(job.id);
                            }}
                            className="px-2 py-0.5 rounded bg-purple-500/20 text-purple-300 hover:bg-purple-500/30 text-[10px] font-bold border border-purple-500/30 flex items-center gap-1 cursor-pointer"
                          >
                            <RotateCw className="w-2.5 h-2.5" />
                            <span>Retry</span>
                          </button>
                        )}

                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            deleteJob(job.id);
                          }}
                          className="p-1 rounded text-slate-500 hover:text-rose-400 hover:bg-rose-500/10 transition-colors cursor-pointer"
                          title="Delete Job Manifest"
                        >
                          <Trash2 className="w-3 h-3" />
                        </button>
                      </div>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </div>

        {/* ------------------------------------------------------------- */}
        {/* Right Column (7 cols): Selected Job Detail & Telemetry */}
        {/* ------------------------------------------------------------- */}
        <div className="lg:col-span-7 flex flex-col h-full overflow-hidden bg-slate-950">
          {selectedJob ? (
            <div className="flex-1 flex flex-col h-full overflow-y-auto p-5 space-y-5">
              {/* --------------------------------------------------------- */}
              {/* Job Header & Action Card */}
              {/* --------------------------------------------------------- */}
              <div className="p-4 rounded-2xl bg-slate-900/70 border border-slate-800 space-y-3 shadow-md">
                <div className="flex items-start justify-between gap-4">
                  <div>
                    <div className="flex items-center gap-2.5 flex-wrap">
                      <h2 className="text-sm font-bold text-white font-mono">{selectedJob.id}</h2>
                      {getStatusBadge(selectedJob.status)}
                      {selectedJob.retryCount > 0 && (
                        <span className="px-2 py-0.5 rounded text-[10px] font-mono bg-purple-500/20 text-purple-300 border border-purple-500/30 font-bold">
                          Retry Attempt #{selectedJob.retryCount + 1}
                        </span>
                      )}
                    </div>
                    <div className="flex items-center gap-3 text-xs text-slate-400 mt-1.5 font-mono">
                      <span>Project: <strong className="text-slate-200">{selectedJob.projectId}</strong></span>
                      <span>•</span>
                      <span>Created: {new Date(selectedJob.createdAt).toLocaleString()}</span>
                      <span>•</span>
                      <span>Duration: <strong className="text-purple-300">{formatDuration(selectedJob.startedAt, selectedJob.completedAt)}</strong></span>
                    </div>
                  </div>

                  {/* Primary Action Buttons */}
                  <div className="flex items-center gap-2 shrink-0">
                    {['QUEUED'].includes(selectedJob.status) && (
                      <button
                        onClick={() => startJob(selectedJob.id)}
                        className="px-3.5 py-1.5 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-bold flex items-center gap-1.5 cursor-pointer shadow-md shadow-emerald-950/50"
                      >
                        <Play className="w-3.5 h-3.5 fill-current" />
                        <span>Start Job</span>
                      </button>
                    )}

                    {['RUNNING', 'PREPARING'].includes(selectedJob.status) && (
                      <button
                        onClick={() => cancelJob(selectedJob.id)}
                        className="px-3.5 py-1.5 rounded-xl bg-rose-600 hover:bg-rose-500 text-white text-xs font-bold flex items-center gap-1.5 cursor-pointer shadow-md shadow-rose-950/50"
                      >
                        <Square className="w-3.5 h-3.5 fill-current" />
                        <span>Cancel Execution</span>
                      </button>
                    )}

                    {selectedJob.status === 'CANCELLING' && (
                      <button
                        disabled
                        className="px-3.5 py-1.5 rounded-xl bg-amber-500/20 text-amber-300 text-xs font-bold flex items-center gap-1.5 cursor-not-allowed border border-amber-500/30"
                      >
                        <Loader2 className="w-3.5 h-3.5 animate-spin" />
                        <span>Cancelling...</span>
                      </button>
                    )}

                    {['FAILED', 'CANCELLED', 'INTERRUPTED'].includes(selectedJob.status) && (
                      <button
                        onClick={() => retryJob(selectedJob.id)}
                        className="px-3.5 py-1.5 rounded-xl bg-purple-600 hover:bg-purple-500 text-white text-xs font-bold flex items-center gap-1.5 cursor-pointer shadow-md shadow-purple-950/50"
                      >
                        <RotateCw className="w-3.5 h-3.5" />
                        <span>Retry Pipeline (Attempt #{selectedJob.retryCount + 1})</span>
                      </button>
                    )}

                    {selectedJob.status === 'COMPLETED' && finalVideoArtifact && (
                      <div className="flex items-center gap-2">
                        <button
                          onClick={() => handleOpenVideoFile(finalVideoArtifact.path)}
                          className="px-3.5 py-1.5 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-bold flex items-center gap-1.5 cursor-pointer shadow-md shadow-emerald-950/50"
                        >
                          <Play className="w-3.5 h-3.5 fill-current" />
                          <span>Open Video</span>
                        </button>

                        <button
                          onClick={() => handleOpenOutputFolder(finalVideoArtifact.path)}
                          className="px-3.5 py-1.5 rounded-xl bg-emerald-600/20 hover:bg-emerald-600/30 text-emerald-300 border border-emerald-500/30 text-xs font-bold flex items-center gap-1.5 cursor-pointer"
                        >
                          <FolderOpen className="w-3.5 h-3.5" />
                          <span>Open Folder</span>
                        </button>

                        <button
                          onClick={() => setAppNavTab('models')}
                          className="px-3 py-1.5 rounded-xl bg-purple-600/20 hover:bg-purple-600/30 text-purple-300 border border-purple-500/30 text-xs font-bold flex items-center gap-1.5 cursor-pointer"
                        >
                          <Plus className="w-3.5 h-3.5" />
                          <span>New AI Job</span>
                        </button>
                      </div>
                    )}
                  </div>
                </div>

                {/* Overall Progress Bar */}
                <div className="space-y-1.5 pt-2.5 border-t border-slate-800">
                  <div className="flex items-center justify-between text-xs font-mono">
                    <span className="text-slate-300 font-semibold">{selectedJob.message}</span>
                    <span className="text-purple-300 font-bold text-sm">{selectedJob.progress.toFixed(1)}%</span>
                  </div>
                  <div className="h-2 w-full bg-slate-950 rounded-full overflow-hidden border border-slate-800">
                    <div
                      className={`h-full transition-all duration-300 ${
                        selectedJob.status === 'FAILED'
                          ? 'bg-rose-500'
                          : selectedJob.status === 'COMPLETED'
                          ? 'bg-emerald-500'
                          : selectedJob.status === 'INTERRUPTED'
                          ? 'bg-amber-500'
                          : 'bg-gradient-to-r from-purple-500 to-indigo-500'
                      }`}
                      style={{ width: `${Math.min(100, Math.max(0, selectedJob.progress))}%` }}
                    />
                  </div>
                </div>

                {/* AI Inference Telemetry Strip (if AI configured) */}
                {selectedJob.aiConfig && (
                  <div className="p-3.5 rounded-xl bg-purple-950/20 border border-purple-500/30 space-y-2 mt-2">
                    <div className="flex items-center justify-between text-xs flex-wrap gap-2">
                      <div className="flex items-center gap-2 flex-wrap">
                        <Sparkles className="w-4 h-4 text-purple-400 shrink-0" />
                        <span className="font-bold text-purple-200">Production AI Model Pinned</span>
                        <span className="px-2 py-0.5 rounded text-[10px] font-mono bg-purple-500/20 text-purple-300 border border-purple-500/30 font-bold">
                          {selectedJob.aiConfig.modelId} {selectedJob.aiConfig.modelVersion ? `v${selectedJob.aiConfig.modelVersion}` : ''}
                        </span>
                        {selectedJob.aiConfig.provider && (
                          <span className="px-1.5 py-0.5 rounded text-[9px] font-mono bg-indigo-500/20 text-indigo-300 border border-indigo-500/30 font-bold">
                            {selectedJob.aiConfig.provider}
                          </span>
                        )}
                        {selectedJob.aiConfig.modelHash && (
                          <span className="px-1.5 py-0.5 rounded text-[9px] font-mono bg-slate-900 text-slate-400 border border-slate-800" title={selectedJob.aiConfig.modelHash}>
                            sha256:{selectedJob.aiConfig.modelHash.slice(0, 8)}...
                          </span>
                        )}
                      </div>
                      <span className="text-[10px] font-mono text-slate-400">
                        Sampling: {selectedJob.aiConfig.frameSampling.mode}
                      </span>
                    </div>

                    {selectedJob.aiMetrics && (
                      <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 pt-1 font-mono text-[11px]">
                        <div className="p-2 rounded-lg bg-slate-950/60 border border-slate-800">
                          <div className="text-slate-500 text-[10px]">Processed</div>
                          <div className="text-emerald-400 font-bold">
                            {selectedJob.aiMetrics.framesProcessed} / {selectedJob.aiMetrics.framesTotal}
                          </div>
                        </div>
                        <div className="p-2 rounded-lg bg-slate-950/60 border border-slate-800">
                          <div className="text-slate-500 text-[10px]">Reused / Cached</div>
                          <div className="text-purple-400 font-bold">
                            {selectedJob.aiMetrics.framesReused}
                          </div>
                        </div>
                        <div className="p-2 rounded-lg bg-slate-950/60 border border-slate-800">
                          <div className="text-slate-500 text-[10px]">Passthrough</div>
                          <div className="text-slate-400 font-bold">
                            {selectedJob.aiMetrics.framesPassthrough}
                          </div>
                        </div>
                        <div className="p-2 rounded-lg bg-slate-950/60 border border-slate-800">
                          <div className="text-slate-500 text-[10px]">Avg Inference</div>
                          <div className="text-indigo-300 font-bold">
                            {selectedJob.aiMetrics.averageInferenceDurationMs.toFixed(1)} ms
                          </div>
                        </div>
                        {selectedJob.aiMetrics.artifactBytesWritten !== undefined && selectedJob.aiMetrics.artifactBytesWritten > 0 && (
                          <div className="p-2 rounded-lg bg-slate-950/60 border border-slate-800">
                            <div className="text-slate-500 text-[10px]">Artifacts Written</div>
                            <div className="text-slate-300 font-bold">
                              {(selectedJob.aiMetrics.artifactBytesWritten / (1024 * 1024)).toFixed(2)} MB
                            </div>
                          </div>
                        )}
                        {selectedJob.aiMetrics.etaMs !== undefined && selectedJob.aiMetrics.etaMs !== null && (
                          <div className="p-2 rounded-lg bg-slate-950/60 border border-slate-800">
                            <div className="text-slate-500 text-[10px]">Inference ETA</div>
                            <div className="text-amber-300 font-bold">
                              {(selectedJob.aiMetrics.etaMs / 1000).toFixed(1)}s
                            </div>
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                )}

                {/* Reconstructed Video Production Output Summary */}
                {finalVideoArtifact && (
                  <div className="p-3.5 rounded-xl bg-indigo-950/20 border border-indigo-500/30 space-y-2 mt-2">
                    <div className="flex items-center justify-between text-xs">
                      <div className="flex items-center gap-2">
                        <Film className="w-4 h-4 text-indigo-400 shrink-0" />
                        <span className="font-bold text-indigo-200">Reconstructed Video Production Output</span>
                        <span className="px-2 py-0.5 rounded text-[10px] font-mono bg-indigo-500/20 text-indigo-300 border border-indigo-500/30 font-bold">
                          H.264 / AAC
                        </span>
                      </div>
                      <span className="text-[10px] font-mono text-emerald-400 font-bold">
                        Verified Artifact
                      </span>
                    </div>

                    <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 pt-1 font-mono text-[11px]">
                      <div className="p-2 rounded-lg bg-slate-950/60 border border-slate-800">
                        <div className="text-slate-500 text-[10px]">File Size</div>
                        <div className="text-indigo-300 font-bold">
                          {(finalVideoArtifact.fileSizeBytes / (1024 * 1024)).toFixed(2)} MB
                        </div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950/60 border border-slate-800">
                        <div className="text-slate-500 text-[10px]">Output Target</div>
                        <div className="text-slate-300 font-bold truncate" title={finalVideoArtifact.path}>
                          {finalVideoArtifact.path.split(/[\\/]/).pop()}
                        </div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950/60 border border-slate-800">
                        <div className="text-slate-500 text-[10px]">Status</div>
                        <div className="text-emerald-400 font-bold">PASS (Validated)</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950/60 border border-slate-800">
                        <div className="text-slate-500 text-[10px]">Manifest</div>
                        <div className="text-purple-300 font-bold">Generated</div>
                      </div>
                    </div>
                  </div>
                )}
              </div>

              {/* --------------------------------------------------------- */}
              {/* Interrupted Banner (if applicable) */}
              {/* --------------------------------------------------------- */}
              {selectedJob.status === 'INTERRUPTED' && (
                <div className="p-4 rounded-2xl bg-amber-950/20 border border-amber-500/40 flex items-start justify-between gap-3 text-amber-200 text-xs">
                  <div className="flex items-start gap-2.5">
                    <AlertTriangle className="w-5 h-5 text-amber-400 shrink-0 mt-0.5" />
                    <div>
                      <h4 className="font-bold text-amber-300">Job Execution Interrupted</h4>
                      <p className="text-amber-200/80 mt-0.5">
                        Execution stopped due to application shutdown or restart. Verified frame & audio artifacts remain cached on disk. Click Retry to safely resume from where it left off.
                      </p>
                    </div>
                  </div>
                  <button
                    onClick={() => retryJob(selectedJob.id)}
                    className="px-3 py-1.5 rounded-xl bg-amber-500 text-slate-950 font-bold hover:bg-amber-400 shrink-0 transition-all cursor-pointer"
                  >
                    Resume Pipeline
                  </button>
                </div>
              )}

              {/* --------------------------------------------------------- */}
              {/* Failed Error Banner (Enhanced Domain-Specific UX) */}
              {/* --------------------------------------------------------- */}
              {selectedJob.status === 'FAILED' && (() => {
                const diag = getHumanReadableError(selectedJob.error);
                return (
                  <div className="p-4 rounded-2xl bg-rose-950/20 border border-rose-500/40 space-y-3 text-xs">
                    <div className="flex items-start justify-between gap-3">
                      <div className="flex items-start gap-2.5">
                        <XCircle className="w-5 h-5 text-rose-400 shrink-0 mt-0.5" />
                        <div className="space-y-1">
                          <h4 className="font-bold text-rose-300 text-sm">{diag?.title || 'Pipeline Execution Failed'}</h4>
                          <p className="text-rose-200 font-sans leading-relaxed">
                            {diag?.description || selectedJob.error?.message || selectedJob.message}
                          </p>
                          {diag?.action && (
                            <div className="mt-2 p-2.5 rounded-xl bg-slate-950/80 border border-rose-500/30 text-rose-300 font-mono text-[11px] flex items-center gap-1.5">
                              <span className="font-bold text-rose-400">Recommended Action:</span>
                              <span>{diag.action}</span>
                            </div>
                          )}
                          {selectedJob.error?.code && (
                            <span className="inline-block mt-1 px-2 py-0.5 rounded text-[10px] font-mono bg-rose-500/20 text-rose-300 border border-rose-500/30 font-bold">
                              Error Code: {selectedJob.error.code}
                            </span>
                          )}
                        </div>
                      </div>

                      <button
                        onClick={() => retryJob(selectedJob.id)}
                        className="px-3.5 py-1.5 rounded-xl bg-rose-600 text-white font-bold hover:bg-rose-500 shrink-0 transition-all cursor-pointer shadow-md shadow-rose-950/50"
                      >
                        Retry Pipeline
                      </button>
                    </div>

                    {selectedJob.error?.details && (
                      <div className="pt-2 border-t border-rose-500/20">
                        <button
                          onClick={() => setShowErrorDetails(!showErrorDetails)}
                          className="text-[11px] text-rose-300 font-semibold underline cursor-pointer"
                        >
                          {showErrorDetails ? 'Hide technical diagnostics' : 'Show technical error diagnostics'}
                        </button>
                        {showErrorDetails && (
                          <pre className="mt-1.5 p-2.5 rounded-lg bg-black/60 font-mono text-[10px] text-rose-200 overflow-x-auto whitespace-pre-wrap">
                            {selectedJob.error.details}
                          </pre>
                        )}
                      </div>
                    )}
                  </div>
                );
              })()}

              {/* --------------------------------------------------------- */}
              {/* Pipeline Stages Breakdown (Dynamic Stepper) */}
              {/* --------------------------------------------------------- */}
              <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Layers className="w-4 h-4 text-purple-400" />
                    <span className="text-xs font-bold text-slate-200">
                      Pipeline Execution Stages ({selectedJob.stages.length})
                    </span>
                  </div>
                  <span className="text-[10px] font-mono text-slate-500">
                    Current: Stage {selectedJob.currentStageIndex + 1} of {selectedJob.totalStages}
                  </span>
                </div>

                <div className="space-y-2">
                  {selectedJob.stages.map((stage, idx) => (
                    <div
                      key={stage.id}
                      className={`p-3 rounded-xl border space-y-2 transition-all ${
                        stage.status === 'RUNNING'
                          ? 'bg-indigo-950/30 border-indigo-500/50 shadow-md text-indigo-100 ring-1 ring-indigo-500/30'
                          : stage.status === 'COMPLETED'
                          ? 'bg-slate-950/80 border-slate-800/80 text-slate-300'
                          : stage.status === 'FAILED'
                          ? 'bg-rose-950/30 border-rose-500/40 text-rose-200'
                          : stage.status === 'CANCELLED'
                          ? 'bg-slate-950/40 border-slate-800/40 text-slate-500'
                          : 'bg-slate-950/40 border-slate-800/40 text-slate-500'
                      }`}
                    >
                      <div className="flex items-center justify-between text-xs">
                        <div className="flex items-center gap-3">
                          <span className={`w-5 h-5 rounded-full flex items-center justify-center font-mono text-[10px] font-bold ${
                            stage.status === 'COMPLETED' ? 'bg-emerald-500/20 text-emerald-300' :
                            stage.status === 'RUNNING' ? 'bg-indigo-500/30 text-indigo-200 ring-2 ring-indigo-400' :
                            stage.status === 'FAILED' ? 'bg-rose-500/20 text-rose-300' : 'bg-slate-800 text-slate-500'
                          }`}>
                            {idx + 1}
                          </span>
                          {getStageIcon(stage.status)}
                          <div>
                            <span className="font-semibold text-slate-200 block">{stage.name}</span>
                            <span className="text-[10px] text-slate-400 font-mono block truncate max-w-[340px]">
                              {stage.message || 'Awaiting execution'}
                            </span>
                          </div>
                        </div>

                        <div className="flex items-center gap-2 font-mono text-[10px]">
                          {stage.status === 'RUNNING' && stage.progress > 0 && (
                            <span className="text-indigo-300 font-bold">{stage.progress.toFixed(0)}%</span>
                          )}
                          <span className={`px-2 py-0.5 rounded font-bold uppercase text-[9px] ${
                            stage.status === 'COMPLETED' ? 'text-emerald-400 bg-emerald-500/10 border border-emerald-500/20' :
                            stage.status === 'RUNNING' ? 'text-indigo-300 bg-indigo-500/20 border border-indigo-500/30 animate-pulse' :
                            stage.status === 'FAILED' ? 'text-rose-400 bg-rose-500/10 border border-rose-500/20' :
                            stage.status === 'CANCELLED' ? 'text-slate-400 bg-slate-800' : 'text-slate-500 bg-slate-900'
                          }`}>
                            {stage.status}
                          </span>
                        </div>
                      </div>

                      {stage.status === 'RUNNING' && (
                        <div className="h-1 w-full bg-slate-950 rounded-full overflow-hidden border border-slate-800">
                          <div
                            className="h-full bg-gradient-to-r from-indigo-500 to-purple-500 transition-all duration-200"
                            style={{ width: `${Math.min(100, Math.max(0, stage.progress))}%` }}
                          />
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              </div>

              {/* --------------------------------------------------------- */}
              {/* Bottom Tabs: Logs | Output Preview | Artifacts */}
              {/* --------------------------------------------------------- */}
              <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3">
                <div className="flex items-center justify-between border-b border-slate-800 pb-2.5">
                  <div className="flex items-center gap-1.5">
                    <button
                      onClick={() => setActiveBottomTab('LOGS')}
                      className={`px-3 py-1.5 rounded-lg text-xs font-bold transition-all cursor-pointer ${
                        activeBottomTab === 'LOGS'
                          ? 'bg-slate-800 text-purple-300 border border-purple-500/30'
                          : 'text-slate-400 hover:text-slate-200'
                      }`}
                    >
                      <div className="flex items-center gap-1.5">
                        <Terminal className="w-3.5 h-3.5" />
                        <span>Terminal Logs ({currentLogs.length})</span>
                      </div>
                    </button>

                    <button
                      onClick={() => setActiveBottomTab('OUTPUT')}
                      className={`px-3 py-1.5 rounded-lg text-xs font-bold transition-all cursor-pointer ${
                        activeBottomTab === 'OUTPUT'
                          ? 'bg-slate-800 text-purple-300 border border-purple-500/30'
                          : 'text-slate-400 hover:text-slate-200'
                      }`}
                    >
                      <div className="flex items-center gap-1.5">
                        <Video className="w-3.5 h-3.5" />
                        <span>Output Preview {finalVideoArtifact ? '✓' : ''}</span>
                      </div>
                    </button>

                    <button
                      onClick={() => setActiveBottomTab('ARTIFACTS')}
                      className={`px-3 py-1.5 rounded-lg text-xs font-bold transition-all cursor-pointer ${
                        activeBottomTab === 'ARTIFACTS'
                          ? 'bg-slate-800 text-purple-300 border border-purple-500/30'
                          : 'text-slate-400 hover:text-slate-200'
                      }`}
                    >
                      <div className="flex items-center gap-1.5">
                        <Layers className="w-3.5 h-3.5" />
                        <span>Artifacts Explorer ({currentArtifacts.length})</span>
                      </div>
                    </button>
                  </div>

                  {/* Logs Controls */}
                  {activeBottomTab === 'LOGS' && (
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => setAutoScroll(!autoScroll)}
                        className={`px-2.5 py-1 rounded text-[10px] font-mono font-bold flex items-center gap-1 border transition-all cursor-pointer ${
                          autoScroll
                            ? 'bg-emerald-500/10 text-emerald-300 border-emerald-500/30'
                            : 'bg-amber-500/10 text-amber-300 border-amber-500/30'
                        }`}
                        title={autoScroll ? 'Auto-scroll is ON' : 'Auto-scroll is PAUSED'}
                      >
                        {autoScroll ? <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" /> : <Pause className="w-2.5 h-2.5" />}
                        <span>{autoScroll ? 'Auto-scroll: ON' : 'Auto-scroll: PAUSED'}</span>
                      </button>

                      <button
                        onClick={handleCopyLogs}
                        disabled={currentLogs.length === 0}
                        className="px-2.5 py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 text-[10px] font-mono font-bold flex items-center gap-1 border border-slate-700 transition-all cursor-pointer disabled:opacity-50"
                        title="Copy logs to clipboard"
                      >
                        {copiedLogs ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                        <span>{copiedLogs ? 'Copied' : 'Copy'}</span>
                      </button>
                    </div>
                  )}
                </div>

                {/* ----------------------------------------------------- */}
                {/* Tab Content: Logs */}
                {/* ----------------------------------------------------- */}
                {activeBottomTab === 'LOGS' && (
                  <div
                    ref={logContainerRef}
                    className="h-56 overflow-y-auto p-3.5 rounded-xl bg-slate-950 border border-slate-800/80 font-mono text-[11px] space-y-1 select-text scroll-smooth"
                  >
                    {currentLogs.length === 0 ? (
                      <span className="text-slate-600">No log entries recorded yet for this job.</span>
                    ) : (
                      currentLogs.map((line, idx) => (
                        <div
                          key={idx}
                          className={
                            line.includes('ERROR') || line.includes('✗')
                              ? 'text-rose-400'
                              : line.includes('WARN') || line.includes('RECOVERY')
                              ? 'text-amber-400'
                              : line.includes('✓') || line.includes('⚡')
                              ? 'text-emerald-400'
                              : 'text-slate-300'
                          }
                        >
                          {line}
                        </div>
                      ))
                    )}
                  </div>
                )}

                {/* ----------------------------------------------------- */}
                {/* Tab Content: Output Video Preview */}
                {/* ----------------------------------------------------- */}
                {activeBottomTab === 'OUTPUT' && (
                  <div className="space-y-3">
                    {outputSrc && finalVideoArtifact ? (
                      <div className="space-y-3">
                        <div className="relative rounded-xl overflow-hidden bg-black aspect-video max-h-64 flex items-center justify-center border border-slate-800 shadow-inner">
                          <video
                            key={outputSrc}
                            src={outputSrc}
                            controls
                            playsInline
                            className="w-full h-full object-contain"
                          />
                        </div>

                        {/* Metadata Cards */}
                        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-xs font-mono">
                          <div className="p-2.5 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                            <span className="text-[10px] text-slate-500 uppercase block">Size</span>
                            <span className="font-bold text-slate-200">{formatFileSize(finalVideoArtifact.fileSizeBytes)}</span>
                          </div>
                          <div className="p-2.5 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                            <span className="text-[10px] text-slate-500 uppercase block">Status</span>
                            <span className="font-bold text-emerald-400">Validated Pass</span>
                          </div>
                          <div className="p-2.5 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                            <span className="text-[10px] text-slate-500 uppercase block">Duration</span>
                            <span className="font-bold text-purple-300">{formatDuration(selectedJob.startedAt, selectedJob.completedAt)}</span>
                          </div>
                          <div className="p-2.5 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5 flex items-center justify-between">
                            <div>
                              <span className="text-[10px] text-slate-500 uppercase block">Folder</span>
                              <span className="font-bold text-slate-300">Disk Location</span>
                            </div>
                            <button
                              onClick={() => handleOpenOutputFolder(finalVideoArtifact.path)}
                              className="p-1 rounded bg-slate-800 hover:bg-slate-700 text-purple-300 cursor-pointer"
                              title="Reveal output video on disk"
                            >
                              <FolderOpen className="w-3.5 h-3.5" />
                            </button>
                          </div>
                        </div>

                        <div className="p-2.5 rounded-lg bg-slate-950/80 border border-slate-800 text-[11px] font-mono text-slate-400 truncate select-all">
                          <span className="text-slate-600 mr-2">Path:</span>
                          {finalVideoArtifact.path}
                        </div>
                      </div>
                    ) : (
                      <div className="h-44 flex flex-col items-center justify-center text-center p-6 border border-dashed border-slate-800/80 rounded-xl text-slate-500 text-xs space-y-2">
                        <Video className="w-7 h-7 text-slate-600" />
                        <span>
                          {selectedJob.status === 'COMPLETED'
                            ? 'No final video artifact registered in job manifest.'
                            : 'Output preview will be available once the pipeline completes Stage 5 & 6.'}
                        </span>
                      </div>
                    )}
                  </div>
                )}

                {/* ----------------------------------------------------- */}
                {/* Tab Content: Artifacts Explorer */}
                {/* ----------------------------------------------------- */}
                {activeBottomTab === 'ARTIFACTS' && (
                  <div className="space-y-3">
                    {currentArtifacts.length === 0 ? (
                      <div className="h-40 flex flex-col items-center justify-center text-center p-6 text-slate-500 text-xs">
                        <span>No artifacts generated for this job yet.</span>
                      </div>
                    ) : (
                      <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5">
                        {currentArtifacts.map((art) => (
                          <div
                            key={art.id}
                            className="p-3 rounded-xl bg-slate-950 border border-slate-800/80 text-xs space-y-1.5 font-mono"
                          >
                            <div className="flex items-center justify-between text-slate-300 font-semibold">
                              <div className="flex items-center gap-1.5">
                                {art.artifactType.includes('video') ? (
                                  <FileVideo className="w-3.5 h-3.5 text-purple-400" />
                                ) : art.artifactType.includes('audio') ? (
                                  <FileAudio className="w-3.5 h-3.5 text-sky-400" />
                                ) : (
                                  <FileText className="w-3.5 h-3.5 text-emerald-400" />
                                )}
                                <span className="uppercase text-[10px] text-purple-300 font-bold">{art.artifactType}</span>
                              </div>
                              <span className="text-[10px] text-slate-500">{formatFileSize(art.fileSizeBytes)}</span>
                            </div>

                            <p className="text-[11px] text-slate-400 truncate select-all" title={art.path}>
                              {art.path}
                            </p>

                            <div className="flex items-center justify-between text-[10px] text-slate-500 pt-1 border-t border-slate-900">
                              <span>Stage: {art.stageId || 'general'}</span>
                              <button
                                onClick={() => handleOpenOutputFolder(art.path)}
                                className="text-purple-400 hover:text-purple-300 flex items-center gap-1 cursor-pointer"
                              >
                                <FolderOpen className="w-2.5 h-2.5" />
                                <span>Reveal</span>
                              </button>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          ) : (
            <div className="flex-1 flex flex-col items-center justify-center text-center p-8 text-slate-500 space-y-3">
              <Layers className="w-10 h-10 text-slate-700" />
              <div>
                <h3 className="text-sm font-bold text-slate-400">No Pipeline Job Selected</h3>
                <p className="text-xs text-slate-600 mt-1">Select a job from the left panel or click "Run Pipeline Job" to launch a new execution.</p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
