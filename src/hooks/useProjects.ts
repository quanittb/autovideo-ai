import { useEffect } from 'react';
import { api } from '../lib/ipc';
import { useProjectStore } from '../stores/projectStore';

export function useProjects() {
  const { projects, setProjects, setLoading, setError, isLoading, error } = useProjectStore();

  useEffect(() => {
    setLoading(true);
    api.listProjects()
      .then((list) => {
        if (list.length > 0) {
          setProjects(list);
        }
        setLoading(false);
      })
      .catch((err) => {
        setError(String(err));
        setLoading(false);
      });
  }, [setProjects, setLoading, setError]);

  return { projects, isLoading, error };
}
