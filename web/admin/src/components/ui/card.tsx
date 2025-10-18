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
    <div className={`rounded-xl border-2 border-slate-200 bg-white shadow-md hover:shadow-lg transition-shadow ${className}`}>
      {(title || description || actions) && (
        <div className="border-b-2 border-blue-100 bg-gradient-to-r from-blue-50 to-white px-6 py-5">
          <div className="flex items-start justify-between">
            <div>
              {title && <h2 className="text-xl font-bold text-slate-900">{title}</h2>}
              {description && <p className="mt-2 text-base text-slate-700 leading-relaxed">{description}</p>}
            </div>
            {actions && <div className="ml-4">{actions}</div>}
          </div>
        </div>
      )}
      <div className="p-6">{children}</div>
    </div>
  );
}
