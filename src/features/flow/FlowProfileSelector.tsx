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
    const id = newProfileId.trim();
    if (!id) return;
    if (profiles.some((p) => p.profileId.toLowerCase() === id.toLowerCase())) {
      setActionFeedback(`Profile ID "${id}" already exists. Please choose a different ID.`);
      return;
    }
    try {
      await onCreateProfile(
        id,
        newProfileName.trim() || id
      );
      setNewProfileId('');
      setNewProfileName('');
      setShowCreate(false);
      setActionFeedback(`✓ Profile "${id}" created and selected.`);
      if (onRefreshProfiles) onRefreshProfiles();
    } catch (err: any) {
      setActionFeedback(`Failed to create profile: ${err?.message || String(err)}`);
    }
  };

  const handleOpenCreateForm = () => {
    if (!showCreate) {
      let nextIndex = profiles.length + 1;
      let defaultId = `profile_${nextIndex}`;
      while (profiles.some((p) => p.profileId === defaultId)) {
        nextIndex++;
        defaultId = `profile_${nextIndex}`;
      }
      setNewProfileId(defaultId);
      setNewProfileName(`Flow Account ${nextIndex}`);
    }
    setShowCreate(!showCreate);
  };

  const isBrowserOpen = selected?.manualBrowserOpen || selected?.browserSessionOpen || false;

  const handleOpenBrowser = async () => {
    if (!selectedProfileId) return;
    setIsOpeningBrowser(true);
    setActionFeedback(null);
    try {
      await openProfileBrowser(selectedProfileId);
      setActionFeedback('Chrome is open for manual Google sign-in.');
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
      setActionFeedback('Chrome closed. Profile lock released.');
      if (onRefreshProfiles) onRefreshProfiles();
    } catch (err: any) {
      setActionFeedback(`Close failed: ${err?.message || String(err)}`);
    } finally {
      setIsClosingBrowser(false);
    }
  };

  const handleVerifyLogin = async () => {
    if (!selectedProfileId) return;
    setIsRefreshing(true);
    setActionFeedback(null);
    try {
      const status = await refreshProfileStatus(selectedProfileId);
      if (status === 'READY') {
        setActionFeedback('✓ Google Flow session verified (READY)');
      } else if (status === 'LOGIN_REQUIRED') {
        setActionFeedback('Google sign-in is required. Open Chrome for Login.');
      } else if (status === 'FLOW_UI_CHANGED') {
        setActionFeedback(
          "Google session may be valid, but Flow's interface could not be recognized."
        );
      } else if (
        status === 'FLOW_ELIGIBILITY_REQUIRED' ||
        status === 'ELIGIBILITY_REQUIRED' ||
        status === 'USER_ACTION_REQUIRED'
      ) {
        setActionFeedback(
          'Google Flow requires account eligibility or account action. Complete the required action manually in Chrome.'
        );
      } else {
        setActionFeedback(`Verification result: ${status}`);
      }
      if (onRefreshProfiles) onRefreshProfiles();
    } catch (err: any) {
      setActionFeedback(`Verification failed: ${err?.message || String(err)}`);
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
          onClick={handleOpenCreateForm}
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
              {isBrowserOpen ? (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-indigo-950/80 border border-indigo-500/50 text-indigo-300 rounded-lg">
                  <ExternalLink className="w-3.5 h-3.5" />
                  Chrome Open (Manual Login)
                </span>
              ) : selected.isLocked ? (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-amber-950/60 border border-amber-600/40 text-amber-300 rounded-lg">
                  <Lock className="w-3.5 h-3.5" />
                  In Use
                </span>
              ) : selected.status === 'READY' ? (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-emerald-950/60 border border-emerald-600/40 text-emerald-300 rounded-lg">
                  <ShieldCheck className="w-3.5 h-3.5" />
                  Verified Ready
                </span>
              ) : selected.status === 'LOGIN_REQUIRED' ? (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-slate-800 border border-slate-700 text-amber-300 rounded-lg">
                  <AlertCircle className="w-3.5 h-3.5 text-amber-400" />
                  Login Required
                </span>
              ) : selected.status === 'FLOW_UI_CHANGED' ? (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-purple-950/60 border border-purple-600/40 text-purple-300 rounded-lg">
                  <AlertCircle className="w-3.5 h-3.5 text-purple-400" />
                  Flow UI Changed
                </span>
              ) : selected.status === 'FLOW_ELIGIBILITY_REQUIRED' ||
                selected.status === 'ELIGIBILITY_REQUIRED' ||
                selected.status === 'USER_ACTION_REQUIRED' ? (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-amber-950/60 border border-amber-600/40 text-amber-300 rounded-lg">
                  <AlertCircle className="w-3.5 h-3.5 text-amber-400" />
                  Action Required
                </span>
              ) : (
                <span className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium bg-slate-800 border border-slate-700 text-slate-400 rounded-lg">
                  <AlertCircle className="w-3.5 h-3.5 text-slate-400" />
                  Unverified
                </span>
              )}

              {isBrowserOpen ? (
                <button
                  type="button"
                  onClick={handleCloseBrowser}
                  disabled={isClosingBrowser}
                  className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium text-rose-300 hover:text-white bg-rose-950/60 hover:bg-rose-900 border border-rose-700/50 rounded-lg transition disabled:opacity-50 cursor-pointer"
                  title="Close login Chrome browser and release profile lock"
                >
                  <XSquare className="w-3.5 h-3.5" />
                  {isClosingBrowser ? 'Closing...' : 'Close Login Browser'}
                </button>
              ) : (
                <>
                  <button
                    type="button"
                    onClick={handleOpenBrowser}
                    disabled={isOpeningBrowser || selected.isLocked}
                    className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium text-indigo-300 hover:text-white bg-indigo-950/40 hover:bg-indigo-900/60 border border-indigo-700/40 rounded-lg transition disabled:opacity-50 cursor-pointer"
                    title="Launch normal installed Google Chrome for manual sign-in"
                  >
                    <ExternalLink className="w-3.5 h-3.5" />
                    {isOpeningBrowser
                      ? 'Opening...'
                      : selected.status === 'READY'
                        ? 'Re-open Chrome'
                        : 'Open Chrome for Login'}
                  </button>

                  <button
                    type="button"
                    onClick={handleVerifyLogin}
                    disabled={isRefreshing || selected.isLocked}
                    className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 border border-slate-700 rounded-lg transition disabled:opacity-50 cursor-pointer"
                    title="Run temporary Playwright check to verify Google Flow session"
                  >
                    <RefreshCw
                      className={`w-3.5 h-3.5 ${isRefreshing ? 'animate-spin' : ''}`}
                    />
                    {selected.status === 'READY' ? 'Verify Again' : 'Verify Login'}
                  </button>
                </>
              )}
            </div>
          )}
        </div>
      )}

      {isBrowserOpen && (
        <div className="flex items-center gap-2 p-2.5 bg-indigo-950/50 border border-indigo-700/40 rounded-lg text-xs text-indigo-200">
          <Info className="w-4 h-4 text-indigo-400 shrink-0" />
          <span>
            Chrome is open for manual Google sign-in. Complete sign-in and any account verification directly in Chrome. AutoVideo does not access your credentials.
          </span>
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
