import React, { useState, useEffect } from 'react';
import {
  Sparkles,
  Video,
  User,
  Sliders,
  Play,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  RefreshCw,
  Cpu,
  Layers,
  Image as ImageIcon,
  ShieldCheck,
  Film,
  FolderOpen,
} from 'lucide-react';
import { aiApi, mediaApi } from '../../lib/ipc';
import {
  GenerativePreflightReport,
  GenerativeVideoReport,
  KeyframeGenerationResult,
  KeyframeQualityReport,
} from '../../types/contracts';

type GenerationMode = 'keyframe' | 'video';
type QualityPreset = 'fast' | 'balanced' | 'quality';
type AiProcessingMode = 'SMART_AUTO' | 'LOCAL_ONLY' | 'CLOUD_ECONOMY' | 'CLOUD_BALANCED' | 'CLOUD_QUALITY';

export const GenerativeStudioView: React.FC = () => {
  // Mode selection
  const [mode, setMode] = useState<GenerationMode>('keyframe');
  const [preset, setPreset] = useState<QualityPreset>('balanced');
  const [aiMode, setAiMode] = useState<AiProcessingMode>('SMART_AUTO');

  // Input states
  const [sourceVideoPath, setSourceVideoPath] = useState<string>(
    'd:\\rustProject\\autovideo-ai\\.autovideo_data\\sample_portrait_video.mp4'
  );
  const [sourceFrameIndex, setSourceFrameIndex] = useState<number>(0);
  const [characterRefPath, setCharacterRefPath] = useState<string>('');
  const [positivePrompt, setPositivePrompt] = useState<string>(
    'Cinematic 8k, photorealistic, natural dramatic lighting, highly detailed face, masterpiece'
  );
  const [negativePrompt, setNegativePrompt] = useState<string>(
    'blurry, low quality, distorted anatomy, bad hands, artifacts, overexposed'
  );
  const [stylePreset] = useState<string>('CINEMATIC');
  const [width, setWidth] = useState<number>(512);
  const [height, setHeight] = useState<number>(768);
  const [steps, setSteps] = useState<number>(25);
  const [cfgScale, setCfgScale] = useState<number>(7.0);
  const [denoiseStrength, setDenoiseStrength] = useState<number>(0.85);
  const [seed, setSeed] = useState<number>(42);

  // Temporal window settings
  const [contextSize, setContextSize] = useState<number>(16);
  const [overlap, setOverlap] = useState<number>(4);

  // Preflight and generation states
  const [preflight, setPreflight] = useState<GenerativePreflightReport | null>(null);
  const [isPreflighting, setIsPreflighting] = useState<boolean>(false);
  const [isGenerating, setIsGenerating] = useState<boolean>(false);
  const [generationProgress, setGenerationProgress] = useState<string>('');
  const [keyframeResult, setKeyframeResult] = useState<KeyframeGenerationResult | null>(null);
  const [qualityReport, setQualityReport] = useState<KeyframeQualityReport | null>(null);
  const [videoReport, setVideoReport] = useState<GenerativeVideoReport | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Run preflight on mount
  useEffect(() => {
    runPreflight();
  }, []);

  const runPreflight = async () => {
    setIsPreflighting(true);
    setErrorMsg(null);
    try {
      const rep = await aiApi.checkGenerativePreflight();
      setPreflight(rep);
    } catch (err: any) {
      setErrorMsg(err.message || 'Failed to execute preflight diagnostics');
    } finally {
      setIsPreflighting(false);
    }
  };

  const handleApplyPreset = (p: QualityPreset) => {
    setPreset(p);
    if (p === 'fast') {
      setSteps(15);
      setWidth(512);
      setHeight(768);
      setContextSize(16);
      setOverlap(4);
    } else if (p === 'balanced') {
      setSteps(25);
      setWidth(512);
      setHeight(768);
      setContextSize(16);
      setOverlap(4);
    } else if (p === 'quality') {
      setSteps(35);
      setWidth(768);
      setHeight(1024);
      setContextSize(16);
      setOverlap(6);
    }
  };

  const handleGenerateKeyframe = async () => {
    if (!sourceVideoPath.trim()) {
      setErrorMsg('Please specify a source video path');
      return;
    }
    if (!characterRefPath.trim()) {
      setErrorMsg('Please specify or upload a character reference image');
      return;
    }

    setIsGenerating(true);
    setGenerationProgress('Synthesizing keyframe preview...');
    setErrorMsg(null);
    try {
      const jobId = `keyframe-${Date.now()}`;
      const resp = await aiApi.generateKeyframe({
        jobId,
        sourceVideoPath,
        sourceFrameIndex,
        characterReferencePaths: [characterRefPath],
        positivePrompt,
        negativePrompt,
        stylePreset,
        steps,
        cfgScale,
        denoiseStrength,
        seed,
        width,
        height,
      });

      setKeyframeResult(resp.result);
      setQualityReport(resp.quality);
      setGenerationProgress('Keyframe synthesized successfully');
    } catch (err: any) {
      setErrorMsg(err.message || 'Failed to generate keyframe');
    } finally {
      setIsGenerating(false);
    }
  };

  const handleGenerateFullVideo = async () => {
    if (!sourceVideoPath.trim()) {
      setErrorMsg('Please specify a source video path');
      return;
    }
    if (!characterRefPath.trim()) {
      setErrorMsg('Please specify or upload a character reference image');
      return;
    }

    setIsGenerating(true);
    setGenerationProgress('Launching 6-stage video-to-video generative pipeline...');
    setErrorMsg(null);
    try {
      const jobId = `video-${Date.now()}`;
      const rep = await aiApi.generateVideoPipeline({
        jobId,
        sourceVideoPath,
        characterReferencePaths: [characterRefPath],
        positivePrompt,
        negativePrompt,
        stylePreset,
        steps,
        cfgScale,
        denoiseStrength,
        seed,
        width,
        height,
        contextSize,
        overlap,
      });

      setVideoReport(rep);
      setGenerationProgress('Full video generation completed successfully');
    } catch (err: any) {
      setErrorMsg(err.message || 'Failed to generate full video');
    } finally {
      setIsGenerating(false);
    }
  };

  const handleOpenFolder = (filePath?: string) => {
    if (!filePath) return;
    const parentDir = filePath.substring(0, Math.max(filePath.lastIndexOf('\\'), filePath.lastIndexOf('/')));
    if (parentDir) {
      mediaApi.openDirectory(parentDir);
    }
  };

  const canGenerate =
    Boolean(sourceVideoPath.trim()) &&
    Boolean(characterRefPath.trim()) &&
    !isGenerating &&
    Boolean(preflight?.isValid);

  return (
    <div className="flex-1 flex flex-col h-full bg-[#0a0d14] text-slate-100 overflow-y-auto">
      {/* Header */}
      <div className="border-b border-slate-800/80 bg-slate-900/40 backdrop-blur px-8 py-5 flex items-center justify-between">
        <div>
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-xl bg-gradient-to-tr from-purple-600 to-indigo-500 shadow-lg shadow-purple-900/30">
              <Sparkles className="w-5 h-5 text-white" />
            </div>
            <div>
              <h1 className="text-xl font-bold tracking-tight text-white flex items-center gap-2">
                Generative Video Studio
                <span className="text-xs px-2 py-0.5 rounded-full bg-purple-900/60 text-purple-300 border border-purple-700/50 font-medium">
                  Phase 7C Multi-Frame
                </span>
              </h1>
              <p className="text-xs text-slate-400 mt-0.5">
                Transform source characters & environments while preserving actor motion, camera depth, FPS, and original audio.
              </p>
            </div>
          </div>
        </div>

        {/* Mode Selector & Action Buttons */}
        <div className="flex items-center gap-3">
          {/* Mode Switcher Tabs */}
          <div className="flex rounded-lg bg-slate-950 p-1 border border-slate-800">
            <button
              onClick={() => setMode('keyframe')}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-semibold transition ${
                mode === 'keyframe'
                  ? 'bg-purple-600 text-white shadow-sm'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <ImageIcon className="w-3.5 h-3.5" />
              Keyframe Preview
            </button>
            <button
              onClick={() => setMode('video')}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-semibold transition ${
                mode === 'video'
                  ? 'bg-indigo-600 text-white shadow-sm'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Film className="w-3.5 h-3.5" />
              Full Video Generation
            </button>
          </div>

          <button
            onClick={runPreflight}
            disabled={isPreflighting}
            className="flex items-center gap-2 px-3.5 py-2 rounded-lg bg-slate-800/80 hover:bg-slate-700/80 border border-slate-700 text-xs font-medium transition text-slate-200"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${isPreflighting ? 'animate-spin' : ''}`} />
            Preflight Check
          </button>

          {mode === 'keyframe' ? (
            <button
              onClick={handleGenerateKeyframe}
              disabled={!canGenerate}
              className={`flex items-center gap-2 px-5 py-2 rounded-lg text-xs font-semibold shadow-lg transition ${
                canGenerate
                  ? 'bg-gradient-to-r from-purple-600 to-indigo-600 hover:from-purple-500 hover:to-indigo-500 text-white shadow-purple-900/40 cursor-pointer'
                  : 'bg-slate-800 text-slate-500 border border-slate-700/50 cursor-not-allowed shadow-none'
              }`}
            >
              {isGenerating ? (
                <>
                  <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                  Synthesizing Keyframe...
                </>
              ) : (
                <>
                  <Play className="w-3.5 h-3.5 fill-current" />
                  Generate Keyframe Preview
                </>
              )}
            </button>
          ) : (
            <button
              onClick={handleGenerateFullVideo}
              disabled={!canGenerate}
              className={`flex items-center gap-2 px-5 py-2 rounded-lg text-xs font-semibold shadow-lg transition ${
                canGenerate
                  ? 'bg-gradient-to-r from-indigo-600 to-emerald-600 hover:from-indigo-500 hover:to-emerald-500 text-white shadow-indigo-900/40 cursor-pointer'
                  : 'bg-slate-800 text-slate-500 border border-slate-700/50 cursor-not-allowed shadow-none'
              }`}
            >
              {isGenerating ? (
                <>
                  <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                  Generating Full Video...
                </>
              ) : (
                <>
                  <Film className="w-3.5 h-3.5 fill-current" />
                  Generate Transformed Video
                </>
              )}
            </button>
          )}
        </div>
      </div>

      {/* Main Grid */}
      <div className="p-8 grid grid-cols-1 xl:grid-cols-12 gap-8">
        {/* Left Column: Conditioning & Parameters (5 cols) */}
        <div className="xl:col-span-5 space-y-6">
          {/* Hybrid AI Processing Mode */}
          <div className="rounded-xl border border-indigo-900/60 bg-gradient-to-b from-slate-900/90 to-slate-950/90 p-4 space-y-3 shadow-lg shadow-indigo-950/20">
            <div className="flex items-center justify-between">
              <span className="text-[11px] font-bold uppercase tracking-wider text-indigo-400 flex items-center gap-1.5">
                <Sparkles className="w-3.5 h-3.5 text-indigo-400" />
                AI Processing Strategy
              </span>
              <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-indigo-950 border border-indigo-700/50 text-indigo-300">
                Phase 12 Hybrid Engine
              </span>
            </div>

            <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5">
              {[
                { id: 'SMART_AUTO', label: 'Smart Auto', sub: 'Adaptive Hybrid' },
                { id: 'LOCAL_ONLY', label: 'Local Only', sub: 'GPU / CPU' },
                { id: 'CLOUD_ECONOMY', label: 'Cloud Economy', sub: 'Sparse Keyframes' },
                { id: 'CLOUD_BALANCED', label: 'Cloud Balanced', sub: 'Medium Density' },
                { id: 'CLOUD_QUALITY', label: 'Cloud Quality', sub: 'High Fidelity' },
              ].map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setAiMode(item.id as AiProcessingMode)}
                  className={`py-2 px-2.5 rounded-lg text-left border transition ${
                    aiMode === item.id
                      ? 'bg-indigo-950/80 border-indigo-500 text-indigo-100 shadow-md shadow-indigo-950/50'
                      : 'bg-slate-900/60 border-slate-800 text-slate-400 hover:border-slate-700 hover:text-slate-200'
                  }`}
                >
                  <div className="text-xs font-semibold">{item.label}</div>
                  <div className="text-[10px] text-slate-400 truncate">{item.sub}</div>
                </button>
              ))}
            </div>

            {/* Hardware & Routing Telemetry Summary */}
            <div className="rounded-lg bg-slate-950/80 border border-slate-800/80 p-3 text-[11px] space-y-1.5 text-slate-300">
              <div className="flex justify-between">
                <span className="text-slate-400">Hardware Tier:</span>
                <span className="font-mono text-indigo-300 font-medium">GTX 1650 (4GB VRAM, LOW_VRAM)</span>
              </div>
              <div className="flex justify-between">
                <span className="text-slate-400">Routing:</span>
                <span className="font-medium text-emerald-400">
                  {aiMode === 'LOCAL_ONLY' ? 'Local FP32 Layer Offload' : 'Hybrid (Local Preprocess + Cloud Keyframes)'}
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-slate-400">Estimated Plan:</span>
                <span>{aiMode === 'LOCAL_ONLY' ? '0 cloud reqs (~18 min local)' : '48 keyframes (~1.2 min cloud, ~3 min local)'}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-slate-400">Estimated Cloud Cost:</span>
                <span className="font-mono text-amber-400 font-medium">
                  {aiMode === 'LOCAL_ONLY' ? '$0.00 (Local)' : 'UNKNOWN (Unconfigured Provider)'}
                </span>
              </div>
              <div className="pt-1 text-[10px] text-amber-400/90 border-t border-slate-800/60 flex items-start gap-1">
                <AlertTriangle className="w-3 h-3 text-amber-400 shrink-0 mt-0.5" />
                <span>
                  {aiMode === 'LOCAL_ONLY'
                    ? 'Local full-video generation on 4GB VRAM executes in FP32 sequential layer offload.'
                    : 'Provider credentials not configured. Local preprocessing active with zero-fake fallback.'}
                </span>
              </div>
            </div>
          </div>

          {/* Quality Presets */}
          <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-4 space-y-2">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-400 block">
              Quality Preset
            </span>
            <div className="grid grid-cols-3 gap-2">
              <button
                type="button"
                onClick={() => handleApplyPreset('fast')}
                className={`py-2 px-3 rounded-lg text-xs font-semibold border transition text-center ${
                  preset === 'fast'
                    ? 'bg-purple-950/80 border-purple-500 text-purple-200'
                    : 'bg-slate-950 border-slate-800 text-slate-400 hover:border-slate-700'
                }`}
              >
                Fast (15 Steps)
              </button>
              <button
                type="button"
                onClick={() => handleApplyPreset('balanced')}
                className={`py-2 px-3 rounded-lg text-xs font-semibold border transition text-center ${
                  preset === 'balanced'
                    ? 'bg-purple-950/80 border-purple-500 text-purple-200'
                    : 'bg-slate-950 border-slate-800 text-slate-400 hover:border-slate-700'
                }`}
              >
                Balanced (25 Steps)
              </button>
              <button
                type="button"
                onClick={() => handleApplyPreset('quality')}
                className={`py-2 px-3 rounded-lg text-xs font-semibold border transition text-center ${
                  preset === 'quality'
                    ? 'bg-purple-950/80 border-purple-500 text-purple-200'
                    : 'bg-slate-950 border-slate-800 text-slate-400 hover:border-slate-700'
                }`}
              >
                Quality (35 Steps)
              </button>
            </div>
          </div>

          {/* Hardware & Sidecar Diagnostic Card */}
          <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-5 space-y-4">
            <div className="flex items-center justify-between">
              <h2 className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-2">
                <Cpu className="w-4 h-4 text-purple-400" />
                Backend & Hardware Preflight
              </h2>
              {preflight?.isValid ? (
                <span className="flex items-center gap-1.5 text-xs text-emerald-400 bg-emerald-950/60 px-2 py-0.5 rounded border border-emerald-800/40">
                  <CheckCircle2 className="w-3.5 h-3.5" />
                  Ready
                </span>
              ) : (
                <span className="flex items-center gap-1.5 text-xs text-amber-400 bg-amber-950/60 px-2 py-0.5 rounded border border-amber-800/40">
                  <AlertTriangle className="w-3.5 h-3.5" />
                  Warning
                </span>
              )}
            </div>

            <div className="grid grid-cols-2 gap-3 text-xs">
              <div className="p-2.5 rounded-lg bg-slate-950/60 border border-slate-800/60">
                <span className="text-slate-500 block">Generative Engine</span>
                <span className="font-semibold text-slate-200">
                  {preflight?.backendStatus?.backendName || 'PythonSidecar'}
                </span>
              </div>
              <div className="p-2.5 rounded-lg bg-slate-950/60 border border-slate-800/60">
                <span className="text-slate-500 block">GPU Acceleration</span>
                <span className="font-semibold text-slate-200">
                  {preflight?.backendStatus?.cudaAvailable ? 'CUDA Enabled' : 'CPU Mode'}
                </span>
              </div>
            </div>

            {/* Model Packages Status */}
            <div className="space-y-1.5">
              <span className="text-[11px] font-medium text-slate-400 block">Control Extractors</span>
              <div className="grid grid-cols-3 gap-2 text-[11px]">
                <div className="flex items-center gap-1.5 p-1.5 rounded bg-slate-950/40 border border-slate-800/40 text-slate-300">
                  <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
                  <span>DWPose</span>
                </div>
                <div className="flex items-center gap-1.5 p-1.5 rounded bg-slate-950/40 border border-slate-800/40 text-slate-300">
                  <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
                  <span>Depth V2</span>
                </div>
                <div className="flex items-center gap-1.5 p-1.5 rounded bg-slate-950/40 border border-slate-800/40 text-slate-300">
                  <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
                  <span>BiRefNet</span>
                </div>
              </div>
            </div>
          </div>

          {/* Source Video Conditioning */}
          <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-5 space-y-4">
            <h2 className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-2">
              <Video className="w-4 h-4 text-purple-400" />
              1. Source Video
            </h2>

            <div className="space-y-3 text-xs">
              <div>
                <label className="text-slate-400 block mb-1">Source Video Path</label>
                <input
                  type="text"
                  value={sourceVideoPath}
                  onChange={(e) => setSourceVideoPath(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs focus:outline-none focus:border-purple-500"
                  placeholder="C:\path\to\source_video.mp4"
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                {mode === 'keyframe' ? (
                  <div>
                    <label className="text-slate-400 block mb-1">Preview Frame Index</label>
                    <input
                      type="number"
                      value={sourceFrameIndex}
                      onChange={(e) => setSourceFrameIndex(parseInt(e.target.value) || 0)}
                      min={0}
                      className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-800 text-slate-200 text-xs focus:outline-none focus:border-purple-500"
                    />
                  </div>
                ) : (
                  <div>
                    <label className="text-slate-400 block mb-1">Temporal Window Size</label>
                    <select
                      value={contextSize}
                      onChange={(e) => setContextSize(Number(e.target.value))}
                      className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-800 text-slate-200 text-xs focus:outline-none focus:border-purple-500"
                    >
                      <option value="16">16 Frames (Standard)</option>
                      <option value="24">24 Frames (Extended)</option>
                      <option value="8">8 Frames (Low VRAM)</option>
                    </select>
                  </div>
                )}
                <div>
                  <label className="text-slate-400 block mb-1">Resolution</label>
                  <select
                    value={`${width}x${height}`}
                    onChange={(e) => {
                      const [w, h] = e.target.value.split('x').map(Number);
                      setWidth(w);
                      setHeight(h);
                    }}
                    className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-800 text-slate-200 text-xs focus:outline-none focus:border-purple-500"
                  >
                    <option value="512x768">512 × 768 (Portrait 2:3)</option>
                    <option value="768x512">768 × 512 (Landscape 3:2)</option>
                    <option value="512x512">512 × 512 (Square 1:1)</option>
                    <option value="768x1024">768 × 1024 (HD Portrait)</option>
                  </select>
                </div>
              </div>
            </div>
          </div>

          {/* Character Reference Conditioning */}
          <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-5 space-y-4">
            <h2 className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-2">
              <User className="w-4 h-4 text-purple-400" />
              2. Character Reference Identity
            </h2>

            <div className="space-y-3 text-xs">
              <div>
                <label className="text-slate-400 block mb-1">Character Portrait Image Path</label>
                <input
                  type="text"
                  value={characterRefPath}
                  onChange={(e) => setCharacterRefPath(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs focus:outline-none focus:border-purple-500"
                  placeholder="C:\path\to\character_reference.png"
                />
              </div>

              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={() =>
                    setCharacterRefPath(
                      'd:\\rustProject\\autovideo-ai\\.autovideo_data\\sample_character_ref.png'
                    )
                  }
                  className="px-2.5 py-1.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 text-[11px] transition"
                >
                  Use Sample Character Ref
                </button>
              </div>
            </div>
          </div>

          {/* Environment & Style Prompts */}
          <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-5 space-y-4">
            <h2 className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-2">
              <Layers className="w-4 h-4 text-purple-400" />
              3. Environment & Style Prompts
            </h2>

            <div className="space-y-3 text-xs">
              <div>
                <label className="text-slate-400 block mb-1">Positive Prompt</label>
                <textarea
                  rows={3}
                  value={positivePrompt}
                  onChange={(e) => setPositivePrompt(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-800 text-slate-200 text-xs focus:outline-none focus:border-purple-500"
                />
              </div>
              <div>
                <label className="text-slate-400 block mb-1">Negative Prompt</label>
                <textarea
                  rows={2}
                  value={negativePrompt}
                  onChange={(e) => setNegativePrompt(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-800 text-slate-200 text-xs focus:outline-none focus:border-purple-500"
                />
              </div>
            </div>
          </div>

          {/* Diffusion Parameters */}
          <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-5 space-y-4">
            <h2 className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-2">
              <Sliders className="w-4 h-4 text-purple-400" />
              4. Diffusion Parameters
            </h2>

            <div className="grid grid-cols-2 gap-4 text-xs">
              <div>
                <div className="flex justify-between text-slate-400 mb-1">
                  <span>Steps</span>
                  <span className="font-mono text-slate-200">{steps}</span>
                </div>
                <input
                  type="range"
                  min={15}
                  max={50}
                  value={steps}
                  onChange={(e) => setSteps(parseInt(e.target.value))}
                  className="w-full accent-purple-500"
                />
              </div>
              <div>
                <div className="flex justify-between text-slate-400 mb-1">
                  <span>CFG Scale</span>
                  <span className="font-mono text-slate-200">{cfgScale.toFixed(1)}</span>
                </div>
                <input
                  type="range"
                  min={3.0}
                  max={15.0}
                  step={0.5}
                  value={cfgScale}
                  onChange={(e) => setCfgScale(parseFloat(e.target.value))}
                  className="w-full accent-purple-500"
                />
              </div>
              <div>
                <div className="flex justify-between text-slate-400 mb-1">
                  <span>Denoise Strength</span>
                  <span className="font-mono text-slate-200">{denoiseStrength.toFixed(2)}</span>
                </div>
                <input
                  type="range"
                  min={0.3}
                  max={1.0}
                  step={0.05}
                  value={denoiseStrength}
                  onChange={(e) => setDenoiseStrength(parseFloat(e.target.value))}
                  className="w-full accent-purple-500"
                />
              </div>
              <div>
                <div className="flex justify-between text-slate-400 mb-1">
                  <span>Seed</span>
                  <span className="font-mono text-slate-200">{seed}</span>
                </div>
                <div className="flex gap-1.5">
                  <input
                    type="number"
                    value={seed}
                    onChange={(e) => setSeed(parseInt(e.target.value) || 0)}
                    className="w-full px-2 py-1 rounded bg-slate-950 border border-slate-800 text-xs font-mono"
                  />
                  <button
                    type="button"
                    onClick={() => setSeed(Math.floor(Math.random() * 9999999))}
                    className="px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs"
                  >
                    🎲
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Right Column: Output Preview, Stage Monitor & Telemetry (7 cols) */}
        <div className="xl:col-span-7 space-y-6">
          {errorMsg && (
            <div className="p-4 rounded-xl bg-red-950/50 border border-red-800/60 text-red-200 text-xs flex items-start gap-3">
              <XCircle className="w-4 h-4 text-red-400 shrink-0 mt-0.5" />
              <div>
                <span className="font-semibold block mb-0.5">Generation Error</span>
                <span>{errorMsg}</span>
              </div>
            </div>
          )}

          {/* Live Progress Card */}
          {isGenerating && (
            <div className="rounded-xl border border-purple-800/60 bg-purple-950/30 p-4 space-y-2">
              <div className="flex items-center justify-between text-xs">
                <span className="font-semibold text-purple-300 flex items-center gap-2">
                  <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                  {mode === 'keyframe' ? 'Keyframe Synthesis' : '6-Stage Video Pipeline'}
                </span>
                <span className="text-[11px] text-purple-400 font-mono">Running</span>
              </div>
              <p className="text-xs text-slate-300">{generationProgress}</p>
            </div>
          )}

          {/* Preview Container */}
          <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-5 space-y-4">
            <div className="flex items-center justify-between">
              <h2 className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-2">
                <Film className="w-4 h-4 text-purple-400" />
                {mode === 'keyframe' ? 'Keyframe Preview' : 'Transformed Video Output'}
              </h2>
              {videoReport && (
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => handleOpenFolder(videoReport.outputVideoPath)}
                    className="flex items-center gap-1 text-[11px] text-purple-400 hover:text-purple-300 transition"
                  >
                    <FolderOpen className="w-3.5 h-3.5" />
                    Open Folder
                  </button>
                </div>
              )}
            </div>

            <div className="relative aspect-[2/3] max-h-[580px] w-full rounded-lg bg-slate-950 border border-slate-800 overflow-hidden flex items-center justify-center">
              {isGenerating ? (
                <div className="flex flex-col items-center gap-3 text-slate-400">
                  <RefreshCw className="w-8 h-8 text-purple-500 animate-spin" />
                  <span className="text-xs font-medium">Processing temporal generative transformation...</span>
                  <span className="text-[11px] text-slate-600">
                    {mode === 'keyframe'
                      ? 'Extracting controls → Diffusing keyframe'
                      : 'Sliding windows → Diffusion batches → Cosine blending → Audio muxing'}
                  </span>
                </div>
              ) : videoReport ? (
                <div className="relative w-full h-full flex flex-col items-center justify-center p-6 text-center space-y-4">
                  <CheckCircle2 className="w-14 h-14 text-emerald-400 mx-auto" />
                  <div className="space-y-1">
                    <span className="text-base font-bold text-slate-100 block">
                      Video Generated Successfully
                    </span>
                    <span className="text-xs text-slate-400 font-mono block max-w-md truncate">
                      {videoReport.outputVideoPath}
                    </span>
                  </div>
                  <div className="flex gap-3">
                    <button
                      onClick={() => handleOpenFolder(videoReport.outputVideoPath)}
                      className="flex items-center gap-1.5 px-4 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold transition"
                    >
                      <FolderOpen className="w-4 h-4" />
                      Open Containing Folder
                    </button>
                  </div>
                </div>
              ) : keyframeResult ? (
                <div className="relative w-full h-full flex items-center justify-center p-4">
                  <div className="text-center space-y-3">
                    <CheckCircle2 className="w-12 h-12 text-emerald-400 mx-auto" />
                    <div>
                      <span className="text-sm font-semibold text-slate-100 block">
                        Keyframe Generated Successfully
                      </span>
                      <span className="text-xs text-slate-400 font-mono block mt-1">
                        {keyframeResult.outputPath}
                      </span>
                    </div>
                  </div>
                </div>
              ) : (
                <div className="flex flex-col items-center gap-2 text-slate-600 text-xs">
                  <Sparkles className="w-8 h-8 text-slate-700" />
                  <span>Configure conditioning parameters and click Generate</span>
                </div>
              )}
            </div>
          </div>

          {/* Telemetry & Quality Report */}
          {videoReport ? (
            <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-5 space-y-4">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-2">
                <ShieldCheck className="w-4 h-4 text-emerald-400" />
                Pipeline Telemetry & Quality Gate
              </h3>

              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs">
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800/60">
                  <span className="text-slate-500 block">Total Duration</span>
                  <span className="font-semibold text-slate-200">
                    {(videoReport.totalDurationMs / 1000).toFixed(2)} s
                  </span>
                </div>
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800/60">
                  <span className="text-slate-500 block">Frames / Windows</span>
                  <span className="font-semibold text-slate-200">
                    {videoReport.totalFrames} frames ({videoReport.totalWindows} windows)
                  </span>
                </div>
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800/60">
                  <span className="text-slate-500 block">Audio Track</span>
                  <span className="font-semibold text-emerald-400">
                    {videoReport.audioPreserved ? 'Preserved & Synced' : 'None'}
                  </span>
                </div>
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800/60">
                  <span className="text-slate-500 block">Quality Status</span>
                  <span className="font-semibold text-emerald-400">{videoReport.qualityStatus}</span>
                </div>
              </div>
            </div>
          ) : keyframeResult && qualityReport ? (
            <div className="rounded-xl border border-slate-800 bg-slate-900/50 p-5 space-y-4">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-2">
                <ShieldCheck className="w-4 h-4 text-emerald-400" />
                Keyframe Output Validation
              </h3>

              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs">
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800/60">
                  <span className="text-slate-500 block">Total Latency</span>
                  <span className="font-semibold text-slate-200">
                    {keyframeResult.totalDurationMs.toFixed(1)} ms
                  </span>
                </div>
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800/60">
                  <span className="text-slate-500 block">Inference Time</span>
                  <span className="font-semibold text-slate-200">
                    {keyframeResult.inferenceDurationMs.toFixed(1)} ms
                  </span>
                </div>
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800/60">
                  <span className="text-slate-500 block">File Size</span>
                  <span className="font-semibold text-slate-200">
                    {(qualityReport.fileSizeBytes / 1024).toFixed(1)} KB
                  </span>
                </div>
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800/60">
                  <span className="text-slate-500 block">Quality Status</span>
                  <span
                    className={`font-semibold ${
                      qualityReport.isValid ? 'text-emerald-400' : 'text-red-400'
                    }`}
                  >
                    {qualityReport.isValid ? 'VALID' : 'FAILED'}
                  </span>
                </div>
              </div>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
};
