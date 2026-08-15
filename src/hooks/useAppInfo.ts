import { useState, useEffect } from 'react';
import { api, AppInfo } from '../lib/ipc';

export function useAppInfo() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    api.getAppInfo().then((info) => {
      setAppInfo(info);
      setIsLoading(false);
    });
  }, []);

  return { appInfo, isLoading };
}
