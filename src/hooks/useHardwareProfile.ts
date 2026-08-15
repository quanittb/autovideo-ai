import { useEffect } from 'react';
import { api } from '../lib/ipc';
import { useSettingsStore } from '../stores/settingsStore';

export function useHardwareProfile() {
  const { hardware, setHardware, storage, setStorage } = useSettingsStore();

  useEffect(() => {
    if (!hardware) {
      api.getHardwareProfile().then(setHardware);
    }
    if (!storage) {
      api.getStoragePaths().then(setStorage);
    }
  }, [hardware, setHardware, storage, setStorage]);

  return { hardware, storage };
}
