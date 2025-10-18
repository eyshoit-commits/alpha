import { ReactNode } from "react";

interface CardProps {
  title?: string;
  description?: string;
  children: ReactNode;
  className?: string;
  actions?: ReactNode;
}

export function Card({ title, description, children, className = "", actions }: CardProps) {
  return (
    <div className={`rounded-xl border-2 border-[#2d3561] bg-[#12172f] shadow-lg shadow-cyan-500/10 hover:shadow-cyan-500/20 hover:border-cyan-500/50 transition-all backdrop-blur-sm ${className}`}>
      {(title || description || actions) && (
        <div className="border-b-2 border-[#2d3561] bg-gradient-to-r from-[#1a1f3a] to-[#12172f] px-6 py-5">
          <div className="flex items-start justify-between">
            <div>
              {title && <h2 className="text-xl font-bold text-cyan-400 tracking-wide">{title}</h2>}
              {description && <p className="mt-2 text-base text-slate-300 leading-relaxed">{description}</p>}
            </div>
            {actions && <div className="ml-4">{actions}</div>}
          </div>
        </div>
      )}
      <div className="p-6">{children}</div>
    </div>
  );
}
