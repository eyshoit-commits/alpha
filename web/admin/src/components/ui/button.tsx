import { ButtonHTMLAttributes, forwardRef } from "react";

type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";
type ButtonSize = "sm" | "md" | "lg";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
}

const variantStyles: Record<ButtonVariant, string> = {
  primary: "bg-gradient-to-r from-cyan-500 to-blue-500 text-white hover:from-cyan-400 hover:to-blue-400 focus:ring-cyan-400 shadow-lg shadow-cyan-500/50 hover:shadow-cyan-400/60 font-bold",
  secondary: "bg-[#1a1f3a] text-cyan-400 border-2 border-cyan-500/50 hover:bg-[#2d3561] hover:border-cyan-400 hover:shadow-lg hover:shadow-cyan-500/30 focus:ring-cyan-400 font-semibold",
  danger: "bg-gradient-to-r from-red-600 to-pink-600 text-white hover:from-red-500 hover:to-pink-500 focus:ring-red-400 shadow-lg shadow-red-500/50",
  ghost: "text-cyan-400 hover:bg-[#1a1f3a] hover:text-cyan-300 focus:ring-cyan-400",
};

const sizeStyles: Record<ButtonSize, string> = {
  sm: "px-3 py-1.5 text-sm",
  md: "px-4 py-2.5 text-sm",
  lg: "px-6 py-3 text-base",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ variant = "primary", size = "md", loading = false, disabled, className = "", children, ...props }, ref) => {
    return (
      <button
        ref={ref}
        disabled={disabled || loading}
        className={`
          inline-flex items-center justify-center gap-2 rounded-lg font-semibold
          transition-colors focus:outline-none focus:ring-2 focus:ring-offset-2
          disabled:cursor-not-allowed disabled:opacity-60
          ${variantStyles[variant]}
          ${sizeStyles[size]}
          ${className}
        `}
        {...props}
      >
        {loading && (
          <svg className="h-4 w-4 animate-spin" fill="none" viewBox="0 0 24 24">
            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
            <path
              className="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            />
          </svg>
        )}
        {children}
      </button>
    );
  }
);

Button.displayName = "Button";
