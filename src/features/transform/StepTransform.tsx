import React, { useEffect } from 'react';
import { useProjectStore } from '../../stores/projectStore';
import { useCloudJobStore } from '../../stores/cloudJobStore';
import { TransformPanel } from './TransformPanel';
import { RealTransformPreview } from './RealTransformPreview';
import { Video } from 'lucide-react';

export const StepTransform: React.FC = () => {
  const { activeProject } = useProjectStore();
  const {
    cloudJobsById,
    selectedInternalJobId,
    authorizedSource,
    authorizedArtifact,
    subscribeToEvents,
    loadProjectCloudJobs,
    authorizeSource,
    authorizeArtifact,
    revokePreview,
  } = useCloudJobStore();

  const selectedJob = selectedInternalJobId ? cloudJobsById[selectedInternalJobId] : null;

  // Startup race protection: Subscribe to events FIRST, then list jobs
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const init = async () => {
      unlisten = await subscribeToEvents();

      if (activeProject?.id) {
        await loadProjectCloudJobs(activeProject.id);
        await authorizeSource(activeProject.id);
      }
    };

    init();

    return () => {
      if (unlisten) unlisten();
      if (activeProject?.id) {
        revokePreview(activeProject.id);
      }
    };
  }, [activeProject?.id, subscribeToEvents, loadProjectCloudJobs, authorizeSource, revokePreview]);

  // When selected completed job changes, authorize artifact preview
  useEffect(() => {
    const isCompleted =
      selectedJob && (selectedJob.state as string).toUpperCase() === 'COMPLETED';
    if (activeProject?.id && selectedJob && isCompleted) {
      authorizeArtifact(activeProject.id, selectedJob.internalJobId);
    }
  }, [activeProject?.id, selectedJob?.state, selectedJob?.internalJobId, authorizeArtifact]);

  if (!activeProject) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center p-8 bg-slate-950 text-slate-400 space-y-3">
        <Video className="w-12 h-12 opacity-30" />
        <p className="text-sm font-semibold">No active project loaded</p>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      <div>
        <h2 className="text-2xl font-bold text-slate-100 tracking-tight">Step 2: Transform Video</h2>
        <p className="text-sm text-slate-400 mt-1">
          Configure real cloud transformations with authoritative preflight and verified local artifacts
        </p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
        {/* Left Column: Configuration Controls */}
        <div className="lg:col-span-5">
          <TransformPanel />
        </div>

        {/* Right Column: Real Format-Aware Preview */}
        <div className="lg:col-span-7">
          <RealTransformPreview
            projectId={activeProject.id}
            selectedJob={selectedJob}
            authorizedSource={authorizedSource}
            authorizedArtifact={authorizedArtifact}
          />
        </div>
      </div>
    </div>
  );
};
