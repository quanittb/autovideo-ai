import { useMemo, useCallback } from 'react';
import { useEditorStore } from '../stores/editorStore';
import { TimeRulerTick } from '../types/editor';

export const useTimeline = () => {
  const { playback, timelineZoom, seek } = useEditorStore();
  const duration = playback.duration || 1;

  // Compute adaptive interval based on duration and zoom
  const rulerTicks = useMemo<TimeRulerTick[]>(() => {
    if (duration <= 0) return [];

    let interval = 5; // Default 5 seconds
    if (duration <= 10) interval = 1;
    else if (duration <= 30) interval = timelineZoom >= 1.5 ? 1 : 2;
    else if (duration <= 60) interval = timelineZoom >= 1.5 ? 2 : 5;
    else if (duration <= 180) interval = timelineZoom >= 2.0 ? 5 : 10;
    else interval = 30;

    const ticks: TimeRulerTick[] = [];
    const count = Math.ceil(duration / interval);

    for (let i = 0; i <= count; i++) {
      const timeSeconds = Math.min(i * interval, duration);
      const mins = Math.floor(timeSeconds / 60);
      const secs = Math.floor(timeSeconds % 60);
      const label = `${mins}:${secs.toString().padStart(2, '0')}`;
      const leftPercent = (timeSeconds / duration) * 100;

      ticks.push({
        timeSeconds,
        label,
        isMajor: i % 2 === 0,
        leftPercent,
        leftPixel: 0,
      });

      if (timeSeconds >= duration) break;
    }

    return ticks;
  }, [duration, timelineZoom]);

  const timeToPercent = useCallback(
    (timeSeconds: number) => {
      if (duration <= 0) return 0;
      return Math.max(0, Math.min((timeSeconds / duration) * 100, 100));
    },
    [duration]
  );

  const percentToTime = useCallback(
    (percent: number) => {
      const clamped = Math.max(0, Math.min(percent, 100));
      return (clamped / 100) * duration;
    },
    [duration]
  );

  const handleTimelineClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const clickX = e.clientX - rect.left;
      const percent = (clickX / rect.width) * 100;
      const targetTime = percentToTime(percent);
      seek(targetTime);
    },
    [percentToTime, seek]
  );

  return {
    rulerTicks,
    timeToPercent,
    percentToTime,
    handleTimelineClick,
  };
};
