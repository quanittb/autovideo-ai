import React, { useState, useEffect } from 'react';
import { 
  CheckCircle2, 
  XCircle, 
  Play, 
  FolderOpen, 
  Terminal, 
  Film, 
  RotateCw, 
  Sliders, 
  FileCode,
  ShieldCheck,
  Loader2
} from 'lucide-react';
import { mediaApi } from '../../lib/ipc';
import { useProjectStore } from '../../stores/projectStore';
import { 
  CacheValidationReport, 
  FrameExtractionResult, 
  AudioExtractionResult, 
  MediaRuntimeStatus 
} from '../../types/contracts';
import { VideoDropZone } from './components/VideoDropZone';

export const MediaVerificationRunner: React.FC = () => {
  const { activeProject, importMediaToProject, loadProject } = useProjectStore();
  const [runtimeStatus, setRuntimeStatus] = useState<MediaRuntimeStatus | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [frameResult, setFrameResult] = useState<FrameExtractionResult | null>(null);
  const [audioResult, setAudioResult] = useState<AudioExtractionResult | null>(null);
  const [validationReport, setValidationReport] = useState<CacheValidationReport | null>(null);
  const [prepStatus, setPrepStatus] = useState<'idle' | 'running' | 'pass' | 'fail'>('idle');

  const addLog = (msg: string) => {
    const time = new Date().toLocaleTimeString();
    setLogLines((prev) => [...prev, `[${time}] ${msg}`]);
  };

  const handleCheckRuntime = async () => {
    addLog('Querying host FFmpeg/FFprobe binaries via mediaApi.getRuntimeStatus()...');
    try {
      const status = await mediaApi.getRuntimeStatus();
      setRuntimeStatus(status);
      if (status.ffmpeg.available) {
        addLog(`✓ FFmpeg detected: ${status.ffmpeg.version}`);
      } else {
        addLog('✗ FFmpeg NOT available in system PATH');
      }
      if (status.ffprobe.available) {
        addLog(`✓ FFprobe detected: ${status.ffprobe.version}`);
      } else {
        addLog('✗ FFprobe NOT available in system PATH');
      }
    } catch (err: any) {
      addLog(`✗ Error checking runtime: ${err?.message || err}`);
    }
  };

  const handlePrepareMedia = async () => {
    if (!activeProject || !activeProject.sourceMedia) {
      addLog('✗ Error: No active project with imported media');
      return;
    }
    setPrepStatus('running');
    addLog(`Preparing media cache directories for media ID: ${activeProject.sourceMedia.mediaId}...`);
    try {
      const dir = await mediaApi.prepareMedia(activeProject.id, activeProject.sourceMedia.mediaId);
      setPrepStatus('pass');
      addLog(`✓ Media workspace prepared at: ${dir}`);
    } catch (err: any) {
      setPrepStatus('fail');
      addLog(`✗ Media preparation failed: ${err?.message || err}`);
    }
  };

  const handleExtractFrames = async () => {
    if (!activeProject || !activeProject.sourceMedia) {
      addLog('✗ Error: Please import a video first.');
      return;
    }
    addLog('Executing REAL FFmpeg Frame Extraction (start: 0s, end: 3s, fps: 2, format: png)...');
    try {
      const res = await mediaApi.extractFrames({
        projectId: activeProject.id,
        mediaId: activeProject.sourceMedia.mediaId,
        startTimeSeconds: 0,
        endTimeSeconds: 3,
        fps: 2,
        format: 'png',
      });
      setFrameResult(res);
      addLog(`✓ Frame Extraction complete. Generated ${res.frameCount} frames at ${res.fps} FPS (cached: ${res.isCached})`);
      await handleValidateCache();
    } catch (err: any) {
      addLog(`✗ Frame extraction failed: ${err?.message || err}`);
    }
  };

  const handleExtractAudio = async () => {
    if (!activeProject || !activeProject.sourceMedia) {
      addLog('✗ Error: Please import a video first.');
      return;
    }
    addLog('Executing REAL FFmpeg Audio Extraction to 16-bit PCM WAV (source.wav)...');
    try {
      const res = await mediaApi.extractAudio(activeProject.id, activeProject.sourceMedia.mediaId);
      setAudioResult(res);
      if (res.hasAudio) {
        addLog(`✓ Audio extracted successfully to: ${res.audioPath} (${res.sampleRate}Hz stereo, cached: ${res.isCached})`);
      } else {
        addLog('ℹ Source video has NO audio track (NO_AUDIO_STREAM). Handled safely.');
      }
      await handleValidateCache();
    } catch (err: any) {
      addLog(`✗ Audio extraction failed: ${err?.message || err}`);
    }
  };

  const handleValidateCache = async () => {
    if (!activeProject || !activeProject.sourceMedia) return;
    try {
      const rep = await mediaApi.validateCache(activeProject.id, activeProject.sourceMedia.mediaId);
      setValidationReport(rep);
      addLog(`✓ Disk Validation: ${rep.totalFramesOnDisk} PNG frames verified on disk, manifestValid=${rep.isManifestValid}`);
    } catch (err: any) {
      addLog(`✗ Cache validation error: ${err?.message || err}`);
    }
  };

  const handleOpenFolder = async () => {
    const targetDir = validationReport?.mediaCacheDir;
    if (!targetDir) {
      addLog('✗ No output folder known yet. Run tests first.');
      return;
    }
    addLog(`Opening directory in Windows Explorer: ${targetDir}`);
    try {
      await mediaApi.openDirectory(targetDir);
    } catch (err: any) {
      addLog(`✗ Could not open directory: ${err?.message || err}`);
    }
  };

  const handleRunAllTests = async () => {
    setIsRunning(true);
    addLog('=== STARTING FULL MEDIA ENGINE VERIFICATION SUITE ===');
    await handleCheckRuntime();
    await handlePrepareMedia();
    await handleExtractFrames();
    await handleExtractAudio();
    await handleValidateCache();
    addLog('=== ALL VERIFICATION TESTS COMPLETED ===');
    setIsRunning(false);
  };

  const handleReloadProject = async () => {
    if (!activeProject) return;
    addLog(`Reloading project ${activeProject.id} from disk...`);
    try {
      await loadProject(activeProject.id);
      await handleValidateCache();
      addLog('✓ Project reloaded successfully from project.json manifest.');
    } catch (err: any) {
      addLog(`✗ Reload failed: ${err?.message || err}`);
    }
  };

  useEffect(() => {
    handleCheckRuntime();
  }, []);

  const sourceMedia = activeProject?.sourceMedia;

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100 font-sans">
      {/* Header */}
      <div className="flex items-center justify-between pb-4 border-b border-slate-800">
        <div>
          <div className="flex items-center gap-2">
            <h1 className="text-2xl font-bold tracking-tight text-white">Media Engine Verification Runner</h1>
            <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-purple-500/20 text-purple-300 border border-purple-500/30">
              PHASE 4A REAL FFMPEG CORE
            </span>
          </div>
          <p className="text-xs text-slate-400 mt-1">
            Real process execution test runner: verifies FFmpeg discovery, frame extraction (2 FPS, 0-3s), audio extraction, binary headers, and cache manifests.
          </p>
        </div>

        <button
          onClick={handleRunAllTests}
          disabled={isRunning || !sourceMedia}
          className="px-5 py-2.5 rounded-xl bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white text-xs font-bold shadow-lg shadow-purple-900/30 flex items-center gap-2 disabled:opacity-50 transition-all"
        >
          {isRunning ? <Loader2 className="w-4 h-4 animate-spin" /> : <Play className="w-4 h-4 fill-current" />}
          <span>Run All Tests</span>
        </button>
      </div>

      {/* Grid: 2 Columns */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-start">
        {/* Left Column (5 cols): Test Controls & Target Video */}
        <div className="lg:col-span-5 space-y-5">
          {/* Target Video Ingestion */}
          <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-slate-200">Test Source Video</span>
              {sourceMedia && (
                <span className="text-[10px] font-mono text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/20">
                  READY
                </span>
              )}
            </div>

            <VideoDropZone
              onVideoSelected={async (path) => {
                if (activeProject) {
                  await importMediaToProject(activeProject.id, path);
                  addLog(`✓ Video imported into project: ${path}`);
                }
              }}
              hasImportedVideo={!!sourceMedia}
            />

            {sourceMedia && (
              <div className="p-3.5 rounded-xl bg-slate-950 border border-slate-800/80 text-xs space-y-1.5 font-mono">
                <div className="flex justify-between text-slate-300 font-semibold truncate">
                  <span className="text-slate-500">File:</span>
                  <span className="truncate ml-2">{sourceMedia.originalFileName}</span>
                </div>
                <div className="flex justify-between text-slate-400 text-[11px]">
                  <span>Resolution:</span>
                  <span>{sourceMedia.width}x{sourceMedia.height} ({sourceMedia.fps} FPS)</span>
                </div>
                <div className="flex justify-between text-slate-400 text-[11px]">
                  <span>Duration:</span>
                  <span>{(sourceMedia.durationMs / 1000).toFixed(1)}s • {(sourceMedia.fileSizeBytes / (1024 * 1024)).toFixed(1)} MB</span>
                </div>
                <div className="flex justify-between text-slate-400 text-[11px]">
                  <span>Codecs:</span>
                  <span className="uppercase">{sourceMedia.videoCodec} / {sourceMedia.audioCodec || 'None'}</span>
                </div>
              </div>
            )}
          </div>

          {/* Test Control Action Buttons */}
          <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3">
            <span className="text-xs font-bold text-slate-200 block">Manual Test Controls</span>
            <div className="grid grid-cols-2 gap-2 text-xs">
              <button
                onClick={handleCheckRuntime}
                className="p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-medium text-left flex items-center gap-2 transition-colors"
              >
                <ShieldCheck className="w-3.5 h-3.5 text-indigo-400" />
                <span>Check FFmpeg/probe</span>
              </button>

              <button
                onClick={handlePrepareMedia}
                disabled={!sourceMedia}
                className="p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-medium text-left flex items-center gap-2 disabled:opacity-40 transition-colors"
              >
                <Sliders className="w-3.5 h-3.5 text-sky-400" />
                <span>Prepare Media</span>
              </button>

              <button
                onClick={handleExtractFrames}
                disabled={!sourceMedia}
                className="p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-medium text-left flex items-center gap-2 disabled:opacity-40 transition-colors"
              >
                <Film className="w-3.5 h-3.5 text-purple-400" />
                <span>Extract Test Frames</span>
              </button>

              <button
                onClick={handleExtractAudio}
                disabled={!sourceMedia}
                className="p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-medium text-left flex items-center gap-2 disabled:opacity-40 transition-colors"
              >
                <FileCode className="w-3.5 h-3.5 text-amber-400" />
                <span>Extract Audio</span>
              </button>

              <button
                onClick={handleOpenFolder}
                disabled={!validationReport}
                className="p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-medium text-left flex items-center gap-2 disabled:opacity-40 transition-colors"
              >
                <FolderOpen className="w-3.5 h-3.5 text-emerald-400" />
                <span>Open Output Folder</span>
              </button>

              <button
                onClick={handleReloadProject}
                disabled={!activeProject}
                className="p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-medium text-left flex items-center gap-2 disabled:opacity-40 transition-colors"
              >
                <RotateCw className="w-3.5 h-3.5 text-rose-400" />
                <span>Reload Project</span>
              </button>
            </div>
          </div>
        </div>

        {/* Right Column (7 cols): Result Dashboard & Live Terminal */}
        <div className="lg:col-span-7 space-y-5">
          {/* Result Status Dashboard */}
          <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4">
            <span className="text-xs font-bold text-slate-200 block">Verification Status Dashboard</span>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs">
              {/* FFmpeg status */}
              <div className="p-3.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-1">
                <div className="flex items-center justify-between">
                  <span className="font-semibold text-slate-300">FFmpeg Engine</span>
                  {runtimeStatus?.ffmpeg.available ? (
                    <span className="text-emerald-400 flex items-center gap-1 font-mono font-bold text-[11px]">
                      <CheckCircle2 className="w-3.5 h-3.5" /> PASS
                    </span>
                  ) : (
                    <span className="text-rose-400 flex items-center gap-1 font-mono font-bold text-[11px]">
                      <XCircle className="w-3.5 h-3.5" /> FAIL
                    </span>
                  )}
                </div>
                <p className="text-[10px] text-slate-500 font-mono truncate">
                  {runtimeStatus?.ffmpeg.version || 'Not Detected'}
                </p>
              </div>

              {/* FFprobe status */}
              <div className="p-3.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-1">
                <div className="flex items-center justify-between">
                  <span className="font-semibold text-slate-300">FFprobe Engine</span>
                  {runtimeStatus?.ffprobe.available ? (
                    <span className="text-emerald-400 flex items-center gap-1 font-mono font-bold text-[11px]">
                      <CheckCircle2 className="w-3.5 h-3.5" /> PASS
                    </span>
                  ) : (
                    <span className="text-rose-400 flex items-center gap-1 font-mono font-bold text-[11px]">
                      <XCircle className="w-3.5 h-3.5" /> FAIL
                    </span>
                  )}
                </div>
                <p className="text-[10px] text-slate-500 font-mono truncate">
                  {runtimeStatus?.ffprobe.version || 'Not Detected'}
                </p>
              </div>

              {/* Media Preparation status */}
              <div className="p-3.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-1">
                <div className="flex items-center justify-between">
                  <span className="font-semibold text-slate-300">Media Preparation</span>
                  {prepStatus === 'pass' ? (
                    <span className="text-emerald-400 flex items-center gap-1 font-mono font-bold text-[11px]">
                      <CheckCircle2 className="w-3.5 h-3.5" /> PASS
                    </span>
                  ) : prepStatus === 'fail' ? (
                    <span className="text-rose-400 flex items-center gap-1 font-mono font-bold text-[11px]">
                      <XCircle className="w-3.5 h-3.5" /> FAIL
                    </span>
                  ) : (
                    <span className="text-slate-500 text-[11px]">{prepStatus === 'running' ? 'Preparing...' : 'Pending'}</span>
                  )}
                </div>
                <p className="text-[10px] text-slate-500 font-mono">
                  {prepStatus === 'pass' ? 'frames/ & audio/ cache created' : 'Workspace directories'}
                </p>
              </div>

              {/* Frame Extraction */}
              <div className="p-3.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-1">
                <div className="flex items-center justify-between">
                  <span className="font-semibold text-slate-300">Frame Extraction</span>
                  {frameResult ? (
                    <span className="text-emerald-400 flex items-center gap-1 font-mono font-bold text-[11px]">
                      <CheckCircle2 className="w-3.5 h-3.5" /> PASS ({frameResult.frameCount} frames)
                    </span>
                  ) : (
                    <span className="text-slate-500 text-[11px]">Pending</span>
                  )}
                </div>
                <p className="text-[10px] text-slate-500 font-mono">
                  {frameResult ? `000000.png..00000${frameResult.frameCount - 1}.png • ${frameResult.fps} FPS` : 'Format: PNG @ 2 FPS'}
                </p>
              </div>

              {/* Audio Extraction */}
              <div className="p-3.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-1">
                <div className="flex items-center justify-between">
                  <span className="font-semibold text-slate-300">Audio Extraction</span>
                  {audioResult ? (
                    <span className="text-emerald-400 flex items-center gap-1 font-mono font-bold text-[11px]">
                      <CheckCircle2 className="w-3.5 h-3.5" /> {audioResult.hasAudio ? 'PASS' : 'NO AUDIO'}
                    </span>
                  ) : (
                    <span className="text-slate-500 text-[11px]">Pending</span>
                  )}
                </div>
                <p className="text-[10px] text-slate-500 font-mono">
                  {audioResult?.hasAudio ? 'source.wav (16-bit PCM)' : 'Safe No-Audio Handling'}
                </p>
              </div>
            </div>

            {/* Output Directory Banner */}
            {validationReport && (
              <div className="p-3.5 rounded-xl bg-indigo-950/40 border border-indigo-500/30 flex items-center justify-between text-xs">
                <div className="space-y-0.5 min-w-0 pr-3">
                  <span className="text-[10px] font-bold uppercase tracking-wider text-indigo-300 block">
                    Verified Output Cache Location
                  </span>
                  <p className="font-mono text-[11px] text-slate-300 truncate">
                    {validationReport.mediaCacheDir}
                  </p>
                </div>
                <button
                  onClick={handleOpenFolder}
                  className="px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shrink-0 flex items-center gap-1.5"
                >
                  <FolderOpen className="w-3.5 h-3.5" />
                  <span>Open Folder</span>
                </button>
              </div>
            )}
          </div>

          {/* Diagnostic Console Logs */}
          <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-2">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2 text-xs font-bold text-slate-200">
                <Terminal className="w-3.5 h-3.5 text-slate-400" />
                <span>Live Verification Console</span>
              </div>
              <button
                onClick={() => setLogLines([])}
                className="text-[10px] text-slate-500 hover:text-slate-400 font-mono"
              >
                Clear
              </button>
            </div>

            <div className="h-44 overflow-y-auto p-3 rounded-xl bg-slate-950 border border-slate-800/80 font-mono text-[11px] text-slate-300 space-y-1 select-text">
              {logLines.length === 0 ? (
                <span className="text-slate-600">No test actions recorded yet. Click "Run All Tests" or individual controls.</span>
              ) : (
                logLines.map((line, idx) => (
                  <div key={idx} className={line.includes('✓') ? 'text-emerald-400' : line.includes('✗') ? 'text-rose-400' : 'text-slate-300'}>
                    {line}
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
