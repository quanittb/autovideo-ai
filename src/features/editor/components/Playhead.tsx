import React, { useRef } from 'react';
import { useEditorStore } from '../stores/editorStore';

interface PlayheadProps {
  timelineRef: React.RefObject<HTMLDivElement | null>;
}

export const Playhead: React.FC<PlayheadProps> = ({ timelineRef }) => {
  const { playback, seek } = useEditorStore();
  const isDraggingRef = useRef(false);

  const duration = playback.duration || 1;
  const leftPercent = Math.max(0, Math.min((playback.currentTime / duration) * 100, 100));

  const formatBadgeTime = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    const ms = Math.floor((secs % 1) * 10);
    return `${m}:${s.toString().padStart(2, '0')}.${ms}`;
  };

  const handleMouseDown = (e: React.MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
    isDraggingRef.current = true;

    const updateSeek = (clientX: number) => {
      if (!timelineRef.current) return;
      const rect = timelineRef.current.getBoundingClientRect();
      const clickX = clientX - rect.left;
      const percent = Math.max(0, Math.min((clickX / rect.width) * 100, 100));
      const targetTime = (percent / 100) * duration;
      seek(targetTime);
    };

    const handleMouseMove = (moveEvent: MouseEvent) => {
      if (isDraggingRef.current) {
        updateSeek(moveEvent.clientX);
      }
    };

    const handleMouseUp = () => {
      isDraggingRef.current = false;
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
  };

  return (
    <div
      className="absolute top-0 bottom-0 z-30 pointer-events-none transform -translate-x-1/2"
      style={{ left: `${leftPercent}%` }}
    >
      {/* Top Playhead Draggable Handle & Badge */}
      <div
        onMouseDown={handleMouseDown}
        className="pointer-events-auto cursor-ew-resize flex flex-col items-center group -mt-1"
      >
        <div className="px-1.5 py-0.5 rounded bg-indigo-500 text-white font-mono text-[9px] font-bold shadow-lg shadow-indigo-900/50 group-hover:bg-indigo-400 group-hover:scale-110 transition-all select-none">
          {formatBadgeTime(playback.currentTime)}
        </div>
        {/* Playhead Arrow indicator */}
        <div className="w-0 h-0 border-l-[4px] border-l-transparent border-r-[4px] border-r-transparent border-t-[5px] border-t-indigo-500 group-hover:border-t-indigo-400" />
      </div>

      {/* Vertical Playhead Line */}
      <div className="w-[1.5px] h-full bg-indigo-500 shadow-[0_0_8px_rgba(99,102,241,0.8)] -mt-0.5" />
    </div>
  );
};
