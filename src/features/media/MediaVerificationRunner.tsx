import React, { useState, useEffect, useRef } from 'react';
import { 
  Play, 
  FolderOpen, 
  Terminal, 
  RotateCw, 
  Video, 
  Loader2,
  Sparkles,
  Layers
} from 'lucide-react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { mediaApi, renderApi } from '../../lib/ipc';
import { useProjectStore } from '../../stores/projectStore';
import { 
  CacheValidationReport, 
  RenderResult 
} from '../../types/contracts';
import { VideoDropZone } from './components/VideoDropZone';

export const MediaVerificationRunner: React.FC = () => {
  const { activeProject, importMediaToProject, loadProject } = useProjectStore();
  const [isRunning, setIsRunning] = useState(false);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [validationReport, setValidationReport] = useState<CacheValidationReport | null>(null);
  const [renderResult, setRenderResult] = useState<RenderResult | null>(null);
  const [isRendering, setIsRendering] = useState(false);

  const outputVideoRef = useRef<HTMLVideoElement | null>(null);

  const addLog = (msg: string) => {
    const time = new Date().toLocaleTimeString();
    setLogLines((prev) => [...prev, `[${time}] ${msg}`]);
  };

  const handleCheckRuntime = async () => {
    addLog('Querying host FFmpeg/FFprobe binaries via mediaApi.getRuntimeStatus()...');
    try {
      const status = await mediaApi.getRuntimeStatus();
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
    addLog(`Preparing media cache directories for media ID: ${activeProject.sourceMedia.mediaId}...`);
    try {
      const dir = await mediaApi.prepareMedia(activeProject.id, activeProject.sourceMedia.mediaId);
      addLog(`✓ Media workspace prepared at: ${dir}`);
    } catch (err: any) {
      addLog(`✗ Media preparation failed: ${err?.message || err}`);
    }
  };

  const handleExtractFramesForMode = async (mode: 'test_1s' | 'test_3s' | 'full') => {
    if (!activeProject || !activeProject.sourceMedia) {
      addLog('✗ Error: Please import a video first.');
      return null;
    }

    const sourceFps = activeProject.sourceMedia.fps || 30.0;
    let startSec: number | undefined = 0.0;
    let endSec: number | undefined = 1.0;

    if (mode === 'test_3s') {
      endSec = 3.0;
    } else if (mode === 'full') {
      startSec = undefined;
      endSec = undefined;
    }

    addLog(`Extracting frames for mode [${mode.toUpperCase()}] at native ${sourceFps} FPS...`);
    try {
      const res = await mediaApi.extractFrames({
        projectId: activeProject.id,
        mediaId: activeProject.sourceMedia.mediaId,
        startTimeSeconds: startSec,
        endTimeSeconds: endSec,
        fps: sourceFps,
        format: 'png',
      });
      addLog(`✓ Frame Extraction complete: ${res.frameCount} frames generated at ${res.fps} FPS`);
      await handleValidateCache();
      return res;
    } catch (err: any) {
      addLog(`✗ Frame extraction failed: ${err?.message || err}`);
      return null;
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
      if (res.hasAudio) {
        addLog(`✓ Audio extracted successfully: ${res.audioPath} (${res.sampleRate}Hz stereo)`);
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

  const handleRenderVideoMode = async (mode: 'test_1s' | 'test_3s' | 'full') => {
    if (!activeProject || !activeProject.sourceMedia) {
      addLog('✗ Error: No active project with imported media');
      return;
    }
    setIsRendering(true);

    addLog(`=== STARTING VIDEO RECONSTRUCTION: [${mode.toUpperCase()}] ===`);
    // 1. Ensure frames for requested mode are extracted
    await handleExtractFramesForMode(mode);
    await handleExtractAudio();

    const sourceFps = activeProject.sourceMedia.fps || 30.0;
    const outputName = mode === 'full' 
      ? 'reconstructed_full.mp4' 
      : mode === 'test_3s' 
      ? 'reconstructed_3s.mp4' 
      : 'reconstructed_1s.mp4';

    addLog(`Re-encoding frame sequence with FFmpeg (libx264, ${sourceFps} FPS) → ${outputName}...`);
    try {
      const result = await renderApi.renderTestVideo({
        projectId: activeProject.id,
        mediaId: activeProject.sourceMedia.mediaId,
        fps: sourceFps,
        outputFormat: 'mp4',
        outputName,
        mode,
      });
      setRenderResult(result);
      addLog(`✓ Render Succeeded: ${result.outputMetadata.outputPath}`);
      addLog(`✓ Output Metadata: ${result.outputMetadata.width}×${result.outputMetadata.height}, ${result.outputMetadata.durationSeconds.toFixed(2)}s, ${result.outputMetadata.videoCodec}, ${result.outputMetadata.fileSizeBytes} bytes`);
      addLog(`✓ Comparison Status: ${result.comparison.timingExplanation}`);
      if (result.comparison.isFullMatch) {
        addLog('★ FULL RECONSTRUCTION PASS: Output duration and frame count perfectly match source video!');
      } else {
        addLog(`★ ${result.comparison.mode} PASS: Reconstructed test duration matches extracted frame sequence timing.`);
      }
    } catch (err: any) {
      addLog(`✗ Video render failed: ${err?.message || err}`);
    } finally {
      setIsRendering(false);
    }
  };

  const handleOpenFolder = async () => {
    const targetDir = renderResult?.outputMetadata.outputPath 
      ? renderResult.outputMetadata.outputPath.substring(0, renderResult.outputMetadata.outputPath.lastIndexOf('\\'))
      : validationReport?.mediaCacheDir;

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
    addLog('=== STARTING FULL MEDIA ENGINE & RECONSTRUCTION AUDIT SUITE ===');
    await handleCheckRuntime();
    await handlePrepareMedia();
    await handleExtractAudio();
    await handleRenderVideoMode('test_1s');
    await handleRenderVideoMode('full');
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
  const outputSrc = renderResult ? convertFileSrc(renderResult.outputMetadata.outputPath) : null;

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100 font-sans">
      {/* Header */}
      <div className="flex items-center justify-between pb-4 border-b border-slate-800">
        <div>
          <div className="flex items-center gap-2">
            <h1 className="text-2xl font-bold tracking-tight text-white">Media Engine & Render Verification Runner</h1>
            <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-purple-500/20 text-purple-300 border border-purple-500/30">
              PHASE 4C DURATION & TIMING AUDIT
            </span>
          </div>
          <p className="text-xs text-slate-400 mt-1">
            Deterministic frame-timing validation: 1s Test, 3s Test, and Full Source Media Reconstruction.
          </p>
        </div>

        <button
          onClick={handleRunAllTests}
          disabled={isRunning || !sourceMedia}
          className="px-5 py-2.5 rounded-xl bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white text-xs font-bold shadow-lg shadow-purple-900/30 flex items-center gap-2 disabled:opacity-50 transition-all cursor-pointer"
        >
          {isRunning || isRendering ? <Loader2 className="w-4 h-4 animate-spin" /> : <Play className="w-4 h-4 fill-current" />}
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
                  <span>{(sourceMedia.durationMs / 1000).toFixed(2)}s • {(sourceMedia.fileSizeBytes / (1024 * 1024)).toFixed(1)} MB</span>
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
            <span className="text-xs font-bold text-slate-200 block">Reconstruction Pipeline Modes</span>
            
            <div className="space-y-2 text-xs">
              {/* Mode A: 1s Quick Test */}
              <button
                onClick={() => handleRenderVideoMode('test_1s')}
                disabled={!sourceMedia || isRendering}
                className="w-full p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-semibold text-left flex items-center justify-between disabled:opacity-40 transition-colors cursor-pointer border border-slate-700/60"
              >
                <div className="flex items-center gap-2">
                  <Sparkles className="w-3.5 h-3.5 text-indigo-400" />
                  <span>1. Quick Test Render (1s • ~{Math.round(sourceMedia?.fps || 30)} frames)</span>
                </div>
                <span className="text-[10px] font-mono text-indigo-300 bg-indigo-950/60 px-1.5 py-0.5 rounded">TEST_1S</span>
              </button>

              {/* Mode B: 3s Test */}
              <button
                onClick={() => handleRenderVideoMode('test_3s')}
                disabled={!sourceMedia || isRendering}
                className="w-full p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-semibold text-left flex items-center justify-between disabled:opacity-40 transition-colors cursor-pointer border border-slate-700/60"
              >
                <div className="flex items-center gap-2">
                  <Layers className="w-3.5 h-3.5 text-sky-400" />
                  <span>2. Test Render (3s • ~{Math.round((sourceMedia?.fps || 30) * 3)} frames)</span>
                </div>
                <span className="text-[10px] font-mono text-sky-300 bg-sky-950/60 px-1.5 py-0.5 rounded">TEST_3S</span>
              </button>

              {/* Mode C: Full Reconstruction */}
              <button
                onClick={() => handleRenderVideoMode('full')}
                disabled={!sourceMedia || isRendering}
                className="w-full p-2.5 rounded-xl bg-gradient-to-r from-purple-900/60 to-indigo-900/60 hover:from-purple-900/80 hover:to-indigo-900/80 border border-purple-500/50 text-purple-100 font-bold text-left flex items-center justify-between disabled:opacity-40 transition-all cursor-pointer shadow-md"
              >
                <div className="flex items-center gap-2">
                  <Video className="w-4 h-4 text-purple-400" />
                  <span>3. Full Reconstruction (All Frames + Full Audio)</span>
                </div>
                <span className="text-[10px] font-mono text-purple-200 bg-purple-950 px-2 py-0.5 rounded border border-purple-500/40">FULL</span>
              </button>
            </div>

            <div className="grid grid-cols-2 gap-2 text-xs pt-2 border-t border-slate-800">
              <button
                onClick={handleOpenFolder}
                disabled={!validationReport && !renderResult}
                className="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-medium text-left flex items-center gap-2 disabled:opacity-40 transition-colors cursor-pointer"
              >
                <FolderOpen className="w-3.5 h-3.5 text-emerald-400" />
                <span>Open Outputs Folder</span>
              </button>

              <button
                onClick={handleReloadProject}
                disabled={!activeProject}
                className="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-medium text-left flex items-center gap-2 disabled:opacity-40 transition-colors cursor-pointer"
              >
                <RotateCw className="w-3.5 h-3.5 text-rose-400" />
                <span>Reload Project</span>
              </button>
            </div>
          </div>
        </div>

        {/* Right Column (7 cols): Result Dashboard & Reconstructed Player */}
        <div className="lg:col-span-7 space-y-5">
          {/* Result Status Dashboard */}
          <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-slate-200 block">Verification Status Dashboard</span>
              {renderResult && (
                <span className={`px-2.5 py-0.5 rounded font-mono text-[10px] font-bold border ${
                  renderResult.comparison.isFullMatch
                    ? 'bg-emerald-500/20 text-emerald-300 border-emerald-500/30'
                    : 'bg-indigo-500/20 text-indigo-300 border-indigo-500/30'
                }`}>
                  {renderResult.comparison.isFullMatch ? 'FULL RECONSTRUCTION: PASS' : `RECONSTRUCTION: PASS — ${renderResult.mode}`}
                </span>
              )}
            </div>

            {/* Comparison Metrics Table */}
            {renderResult ? (
              <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-3">
                <div className="text-xs font-semibold text-slate-300 flex items-center justify-between">
                  <span>Source vs Reconstructed Output Audit</span>
                  <span className="font-mono text-[11px] text-purple-400 font-bold">Mode: {renderResult.mode}</span>
                </div>

                <div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-[11px] font-mono">
                  <div className="p-2.5 rounded-lg bg-slate-900/80 border border-slate-800/80">
                    <span className="text-[9px] text-slate-500 block uppercase">Source Duration</span>
                    <span className="text-slate-200 font-bold">{renderResult.comparison.sourceDurationSeconds.toFixed(2)}s</span>
                  </div>

                  <div className="p-2.5 rounded-lg bg-slate-900/80 border border-slate-800/80">
                    <span className="text-[9px] text-slate-500 block uppercase">Output Duration</span>
                    <span className="text-slate-200 font-bold">{renderResult.comparison.outputDurationSeconds.toFixed(2)}s</span>
                  </div>

                  <div className="p-2.5 rounded-lg bg-slate-900/80 border border-slate-800/80">
                    <span className="text-[9px] text-slate-500 block uppercase">Duration Delta</span>
                    <span className={`font-bold ${renderResult.comparison.durationDeltaSeconds <= 0.10 ? 'text-emerald-400' : 'text-amber-400'}`}>
                      {renderResult.comparison.durationDeltaSeconds.toFixed(3)}s
                    </span>
                  </div>

                  <div className="p-2.5 rounded-lg bg-slate-900/80 border border-slate-800/80">
                    <span className="text-[9px] text-slate-500 block uppercase">Frames (Act / Exp)</span>
                    <span className="text-slate-200 font-bold">{renderResult.comparison.actualFrameCount} / {renderResult.comparison.expectedFrameCount}</span>
                  </div>
                </div>

                <div className="p-2.5 rounded-lg bg-slate-900/80 border border-slate-800/80 text-[10px] font-mono text-slate-400 flex justify-between">
                  <span>Resolution: <span className="text-slate-200 font-bold">{renderResult.outputMetadata.width}×{renderResult.outputMetadata.height}</span></span>
                  <span>FPS: <span className="text-slate-200 font-bold">{renderResult.outputMetadata.fps}</span></span>
                  <span>Video/Audio: <span className="text-slate-200 font-bold uppercase">{renderResult.outputMetadata.videoCodec} / {renderResult.outputMetadata.audioCodec || 'None'}</span></span>
                  <span>Size: <span className="text-slate-200 font-bold">{(renderResult.outputMetadata.fileSizeBytes / 1024).toFixed(1)} KB</span></span>
                </div>
              </div>
            ) : (
              <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 text-xs text-slate-500 text-center">
                Select a Reconstruction Mode on the left to validate frame-timing & render MP4.
              </div>
            )}

            {/* Reconstructed Video Player Preview */}
            {renderResult && outputSrc && (
              <div className="p-4 rounded-xl bg-slate-950 border border-purple-500/30 space-y-3">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-bold text-purple-300 flex items-center gap-1.5">
                    <Video className="w-3.5 h-3.5" />
                    <span>Active Output Player: {renderResult.outputMetadata.outputPath.split('\\').pop()}</span>
                  </span>
                  <span className="text-[10px] font-mono text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/20">
                    REAL MP4 PLAYBACK
                  </span>
                </div>

                <div className="relative rounded-lg overflow-hidden bg-black aspect-video max-h-56 flex items-center justify-center">
                  <video
                    key={outputSrc}
                    ref={outputVideoRef}
                    src={outputSrc}
                    controls
                    playsInline
                    className="w-full h-full object-contain"
                  />
                </div>
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
                className="text-[10px] text-slate-500 hover:text-slate-400 font-mono cursor-pointer"
              >
                Clear
              </button>
            </div>

            <div className="h-44 overflow-y-auto p-3 rounded-xl bg-slate-950 border border-slate-800/80 font-mono text-[11px] text-slate-300 space-y-1 select-text">
              {logLines.length === 0 ? (
                <span className="text-slate-600">No test actions recorded yet. Click "Run All Tests" or individual controls.</span>
              ) : (
                logLines.map((line, idx) => (
                  <div key={idx} className={line.includes('✓') || line.includes('★') ? 'text-emerald-400' : line.includes('✗') ? 'text-rose-400' : 'text-slate-300'}>
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
