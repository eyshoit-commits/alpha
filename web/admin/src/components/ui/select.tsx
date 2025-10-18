import { forwardRef, SelectHTMLAttributes } from "react";

interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "onChange"> {
  label?: string;
  options: SelectOption[];
  value: string;
  onChange: (value: string) => void;
  error?: string;
  helperText?: string;
  required?: boolean;
}

export const Select = forwardRef<HTMLSelectElement, SelectProps>(
  ({ label, options, value, onChange, error, helperText, required, className = "", ...props }, ref) => {
    return (
      <div className="flex flex-col">
        {label && (
          <label className="mb-2 text-sm font-medium text-slate-700">
            {label}
            {required && <span className="ml-1 text-red-500">*</span>}
          </label>
        )}
        <select
          ref={ref}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className={`
            rounded-lg border-2 px-4 py-3 text-base shadow-sm transition-all font-medium
            focus:outline-none focus:ring-2 focus:ring-blue-400 focus:border-blue-500
            disabled:cursor-not-allowed disabled:bg-slate-100 disabled:text-slate-500
            ${error ? "border-red-300 bg-red-50 text-red-900" : "border-slate-300 bg-white hover:border-blue-300"}
            ${className}
          `}
          {...props}
        >
          {options.map((option) => (
            <option key={option.value} value={option.value} disabled={option.disabled}>
              {option.label}
            </option>
          ))}
        </select>
        {error && <p className="mt-1.5 text-sm text-red-600">{error}</p>}
        {helperText && !error && <p className="mt-1.5 text-sm text-slate-500">{helperText}</p>}
      </div>
    );
  }
);

Select.displayName = "Select";
