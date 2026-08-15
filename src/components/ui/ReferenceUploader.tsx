import React, { useState } from 'react';
import { UploadCloud, X } from 'lucide-react';

interface ReferenceUploaderProps {
  onImageSelected?: (uri: string) => void;
  label?: string;
  className?: string;
}

export const ReferenceUploader: React.FC<ReferenceUploaderProps> = ({
  onImageSelected,
  label = 'Reference Image (Optional)',
  className = '',
}) => {
  const [selectedImage, setSelectedImage] = useState<string | null>(null);

  const handleSimulatedUpload = () => {
    const fixtureImage = '🐰';
    setSelectedImage(fixtureImage);
    if (onImageSelected) onImageSelected(fixtureImage);
  };

  const handleClear = (e: React.MouseEvent) => {
    e.stopPropagation();
    setSelectedImage(null);
  };

  return (
    <div className={`space-y-1.5 ${className}`}>
      <div className="flex items-center justify-between">
        <label className="text-xs font-semibold text-slate-300">{label}</label>
        <span className="text-[10px] text-slate-500">PNG, JPG up to 10MB</span>
      </div>

      {selectedImage ? (
        <div className="relative h-24 rounded-xl bg-slate-950 border border-indigo-500/50 p-3 flex items-center justify-between group">
          <div className="flex items-center gap-3">
            <div className="w-16 h-16 rounded-lg bg-purple-950/60 border border-purple-800/60 flex items-center justify-center text-3xl">
              {selectedImage}
            </div>
            <div>
              <span className="text-xs font-medium text-slate-200 block">white_rabbit_reference.png</span>
              <span className="text-[10px] text-slate-500">1024 x 1024 • 1.4 MB</span>
            </div>
          </div>

          <button
            onClick={handleClear}
            className="p-1.5 rounded-lg bg-slate-900 text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
            aria-label="Remove reference image"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      ) : (
        <div
          onClick={handleSimulatedUpload}
          className="h-24 rounded-xl border border-dashed border-slate-700 hover:border-indigo-500/60 bg-slate-950/50 hover:bg-slate-900/40 p-3 flex flex-col items-center justify-center text-center cursor-pointer transition-all group"
        >
          <UploadCloud className="w-5 h-5 text-indigo-400 mb-1 group-hover:scale-110 transition-transform" />
          <span className="text-xs font-medium text-slate-300">
            Drop reference image or <span className="text-indigo-400">browse</span>
          </span>
          <span className="text-[10px] text-slate-500 mt-0.5">Use as character identity target</span>
        </div>
      )}
    </div>
  );
};
