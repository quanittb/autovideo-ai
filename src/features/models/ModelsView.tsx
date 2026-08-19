import React, { useState, useEffect } from 'react';
import { 
  Cpu, 
  Layers, 
  RotateCw, 
  CheckCircle2, 
  XCircle, 
  Activity, 
  Play, 
  Terminal,
  Sliders,
  Maximize2,
  ShieldCheck,
  Package,
  Plus,
  RotateCcw,
  Trash2,
  Check,
  Square,
  Sparkles,
  AlertTriangle,
  FileVideo,
  FolderOpen
} from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { aiApi } from '../../lib/ipc';
import { useJobStore } from '../../stores/jobStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { 
  RuntimeStatus, 
  ProviderInfo, 
  DeviceInfo, 
  AiModelManifest, 
  OnnxModelMetadata, 
  InferenceResult,
  ExecutionProvider,
  PreprocessConfig,
  PreprocessResult,
  PreprocessValidationResult,
  ResizeFilter,
  ChannelOrder,
  NormalizationMode,
  TensorLayout,
  AiModelFamily,
  AiModelPackage,
  ModelValidationReport,
  ImportModelRequest,
  AspectHandlingMode,
  AiJobConfig,
  AiJobPreflightReport,
  ResolvedProductionModel,
  FrameSamplingMode,
  AiFrameOutputMode
} from '../../types/contracts';

