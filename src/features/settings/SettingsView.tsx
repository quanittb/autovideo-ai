import React, { useState, useEffect } from 'react';
import { 
  Cpu, 
  Folder, 
  Sliders, 
  Lock, 
  Info, 
  Sparkles, 
  Zap, 
  CheckCircle2,
  Trash2,
  RotateCw,
  Key,
  ShieldCheck,
  AlertCircle
} from 'lucide-react';
import { useHardwareProfile } from '../../hooks/useHardwareProfile';
import { useAppInfo } from '../../hooks/useAppInfo';
import { api, flowApi, GeminiCredentialStatus } from '../../lib/ipc';
import { StorageUsageReport } from '../../types/contracts';

export const SettingsView: React.FC = () => {
  const { hardware, storage } = useHardwareProfile();
  const { appInfo } = useAppInfo();
  const [activeTab, setActiveTab] = useState<'general' | 'models' | 'gpu' | 'storage' | 'performance' | 'privacy' | 'about'>('gpu');
  const [storageReport, setStorageReport] = useState<StorageUsageReport | null>(null);
  const [isLoadingStorage, setIsLoadingStorage] = useState<boolean>(false);
  const [isClearingCache, setIsClearingCache] = useState<boolean>(false);
  const [isCleaningTemp, setIsCleaningTemp] = useState<boolean>(false);
  const [storageActionMessage, setStorageActionMessage] = useState<string | null>(null);

  const [geminiStatus, setGeminiStatus] = useState<GeminiCredentialStatus | null>(null);
  const [geminiKeyInput, setGeminiKeyInput] = useState<string>('');
  const [isSavingGeminiKey, setIsSavingGeminiKey] = useState<boolean>(false);
  const [isTestingGeminiKey, setIsTestingGeminiKey] = useState<boolean>(false);
  const [geminiMessage, setGeminiMessage] = useState<{ type: 'success' | 'error' | 'warning'; text: string } | null>(null);

  const fetchGeminiStatus = async () => {
    try {
      const status = await flowApi.getGeminiStatus();
      setGeminiStatus(status);
    } catch (err) {
      console.error('Failed to get Gemini status:', err);
    }
  };

  const handleSaveAndTestGeminiKey = async () => {
    if (!geminiKeyInput.trim()) return;
    setIsSavingGeminiKey(true);
    setGeminiMessage(null);
    try {
      await flowApi.setGeminiApiKey(geminiKeyInput.trim());
      setGeminiKeyInput('');
      const testRes = await flowApi.testGeminiApiKey();
      setGeminiStatus(testRes);
      if (testRes.verificationStatus === 'VALID') {
        setGeminiMessage({ type: 'success', text: 'Gemini API Key securely stored and verified successfully.' });
      } else {
        setGeminiMessage({
          type: 'warning',
          text: `Stored securely in OS Keychain, but verification status: ${testRes.verificationStatus}${testRes.sanitizedMessage ? ` — ${testRes.sanitizedMessage}` : ''}`,
        });
      }
    } catch (err: any) {
      setGeminiMessage({ type: 'error', text: typeof err === 'string' ? err : err?.message || 'Failed to save Gemini key' });
    } finally {
      setIsSavingGeminiKey(false);
    }
  };

  const handleTestAgain = async () => {
    setIsTestingGeminiKey(true);
    setGeminiMessage(null);
    try {
      const testRes = await flowApi.testGeminiApiKey();
      setGeminiStatus(testRes);
      if (testRes.verificationStatus === 'VALID') {
        setGeminiMessage({ type: 'success', text: `API Access Verified (${testRes.model || 'gemini-3.5-flash-lite'}).` });
      } else {
        setGeminiMessage({
          type: 'warning',
          text: `Verification status: ${testRes.verificationStatus}${testRes.sanitizedMessage ? ` — ${testRes.sanitizedMessage}` : ''}`,
        });
      }
    } catch (err: any) {
      setGeminiMessage({ type: 'error', text: typeof err === 'string' ? err : err?.message || 'Failed to test Gemini key' });
    } finally {
      setIsTestingGeminiKey(false);
    }
  };

  const handleClearGeminiKey = async () => {
    setIsSavingGeminiKey(true);
    setGeminiMessage(null);
    try {
      await flowApi.clearGeminiApiKey();
      setGeminiMessage({ type: 'success', text: 'Gemini API Key removed from secure storage.' });
      await fetchGeminiStatus();
    } catch (err: any) {
      setGeminiMessage({ type: 'error', text: typeof err === 'string' ? err : err?.message || 'Failed to clear Gemini key' });
    } finally {
      setIsSavingGeminiKey(false);
    }
  };

  const fetchStorageUsage = async () => {
    setIsLoadingStorage(true);
    try {
      const rep = await api.getStorageUsage();
      setStorageReport(rep);
    } catch (err) {
      console.error('Failed to get storage usage:', err);
    } finally {
      setIsLoadingStorage(false);
    }
  };

  useEffect(() => {
    if (activeTab === 'storage') {
      fetchStorageUsage();
    } else if (activeTab === 'models') {
      fetchGeminiStatus();
    }
  }, [activeTab]);

  const handleClearCache = async () => {
    setIsClearingCache(true);
    setStorageActionMessage(null);
    try {
      const freed = await api.clearStorageCache();
      setStorageActionMessage(`Cleared ${formatBytes(freed)} of cache data successfully.`);
      await fetchStorageUsage();
    } catch (err) {
      console.error('Failed to clear cache:', err);
    } finally {
      setIsClearingCache(false);
    }
  };

  const handleCleanupTemp = async () => {
    setIsCleaningTemp(true);
    setStorageActionMessage(null);
    try {
      const freed = await api.cleanupTempStorage();
      setStorageActionMessage(`Cleaned ${formatBytes(freed)} of temporary workspace files.`);
      await fetchStorageUsage();
    } catch (err) {
      console.error('Failed to clean temp files:', err);
    } finally {
      setIsCleaningTemp(false);
    }
  };

  const formatBytes = (bytes: number): string => {
    if (!bytes || bytes <= 0) return '0 B';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  };

  const tabs: { id: typeof activeTab; label: string; icon: React.ReactNode }[] = [
    { id: 'general', label: 'General', icon: <Sliders className="w-4 h-4" /> },
    { id: 'models', label: 'AI Models', icon: <Sparkles className="w-4 h-4" /> },
    { id: 'gpu', label: 'GPU / Runtime', icon: <Cpu className="w-4 h-4" /> },
    { id: 'storage', label: 'Storage', icon: <Folder className="w-4 h-4" /> },
    { id: 'performance', label: 'Performance', icon: <Zap className="w-4 h-4" /> },
    { id: 'privacy', label: 'Privacy', icon: <Lock className="w-4 h-4" /> },
    { id: 'about', label: 'About', icon: <Info className="w-4 h-4" /> },
  ];

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      <div>
        <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Application Settings</h2>
        <p className="text-sm text-slate-400 mt-1">Configure hardware acceleration, local storage, and AI runtimes</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
        {/* Left Settings Sidebar (3 cols) */}
        <div className="lg:col-span-3 bg-slate-900/60 border border-slate-800/80 rounded-2xl p-2 space-y-1">
          {tabs.map((tab) => {
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-semibold transition-all text-left ${
                  isActive
                    ? 'bg-indigo-600 text-white shadow-md shadow-indigo-900/30'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900'
                }`}
              >
                {tab.icon}
                <span>{tab.label}</span>
              </button>
            );
          })}
        </div>

        {/* Right Settings Content (9 cols) */}
        <div className="lg:col-span-9 bg-slate-900/60 border border-slate-800/80 rounded-2xl p-6 space-y-6">
          {/* Tab 1: General */}
          {activeTab === 'general' && (
            <div className="space-y-5">
              <h3 className="text-base font-bold text-slate-200">General Preferences</h3>
              <div className="space-y-4 text-xs">
                <div className="flex items-center justify-between p-3.5 rounded-xl bg-slate-950 border border-slate-800">
                  <div>
                    <span className="font-semibold text-slate-200 block">Auto-save transformation sessions</span>
                    <span className="text-slate-500">Automatically cache intermediate plans and keyframe indices</span>
                  </div>
                  <input type="checkbox" defaultChecked className="w-4 h-4 accent-indigo-600 cursor-pointer" />
                </div>

                <div className="flex items-center justify-between p-3.5 rounded-xl bg-slate-950 border border-slate-800">
                  <div>
                    <span className="font-semibold text-slate-200 block">Default Export Quality</span>
                    <span className="text-slate-500">Preset quality for newly initiated projects</span>
                  </div>
                  <select className="p-2 rounded-lg bg-slate-900 border border-slate-700 text-slate-200 text-xs">
                    <option>High Quality (1080p)</option>
                    <option>Standard</option>
                    <option>4K Ultra HD</option>
                  </select>
                </div>
              </div>
            </div>
          )}

          {/* Tab 2: AI Models */}
          {activeTab === 'models' && (
            <div className="space-y-6">
              <div className="space-y-4">
                <h3 className="text-base font-bold text-slate-200">AI Model Directory & Provider</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  AutoVideo AI maintains local neural model weights on disk. You can configure download servers and verify checksum integrity.
                </p>
                <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-2 text-xs">
                  <span className="text-slate-400 block font-semibold">Model Provider Strategy:</span>
                  <div className="flex items-center gap-2">
                    <span className="px-2.5 py-1 rounded-lg bg-indigo-950/60 border border-indigo-500/40 text-indigo-300 font-mono">
                      Local-First ONNX / DirectML Provider
                    </span>
                    <span className="px-2.5 py-1 rounded-lg bg-slate-900 border border-slate-800 text-slate-500 font-mono">
                      Future Cloud Adapter (Ready)
                    </span>
                  </div>
                </div>
              </div>

              {/* Gemini API Key Configuration Card */}
              <div className="p-5 rounded-2xl bg-slate-950 border border-slate-800 space-y-4">
                <div className="flex items-center justify-between flex-wrap gap-2">
                  <div className="flex items-center gap-2">
                    <Key className="w-4 h-4 text-indigo-400" />
                    <h4 className="text-sm font-bold text-slate-200">Google Gemini API Key (Flow Prompt Refinement)</h4>
                  </div>
                  <div className="flex items-center gap-2 flex-wrap">
                    <span
                      className={`px-2.5 py-0.5 rounded-full text-xs font-semibold border flex items-center gap-1 ${
                        geminiStatus?.isConfigured
                          ? 'bg-emerald-950/80 border-emerald-500/30 text-emerald-400'
                          : 'bg-slate-900 border-slate-700 text-slate-400'
                      }`}
                    >
                      {geminiStatus?.isConfigured ? <ShieldCheck className="w-3 h-3" /> : <AlertCircle className="w-3 h-3" />}
                      Source:{' '}
                      {geminiStatus?.source === 'USER_OVERRIDE'
                        ? 'Custom Key (OS Keychain)'
                        : geminiStatus?.source === 'ENVIRONMENT'
                        ? 'Environment Variable'
                        : geminiStatus?.source === 'APPLICATION_DEFAULT'
                        ? 'Application Default'
                        : 'Not Configured (Optional)'}
                    </span>
                    {geminiStatus?.isConfigured && (
                      <span
                        className={`px-2.5 py-0.5 rounded-full text-xs font-semibold border flex items-center gap-1 ${
                          geminiStatus.verificationStatus === 'VALID'
                            ? 'bg-emerald-950/80 border-emerald-500/30 text-emerald-400'
                            : geminiStatus.verificationStatus === 'UNVERIFIED'
                            ? 'bg-slate-900 border-slate-700 text-slate-400'
                            : 'bg-rose-950/80 border-rose-500/30 text-rose-400'
                        }`}
                      >
                        API Access: {geminiStatus.verificationStatus}
                      </span>
                    )}
                  </div>
                </div>

                <p className="text-xs text-slate-400 leading-relaxed">
                  Gemini API key is used exclusively for <strong>user-initiated prompt optimization</strong> in Google Flow ({geminiStatus?.model || 'gemini-3.5-flash-lite'}). Keys are securely stored in the <strong>OS Credential Manager / Keychain</strong> and are never written to disk or sent to logs.
                </p>

                {geminiMessage && (
                  <div
                    className={`p-3 rounded-xl text-xs flex items-center gap-2 ${
                      geminiMessage.type === 'success'
                        ? 'bg-emerald-500/10 border border-emerald-500/20 text-emerald-300'
                        : geminiMessage.type === 'warning'
                        ? 'bg-amber-500/10 border border-amber-500/20 text-amber-300'
                        : 'bg-red-500/10 border border-red-500/20 text-red-300'
                    }`}
                  >
                    <Info className="w-4 h-4 shrink-0" />
                    <span>{geminiMessage.text}</span>
                  </div>
                )}

                <div className="flex flex-col sm:flex-row items-stretch sm:items-center gap-3">
                  <input
                    type="password"
                    value={geminiKeyInput}
                    onChange={(e) => setGeminiKeyInput(e.target.value)}
                    placeholder="Enter Gemini API Key (e.g. AIzaSy...)"
                    className="flex-1 px-3 py-2 text-xs font-mono text-slate-100 bg-slate-900 border border-slate-700 rounded-xl focus:outline-none focus:border-indigo-500 transition"
                  />
                  <div className="flex items-center gap-2 flex-wrap">
                    <button
                      type="button"
                      onClick={handleSaveAndTestGeminiKey}
                      disabled={isSavingGeminiKey || isTestingGeminiKey || !geminiKeyInput.trim()}
                      className="px-4 py-2 text-xs font-semibold text-white bg-indigo-600 hover:bg-indigo-500 disabled:opacity-40 disabled:cursor-not-allowed rounded-xl shadow transition cursor-pointer"
                    >
                      {isSavingGeminiKey ? 'Saving & Testing...' : 'Save & Test API Key'}
                    </button>
                    {geminiStatus?.stored && (
                      <button
                        type="button"
                        onClick={handleTestAgain}
                        disabled={isSavingGeminiKey || isTestingGeminiKey}
                        className="px-3 py-2 text-xs font-semibold text-indigo-300 hover:text-white bg-indigo-950/60 hover:bg-indigo-900 border border-indigo-700/50 rounded-xl transition cursor-pointer disabled:opacity-40"
                      >
                        {isTestingGeminiKey ? 'Testing...' : 'Test Again'}
                      </button>
                    )}
                    {geminiStatus?.stored && (
                      <button
                        type="button"
                        onClick={handleClearGeminiKey}
                        disabled={isSavingGeminiKey || isTestingGeminiKey}
                        className="px-3 py-2 text-xs font-semibold text-red-400 hover:text-red-300 bg-red-950/40 hover:bg-red-900/40 border border-red-800/40 rounded-xl transition cursor-pointer disabled:opacity-40"
                      >
                        Remove Key
                      </button>
                    )}
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Tab 3: GPU / Runtime */}
          {activeTab === 'gpu' && (
            <div className="space-y-5">
              <h3 className="text-base font-bold text-slate-200">Hardware Acceleration & Compute Runtime</h3>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs">
                <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-1">
                  <span className="text-slate-500 block">Primary GPU Device:</span>
                  <span className="font-mono font-semibold text-slate-200 text-sm">{hardware?.gpuName || 'DirectX 12 Primary GPU'}</span>
                </div>
                <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-1">
                  <span className="text-slate-500 block">Operating System:</span>
                  <span className="font-mono font-semibold text-slate-200 text-sm uppercase">{hardware?.os} ({hardware?.arch})</span>
                </div>
                <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-1">
                  <span className="text-slate-500 block">DirectML Hardware Support:</span>
                  <span className={`font-bold ${hardware?.isDirectmlSupported ? 'text-emerald-400' : 'text-slate-500'}`}>
                    {hardware?.isDirectmlSupported ? 'Active & Supported' : 'Unavailable'}
                  </span>
                </div>
                <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-1">
                  <span className="text-slate-500 block">Available Logical CPU Threads:</span>
                  <span className="font-mono font-semibold text-slate-200 text-sm">{hardware?.cpuCores || 8} Threads</span>
                </div>
              </div>
            </div>
          )}

          {/* Tab 4: Storage */}
          {activeTab === 'storage' && (
            <div className="space-y-6">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="text-base font-bold text-slate-200">Storage & Cache Management</h3>
                  <p className="text-xs text-slate-400 mt-0.5">
                    Monitor disk space consumed by project files, AI artifact caches, model weights, and temporary frames.
                  </p>
                </div>
                <button
                  type="button"
                  onClick={fetchStorageUsage}
                  disabled={isLoadingStorage}
                  className="px-3 py-1.5 rounded-xl bg-slate-900 hover:bg-slate-800 border border-slate-700 text-slate-300 text-xs font-semibold flex items-center gap-1.5 transition-colors cursor-pointer"
                >
                  <RotateCw className={`w-3.5 h-3.5 text-indigo-400 ${isLoadingStorage ? 'animate-spin' : ''}`} />
                  <span>Refresh</span>
                </button>
              </div>

              {storageActionMessage && (
                <div className="p-3 rounded-xl bg-emerald-500/10 border border-emerald-500/20 text-xs text-emerald-300 flex items-center gap-2">
                  <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                  <span>{storageActionMessage}</span>
                </div>
              )}

              {/* Disk Usage Overview Card */}
              <div className="p-5 rounded-2xl bg-slate-950 border border-slate-800 space-y-4">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-bold text-slate-300 uppercase tracking-wider">
                    Total Storage Allocated
                  </span>
                  <span className="text-sm font-bold font-mono text-indigo-300">
                    {formatBytes(storageReport?.totalBytes || 0)}
                  </span>
                </div>

                {/* Storage Distribution Breakdown */}
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs">
                  <div className="p-3 rounded-xl bg-slate-900/80 border border-slate-800 space-y-1">
                    <span className="text-[10px] text-slate-500 block uppercase font-semibold">Projects</span>
                    <span className="font-mono font-bold text-slate-200">{formatBytes(storageReport?.projectsBytes || 0)}</span>
                  </div>
                  <div className="p-3 rounded-xl bg-slate-900/80 border border-slate-800 space-y-1">
                    <span className="text-[10px] text-slate-500 block uppercase font-semibold">Media / AI Cache</span>
                    <span className="font-mono font-bold text-purple-300">{formatBytes((storageReport?.cacheBytes || 0) + (storageReport?.aiCacheBytes || 0))}</span>
                  </div>
                  <div className="p-3 rounded-xl bg-slate-900/80 border border-slate-800 space-y-1">
                    <span className="text-[10px] text-slate-500 block uppercase font-semibold">Model Packages</span>
                    <span className="font-mono font-bold text-emerald-300">{formatBytes(storageReport?.modelsBytes || 0)}</span>
                  </div>
                  <div className="p-3 rounded-xl bg-slate-900/80 border border-slate-800 space-y-1">
                    <span className="text-[10px] text-slate-500 block uppercase font-semibold">Temp Workspaces</span>
                    <span className="font-mono font-bold text-amber-300">{formatBytes(storageReport?.tempBytes || 0)}</span>
                  </div>
                </div>

                {/* Cleanup Actions */}
                <div className="pt-2 flex flex-wrap items-center gap-3 border-t border-slate-800/80">
                  <button
                    type="button"
                    onClick={handleClearCache}
                    disabled={isClearingCache}
                    className="px-4 py-2 rounded-xl bg-indigo-600/20 hover:bg-indigo-600/30 border border-indigo-500/40 text-indigo-300 text-xs font-semibold flex items-center gap-2 transition-all cursor-pointer"
                  >
                    <Trash2 className="w-3.5 h-3.5 text-indigo-400" />
                    <span>{isClearingCache ? 'Clearing Cache...' : 'Clear Media Cache'}</span>
                  </button>

                  <button
                    type="button"
                    onClick={handleCleanupTemp}
                    disabled={isCleaningTemp}
                    className="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 border border-slate-700 text-slate-300 text-xs font-semibold flex items-center gap-2 transition-all cursor-pointer"
                  >
                    <Folder className="w-3.5 h-3.5 text-amber-400" />
                    <span>{isCleaningTemp ? 'Cleaning Temp...' : 'Clean Temporary Files'}</span>
                  </button>
                </div>
              </div>

              {/* Directory Paths Reference */}
              <div className="space-y-3 text-xs">
                <span className="text-slate-400 block font-semibold">Local Storage Paths:</span>
                <div>
                  <span className="text-slate-500 block mb-1">Projects Directory:</span>
                  <input
                    type="text"
                    readOnly
                    value={storage?.projectsDir || './.autovideo_data/projects'}
                    className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-300"
                  />
                </div>

                <div>
                  <span className="text-slate-500 block mb-1">Model Weights Directory:</span>
                  <input
                    type="text"
                    readOnly
                    value={storage?.modelsDir || './.autovideo_data/models'}
                    className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-300"
                  />
                </div>

                <div>
                  <span className="text-slate-500 block mb-1">Temporary Buffer Directory:</span>
                  <input
                    type="text"
                    readOnly
                    value={storage?.tempDir || './.autovideo_data/temp'}
                    className="w-full p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-300"
                  />
                </div>
              </div>
            </div>
          )}

          {/* Tab 5: Performance */}
          {activeTab === 'performance' && (
            <div className="space-y-4">
              <h3 className="text-base font-bold text-slate-200">Performance & VRAM Management</h3>
              <div className="space-y-3 text-xs">
                <div className="flex items-center justify-between p-3.5 rounded-xl bg-slate-950 border border-slate-800">
                  <div>
                    <span className="font-semibold text-slate-200 block">VRAM Auto-Unload</span>
                    <span className="text-slate-500">Unload diffusion weights when idle for 5 minutes</span>
                  </div>
                  <input type="checkbox" defaultChecked className="w-4 h-4 accent-indigo-600 cursor-pointer" />
                </div>
                <div className="flex items-center justify-between p-3.5 rounded-xl bg-slate-950 border border-slate-800">
                  <div>
                    <span className="font-semibold text-slate-200 block">FFmpeg Hardware Decoding</span>
                    <span className="text-slate-500">Use D3D11VA / VideoToolbox acceleration</span>
                  </div>
                  <input type="checkbox" defaultChecked className="w-4 h-4 accent-indigo-600 cursor-pointer" />
                </div>
              </div>
            </div>
          )}

          {/* Tab 6: Privacy */}
          {activeTab === 'privacy' && (
            <div className="space-y-4">
              <h3 className="text-base font-bold text-slate-200">Privacy & Security</h3>
              <p className="text-xs text-slate-400 leading-relaxed">
                AutoVideo AI is built local-first. Video frames, prompts, and audio streams never leave your device unless you explicitly enable a Cloud Rendering Provider.
              </p>
              <div className="p-4 rounded-xl bg-emerald-500/10 border border-emerald-500/20 text-xs text-emerald-300 flex items-center gap-2.5">
                <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                <span>100% Local Inference & Media Processing Enabled</span>
              </div>
            </div>
          )}

          {/* Tab 7: About */}
          {activeTab === 'about' && (
            <div className="space-y-4">
              <h3 className="text-base font-bold text-slate-200">About AutoVideo AI</h3>
              <div className="space-y-2 text-xs text-slate-400">
                <p>
                  <strong>AutoVideo AI</strong> — AI-powered desktop video transformation studio.
                </p>
                <div className="p-3.5 rounded-xl bg-slate-950 border border-slate-800 font-mono space-y-1">
                  <div>App Version: <span className="text-slate-200 font-bold">{appInfo?.version || '0.1.0'}</span></div>
                  <div>Build Environment: <span className="text-slate-200">{appInfo?.environment || 'development'}</span></div>
                  <div>Architecture: <span className="text-indigo-400 font-bold">Phase 2 Desktop UI Foundation</span></div>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
