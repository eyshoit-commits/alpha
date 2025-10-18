import { forwardRef, InputHTMLAttributes } from "react";

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  error?: string;
  helperText?: string;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ label, error, helperText, required, className = "", ...props }, ref) => {
    return (
      <div className="flex flex-col">
        {label && (
          <label className="mb-2 text-sm font-bold text-[#75ffaf]">
            {label}
            {required && <span className="ml-1 text-[#EC5800]">*</span>}
          </label>
        )}
        <input
          ref={ref}
          className={`
            rounded-lg border-2 px-4 py-3 text-base shadow-sm transition-all
            focus:outline-none focus:ring-2 focus:ring-[#75ffaf] focus:border-[#75ffaf] focus:shadow-lg focus:shadow-[#75ffaf]/20
            disabled:cursor-not-allowed disabled:bg-[#0a0e27] disabled:text-slate-600
            placeholder:text-slate-500
            ${error ? "border-red-400 bg-red-900/20 text-red-300" : "border-[#75ffaf]/40 bg-[#1a1f3a] text-slate-200 hover:border-[#75ffaf]/70"}
            ${className}
          `}
          {...props}
        />
        {error && <p className="mt-1.5 text-sm text-red-600">{error}</p>}
        {helperText && !error && <p className="mt-1.5 text-sm text-slate-500">{helperText}</p>}
      </div>
    );
  }
);

Input.displayName = "Input";
