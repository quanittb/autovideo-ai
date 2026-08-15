import React from 'react';

interface MockBadgeProps {
  label?: string;
  className?: string;
}

export const MockBadge: React.FC<MockBadgeProps> = ({ 
  label = 'DEMO DATA / MOCK', 
  className = '' 
}) => {
  return (
    <div 
      className={`inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-semibold bg-amber-500/10 text-amber-400 border border-amber-500/20 backdrop-blur-sm ${className}`}
      title="This view uses fixture demo media to demonstrate UI layout. Real AI model weights are not loaded."
    >
      <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
      <span>{label}</span>
    </div>
  );
};
