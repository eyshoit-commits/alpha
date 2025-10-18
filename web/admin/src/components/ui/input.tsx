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
          <label className="mb-2 text-sm font-medium text-slate-700">
            {label}
            {required && <span className="ml-1 text-red-500">*</span>}
          </label>
        )}
        <input
          ref={ref}
          className={`
            rounded-lg border-2 px-4 py-3 text-base shadow-sm transition-all
            focus:outline-none focus:ring-2 focus:ring-blue-400 focus:border-blue-500
            disabled:cursor-not-allowed disabled:bg-slate-100 disabled:text-slate-500
            placeholder:text-slate-500
            ${error ? "border-red-300 bg-red-50 text-red-900" : "border-slate-300 bg-white hover:border-blue-300"}
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
