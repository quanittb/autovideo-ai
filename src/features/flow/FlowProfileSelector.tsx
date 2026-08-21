import React, { useState } from 'react';
import {
  User,
  ShieldCheck,
  AlertCircle,
  Plus,
  Lock,
  ExternalLink,
  RefreshCw,
  XSquare,
  Info,
} from 'lucide-react';
import { FlowProfileSnapshot } from '../../lib/ipc';
import { useFlowJobStore } from '../../stores/flowJobStore';

interface FlowProfileSelectorProps {
  profiles: FlowProfileSnapshot[];
  selectedProfileId: string | null;
  isLoading: boolean;
  onSelectProfile: (profileId: string) => void;
  onCreateProfile: (profileId: string, name: string) => Promise<void>;
  onRefreshProfiles?: () => void;
}

export const FlowProfileSelector: React.FC<FlowProfileSelectorProps> = ({
  profiles,
  selectedProfileId,
  isLoading,
  onSelectProfile,
  onCreateProfile,
  onRefreshProfiles,
}) => {
  const [showCreate, setShowCreate] = useState(false);
  const [newProfileId, setNewProfileId] = useState('');
  const [newProfileName, setNewProfileName] = useState('');
  const [isOpeningBrowser, setIsOpeningBrowser] = useState(false);
  const [isClosingBrowser, setIsClosingBrowser] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [actionFeedback, setActionFeedback] = useState<string | null>(null);

  const { openProfileBrowser, closeProfileBrowser, refreshProfileStatus } =
    useFlowJobStore();

  const selected = profiles.find((p) => p.profileId === selectedProfileId);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newProfileId.trim()) return;
    await onCreateProfile(
      newProfileId.trim(),
      newProfileName.trim() || newProfileId.trim()
    );
    setNewProfileId('');
    setNewProfileName('');
    setShowCreate(false);
  };

  const handleOpenBrowser = async () => {
    if (!selectedProfileId) return;
    setIsOpeningBrowser(true);
    setActionFeedback(null);
    try {
      await openProfileBrowser(selectedProfileId);
      setActionFeedback('Browser launched for manual login.');
    } catch (err: any) {
      setActionFeedback(`Failed: ${err?.message || String(err)}`);
    } finally {
      setIsOpeningBrowser(false);
    }
  };

  const handleCloseBrowser = async () => {
    if (!selectedProfileId) return;
    setIsClosingBrowser(true);
    setActionFeedback(null);
    try {
      await closeProfileBrowser(selectedProfileId);
      setActionFeedback('Browser closed. Profile lock released.');
    } catch (err: any) {
      setActionFeedback(`Close failed: ${err?.message || String(err)}`);
    } finally {
      setIsClosingBrowser(false);
    }
  };

  const handleRefreshStatus = async () => {
    if (!selectedProfileId) return;
    setIsRefreshing(true);
    setActionFeedback(null);
    try {
      const status = await refreshProfileStatus(selectedProfileId);
      setActionFeedback(`Profile Status: ${status}`);
      if (onRefreshProfiles) onRefreshProfiles();
    } catch (err: any) {
      setActionFeedback(`Check failed: ${err?.message || String(err)}`);
    } finally {
      setIsRefreshing(false);
    }
  };

  return (
    <div className="flex flex-col gap-2 p-4 bg-slate-900/60 border border-slate-800 rounded-xl">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <User className="w-4 h-4 text-indigo-400" />
          <span className="text-sm font-semibold text-slate-200">
            Google Flow Profile
          </span>
        </div>

        <button
          type="button"
          onClick={() => setShowCreate(!showCreate)}
          className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium text-indigo-300 hover:text-white bg-indigo-950/60 hover:bg-indigo-900 border border-indigo-700/50 rounded-lg transition cursor-pointer"
        >
          <Plus className="w-3 h-3" />
          New Profile
        </button>
      </div>

      {showCreate ? (
        <form
          onSubmit={handleCreate}
          className="flex flex-col gap-2 p-3 bg-slate-950/70 border border-indigo-500/30 rounded-lg"
        >
          <span className="text-xs font-medium text-slate-300">
            Create Isolated Flow Profile
          </span>
          <div className="grid grid-cols-2 gap-2">
            <input
              type="text"
              placeholder="Profile ID (e.g. account_1)"
              value={newProfileId}
              onChange={(e) => setNewProfileId(e.target.value)}
              className="px-2.5 py-1.5 text-xs text-slate-100 bg-slate-900 border border-slate-700 rounded focus:outline-none focus:border-indigo-500"
            />
            <input
              type="text"
              placeholder="Display Name"
              value={newProfileName}
              onChange={(e) => setNewProfileName(e.target.value)}
              className="px-2.5 py-1.5 text-xs text-slate-100 bg-slate-900 border border-slate-700 rounded focus:outline-none focus:border-indigo-500"
            />
          </div>
          <div className="flex justify-end gap-2 mt-1">
            <button
              type="button"
              onClick={() => setShowCreate(false)}
              className="px-2.5 py-1 text-xs text-slate-400 hover:text-slate-200"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!newProfileId.trim()}
              className="px-3 py-1 text-xs font-medium text-white bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 rounded cursor-pointer"
            >
              Save Profile
            </button>
          </div>
        </form>
      ) : (
        <div className="flex flex-col sm:flex-row items-stretch sm:items-center gap-3">
          <select
            value={selectedProfileId || ''}
            onChange={(e) => onSelectProfile(e.target.value)}
            disabled={isLoading || profiles.length === 0}
            className="flex-1 px-3 py-2 text-sm text-slate-200 bg-slate-950/70 border border-slate-700 rounded-lg focus:outline-none focus:border-indigo-500"
          >
            {profiles.length === 0 ? (
              <option value="">No profiles found (create one to begin)</option>
            ) : (
              profiles.map((p) => (
                <option key={p.profileId} value={p.profileId}>
                  {p.name} ({p.profileId})
                </option>
              ))
            )}
          </select>

          {selected && (
            <div className="flex items-center gap-2 flex-wrap">
              {selected.browserSessionOpen ? (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-indigo-950/80 border border-indigo-500/50 text-indigo-300 rounded-lg">
                  <ExternalLink className="w-3.5 h-3.5" />
                  Browser Open
                </span>
              ) : selected.isLocked ? (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-amber-950/60 border border-amber-600/40 text-amber-300 rounded-lg">
                  <Lock className="w-3.5 h-3.5" />
                  In Use
                </span>
              ) : selected.status === 'READY' ? (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-emerald-950/60 border border-emerald-600/40 text-emerald-300 rounded-lg">
                  <ShieldCheck className="w-3.5 h-3.5" />
                  Ready
                </span>
              ) : selected.status === 'LOGIN_REQUIRED' ? (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-slate-800 border border-slate-700 text-amber-300 rounded-lg">
                  <AlertCircle className="w-3.5 h-3.5 text-amber-400" />
                  Login Required
                </span>
              ) : (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-slate-800 border border-slate-700 text-slate-400 rounded-lg">
                  <AlertCircle className="w-3.5 h-3.5 text-slate-400" />
                  Unverified
                </span>
              )}

              {selected.browserSessionOpen ? (
                <button
                  type="button"
                  onClick={handleCloseBrowser}
                  disabled={isClosingBrowser}
                  className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium text-rose-300 hover:text-white bg-rose-950/60 hover:bg-rose-900 border border-rose-700/50 rounded-lg transition disabled:opacity-50 cursor-pointer"
                  title="Close login Chromium session and release profile lock"
                >
                  <XSquare className="w-3.5 h-3.5" />
                  {isClosingBrowser ? 'Closing...' : 'Close Browser'}
                </button>
              ) : (
                <button
                  type="button"
                  onClick={handleOpenBrowser}
                  disabled={isOpeningBrowser || selected.isLocked}
                  className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium text-indigo-300 hover:text-white bg-indigo-950/40 hover:bg-indigo-900/60 border border-indigo-700/40 rounded-lg transition disabled:opacity-50 cursor-pointer"
                  title="Launch headed Chromium browser to log into Google Flow manually"
                >
                  <ExternalLink className="w-3.5 h-3.5" />
                  {isOpeningBrowser ? 'Opening...' : 'Open Browser / Login'}
                </button>
              )}

              <button
                type="button"
                onClick={handleRefreshStatus}
                disabled={isRefreshing || (selected.isLocked && !selected.browserSessionOpen)}
                className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 border border-slate-700 rounded-lg transition disabled:opacity-50 cursor-pointer"
                title="Check auth readiness against Google Flow"
              >
                <RefreshCw
                  className={`w-3.5 h-3.5 ${isRefreshing ? 'animate-spin' : ''}`}
                />
                Refresh Status
              </button>
            </div>
          )}
        </div>
      )}

      {selected?.browserSessionOpen && (
        <div className="flex items-center gap-2 p-2.5 bg-indigo-950/50 border border-indigo-700/40 rounded-lg text-xs text-indigo-200">
          <Info className="w-4 h-4 text-indigo-400 shrink-0" />
          {selected.status === 'READY' ? (
            <span>
              Google Flow login verified. <strong>Close the login browser</strong> before starting generation.
            </span>
          ) : (
            <span>
              Browser Open — complete Google login in the browser window, then click <strong>Refresh Status</strong>.
            </span>
          )}
        </div>
      )}

      {actionFeedback && (
        <span className="text-[11px] text-indigo-300 bg-indigo-950/40 px-2 py-1 rounded border border-indigo-800/40">
          {actionFeedback}
        </span>
      )}

      <span className="text-[11px] text-slate-500">
        Each profile maintains separate browser session cookies and isolated Chrome storage.
      </span>
    </div>
  );
};
