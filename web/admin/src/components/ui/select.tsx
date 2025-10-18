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
          <label className="mb-2 text-base font-bold text-[#75ffaf]">
            {label}
            {required && <span className="ml-1 text-[#D3188C]">*</span>}
          </label>
        )}
        <select
          ref={ref}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className={`
            rounded-lg border-2 px-4 py-3 text-base shadow-sm transition-all font-medium
            focus:outline-none focus:ring-2 focus:ring-[#5bec92] focus:border-[#5bec92] focus:shadow-lg focus:shadow-[#5bec92]/30
            disabled:cursor-not-allowed disabled:bg-[#0a0e27] disabled:text-slate-600
            ${error ? "border-red-400 bg-red-900/20 text-red-300" : "border-[#5bec92]/60 bg-[#1a1f3a] text-slate-200 hover:border-[#5bec92]"}
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
