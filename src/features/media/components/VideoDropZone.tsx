import React, { useEffect, useState, useRef } from 'react';
import { UploadCloud, AlertCircle, Loader2, CheckCircle2 } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';

export interface VideoDropZoneProps {
  onVideoSelected: (filePath: string) => Promise<void> | void;
  disabled?: boolean;
  hasImportedVideo?: boolean;
  className?: string;
}

const SUPPORTED_EXTENSIONS = ['mp4', 'mov', 'avi', 'mkv'];

export const VideoDropZone: React.FC<VideoDropZoneProps> = ({
  onVideoSelected,
  disabled = false,
  hasImportedVideo = false,
  className = '',
}) => {
  const [isDraggingOver, setIsDraggingOver] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [validationError, setValidationError] = useState<string | null>(null);
  const [successNotice, setSuccessNotice] = useState<boolean>(false);
  const importingRef = useRef(false);

  // Validate extension on frontend before triggering Rust import
  const validateExtension = (filePath: string): boolean => {
    const cleanPath = filePath.trim();
    const dotIndex = cleanPath.lastIndexOf('.');
    if (dotIndex === -1) {
      setValidationError('Unsupported video format. Supported formats: MP4, MOV, AVI, MKV');
      return false;
    }
    const ext = cleanPath.slice(dotIndex + 1).toLowerCase();
    if (!SUPPORTED_EXTENSIONS.includes(ext)) {
      setValidationError(`Unsupported video format: .${ext}. Supported: MP4, MOV, AVI, MKV`);
      return false;
    }
    setValidationError(null);
    return true;
  };

  const processFilePath = async (filePath: string) => {
    if (importingRef.current || disabled) {
      return;
    }

    if (!validateExtension(filePath)) {
      return;
    }

    importingRef.current = true;
    setIsImporting(true);
    setValidationError(null);
    setSuccessNotice(false);

    try {
      await onVideoSelected(filePath);
      setSuccessNotice(true);
      setTimeout(() => setSuccessNotice(false), 3000);
    } catch (err: any) {
      setValidationError(err?.message || 'Unable to import video file');
    } finally {
      setIsImporting(false);
      importingRef.current = false;
    }
  };

  // 1. Native Windows File Picker Trigger
  const handleOpenNativePicker = async (e?: React.MouseEvent) => {
    if (e) {
      e.stopPropagation();
    }
    if (isImporting || disabled) return;

    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: 'Select Video File (MP4, MOV, AVI, MKV)',
        filters: [
          {
            name: 'Video Files',
            extensions: ['mp4', 'mov', 'avi', 'mkv'],
          },
        ],
      });

      if (!selected) {
        // User cancelled dialog
        return;
      }

      const filePath = typeof selected === 'string' ? selected : selected[0];
      if (filePath) {
        await processFilePath(filePath);
      }
    } catch (err: any) {
      console.warn('Native dialog error/fallback:', err);
    }
  };

  // 2. Tauri 2 Desktop Window Drag & Drop Event Listener
  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | null = null;

    const setupTauriDragDrop = async () => {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        const win = getCurrentWindow();
        if (win && typeof win.onDragDropEvent === 'function') {
          const fn = await win.onDragDropEvent((event) => {
            if (!isMounted) return;
            const payload = event.payload;

            if (payload.type === 'over' || payload.type === 'enter') {
              setIsDraggingOver(true);
            } else if (payload.type === 'leave') {
              setIsDraggingOver(false);
            } else if (payload.type === 'drop') {
              setIsDraggingOver(false);
              const paths = payload.paths;
              if (paths && paths.length > 0) {
                const droppedPath = paths[0];
                processFilePath(droppedPath);
              }
            }
          });
          unlisten = fn;
        }
      } catch (err) {
        // In web fallback or test environment, Tauri window event might not be available
        console.info('Tauri desktop drag & drop initialized or skipped in non-Tauri context');
      }
    };

    setupTauriDragDrop();

    return () => {
      isMounted = false;
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  // 3. Webview / HTML5 Drag & Drop Fallback
  const handleHtml5DragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!isImporting && !disabled) {
      setIsDraggingOver(true);
    }
  };

  const handleHtml5DragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDraggingOver(false);
  };

  const handleHtml5Drop = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDraggingOver(false);
    if (isImporting || disabled) return;

    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      const file = e.dataTransfer.files[0];
      // In Tauri desktop webview, File objects often have a 'path' property
      const filePath = (file as any).path || file.name;
      if (filePath) {
        processFilePath(filePath);
      }
    }
  };

  return (
    <div className={`space-y-3 ${className}`}>
      <div
        onClick={handleOpenNativePicker}
        onDragOver={handleHtml5DragOver}
        onDragLeave={handleHtml5DragLeave}
        onDrop={handleHtml5Drop}
        className={`relative border-2 border-dashed rounded-2xl p-10 transition-all flex flex-col items-center justify-center text-center cursor-pointer select-none ${
          isDraggingOver
            ? 'border-purple-400 bg-purple-950/40 scale-[1.01] shadow-xl shadow-purple-900/30'
            : isImporting
            ? 'border-indigo-500 bg-slate-900/60 opacity-80 cursor-wait'
            : 'border-slate-700 hover:border-indigo-500/80 bg-slate-900/40 hover:bg-slate-900/60'
        }`}
      >
        <div
          className={`w-16 h-16 rounded-2xl flex items-center justify-center mb-4 transition-transform shadow-lg ${
            isDraggingOver
              ? 'bg-purple-600/20 border border-purple-400 text-purple-300 scale-110'
              : isImporting
              ? 'bg-indigo-600/20 border border-indigo-500 text-indigo-300 animate-spin'
              : 'bg-indigo-600/10 border border-indigo-500/20 text-indigo-400 group-hover:scale-110 shadow-indigo-900/20'
          }`}
        >
          {isImporting ? (
            <Loader2 className="w-8 h-8" />
          ) : isDraggingOver ? (
            <UploadCloud className="w-8 h-8 text-purple-400 animate-bounce" />
          ) : (
            <UploadCloud className="w-8 h-8" />
          )}
        </div>

        <h3 className="text-lg font-semibold text-slate-200">
          {isImporting
            ? 'Importing video...'
            : isDraggingOver
            ? 'Release to import video'
            : hasImportedVideo
            ? 'Replace Video File'
            : 'Drag video here'}
        </h3>

        <p className="text-xs text-indigo-400 hover:underline font-medium mt-1">
          {isImporting
            ? 'Extracting metadata and indexing...'
            : isDraggingOver
            ? 'Drop to start import'
            : 'or click to browse from disk'}
        </p>

        <div className="mt-6 pt-4 border-t border-slate-800/80 text-xs text-slate-500 space-y-1">
          <p>Accepted formats: <strong>MP4, MOV, AVI, MKV</strong></p>
          <p>Maximum file size: <strong>2 GB</strong> • Recommended: <strong>30–90 seconds</strong></p>
        </div>
      </div>

      {/* Validation Error Display */}
      {validationError && (
        <div className="p-3.5 rounded-xl bg-rose-500/10 border border-rose-500/20 text-xs text-rose-300 flex items-start gap-2.5">
          <AlertCircle className="w-4 h-4 text-rose-400 shrink-0 mt-0.5" />
          <div className="flex-1">
            <span className="font-semibold block">{validationError}</span>
          </div>
        </div>
      )}

      {/* Success Notification */}
      {successNotice && (
        <div className="p-3 rounded-xl bg-emerald-500/10 border border-emerald-500/20 text-xs text-emerald-300 flex items-center gap-2">
          <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
          <span>Video imported and verified successfully!</span>
        </div>
      )}
    </div>
  );
};