export const ModelsView: React.FC = () => {
  const { createAiJob, runPreflight, startJob, selectJob } = useJobStore();
  const { activeProject } = useProjectStore();
  const { setActiveTab: setAppNavTab } = useUiStore();

  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatus | null>(null);
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [deviceInfo, setDeviceInfo] = useState<DeviceInfo | null>(null);
  const [registeredModels, setRegisteredModels] = useState<AiModelManifest[]>([]);
  const [families, setFamilies] = useState<AiModelFamily[]>([]);
  const [activeMetadata, setActiveMetadata] = useState<OnnxModelMetadata | null>(null);
  const [resourceLimits, setResourceLimits] = useState<import('../../types/contracts').AiResourceLimits | null>(null);
  const [runtimeResources, setRuntimeResources] = useState<import('../../types/contracts').AiRuntimeResources | null>(null);
  
  // Phase 6F Model Management State
  const [selectedFamilyId, setSelectedFamilyId] = useState<string | null>(null);
  const [selectedVersionStr, setSelectedVersionStr] = useState<string | null>(null);
  const [validationReport, setValidationReport] = useState<ModelValidationReport | null>(null);
  const [isValidating, setIsValidating] = useState<boolean>(false);
  const [isImportModalOpen, setIsImportModalOpen] = useState<boolean>(false);

  // Import Form State
  const [importSourcePath, setImportSourcePath] = useState<string>('');
  const [importModelId, setImportModelId] = useState<string>('');
  const [importModelName, setImportModelName] = useState<string>('');
  const [importVersion, setImportVersion] = useState<string>('1.0.0');
  const [importDisplayName, setImportDisplayName] = useState<string>('');
  const [importDescription, setImportDescription] = useState<string>('');
  const [importPreset, setImportPreset] = useState<'image' | 'mask' | 'bbox'>('mask');
  const [importTargetWidth, setImportTargetWidth] = useState<number>(640);
  const [importTargetHeight, setImportTargetHeight] = useState<number>(640);
  const [importLayout, setImportLayout] = useState<TensorLayout>('NCHW');
  const [importChannelOrder, setImportChannelOrder] = useState<ChannelOrder>('RGB');
  const [importNormMode, setImportNormMode] = useState<NormalizationMode>('ZERO_TO_ONE');
  const [importAspect, setImportAspect] = useState<AspectHandlingMode>('STRETCH');

  // Phase 6G & 6J Orchestration & Presets State
  const [aiPreset, setAiPreset] = useState<'fast' | 'balanced' | 'quality'>('balanced');
  const [showAdvancedSettings, setShowAdvancedSettings] = useState<boolean>(false);
  const [orchSourcePath, setOrchSourcePath] = useState<string>('');
  const [orchSamplingMode, setOrchSamplingMode] = useState<FrameSamplingMode>('every_nth');
  const [orchSamplingNth, setOrchSamplingNth] = useState<number>(2);
  const [orchProvider, setOrchProvider] = useState<ExecutionProvider | 'AUTO'>('AUTO');
  const [orchPreflightReport, setOrchPreflightReport] = useState<AiJobPreflightReport | null>(null);
  const [orchResolvedModel, setOrchResolvedModel] = useState<ResolvedProductionModel | null>(null);
  const [isPreflighting, setIsPreflighting] = useState<boolean>(false);
  const [isCreatingJob, setIsCreatingJob] = useState<boolean>(false);

  // Phase 6B Console State
  const [inferenceResult, setInferenceResult] = useState<InferenceResult | null>(null);
  const [inferenceInputStr, setInferenceInputStr] = useState<string>('1.0, 2.0, 3.0, 4.0');
  
  // Phase 6C Preprocessing Lab State
  const [prepImagePath, setPrepImagePath] = useState<string>('');
  const [prepTargetWidth, setPrepTargetWidth] = useState<number>(2);
  const [prepTargetHeight, setPrepTargetHeight] = useState<number>(2);
  const [prepResizeFilter, setPrepResizeFilter] = useState<ResizeFilter>('BILINEAR');
  const [prepLetterbox, setPrepLetterbox] = useState<boolean>(false);
  const [prepCenterCrop, setPrepCenterCrop] = useState<boolean>(false);
  const [prepCropWidth, setPrepCropWidth] = useState<number>(2);
  const [prepCropHeight, setPrepCropHeight] = useState<number>(2);
  const [prepChannelOrder, setPrepChannelOrder] = useState<ChannelOrder>('RGB');
  const [prepNormMode, setPrepNormMode] = useState<NormalizationMode>('ZERO_TO_ONE');
  const [prepLayout, setPrepLayout] = useState<TensorLayout>('NCHW');
  const [prepBatchSize] = useState<number>(1);
  
  // Lab Execution Results
  const [prepValidationResult, setPrepValidationResult] = useState<PreprocessValidationResult | null>(null);
  const [prepPreviewResult, setPrepPreviewResult] = useState<PreprocessResult | null>(null);
  
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionSuccess, setActionSuccess] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [isProcessing, setIsProcessing] = useState<boolean>(false);
  const [selectedProvider, setSelectedProvider] = useState<ExecutionProvider | 'AUTO'>('AUTO');
  const [activeTab, setActiveTab] = useState<'ORCHESTRATION' | 'PACKAGES' | 'LAB' | 'CONSOLE' | 'RUNTIME'>('ORCHESTRATION');

  const loadData = async () => {
    setIsLoading(true);
    setActionError(null);
    try {
      const [status, provs, dev, models, modelFamilies, limits, res] = await Promise.all([
        aiApi.getRuntimeStatus().catch(() => null),
        aiApi.getProviders().catch(() => []),
        aiApi.getDevices().catch(() => null),
        aiApi.listModels().catch(() => []),
        aiApi.listModelFamilies().catch(() => []),
        aiApi.getResourceLimits().catch(() => null),
        aiApi.getRuntimeResources().catch(() => null),
      ]);
      setRuntimeStatus(status);
      setProviders(provs);
      setDeviceInfo(dev);
      setRegisteredModels(models);
      setFamilies(modelFamilies);
      setResourceLimits(limits);
      setRuntimeResources(res);

      if (modelFamilies.length > 0 && !selectedFamilyId) {
        setSelectedFamilyId(modelFamilies[0].modelId);
        setSelectedVersionStr(modelFamilies[0].activeVersion || Object.keys(modelFamilies[0].versions)[0] || null);
      }

      if (status?.loadedModelId) {
        const meta = await aiApi.inspectModel().catch(() => null);
        setActiveMetadata(meta);
      } else {
        setActiveMetadata(null);
      }
    } catch (err: any) {
      console.error('Failed to load AI runtime data:', err);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  const currentFamily = families.find(f => f.modelId === selectedFamilyId) || families[0] || null;
  const currentVersionList = currentFamily ? Object.values(currentFamily.versions) : [];
  const currentPackage: AiModelPackage | null = currentFamily && selectedVersionStr 
    ? (currentFamily.versions[selectedVersionStr] || null)
    : (currentFamily && currentFamily.activeVersion ? currentFamily.versions[currentFamily.activeVersion] : null);

  // Update Orchestration model resolution when selected model changes
  useEffect(() => {
    if (selectedFamilyId) {
      const prov = orchProvider === 'AUTO' ? undefined : orchProvider;
      aiApi.resolveProductionModel(selectedFamilyId, selectedVersionStr || undefined, prov)
        .then((res) => {
          setOrchResolvedModel(res);
          setOrchPreflightReport(null);
        })
        .catch(() => {
          setOrchResolvedModel(null);
        });
    }
  }, [selectedFamilyId, selectedVersionStr, orchProvider]);

  const handleValidatePackage = async (modelId: string, version: string) => {
    setIsValidating(true);
    setActionError(null);
    setActionSuccess(null);
    try {
      const report = await aiApi.validateModelPackage(modelId, version);
      setValidationReport(report);
      if (report.valid) {
        setActionSuccess(`Model package '${modelId}' v${version} is fully validated & production-ready.`);
      } else {
        setActionError(`Validation failed: ${report.errors.join('; ')}`);
      }
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsValidating(false);
    }
  };

  const handleActivateVersion = async (modelId: string, version: string) => {
    setIsProcessing(true);
    setActionError(null);
    setActionSuccess(null);
    try {
      await aiApi.activateModelVersion(modelId, version);
      setActionSuccess(`Model '${modelId}' version ${version} is now ACTIVE.`);
      await loadData();
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleRollback = async (modelId: string) => {
    setIsProcessing(true);
    setActionError(null);
    setActionSuccess(null);
    try {
      const pkg = await aiApi.rollbackModel(modelId);
      setActionSuccess(`Model '${modelId}' rolled back to version ${pkg.version}.`);
      await loadData();
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleRemoveVersion = async (modelId: string, version: string) => {
    if (!confirm(`Are you sure you want to remove version ${version} of model '${modelId}'?`)) {
      return;
    }
    setIsProcessing(true);
    setActionError(null);
    setActionSuccess(null);
    try {
      await aiApi.removeModelVersion(modelId, version);
      setActionSuccess(`Version ${version} removed successfully.`);
      await loadData();
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleImportSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!importSourcePath || !importModelId || !importVersion) {
      setActionError('Source path, model ID, and version are required.');
      return;
    }

    setIsProcessing(true);
    setActionError(null);
    setActionSuccess(null);
    try {
      const req: ImportModelRequest = {
        sourcePath: importSourcePath,
        modelId: importModelId,
        modelName: importModelName || importModelId,
        version: importVersion,
        displayName: importDisplayName || importModelName || importModelId,
        description: importDescription || 'Imported production model package',
        profile: {
          input: {
            targetWidth: importTargetWidth,
            targetHeight: importTargetHeight,
            channelOrder: importChannelOrder,
            colorSpace: 'sRGB',
            layout: importLayout,
            normalization: {
              mode: importNormMode,
            },
            resizeFilter: 'BILINEAR',
            aspectHandling: {
              mode: importAspect,
            },
            dataType: 'FLOAT32',
          },
          output: {
            outputType: importPreset === 'mask' ? 'MASK' : (importPreset === 'bbox' ? 'BBOX' : 'IMAGE'),
            coordinateRestoration: false,
          },
        },
      };

      await aiApi.importModel(req);
      setActionSuccess(`Model '${importModelId}' v${importVersion} successfully imported and registered.`);
      setIsImportModalOpen(false);
      await loadData();
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  // Phase 6G Orchestration Actions
  const handleRunOrchestrationPreflight = async () => {
    if (!orchSourcePath.trim()) {
      setActionError('Please specify a valid source video file path');
      return;
    }
    if (!selectedFamilyId) {
      setActionError('Please select a production model family');
      return;
    }

    setIsPreflighting(true);
    setActionError(null);
    setActionSuccess(null);
    try {
      const outputMode: AiFrameOutputMode = currentPackage?.profile.output.outputType === 'MASK' ? 'mask' : 'image';
      const aiConfig: AiJobConfig = {
        enabled: true,
        modelId: selectedFamilyId,
        modelVersion: selectedVersionStr || undefined,
        provider: orchProvider === 'AUTO' ? undefined : orchProvider,
        preprocessing: {
          targetWidth: currentPackage?.profile.input.targetWidth || 640,
          targetHeight: currentPackage?.profile.input.targetHeight || 640,
          resizeFilter: 'BILINEAR',
          letterbox: currentPackage?.profile.input.aspectHandling.mode === 'LETTERBOX',
          letterboxPad: [114, 114, 114],
          centerCrop: currentPackage?.profile.input.aspectHandling.mode === 'CENTER_CROP',
          channelOrder: currentPackage?.profile.input.channelOrder || 'RGB',
          normalization: {
            mode: currentPackage?.profile.input.normalization.mode || 'ZERO_TO_ONE',
          },
          layout: currentPackage?.profile.input.layout || 'NCHW',
          batchSize: 1,
        },
        frameSampling: {
          mode: orchSamplingMode,
          nth: orchSamplingMode === 'every_nth' ? orchSamplingNth : undefined,
        },
        outputMode,
      };

      const report = await runPreflight(orchSourcePath, aiConfig);
      setOrchPreflightReport(report);
      if (report.isValid) {
        setActionSuccess('Deep preflight validation passed! All checks green and ready for job creation.');
      } else {
        setActionError(`Preflight checks failed: ${report.errors.join('; ')}`);
      }
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsPreflighting(false);
    }
  };

  const handleBrowseVideo = async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [
          {
            name: 'Video Files',
            extensions: ['mp4', 'mov', 'avi', 'mkv', 'webm'],
          },
        ],
      });
      if (selected && typeof selected === 'string') {
        setOrchSourcePath(selected);
        setOrchPreflightReport(null);
      }
    } catch (err: any) {
      setActionError(`File selection failed: ${err?.message || err}`);
    }
  };

  const handleUseProjectMedia = () => {
    if (activeProject?.sourceMedia?.sourcePath) {
      setOrchSourcePath(activeProject.sourceMedia.sourcePath);
      setOrchPreflightReport(null);
      setActionSuccess(`Loaded media from active project '${activeProject.name}'`);
    } else {
      setActionError('Active project does not have imported source media.');
    }
  };

  const handleSelectPreset = (preset: 'fast' | 'balanced' | 'quality') => {
    setAiPreset(preset);
    setOrchPreflightReport(null);
    if (preset === 'fast') {
      setOrchSamplingMode('every_nth');
      setOrchSamplingNth(3);
    } else if (preset === 'balanced') {
      setOrchSamplingMode('every_nth');
      setOrchSamplingNth(2);
    } else {
      setOrchSamplingMode('all');
      setOrchSamplingNth(1);
    }
  };

  const handleCreateProductionJob = async () => {
    if (!activeProject?.id) {
      setActionError('Please open or select an active project first in the Projects tab');
      return;
    }
    if (!orchSourcePath.trim()) {
      setActionError('Source video path is required');
      return;
    }
    if (!selectedFamilyId) {
      setActionError('Production model is required');
      return;
    }
    if (!orchPreflightReport?.isValid) {
      setActionError('Preflight validation is required and must pass all checks before starting the job.');
      return;
    }

    setIsCreatingJob(true);
    setActionError(null);
    setActionSuccess(null);
    try {
      const outputMode: AiFrameOutputMode = currentPackage?.profile.output.outputType === 'MASK' ? 'mask' : 'image';
      const aiConfig: AiJobConfig = {
        enabled: true,
        modelId: selectedFamilyId,
        modelVersion: selectedVersionStr || undefined,
        provider: orchProvider === 'AUTO' ? undefined : orchProvider,
        preprocessing: {
          targetWidth: currentPackage?.profile.input.targetWidth || 640,
          targetHeight: currentPackage?.profile.input.targetHeight || 640,
          resizeFilter: 'BILINEAR',
          letterbox: currentPackage?.profile.input.aspectHandling.mode === 'LETTERBOX',
          letterboxPad: [114, 114, 114],
          centerCrop: currentPackage?.profile.input.aspectHandling.mode === 'CENTER_CROP',
          channelOrder: currentPackage?.profile.input.channelOrder || 'RGB',
          normalization: {
            mode: currentPackage?.profile.input.normalization.mode || 'ZERO_TO_ONE',
          },
          layout: currentPackage?.profile.input.layout || 'NCHW',
          batchSize: 1,
        },
        frameSampling: {
          mode: orchSamplingMode,
          nth: orchSamplingMode === 'every_nth' ? orchSamplingNth : undefined,
        },
        outputMode,
      };

      const job = await createAiJob(activeProject.id, [orchSourcePath], aiConfig);
      await startJob(job.id);
      selectJob(job.id);
      setAppNavTab('jobs');
    } catch (err: any) {
      setActionError(err.message || String(err));
      setIsCreatingJob(false);
    }
  };

  const handleGenerateTestModel = async () => {
    setIsProcessing(true);
    setActionError(null);
    setActionSuccess(null);
    try {
      const m = await aiApi.generateTestModel();
      setActionSuccess(`Generated 1D Test Math Multiplier model (${m.id})`);
      await loadData();
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleGenerateImageTestModel = async () => {
    setIsProcessing(true);
    setActionError(null);
    setActionSuccess(null);
    try {
      const m = await aiApi.generateImageTestModel();
      setActionSuccess(`Generated 4D Image Multiplier model (${m.id}) [1, 3, 2, 2]`);
      await loadData();
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleLoadModel = async (modelId: string) => {
    setIsProcessing(true);
    setActionError(null);
    setActionSuccess(null);
    try {
      const prov = selectedProvider === 'AUTO' ? undefined : selectedProvider;
      const meta = await aiApi.loadModel(modelId, prov);
      setActiveMetadata(meta);
      setActionSuccess(`Model '${modelId}' successfully loaded into active ONNX session.`);
      await loadData();
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleUnloadModel = async () => {
    setIsProcessing(true);
    setActionError(null);
    setActionSuccess(null);
    try {
      await aiApi.unloadModel();
      setActiveMetadata(null);
      setActionSuccess('Active model successfully unloaded.');
      await loadData();
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleRunInference = async () => {
    if (!runtimeStatus?.loadedModelId) {
      setActionError('No model loaded in active runtime session');
      return;
    }

    setIsProcessing(true);
    setActionError(null);
    try {
      const values = inferenceInputStr
        .split(',')
        .map((s) => parseFloat(s.trim()))
        .filter((n) => !isNaN(n));

      if (values.length !== 4) {
        throw new Error('Please provide exactly 4 comma-separated float numbers (e.g. 1.0, 2.0, 3.0, 4.0)');
      }

      const res = await aiApi.runInference({
        modelId: runtimeStatus.loadedModelId,
        inputs: [
          {
            name: activeMetadata?.inputs[0]?.name || 'X',
            dataType: 'FLOAT32',
            shape: [1, 4],
            dataF32: values,
          },
        ],
      });
      setInferenceResult(res);
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const handlePreviewPreprocess = async () => {
    if (!prepImagePath.trim()) {
      setActionError('Please specify a valid test image path');
      return;
    }

    setIsProcessing(true);
    setActionError(null);
    try {
      const prepConfig: PreprocessConfig = {
        targetWidth: prepTargetWidth,
        targetHeight: prepTargetHeight,
        resizeFilter: prepResizeFilter,
        letterbox: prepLetterbox,
        letterboxPad: [114, 114, 114],
        centerCrop: prepCenterCrop,
        cropWidth: prepCenterCrop ? prepCropWidth : undefined,
        cropHeight: prepCenterCrop ? prepCropHeight : undefined,
        channelOrder: prepChannelOrder,
        normalization: {
          mode: prepNormMode,
        },
        layout: prepLayout,
        batchSize: prepBatchSize,
      };

      const res = await aiApi.previewPreprocess(prepImagePath, prepConfig);
      setPrepPreviewResult(res);
      setActionSuccess('Image successfully preprocessed into tensor preview');
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleValidatePreprocess = async () => {
    if (!runtimeStatus?.loadedModelId) {
      setActionError('Load an active model in the runtime first to validate preprocessing against it');
      return;
    }

    setIsProcessing(true);
    setActionError(null);
    try {
      const prepConfig: PreprocessConfig = {
        targetWidth: prepTargetWidth,
        targetHeight: prepTargetHeight,
        resizeFilter: prepResizeFilter,
        letterbox: prepLetterbox,
        letterboxPad: [114, 114, 114],
        centerCrop: prepCenterCrop,
        cropWidth: prepCenterCrop ? prepCropWidth : undefined,
        cropHeight: prepCenterCrop ? prepCropHeight : undefined,
        channelOrder: prepChannelOrder,
        normalization: {
          mode: prepNormMode,
        },
        layout: prepLayout,
        batchSize: prepBatchSize,
      };

      const res = await aiApi.validatePreprocess(runtimeStatus.loadedModelId, prepConfig);
      setPrepValidationResult(res);
      if (res.isValid) {
        setActionSuccess('Preprocessing configuration is 100% compatible with model input tensors');
      } else {
        setActionError(`Preprocessing mismatch: ${res.errors.join(', ')}`);
      }
    } catch (err: any) {
      setActionError(err.message || String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const formatBytes = (bytes?: number) => {
    if (!bytes) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  };

  return (
    <div className="space-y-6">
      {/* Header with quick actions */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-bold tracking-tight text-white flex items-center gap-2">
            <Sparkles className="w-5 h-5 text-purple-400" />
            <span>AI Production Studio & Model Orchestration</span>
          </h2>
          <p className="text-xs text-slate-400">
            Production model selection, deep preflight validation, immutable job pinning, and ONNX execution.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={() => setIsImportModalOpen(true)}
            className="px-3.5 py-1.5 rounded-xl bg-purple-600 hover:bg-purple-500 text-white text-xs font-bold flex items-center gap-1.5 transition-all shadow-lg shadow-purple-500/20 cursor-pointer"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Import ONNX Model</span>
          </button>

          <button
            onClick={loadData}
            disabled={isLoading}
            className="p-2 rounded-xl bg-slate-900 border border-slate-800 hover:bg-slate-800 text-slate-300 transition-all cursor-pointer disabled:opacity-50"
            title="Refresh Registry"
          >
            <RotateCw className={`w-4 h-4 ${isLoading ? 'animate-spin text-purple-400' : ''}`} />
          </button>
        </div>
      </div>

      {/* Action alerts */}
      {actionError && (
        <div className="p-3.5 rounded-2xl bg-rose-950/40 border border-rose-500/30 text-rose-300 text-xs flex items-center gap-2.5">
          <XCircle className="w-4 h-4 shrink-0 text-rose-400" />
          <span className="font-mono">{actionError}</span>
        </div>
      )}

      {actionSuccess && (
        <div className="p-3.5 rounded-2xl bg-emerald-950/40 border border-emerald-500/30 text-emerald-300 text-xs flex items-center gap-2.5">
          <CheckCircle2 className="w-4 h-4 shrink-0 text-emerald-400" />
          <span className="font-mono">{actionSuccess}</span>
        </div>
      )}

      {/* Tabs */}
      <div className="flex items-center gap-2 border-b border-slate-800 pb-2 flex-wrap">
        <button
          onClick={() => setActiveTab('ORCHESTRATION')}
          className={`px-4 py-2 rounded-xl text-xs font-bold transition-all flex items-center gap-2 cursor-pointer ${
            activeTab === 'ORCHESTRATION'
              ? 'bg-purple-600/20 text-purple-300 border border-purple-500/40'
              : 'text-slate-400 hover:text-slate-200'
          }`}
        >
          <Sparkles className="w-3.5 h-3.5" />
          <span>Job Orchestration & Preflight</span>
        </button>

        <button
          onClick={() => setActiveTab('PACKAGES')}
          className={`px-4 py-2 rounded-xl text-xs font-bold transition-all flex items-center gap-2 cursor-pointer ${
            activeTab === 'PACKAGES'
              ? 'bg-purple-600/20 text-purple-300 border border-purple-500/40'
              : 'text-slate-400 hover:text-slate-200'
          }`}
        >
          <Package className="w-3.5 h-3.5" />
          <span>Model Registry & Packages</span>
          <span className="px-1.5 py-0.2 rounded bg-purple-500/30 text-[10px] text-purple-200">
            {families.length}
          </span>
        </button>

        <button
          onClick={() => setActiveTab('LAB')}
          className={`px-4 py-2 rounded-xl text-xs font-bold transition-all flex items-center gap-2 cursor-pointer ${
            activeTab === 'LAB'
              ? 'bg-purple-600/20 text-purple-300 border border-purple-500/40'
              : 'text-slate-400 hover:text-slate-200'
          }`}
        >
          <Sliders className="w-3.5 h-3.5" />
          <span>Tensor Pipeline Lab</span>
        </button>

        <button
          onClick={() => setActiveTab('CONSOLE')}
          className={`px-4 py-2 rounded-xl text-xs font-bold transition-all flex items-center gap-2 cursor-pointer ${
            activeTab === 'CONSOLE'
              ? 'bg-purple-600/20 text-purple-300 border border-purple-500/40'
              : 'text-slate-400 hover:text-slate-200'
          }`}
        >
          <Terminal className="w-3.5 h-3.5" />
          <span>Inference Console</span>
        </button>

        <button
          onClick={() => setActiveTab('RUNTIME')}
          className={`px-4 py-2 rounded-xl text-xs font-bold transition-all flex items-center gap-2 cursor-pointer ${
            activeTab === 'RUNTIME'
              ? 'bg-purple-600/20 text-purple-300 border border-purple-500/40'
              : 'text-slate-400 hover:text-slate-200'
          }`}
        >
          <Activity className="w-3.5 h-3.5" />
          <span>Hardware & Providers</span>
        </button>
      </div>

      {/* ------------------------------------------------------------- */}
      {/* TAB 1: PRODUCTION JOB ORCHESTRATION & PREFLIGHT (PHASE 6G) */}
      {/* ------------------------------------------------------------- */}
      {activeTab === 'ORCHESTRATION' && (
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
          {/* Left: Job Configuration Form */}
          <div className="lg:col-span-6 space-y-5">
            <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4">
              <div className="flex items-center justify-between">
                <span className="text-xs font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2">
                  <FileVideo className="w-4 h-4 text-purple-400" />
                  <span>Production AI Video Pipeline</span>
                </span>
                <button
                  type="button"
                  onClick={() => setShowAdvancedSettings(!showAdvancedSettings)}
                  className="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 text-[11px] font-semibold flex items-center gap-1.5 transition-all cursor-pointer"
                >
                  <Sliders className="w-3 h-3 text-indigo-400" />
                  <span>{showAdvancedSettings ? 'Simple View' : 'Advanced Settings'}</span>
                </button>
              </div>

              {/* 1. Presets Selector */}
              <div className="space-y-1.5">
                <label className="text-slate-400 font-semibold block text-xs">AI Processing Preset:</label>
                <div className="grid grid-cols-3 gap-2.5">
                  <button
                    type="button"
                    onClick={() => handleSelectPreset('fast')}
                    className={`p-3 rounded-xl border text-left transition-all cursor-pointer ${
                      aiPreset === 'fast'
                        ? 'bg-indigo-600/20 border-indigo-500 text-white shadow-md shadow-indigo-950/50'
                        : 'bg-slate-950/60 border-slate-800 text-slate-400 hover:border-slate-700'
                    }`}
                  >
                    <div className="flex items-center justify-between mb-1">
                      <span className="font-bold text-xs flex items-center gap-1 text-indigo-300">
                        <Sparkles className="w-3.5 h-3.5 text-amber-400" />
                        <span>Fast</span>
                      </span>
                      {aiPreset === 'fast' && <Check className="w-3.5 h-3.5 text-indigo-400" />}
                    </div>
                    <p className="text-[10px] text-slate-400 leading-tight">Every 3rd frame. Rapid draft previews.</p>
                  </button>

                  <button
                    type="button"
                    onClick={() => handleSelectPreset('balanced')}
                    className={`p-3 rounded-xl border text-left transition-all cursor-pointer ${
                      aiPreset === 'balanced'
                        ? 'bg-purple-600/20 border-purple-500 text-white shadow-md shadow-purple-950/50'
                        : 'bg-slate-950/60 border-slate-800 text-slate-400 hover:border-slate-700'
                    }`}
                  >
                    <div className="flex items-center justify-between mb-1">
                      <span className="font-bold text-xs flex items-center gap-1 text-purple-300">
                        <Sparkles className="w-3.5 h-3.5 text-purple-400" />
                        <span>Balanced</span>
                      </span>
                      {aiPreset === 'balanced' && <Check className="w-3.5 h-3.5 text-purple-400" />}
                    </div>
                    <p className="text-[10px] text-slate-400 leading-tight">Every 2nd frame. Recommended default.</p>
                  </button>

                  <button
                    type="button"
                    onClick={() => handleSelectPreset('quality')}
                    className={`p-3 rounded-xl border text-left transition-all cursor-pointer ${
                      aiPreset === 'quality'
                        ? 'bg-emerald-600/20 border-emerald-500 text-white shadow-md shadow-emerald-950/50'
                        : 'bg-slate-950/60 border-slate-800 text-slate-400 hover:border-slate-700'
                    }`}
                  >
                    <div className="flex items-center justify-between mb-1">
                      <span className="font-bold text-xs flex items-center gap-1 text-emerald-300">
                        <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
                        <span>Quality</span>
                      </span>
                      {aiPreset === 'quality' && <Check className="w-3.5 h-3.5 text-emerald-400" />}
                    </div>
                    <p className="text-[10px] text-slate-400 leading-tight">100% all frames. Maximum fidelity.</p>
                  </button>
                </div>
              </div>

              <div className="space-y-3.5 text-xs">
                {/* Source Video File */}
                <div className="space-y-1.5">
                  <div className="flex items-center justify-between">
                    <label className="text-slate-400 font-semibold">Input Source Video:</label>
                    <div className="flex items-center gap-1.5">
                      {activeProject?.sourceMedia?.sourcePath && (
                        <button
                          type="button"
                          onClick={handleUseProjectMedia}
                          className="px-2 py-0.5 rounded-lg bg-indigo-500/20 hover:bg-indigo-500/30 border border-indigo-500/30 text-indigo-300 text-[10px] font-bold cursor-pointer transition-all"
                        >
                          Use Project Media
                        </button>
                      )}
                      <button
                        type="button"
                        onClick={handleBrowseVideo}
                        className="px-2 py-0.5 rounded-lg bg-slate-800 hover:bg-slate-700 border border-slate-700 text-slate-300 text-[10px] font-bold flex items-center gap-1 cursor-pointer transition-all"
                      >
                        <FolderOpen className="w-3 h-3 text-purple-400" />
                        <span>Browse Video...</span>
                      </button>
                    </div>
                  </div>
                  <input
                    type="text"
                    value={orchSourcePath}
                    onChange={(e) => {
                      setOrchSourcePath(e.target.value);
                      setOrchPreflightReport(null);
                    }}
                    placeholder="Select a video file (.mp4, .mov, .mkv, .avi, .webm)..."
                    className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs"
                  />
                </div>

                {/* Model Selection */}
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  <div className="space-y-1.5">
                    <label className="text-slate-400 font-semibold">Active AI Model:</label>
                    <select
                      value={selectedFamilyId || ''}
                      onChange={(e) => {
                        setSelectedFamilyId(e.target.value);
                        setOrchPreflightReport(null);
                        const fam = families.find(f => f.modelId === e.target.value);
                        setSelectedVersionStr(fam?.activeVersion || (fam ? Object.keys(fam.versions)[0] : null));
                      }}
                      className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 text-xs font-mono"
                    >
                      {families.map((f) => (
                        <option key={f.modelId} value={f.modelId}>
                          {f.name} ({f.modelId})
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="space-y-1.5">
                    <label className="text-slate-400 font-semibold">Model Version:</label>
                    <select
                      value={selectedVersionStr || ''}
                      onChange={(e) => {
                        setSelectedVersionStr(e.target.value);
                        setOrchPreflightReport(null);
                      }}
                      className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 text-xs font-mono"
                    >
                      {currentVersionList.map((pkg) => (
                        <option key={pkg.version} value={pkg.version}>
                          v{pkg.version} {currentFamily?.activeVersion === pkg.version ? '(ACTIVE)' : ''}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>

                {/* Advanced Settings Section (Collapsible) */}
                {showAdvancedSettings && (
                  <div className="p-3.5 rounded-xl bg-slate-950/80 border border-slate-800 space-y-3 pt-3">
                    <span className="text-[11px] font-bold text-slate-400 uppercase tracking-wider block">
                      Advanced Pipeline Controls
                    </span>

                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                      <div className="space-y-1.5">
                        <label className="text-slate-400 font-semibold">Hardware Provider:</label>
                        <select
                          value={orchProvider}
                          onChange={(e) => {
                            setOrchProvider(e.target.value as any);
                            setOrchPreflightReport(null);
                          }}
                          className="w-full px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-slate-200 text-xs font-mono"
                        >
                          <option value="AUTO">AUTO (Recommended)</option>
                          <option value="CPU">CPU Universal</option>
                          <option value="DIRECT_ML">DirectML (GPU)</option>
                          <option value="CUDA">NVIDIA CUDA</option>
                        </select>
                      </div>

                      <div className="space-y-1.5">
                        <label className="text-slate-400 font-semibold">Sampling Mode:</label>
                        <select
                          value={orchSamplingMode}
                          onChange={(e) => {
                            setOrchSamplingMode(e.target.value as any);
                            setOrchPreflightReport(null);
                          }}
                          className="w-full px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-slate-200 text-xs font-mono"
                        >
                          <option value="all">All Frames (100% AI)</option>
                          <option value="every_nth">Every Nth Frame (Passthrough)</option>
                        </select>
                      </div>
                    </div>

                    {orchSamplingMode === 'every_nth' && (
                      <div className="space-y-1.5">
                        <label className="text-slate-400 font-semibold">Sample Interval (N):</label>
                        <input
                          type="number"
                          min={2}
                          max={60}
                          value={orchSamplingNth}
                          onChange={(e) => {
                            setOrchSamplingNth(parseInt(e.target.value) || 2);
                            setOrchPreflightReport(null);
                          }}
                          className="w-full px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-slate-200 font-mono text-xs"
                        />
                      </div>
                    )}
                  </div>
                )}

                {/* Preflight Gate Warning when unverified or failed */}
                {!orchPreflightReport?.isValid && (
                  <div className="p-3 rounded-xl bg-amber-950/20 border border-amber-500/30 text-amber-300 text-[11px] flex items-center gap-2">
                    <AlertTriangle className="w-4 h-4 shrink-0 text-amber-400" />
                    <span>Run Deep Preflight Check first. Start Pipeline is enabled only when all preflight checks pass.</span>
                  </div>
                )}

                {/* Preflight & Create Action Buttons */}
                <div className="flex items-center gap-3 pt-3 border-t border-slate-800">
                  <button
                    onClick={handleRunOrchestrationPreflight}
                    disabled={isPreflighting || !orchSourcePath.trim() || !selectedFamilyId}
                    className="px-4 py-2.5 rounded-xl bg-purple-600/20 hover:bg-purple-600/30 border border-purple-500/40 text-purple-300 text-xs font-bold flex items-center gap-2 transition-all cursor-pointer disabled:opacity-50"
                  >
                    <ShieldCheck className={`w-4 h-4 ${isPreflighting ? 'animate-spin' : ''}`} />
                    <span>{isPreflighting ? 'Running Preflight...' : 'Run Deep Preflight Check'}</span>
                  </button>

                  <button
                    onClick={handleCreateProductionJob}
                    disabled={isCreatingJob || !orchPreflightReport?.isValid}
                    className="px-4 py-2.5 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-bold flex items-center gap-2 transition-all cursor-pointer shadow-lg shadow-emerald-500/20 disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    <Play className="w-3.5 h-3.5 fill-current" />
                    <span>{isCreatingJob ? 'Starting AI Job...' : 'Start AI Video Pipeline'}</span>
                  </button>
                </div>
              </div>
            </div>

            {/* Resolved Production Model Specs Card */}
            {orchResolvedModel && (
              <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3 font-mono text-xs">
                <div className="flex items-center justify-between font-sans">
                  <span className="text-xs font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2">
                    <ShieldCheck className="w-4 h-4 text-emerald-400" />
                    <span>Pinned Model Package Specs</span>
                  </span>
                  <span className={`px-2 py-0.5 rounded-full text-[10px] font-bold flex items-center gap-1 ${
                    currentPackage?.isProduction
                      ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30'
                      : 'bg-amber-500/20 text-amber-300 border border-amber-500/30'
                  }`}>
                    {currentPackage?.isProduction ? (
                      <>
                        <CheckCircle2 className="w-3 h-3 text-emerald-400" />
                        <span>Production Model</span>
                      </>
                    ) : (
                      <>
                        <Sparkles className="w-3 h-3 text-amber-400" />
                        <span>Development / Test Model</span>
                      </>
                    )}
                  </span>
                </div>

                <div className="grid grid-cols-2 gap-2 text-[11px]">
                  <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-0.5">
                    <span className="text-slate-500 block text-[10px]">Model & Version</span>
                    <span className="text-slate-200 font-bold">{orchResolvedModel.modelId} v{orchResolvedModel.modelVersion}</span>
                  </div>
                  <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-0.5">
                    <span className="text-slate-500 block text-[10px]">Resolved Provider</span>
                    <span className="text-purple-300 font-bold">{orchResolvedModel.provider}</span>
                  </div>
                </div>

                <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-0.5 text-[11px]">
                  <span className="text-slate-500 block text-[10px]">Pinned SHA-256 Checksum</span>
                  <span className="text-purple-300 break-all text-[10px]">{orchResolvedModel.modelHash}</span>
                </div>

                <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 flex items-center justify-between text-[11px]">
                  <span className="text-slate-400">Profile Geometry & Layout:</span>
                  <span className="text-slate-200">
                    {orchResolvedModel.profile.input.targetWidth}x{orchResolvedModel.profile.input.targetHeight} • {orchResolvedModel.profile.input.layout} ({orchResolvedModel.profile.input.channelOrder})
                  </span>
                </div>
              </div>
            )}
          </div>

          {/* Right: Preflight Checklist & Diagnostics */}
          <div className="lg:col-span-6 space-y-5">
            {orchPreflightReport ? (
              <div className={`p-5 rounded-2xl border space-y-4 font-mono text-xs ${
                orchPreflightReport.isValid
                  ? 'bg-emerald-950/15 border-emerald-500/30'
                  : 'bg-rose-950/15 border-rose-500/30'
              }`}>
                <div className="flex items-center justify-between font-sans">
                  <span className="text-xs font-bold uppercase tracking-wider flex items-center gap-2">
                    <ShieldCheck className={`w-4 h-4 ${orchPreflightReport.isValid ? 'text-emerald-400' : 'text-rose-400'}`} />
                    <span>Preflight Validation Report</span>
                  </span>
                  <span className={`px-2.5 py-0.5 rounded-full text-[10px] font-bold ${
                    orchPreflightReport.isValid
                      ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/40'
                      : 'bg-rose-500/20 text-rose-300 border border-rose-500/40'
                  }`}>
                    {orchPreflightReport.isValid ? 'ALL CHECKS PASSED' : 'PREFLIGHT FAILED'}
                  </span>
                </div>

                {/* Individual Check Cards */}
                <div className="space-y-2 max-h-[480px] overflow-y-auto pr-1">
                  {orchPreflightReport.checks.map((c, i) => (
                    <div
                      key={i}
                      className={`p-3 rounded-xl border flex items-start justify-between gap-3 text-[11px] ${
                        c.status === 'PASS'
                          ? 'bg-slate-950/80 border-slate-800/80 text-slate-300'
                          : c.status === 'WARN'
                          ? 'bg-amber-950/20 border-amber-500/30 text-amber-300'
                          : 'bg-rose-950/20 border-rose-500/30 text-rose-300'
                      }`}
                    >
                      <div className="space-y-0.5">
                        <div className="flex items-center gap-1.5 font-bold font-sans">
                          {c.status === 'PASS' && <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 shrink-0" />}
                          {c.status === 'WARN' && <AlertTriangle className="w-3.5 h-3.5 text-amber-400 shrink-0" />}
                          {c.status === 'FAIL' && <XCircle className="w-3.5 h-3.5 text-rose-400 shrink-0" />}
                          <span>{c.check}</span>
                        </div>
                        <p className="text-[11px] text-slate-400 font-mono">{c.message}</p>
                        {c.technicalDetail && (
                          <p className="text-[10px] text-slate-500 font-mono">{c.technicalDetail}</p>
                        )}
                      </div>
                      <span className={`px-1.5 py-0.2 rounded text-[9px] font-bold shrink-0 ${
                        c.status === 'PASS'
                          ? 'bg-emerald-500/10 text-emerald-300'
                          : c.status === 'WARN'
                          ? 'bg-amber-500/10 text-amber-300'
                          : 'bg-rose-500/10 text-rose-300'
                      }`}>
                        {c.status}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ) : (
              <div className="h-64 flex flex-col items-center justify-center text-center p-6 border border-dashed border-slate-800 rounded-2xl text-slate-500 text-xs">
                <ShieldCheck className="w-8 h-8 mb-2 text-slate-600 stroke-1" />
                <span>Configure your source video and model on the left, then click 'Run Deep Preflight Check'.</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* ------------------------------------------------------------- */}
      {/* TAB 2: PRODUCTION MODEL PACKAGES & VERSIONS (PHASE 6F) */}
      {/* ------------------------------------------------------------- */}
      {activeTab === 'PACKAGES' && (
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
          {/* Left Column: Model Families List */}
          <div className="lg:col-span-4 space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-slate-400 uppercase tracking-wider">Installed Model Families</span>
              <div className="flex gap-1.5">
                <button
                  onClick={handleGenerateImageTestModel}
                  disabled={isProcessing}
                  className="px-2 py-1 rounded bg-slate-900 border border-slate-800 hover:border-purple-500/40 text-[10px] text-purple-300 font-mono cursor-pointer"
                  title="Generate 4D Image ONNX Model fixture"
                >
                  + Image Fixture
                </button>
                <button
                  onClick={handleGenerateTestModel}
                  disabled={isProcessing}
                  className="px-2 py-1 rounded bg-slate-900 border border-slate-800 hover:border-purple-500/40 text-[10px] text-purple-300 font-mono cursor-pointer"
                  title="Generate 1D Math ONNX Model fixture"
                >
                  + Math Fixture
                </button>
              </div>
            </div>

            <div className="space-y-2.5">
              {families.length === 0 ? (
                <div className="p-6 rounded-2xl bg-slate-900/40 border border-dashed border-slate-800 text-center space-y-2 text-slate-500 text-xs">
                  <Package className="w-8 h-8 mx-auto text-slate-600 stroke-1" />
                  <p>No model packages installed yet.</p>
                  <p className="text-[10px]">Import an ONNX model or create a test fixture.</p>
                </div>
              ) : (
                families.map((f) => {
                  const isSelected = selectedFamilyId === f.modelId;
                  const versionCount = Object.keys(f.versions).length;
                  return (
                    <div
                      key={f.modelId}
                      onClick={() => {
                        setSelectedFamilyId(f.modelId);
                        setSelectedVersionStr(f.activeVersion || Object.keys(f.versions)[0] || null);
                        setValidationReport(null);
                      }}
                      className={`p-4 rounded-2xl border transition-all cursor-pointer space-y-2 ${
                        isSelected
                          ? 'bg-purple-950/20 border-purple-500/50 shadow-md shadow-purple-950/20'
                          : 'bg-slate-900/60 border-slate-800 hover:border-slate-700'
                      }`}
                    >
                      <div className="flex items-start justify-between">
                        <div>
                          <h4 className="font-bold text-white text-sm">{f.name}</h4>
                          <span className="text-[10px] font-mono text-slate-400">{f.modelId}</span>
                        </div>
                        {f.activeVersion && (
                          <span className="px-2 py-0.5 rounded-full bg-emerald-500/10 border border-emerald-500/30 text-emerald-300 text-[10px] font-bold font-mono">
                            ACTIVE v{f.activeVersion}
                          </span>
                        )}
                      </div>

                      <div className="flex items-center justify-between text-[10px] text-slate-500 font-mono pt-2 border-t border-slate-800/60">
                        <span>{versionCount} {versionCount === 1 ? 'version' : 'versions'} installed</span>
                        {f.previousVersion && (
                          <span className="text-indigo-400">Rollback available (v{f.previousVersion})</span>
                        )}
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </div>

          {/* Right Column: Model Package Details, Versions & Validation */}
          <div className="lg:col-span-8 space-y-5">
            {currentFamily && currentPackage ? (
              <>
                {/* Active Version & Package Overview Card */}
                <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4">
                  <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                    <div>
                      <div className="flex items-center gap-2">
                        <h3 className="text-base font-bold text-white">{currentPackage.displayName || currentPackage.modelName}</h3>
                        <span className="px-2 py-0.5 rounded text-[10px] font-bold font-mono bg-purple-500/20 text-purple-300 border border-purple-500/30 uppercase">
                          v{currentPackage.version}
                        </span>
                        {currentFamily.activeVersion === currentPackage.version ? (
                          <span className="px-2 py-0.5 rounded text-[10px] font-bold font-mono bg-emerald-500/20 text-emerald-300 border border-emerald-500/30">
                            ACTIVE IN PRODUCTION
                          </span>
                        ) : (
                          <span className="px-2 py-0.5 rounded text-[10px] font-bold font-mono bg-slate-800 text-slate-400">
                            INACTIVE VERSION
                          </span>
                        )}
                      </div>
                      <p className="text-xs text-slate-400 mt-1">{currentPackage.description}</p>
                    </div>

                    {/* Version Selector Pill List */}
                    <div className="flex items-center gap-1.5 flex-wrap">
                      {currentVersionList.map((pkg) => (
                        <button
                          key={pkg.version}
                          onClick={() => {
                            setSelectedVersionStr(pkg.version);
                            setValidationReport(null);
                          }}
                          className={`px-2.5 py-1 rounded-lg text-xs font-mono transition-all cursor-pointer ${
                            selectedVersionStr === pkg.version
                              ? 'bg-purple-600 text-white font-bold'
                              : 'bg-slate-950 border border-slate-800 text-slate-400 hover:text-slate-200'
                          }`}
                        >
                          v{pkg.version}
                        </button>
                      ))}
                    </div>
                  </div>

                  {/* Actions Strip */}
                  <div className="flex items-center justify-between pt-3 border-t border-slate-800/80 flex-wrap gap-2">
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => handleValidatePackage(currentFamily.modelId, currentPackage.version)}
                        disabled={isValidating}
                        className="px-3.5 py-1.5 rounded-xl bg-purple-600/20 hover:bg-purple-600/30 border border-purple-500/40 text-purple-300 text-xs font-bold flex items-center gap-1.5 transition-all cursor-pointer disabled:opacity-50"
                      >
                        <ShieldCheck className={`w-3.5 h-3.5 ${isValidating ? 'animate-spin' : ''}`} />
                        <span>{isValidating ? 'Validating Graph...' : 'Deep Validate Package'}</span>
                      </button>

                      {currentFamily.activeVersion !== currentPackage.version && (
                        <button
                          onClick={() => handleActivateVersion(currentFamily.modelId, currentPackage.version)}
                          disabled={isProcessing}
                          className="px-3.5 py-1.5 rounded-xl bg-emerald-600/20 hover:bg-emerald-600/30 border border-emerald-500/40 text-emerald-300 text-xs font-bold flex items-center gap-1.5 transition-all cursor-pointer disabled:opacity-50"
                        >
                          <Check className="w-3.5 h-3.5" />
                          <span>Activate v{currentPackage.version}</span>
                        </button>
                      )}

                      {currentFamily.previousVersion && (
                        <button
                          onClick={() => handleRollback(currentFamily.modelId)}
                          disabled={isProcessing}
                          className="px-3 py-1.5 rounded-xl bg-indigo-600/20 hover:bg-indigo-600/30 border border-indigo-500/40 text-indigo-300 text-xs font-bold flex items-center gap-1.5 transition-all cursor-pointer disabled:opacity-50"
                        >
                          <RotateCcw className="w-3.5 h-3.5" />
                          <span>Rollback to v{currentFamily.previousVersion}</span>
                        </button>
                      )}
                    </div>

                    <button
                      onClick={() => handleRemoveVersion(currentFamily.modelId, currentPackage.version)}
                      disabled={isProcessing || (currentFamily.activeVersion === currentPackage.version && currentVersionList.length > 1)}
                      className="px-2.5 py-1.5 rounded-xl bg-slate-950 border border-slate-800 hover:border-rose-500/40 hover:text-rose-300 text-slate-500 text-xs font-mono transition-all cursor-pointer disabled:opacity-30"
                      title={currentFamily.activeVersion === currentPackage.version && currentVersionList.length > 1 ? "Cannot delete currently active version" : "Remove version"}
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>

                {/* Validation Report Card */}
                {validationReport && (
                  <div className={`p-5 rounded-2xl border space-y-3 font-mono text-xs ${
                    validationReport.valid 
                      ? 'bg-emerald-950/20 border-emerald-500/40' 
                      : 'bg-rose-950/20 border-rose-500/40'
                  }`}>
                    <div className="flex items-center justify-between font-sans">
                      <span className="text-xs font-bold uppercase tracking-wider flex items-center gap-2">
                        <ShieldCheck className={`w-4 h-4 ${validationReport.valid ? 'text-emerald-400' : 'text-rose-400'}`} />
                        <span>Package Deep Validation Report</span>
                      </span>
                      <span className={`px-2.5 py-0.5 rounded-full text-[10px] font-bold ${
                        validationReport.valid 
                          ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/40' 
                          : 'bg-rose-500/20 text-rose-300 border border-rose-500/40'
                      }`}>
                        {validationReport.valid ? 'PASSED & PRODUCTION READY' : 'VALIDATION FAILED'}
                      </span>
                    </div>

                    <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 pt-2 text-[11px]">
                      <div className="p-2.5 rounded-xl bg-slate-950/80 border border-slate-800/80 flex items-center justify-between">
                        <span className="text-slate-400">File SHA-256:</span>
                        <span className={validationReport.integrityValid ? 'text-emerald-400' : 'text-rose-400'}>
                          {validationReport.integrityValid ? '✓ MATCH' : '✗ MISMATCH'}
                        </span>
                      </div>
                      <div className="p-2.5 rounded-xl bg-slate-950/80 border border-slate-800/80 flex items-center justify-between">
                        <span className="text-slate-400">ONNX Graph:</span>
                        <span className={validationReport.onnxValid ? 'text-emerald-400' : 'text-rose-400'}>
                          {validationReport.onnxValid ? '✓ VALID' : '✗ INVALID'}
                        </span>
                      </div>
                      <div className="p-2.5 rounded-xl bg-slate-950/80 border border-slate-800/80 flex items-center justify-between">
                        <span className="text-slate-400">Profile Match:</span>
                        <span className={validationReport.profileValid ? 'text-emerald-400' : 'text-rose-400'}>
                          {validationReport.profileValid ? '✓ MATCH' : '✗ MISMATCH'}
                        </span>
                      </div>
                      <div className="p-2.5 rounded-xl bg-slate-950/80 border border-slate-800/80 flex items-center justify-between">
                        <span className="text-slate-400">Hardware EP:</span>
                        <span className="text-emerald-400 font-bold">
                          ✓ AVAILABLE
                        </span>
                      </div>
                    </div>

                    {validationReport.errors.length > 0 && (
                      <div className="p-3 rounded-xl bg-rose-950/40 border border-rose-500/30 text-rose-300 text-[11px] space-y-1">
                        <span className="font-bold block">Validation Errors:</span>
                        {validationReport.errors.map((err, i) => (
                          <p key={i}>• {err}</p>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {/* Package Specifications & Profiles */}
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs font-mono">
                  {/* File & Hash Specs */}
                  <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3">
                    <span className="text-xs font-bold text-slate-300 uppercase tracking-wider font-sans block">
                      Physical File & Integrity
                    </span>
                    <div className="space-y-2 text-[11px]">
                      <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-1">
                        <span className="text-slate-500 block text-[10px]">Managed File Path</span>
                        <span className="text-slate-200 break-all block">{currentPackage.modelFile}</span>
                      </div>
                      <div className="grid grid-cols-2 gap-2">
                        <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80">
                          <span className="text-slate-500 block text-[10px]">File Size</span>
                          <span className="text-slate-200 font-bold">{formatBytes(currentPackage.fileSizeBytes)}</span>
                        </div>
                        <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80">
                          <span className="text-slate-500 block text-[10px]">Format</span>
                          <span className="text-purple-300 uppercase font-bold">{currentPackage.modelFormat}</span>
                        </div>
                      </div>
                      <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-1">
                        <span className="text-slate-500 block text-[10px]">SHA-256 Checksum</span>
                        <span className="text-purple-300 break-all text-[10px]">{currentPackage.sha256}</span>
                      </div>
                    </div>
                  </div>

                  {/* Preprocessing & Output Profile Specs */}
                  <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3">
                    <span className="text-xs font-bold text-slate-300 uppercase tracking-wider font-sans block">
                      Validated Model Profile
                    </span>
                    <div className="space-y-2 text-[11px]">
                      <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-1">
                        <div className="flex justify-between text-slate-400">
                          <span>Target Geometry:</span>
                          <span className="text-slate-200">{currentPackage.profile.input.targetWidth}x{currentPackage.profile.input.targetHeight}</span>
                        </div>
                        <div className="flex justify-between text-slate-400">
                          <span>Layout & Channel:</span>
                          <span className="text-slate-200">{currentPackage.profile.input.layout} • {currentPackage.profile.input.channelOrder}</span>
                        </div>
                        <div className="flex justify-between text-slate-400">
                          <span>Normalization:</span>
                          <span className="text-slate-200">{currentPackage.profile.input.normalization.mode}</span>
                        </div>
                        <div className="flex justify-between text-slate-400">
                          <span>Aspect Handling:</span>
                          <span className="text-slate-200">{currentPackage.profile.input.aspectHandling.mode}</span>
                        </div>
                      </div>

                      <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-1">
                        <div className="flex justify-between text-slate-400">
                          <span>Output Type:</span>
                          <span className="text-emerald-400 font-bold">{currentPackage.profile.output.outputType}</span>
                        </div>
                        <div className="flex justify-between text-slate-400">
                          <span>Confidence Threshold:</span>
                          <span className="text-slate-200">{currentPackage.profile.output.threshold ?? 'Default (0.5)'}</span>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </>
            ) : (
              <div className="h-64 flex flex-col items-center justify-center text-center p-6 border border-dashed border-slate-800 rounded-2xl text-slate-500 text-xs">
                <Package className="w-8 h-8 mb-2 text-slate-600 stroke-1" />
                <span>Select a model family on the left to inspect versions and profiles.</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* ------------------------------------------------------------- */}
      {/* IMPORT ONNX MODEL MODAL */}
      {/* ------------------------------------------------------------- */}
      {isImportModalOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4">
          <div className="w-full max-w-2xl bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl p-6 space-y-5">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <h3 className="text-base font-bold text-white flex items-center gap-2">
                <Plus className="w-4 h-4 text-purple-400" />
                <span>Import Local ONNX Model Package</span>
              </h3>
              <button
                onClick={() => setIsImportModalOpen(false)}
                className="text-slate-400 hover:text-white text-xs cursor-pointer"
              >
                ✕ Close
              </button>
            </div>

            <form onSubmit={handleImportSubmit} className="space-y-4 text-xs">
              <div className="space-y-1.5">
                <label className="text-slate-400 font-semibold">Local .onnx File Path:</label>
                <input
                  type="text"
                  value={importSourcePath}
                  onChange={(e) => setImportSourcePath(e.target.value)}
                  placeholder="D:/models/my-model.onnx"
                  className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono"
                  required
                />
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                <div className="space-y-1.5">
                  <label className="text-slate-400 font-semibold">Model ID (Family):</label>
                  <input
                    type="text"
                    value={importModelId}
                    onChange={(e) => setImportModelId(e.target.value)}
                    placeholder="person-segmentation"
                    className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono"
                    required
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-slate-400 font-semibold">Semantic Version:</label>
                  <input
                    type="text"
                    value={importVersion}
                    onChange={(e) => setImportVersion(e.target.value)}
                    placeholder="1.0.0"
                    className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono"
                    required
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-slate-400 font-semibold">Model Title / Name:</label>
                  <input
                    type="text"
                    value={importModelName}
                    onChange={(e) => setImportModelName(e.target.value)}
                    placeholder="Person Segmenter"
                    className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200"
                  />
                </div>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div className="space-y-1.5">
                  <label className="text-slate-400 font-semibold">Display Name:</label>
                  <input
                    type="text"
                    value={importDisplayName}
                    onChange={(e) => setImportDisplayName(e.target.value)}
                    placeholder="Person Segmentation Pro"
                    className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200"
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-slate-400 font-semibold">Description:</label>
                  <input
                    type="text"
                    value={importDescription}
                    onChange={(e) => setImportDescription(e.target.value)}
                    placeholder="Production ONNX model for video inference"
                    className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200"
                  />
                </div>
              </div>

              {/* Profile Config Grid */}
              <div className="p-4 rounded-xl bg-slate-950/80 border border-slate-800 space-y-3 font-mono">
                <span className="text-[11px] font-bold text-slate-300 uppercase tracking-wider font-sans block">
                  Model Profile Specifications
                </span>
                <div className="grid grid-cols-2 sm:grid-cols-3 gap-3 text-[11px]">
                  <div className="space-y-1">
                    <span className="text-slate-500">Output Mode:</span>
                    <select
                      value={importPreset}
                      onChange={(e) => setImportPreset(e.target.value as any)}
                      className="w-full p-1.5 rounded bg-slate-900 border border-slate-800 text-slate-200"
                    >
                      <option value="mask">MASK</option>
                      <option value="image">IMAGE</option>
                      <option value="bbox">BBOX</option>
                    </select>
                  </div>
                  <div className="space-y-1">
                    <span className="text-slate-500">Input Size (W x H):</span>
                    <div className="flex items-center gap-1">
                      <input
                        type="number"
                        value={importTargetWidth}
                        onChange={(e) => setImportTargetWidth(parseInt(e.target.value) || 640)}
                        className="w-1/2 p-1.5 rounded bg-slate-900 border border-slate-800 text-slate-200 text-center"
                      />
                      <input
                        type="number"
                        value={importTargetHeight}
                        onChange={(e) => setImportTargetHeight(parseInt(e.target.value) || 640)}
                        className="w-1/2 p-1.5 rounded bg-slate-900 border border-slate-800 text-slate-200 text-center"
                      />
                    </div>
                  </div>
                  <div className="space-y-1">
                    <span className="text-slate-500">Tensor Layout:</span>
                    <select
                      value={importLayout}
                      onChange={(e) => setImportLayout(e.target.value as any)}
                      className="w-full p-1.5 rounded bg-slate-900 border border-slate-800 text-slate-200"
                    >
                      <option value="NCHW">NCHW</option>
                      <option value="NHWC">NHWC</option>
                    </select>
                  </div>
                  <div className="space-y-1">
                    <span className="text-slate-500">Channel Order:</span>
                    <select
                      value={importChannelOrder}
                      onChange={(e) => setImportChannelOrder(e.target.value as any)}
                      className="w-full p-1.5 rounded bg-slate-900 border border-slate-800 text-slate-200"
                    >
                      <option value="RGB">RGB</option>
                      <option value="BGR">BGR</option>
                      <option value="RGBA">RGBA</option>
                      <option value="GRAY">Grayscale</option>
                    </select>
                  </div>
                  <div className="space-y-1">
                    <span className="text-slate-500">Normalization:</span>
                    <select
                      value={importNormMode}
                      onChange={(e) => setImportNormMode(e.target.value as any)}
                      className="w-full p-1.5 rounded bg-slate-900 border border-slate-800 text-slate-200"
                    >
                      <option value="ZERO_TO_ONE">[0, 1]</option>
                      <option value="MINUS_ONE_TO_ONE">[-1, 1]</option>
                      <option value="IDENTITY">Identity</option>
                      <option value="IMAGENET">ImageNet Mean/Std</option>
                    </select>
                  </div>
                  <div className="space-y-1">
                    <span className="text-slate-500">Aspect Handling:</span>
                    <select
                      value={importAspect}
                      onChange={(e) => setImportAspect(e.target.value as any)}
                      className="w-full p-1.5 rounded bg-slate-900 border border-slate-800 text-slate-200"
                    >
                      <option value="STRETCH">Stretch</option>
                      <option value="LETTERBOX">Letterbox</option>
                      <option value="CENTER_CROP">Center Crop</option>
                    </select>
                  </div>
                </div>
              </div>

              <div className="flex items-center justify-end gap-2 pt-2">
                <button
                  type="button"
                  onClick={() => setIsImportModalOpen(false)}
                  className="px-4 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-400 hover:text-white cursor-pointer"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={isProcessing}
                  className="px-4 py-2 rounded-xl bg-purple-600 hover:bg-purple-500 text-white font-bold flex items-center gap-1.5 cursor-pointer disabled:opacity-50"
                >
                  <Plus className="w-3.5 h-3.5" />
                  <span>{isProcessing ? 'Importing & Validating...' : 'Validate & Import Model'}</span>
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* ------------------------------------------------------------- */}
      {/* TAB 3: TENSOR PREPROCESSING LAB (PHASE 6C) */}
      {/* ------------------------------------------------------------- */}
      {activeTab === 'LAB' && (
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
          <div className="lg:col-span-6 space-y-5">
            <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4">
              <span className="text-xs font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2">
                <Sliders className="w-4 h-4 text-purple-400" />
                <span>Preprocessing Configuration</span>
              </span>

              <div className="space-y-3.5 text-xs">
                <div className="space-y-1.5">
                  <label className="text-slate-400 font-semibold">Test Frame / Image File Path:</label>
                  <input
                    type="text"
                    value={prepImagePath}
                    onChange={(e) => setPrepImagePath(e.target.value)}
                    placeholder="D:/test/frame.png"
                    className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs"
                  />
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1.5">
                    <label className="text-slate-400 font-semibold">Target Width:</label>
                    <input
                      type="number"
                      value={prepTargetWidth}
                      onChange={(e) => setPrepTargetWidth(parseInt(e.target.value) || 2)}
                      className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs"
                    />
                  </div>
                  <div className="space-y-1.5">
                    <label className="text-slate-400 font-semibold">Target Height:</label>
                    <input
                      type="number"
                      value={prepTargetHeight}
                      onChange={(e) => setPrepTargetHeight(parseInt(e.target.value) || 2)}
                      className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs"
                    />
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1.5">
                    <label className="text-slate-400 font-semibold">Channel Order:</label>
                    <select
                      value={prepChannelOrder}
                      onChange={(e) => setPrepChannelOrder(e.target.value as any)}
                      className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs"
                    >
                      <option value="RGB">RGB</option>
                      <option value="BGR">BGR</option>
                      <option value="RGBA">RGBA</option>
                      <option value="GRAY">Grayscale (1 Channel)</option>
                    </select>
                  </div>

                  <div className="space-y-1.5">
                    <label className="text-slate-400 font-semibold">Tensor Layout:</label>
                    <select
                      value={prepLayout}
                      onChange={(e) => setPrepLayout(e.target.value as any)}
                      className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs"
                    >
                      <option value="NCHW">NCHW (Planar Batch)</option>
                      <option value="NHWC">NHWC (Interleaved Batch)</option>
                    </select>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1.5">
                    <label className="text-slate-400 font-semibold">Resize Filter:</label>
                    <select
                      value={prepResizeFilter}
                      onChange={(e) => setPrepResizeFilter(e.target.value as any)}
                      className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs"
                    >
                      <option value="BILINEAR">Bilinear</option>
                      <option value="NEAREST">Nearest Neighbor</option>
                      <option value="BICUBIC">Bicubic</option>
                    </select>
                  </div>

                  <div className="space-y-1.5">
                    <label className="text-slate-400 font-semibold">Normalization:</label>
                    <select
                      value={prepNormMode}
                      onChange={(e) => setPrepNormMode(e.target.value as any)}
                      className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs"
                    >
                      <option value="ZERO_TO_ONE">[0, 1]</option>
                      <option value="MINUS_ONE_TO_ONE">[-1, 1]</option>
                      <option value="IDENTITY">Identity (0..255)</option>
                      <option value="IMAGENET">ImageNet</option>
                    </select>
                  </div>
                </div>

                <div className="flex items-center gap-4 pt-1">
                  <label className="flex items-center gap-2 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={prepLetterbox}
                      onChange={(e) => setPrepLetterbox(e.target.checked)}
                      className="rounded bg-slate-950 border-slate-800 text-purple-600"
                    />
                    <span className="text-slate-300">Letterbox Padding</span>
                  </label>
                  <label className="flex items-center gap-2 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={prepCenterCrop}
                      onChange={(e) => setPrepCenterCrop(e.target.checked)}
                      className="rounded bg-slate-950 border-slate-800 text-purple-600"
                    />
                    <span className="text-slate-300">Center Crop</span>
                  </label>
                </div>

                {prepCenterCrop && (
                  <div className="grid grid-cols-2 gap-3">
                    <div className="space-y-1.5">
                      <label className="text-slate-400 font-semibold">Crop Width:</label>
                      <input
                        type="number"
                        value={prepCropWidth}
                        onChange={(e) => setPrepCropWidth(parseInt(e.target.value) || 2)}
                        className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs"
                      />
                    </div>
                    <div className="space-y-1.5">
                      <label className="text-slate-400 font-semibold">Crop Height:</label>
                      <input
                        type="number"
                        value={prepCropHeight}
                        onChange={(e) => setPrepCropHeight(parseInt(e.target.value) || 2)}
                        className="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 font-mono text-xs"
                      />
                    </div>
                  </div>
                )}

                <div className="flex items-center gap-2 pt-2">
                  <button
                    onClick={handlePreviewPreprocess}
                    disabled={isProcessing}
                    className="px-4 py-2 rounded-xl bg-purple-600 hover:bg-purple-500 text-white text-xs font-bold flex items-center gap-2 transition-all cursor-pointer disabled:opacity-50"
                  >
                    <Play className="w-3.5 h-3.5 fill-current" />
                    <span>Run Preprocessing Preview</span>
                  </button>

                  <button
                    onClick={handleValidatePreprocess}
                    disabled={isProcessing || !runtimeStatus?.loadedModelId}
                    className="px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 hover:border-purple-500/40 text-purple-300 text-xs font-semibold flex items-center gap-1.5 cursor-pointer disabled:opacity-50"
                  >
                    <ShieldCheck className="w-3.5 h-3.5" />
                    <span>Validate with Active Model</span>
                  </button>
                </div>
              </div>
            </div>
          </div>

          <div className="lg:col-span-6 space-y-5">
            {prepValidationResult && (
              <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3 font-mono text-xs">
                <span className="text-xs font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2 font-sans">
                  <ShieldCheck className="w-4 h-4 text-purple-400" />
                  <span>Model Input Signature Validation</span>
                </span>

                <div className="p-3 rounded-xl bg-slate-950 border border-slate-800/80 space-y-1 text-[11px]">
                  <div className="flex justify-between text-slate-400">
                    <span>Validation Status:</span>
                    <span className={`font-bold ${prepValidationResult.isValid ? 'text-emerald-400' : 'text-rose-400'}`}>
                      {prepValidationResult.isValid ? 'COMPATIBLE' : 'MISMATCH'}
                    </span>
                  </div>
                  <div className="flex justify-between text-slate-400">
                    <span>Produced Shape:</span>
                    <span className="text-purple-300">[{prepValidationResult.producedShape.join(', ')}]</span>
                  </div>
                </div>

                {prepValidationResult.errors.length > 0 && (
                  <div className="p-3 rounded-xl bg-rose-950/40 border border-rose-500/30 text-rose-300 text-[11px] space-y-1">
                    {prepValidationResult.errors.map((err, i) => (
                      <p key={i}>• {err}</p>
                    ))}
                  </div>
                )}
              </div>
            )}

            {prepPreviewResult && (
              <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3 font-mono text-xs">
                <span className="text-xs font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2 font-sans">
                  <Maximize2 className="w-4 h-4 text-emerald-400" />
                  <span>Preprocessing Output Metrics</span>
                </span>

                <div className="p-3 rounded-xl bg-slate-950 border border-slate-800/80 space-y-1 text-[11px]">
                  <div className="flex justify-between text-slate-400">
                    <span>Source Dimensions:</span>
                    <span className="text-slate-200">{prepPreviewResult.sourceWidth}x{prepPreviewResult.sourceHeight}</span>
                  </div>
                  <div className="flex justify-between text-slate-400">
                    <span>Processed Geometry:</span>
                    <span className="text-slate-200">{prepPreviewResult.processedWidth}x{prepPreviewResult.processedHeight}</span>
                  </div>
                  <div className="flex justify-between text-slate-400">
                    <span>Processed Tensor Shape:</span>
                    <span className="text-purple-300 font-bold">[{prepPreviewResult.tensor.shape.join(', ')}]</span>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* ------------------------------------------------------------- */}
      {/* TAB 4: INFERENCE CONSOLE (PHASE 6B) */}
      {/* ------------------------------------------------------------- */}
      {activeTab === 'CONSOLE' && (
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
          <div className="lg:col-span-8 space-y-5">
            <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4">
              <span className="text-xs font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2">
                <Play className="w-4 h-4 text-purple-400" />
                <span>Synchronous Test Inference</span>
              </span>

              <div className="space-y-3 text-xs">
                <div className="space-y-1.5">
                  <label className="text-slate-400 font-semibold">Input Vector (Float32 values for [1, 4] tensor):</label>
                  <input
                    type="text"
                    value={inferenceInputStr}
                    onChange={(e) => setInferenceInputStr(e.target.value)}
                    placeholder="1.0, 2.0, 3.0, 4.0"
                    className="w-full px-3.5 py-2 rounded-xl bg-slate-950 border border-slate-800 font-mono text-slate-200"
                  />
                </div>

                <div className="flex items-center gap-2">
                  <button
                    onClick={handleRunInference}
                    disabled={isProcessing || !runtimeStatus?.loadedModelId}
                    className="px-4 py-2 rounded-xl bg-purple-600 hover:bg-purple-500 text-white font-bold flex items-center gap-2 transition-all cursor-pointer disabled:opacity-50"
                  >
                    <Play className="w-3.5 h-3.5 fill-current" />
                    <span>Execute Native Inference</span>
                  </button>

                  {runtimeStatus?.loadedModelId && (
                    <button
                      onClick={handleUnloadModel}
                      disabled={isProcessing}
                      className="px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 hover:border-rose-500/40 text-slate-400 hover:text-rose-300 font-semibold flex items-center gap-1.5 cursor-pointer disabled:opacity-50"
                    >
                      <Square className="w-3.5 h-3.5" />
                      <span>Unload Session</span>
                    </button>
                  )}
                </div>
              </div>

              {inferenceResult && (
                <div className="p-4 rounded-xl bg-slate-950 border border-purple-500/30 space-y-2 font-mono text-xs">
                  <div className="flex items-center justify-between text-slate-400">
                    <span>Inference Duration:</span>
                    <span className="text-emerald-400 font-bold">{inferenceResult.inferenceDurationMs.toFixed(2)} ms</span>
                  </div>
                  {inferenceResult.outputs.map((out) => (
                    <div key={out.name} className="text-slate-300">
                      <span className="text-purple-300 font-bold">{out.name}:</span> [{out.dataF32?.join(', ')}]
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>

          <div className="lg:col-span-4 space-y-5">
            <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4">
              <div className="flex items-center justify-between">
                <span className="text-xs font-bold text-slate-300 uppercase tracking-wider block">
                  Provider Selection
                </span>
                <select
                  value={selectedProvider}
                  onChange={(e) => setSelectedProvider(e.target.value as any)}
                  className="px-2 py-1 rounded bg-slate-950 border border-slate-800 text-[10px] font-mono text-slate-300"
                >
                  <option value="AUTO">Auto Detect</option>
                  <option value="CPU">CPU</option>
                  <option value="DIRECT_ML">DirectML</option>
                  <option value="CUDA">CUDA</option>
                </select>
              </div>

              <span className="text-xs font-bold text-slate-300 uppercase tracking-wider block pt-2 border-t border-slate-800">
                Session Lifecycle
              </span>

              <div className="space-y-2 text-xs font-mono">
                {registeredModels.map((m) => (
                  <button
                    key={m.id}
                    onClick={() => handleLoadModel(m.id)}
                    disabled={isProcessing || runtimeStatus?.loadedModelId === m.id}
                    className="w-full p-3 rounded-xl bg-slate-950 border border-slate-800 hover:border-purple-500/40 text-left transition-all cursor-pointer disabled:opacity-50"
                  >
                    <div className="font-bold text-slate-200">{m.name}</div>
                    <div className="text-[10px] text-slate-500">{m.id}</div>
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* ------------------------------------------------------------- */}
      {/* TAB 5: HARDWARE & PROVIDERS */}
      {/* ------------------------------------------------------------- */}
      {activeTab === 'RUNTIME' && (
        <div className="space-y-6">
          <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-4">
            <span className="text-xs font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2">
              <Layers className="w-4 h-4 text-indigo-400" />
              <span>Detected Hardware Execution Providers</span>
            </span>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 font-mono text-xs">
              {providers.map((p) => (
                <div key={p.provider} className="p-4 rounded-xl border bg-slate-950/80 border-slate-800 space-y-2">
                  <div className="flex items-center justify-between">
                    <span className="font-bold text-white">{p.provider}</span>
                    <span className={`px-2 py-0.5 rounded text-[9px] font-bold ${p.available ? 'bg-emerald-500/10 text-emerald-300 border border-emerald-500/20' : 'bg-slate-800 text-slate-500'}`}>
                      {p.available ? 'AVAILABLE' : 'UNAVAILABLE'}
                    </span>
                  </div>
                  <p className="text-[11px] text-slate-400 font-sans">{p.reason || 'Hardware driver evaluation complete.'}</p>
                </div>
              ))}
            </div>
          </div>

          {deviceInfo && (
            <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3">
              <span className="text-xs font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2">
                <Cpu className="w-4 h-4 text-sky-400" />
                <span>Host Hardware Diagnostics</span>
              </span>

              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs font-mono">
                <div className="p-3 rounded-xl bg-slate-950 border border-slate-800/80">
                  <span className="text-[10px] text-slate-500 uppercase block">CPU Cores</span>
                  <span className="font-bold text-slate-200">{deviceInfo.cpuCores} Threads</span>
                </div>
                <div className="p-3 rounded-xl bg-slate-950 border border-slate-800/80">
                  <span className="text-[10px] text-slate-500 uppercase block">Primary GPU</span>
                  <span className="font-bold text-purple-300 truncate block">{deviceInfo.gpuName || 'System GPU'}</span>
                </div>
                <div className="p-3 rounded-xl bg-slate-950 border border-slate-800/80">
                  <span className="text-[10px] text-slate-500 uppercase block">System RAM</span>
                  <span className="font-bold text-slate-200">{formatBytes(deviceInfo.totalMemoryBytes)}</span>
                </div>
                <div className="p-3 rounded-xl bg-slate-950 border border-slate-800/80">
                  <span className="text-[10px] text-slate-500 uppercase block">DirectML Hardware</span>
                  <span className="font-bold text-emerald-400">{deviceInfo.isDirectmlSupported ? 'Supported' : 'Unsupported'}</span>
                </div>
              </div>
            </div>
          )}
          {resourceLimits && (
            <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800 space-y-3">
              <span className="text-xs font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2">
                <ShieldCheck className="w-4 h-4 text-emerald-400" />
                <span>Production Resource Limits & Memory Safeguards</span>
              </span>

              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs font-mono">
                <div className="p-3 rounded-xl bg-slate-950 border border-slate-800/80">
                  <span className="text-[10px] text-slate-500 uppercase block">Max Frame Size</span>
                  <span className="font-bold text-slate-200">{resourceLimits.maxFrameWidth} x {resourceLimits.maxFrameHeight}</span>
                </div>
                <div className="p-3 rounded-xl bg-slate-950 border border-slate-800/80">
                  <span className="text-[10px] text-slate-500 uppercase block">Max Tensor Elements</span>
                  <span className="font-bold text-indigo-300">{resourceLimits.maxTensorElements.toLocaleString()}</span>
                </div>
                <div className="p-3 rounded-xl bg-slate-950 border border-slate-800/80">
                  <span className="text-[10px] text-slate-500 uppercase block">In-Flight Concurrency</span>
                  <span className="font-bold text-slate-200">{resourceLimits.maxInflightFrames} frame / {resourceLimits.maxConcurrentInference} infer</span>
                </div>
                <div className="p-3 rounded-xl bg-slate-950 border border-slate-800/80">
                  <span className="text-[10px] text-slate-500 uppercase block">Job Disk Quota</span>
                  <span className="font-bold text-emerald-400">{formatBytes(resourceLimits.maxJobDiskBytes)}</span>
                </div>
                {runtimeResources && (
                  <div className="p-3 rounded-xl bg-slate-950 border border-slate-800/80 col-span-2 sm:col-span-4 flex items-center justify-between">
                    <span className="text-[11px] text-slate-400">Process Working Set Memory (Resident RAM)</span>
                    <span className="font-bold text-purple-300">{formatBytes(runtimeResources.processMemoryBytes)}</span>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
