import React from 'react';
import { TimeRulerTick } from '../types/editor';

interface TimelineRulerProps {
  ticks: TimeRulerTick[];
  onRulerClick: (e: React.MouseEvent<HTMLDivElement>) => void;
}

export const TimelineRuler: React.FC<TimelineRulerProps> = ({ ticks, onRulerClick }) => {
  return (
    <div
      onClick={onRulerClick}
      className="relative h-6 bg-slate-950/90 border-b border-slate-800 text-[10px] font-mono text-slate-500 select-none cursor-pointer overflow-hidden"
    >
      {ticks.map((tick, idx) => (
        <div
          key={idx}
          className="absolute top-0 bottom-0 flex flex-col justify-between pointer-events-none transform -translate-x-1/2"
          style={{ left: `${tick.leftPercent}%` }}
        >
          <span className="text-[9px] px-1 text-slate-400 font-semibold">{tick.label}</span>
          <div
            className={`w-[1px] ${
              tick.isMajor ? 'h-2 bg-slate-600' : 'h-1 bg-slate-800'
            }`}
          />
        </div>
      ))}
    </div>
  );
};
