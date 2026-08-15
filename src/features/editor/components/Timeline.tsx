import React, { useRef } from 'react';
import { TimelineRuler } from './TimelineRuler';
import { Playhead } from './Playhead';
import { VideoTrack } from './VideoTrack';
import { AudioTrack } from './AudioTrack';
import { useTimeline } from '../hooks/useTimeline';
import { useEditorStore } from '../stores/editorStore';

export const Timeline: React.FC = () => {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const trackContainerRef = useRef<HTMLDivElement | null>(null);
  const { timelineZoom, setTimelineZoom } = useEditorStore();

  const { rulerTicks, handleTimelineClick } = useTimeline();

  // Wheel zoom handler with Ctrl key
  const handleWheel = (e: React.WheelEvent) => {
    if (e.ctrlKey) {
      e.preventDefault();
      if (e.deltaY < 0) {
        setTimelineZoom(timelineZoom + 0.1);
      } else {
        setTimelineZoom(timelineZoom - 0.1);
      }
    }
  };

  return (
    <div
      ref={containerRef}
      onWheel={handleWheel}
      className="flex-1 flex flex-col bg-slate-950/90 border border-slate-800 rounded-2xl overflow-hidden shadow-2xl min-h-[190px]"
    >
      {/* Scrollable Timeline Area */}
      <div className="flex-1 overflow-x-auto overflow-y-hidden select-none relative">
        <div
          ref={trackContainerRef}
          onClick={handleTimelineClick}
          className="relative min-w-full cursor-pointer transition-all duration-75"
          style={{ width: `${Math.max(100, timelineZoom * 100)}%` }}
        >
          {/* Time Ruler */}
          <TimelineRuler ticks={rulerTicks} onRulerClick={handleTimelineClick} />

          {/* Interactive Playhead Line */}
          <Playhead timelineRef={trackContainerRef} />

          {/* Media Tracks Container */}
          <div className="p-3 space-y-2">
            <VideoTrack />
            <AudioTrack />
          </div>
        </div>
      </div>
    </div>
  );
};
