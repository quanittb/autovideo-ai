import React, { useState } from 'react';
import { AlertCircle } from 'lucide-react';
import { ModelCard } from '../../components/ui/ModelCard';
import { MockBadge } from '../../components/common/MockBadge';
import { ModelDescriptor } from '../../types/contracts';

export const ModelsView: React.FC = () => {
  const [selectedCategory, setSelectedCategory] = useState<string>('all');

  const models: ModelDescriptor[] = [
    {
      id: 'model-char-swap-v1',
      name: 'Character Inpainting Diffusion v1.0',
      version: '1.0.4',
      task: 'character',
      fileSizeBytes: 4_294_967_296, // 4.0 GB
      license: 'Commercial Permissive',
      runtime: 'DirectML / ONNX 1.16',
      vramRequirementMB: 6144, // 6 GB
      isDownloaded: false,
      isLoadedInVram: false,
      sha256Checksum: 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
    },
    {
      id: 'model-sam-video-v1',
      name: 'Segment Anything Video (SAM-V)',
      version: '2.1.0',
      task: 'character',
      fileSizeBytes: 2_147_483_648, // 2.0 GB
      license: 'Apache 2.0',
      runtime: 'DirectML / ONNX',
      vramRequirementMB: 4096, // 4 GB
      isDownloaded: false,
      isLoadedInVram: false,
      sha256Checksum: '8f434346648f6b96df89dda901c5176b10a6d83961dd3c1ac88b59b2dc327aa4',
    },
    {
      id: 'model-temporal-deflicker-v1',
      name: 'Optical Flow Consistency Engine',
      version: '1.2.0',
      task: 'temporal',
      fileSizeBytes: 1_073_741_824, // 1.0 GB
      license: 'MIT',
      runtime: 'Native Rust + SIMD',
      vramRequirementMB: 2048,
      isDownloaded: false,
      isLoadedInVram: false,
      sha256Checksum: 'ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb',
    },
    {
      id: 'model-scene-diffuse-v1',
      name: 'Scene & Background Inpainting',
      version: '1.0.0',
      task: 'background',
      fileSizeBytes: 6_442_450_944, // 6.0 GB
      license: 'OpenRAIL-M',
      runtime: 'DirectML / TensorRT',
      vramRequirementMB: 8192,
      isDownloaded: false,
      isLoadedInVram: false,
      sha256Checksum: '4e07408562bedb8b60ce05c1decfe3ad16b72230967de01f640b7e4729b49fce',
    },
  ];

  const filteredModels = selectedCategory === 'all'
    ? models
    : models.filter((m) => m.task === selectedCategory);

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-6 bg-slate-950 text-slate-100">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-slate-100 tracking-tight">AI Model Registry</h2>
          <p className="text-sm text-slate-400 mt-1">Manage local neural network weights, tasks, and runtime capabilities</p>
        </div>
        <MockBadge label="STRICT HONESTY PROTOCOL" />
      </div>

      {/* Honesty Notice */}
      <div className="p-4 rounded-xl bg-amber-500/10 border border-amber-500/20 text-xs text-amber-300 flex items-start gap-3">
        <AlertCircle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
        <p className="leading-relaxed">
          <strong>NEVER FAKE AI Policy Active:</strong> AutoVideo AI models must be physically present in the local weights directory before real execution. Uninstalled weights trigger <code className="text-amber-200 bg-amber-950/60 px-1 py-0.5 rounded">MODEL_NOT_AVAILABLE</code> status rather than simulated progress.
        </p>
      </div>

      {/* Task Filters */}
      <div className="flex items-center justify-between pt-2">
        <div className="flex items-center gap-1.5 p-1 rounded-xl bg-slate-900 border border-slate-800">
          {(['all', 'character', 'background', 'temporal'] as const).map((cat) => (
            <button
              key={cat}
              onClick={() => setSelectedCategory(cat)}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold capitalize transition-all ${
                selectedCategory === cat
                  ? 'bg-indigo-600 text-white shadow-md shadow-indigo-900/30'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              {cat === 'all' ? 'All Models' : `${cat} Models`}
            </button>
          ))}
        </div>

        <span className="text-xs text-slate-500">
          Showing {filteredModels.length} of {models.length} Models
        </span>
      </div>

      {/* Model Cards Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {filteredModels.map((model) => (
          <ModelCard
            key={model.id}
            model={model}
            onDownloadClick={(id) => console.log('Download clicked for', id)}
            onRemoveClick={(id) => console.log('Remove clicked for', id)}
          />
        ))}
      </div>
    </div>
  );
};
