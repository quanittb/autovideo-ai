import React, { useState } from 'react';
import { User, ShieldCheck, AlertCircle, Plus, Lock } from 'lucide-react';
import { FlowProfileInfo } from '../../lib/ipc';

interface FlowProfileSelectorProps {
  profiles: FlowProfileInfo[];
  selectedProfileId: string | null;
  isLoading: boolean;
  onSelectProfile: (profileId: string) => void;
  onCreateProfile: (profileId: string, name: string) => Promise<void>;
}

export const FlowProfileSelector: React.FC<FlowProfileSelectorProps> = ({
  profiles,
  selectedProfileId,
  isLoading,
  onSelectProfile,
  onCreateProfile,
}) => {
  const [showCreate, setShowCreate] = useState(false);
  const [newProfileId, setNewProfileId] = useState('');
  const [newProfileName, setNewProfileName] = useState('');

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

  return (
    <div className="flex flex-col gap-2 p-4 bg-slate-900/60 border border-slate-800 rounded-xl">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <User className="w-4 h-4 text-indigo-400" />
          <span className="text-sm font-semibold text-slate-200">Google Flow Profile</span>
        </div>

        <button
          type="button"
          onClick={() => setShowCreate(!showCreate)}
          className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium text-indigo-300 hover:text-white bg-indigo-950/60 hover:bg-indigo-900 border border-indigo-700/50 rounded-lg transition"
        >
          <Plus className="w-3 h-3" />
          New Profile
        </button>
      </div>

      {showCreate ? (
        <form onSubmit={handleCreate} className="flex flex-col gap-2 p-3 bg-slate-950/70 border border-indigo-500/30 rounded-lg">
          <span className="text-xs font-medium text-slate-300">Create Isolated Flow Profile</span>
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
              className="px-3 py-1 text-xs font-medium text-white bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 rounded"
            >
              Save Profile
            </button>
          </div>
        </form>
      ) : (
        <div className="flex items-center gap-3">
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
            <div className="flex items-center gap-1.5 text-xs font-medium">
              {selected.isLocked ? (
                <span className="flex items-center gap-1 px-2.5 py-1 bg-amber-950/60 border border-amber-600/40 text-amber-300 rounded-lg">
                  <Lock className="w-3.5 h-3.5" />
                  In Use
                </span>
              ) : selected.isAuthenticated ? (
                <span className="flex items-center gap-1 px-2.5 py-1 bg-emerald-950/60 border border-emerald-600/40 text-emerald-300 rounded-lg">
                  <ShieldCheck className="w-3.5 h-3.5" />
                  Ready
                </span>
              ) : (
                <span className="flex items-center gap-1 px-2.5 py-1 bg-slate-800 border border-slate-700 text-slate-300 rounded-lg">
                  <AlertCircle className="w-3.5 h-3.5 text-amber-400" />
                  Check Auth
                </span>
              )}
            </div>
          )}
        </div>
      )}

      <span className="text-[11px] text-slate-500">
        Each profile maintains separate browser session cookies and isolated Chrome storage.
      </span>
    </div>
  );
};
