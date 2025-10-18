type AlertVariant = "success" | "error" | "warning" | "info";

interface AlertProps {
  variant: AlertVariant;
  title?: string;
  children: React.ReactNode;
  onClose?: () => void;
}

const variantStyles: Record<AlertVariant, { container: string; title: string; icon: string }> = {
  success: {
    container: "border-emerald-200 bg-emerald-50 text-emerald-900",
    title: "text-emerald-900",
    icon: "text-emerald-600",
  },
  error: {
    container: "border-red-200 bg-red-50 text-red-900",
    title: "text-red-900",
    icon: "text-red-600",
  },
  warning: {
    container: "border-amber-200 bg-amber-50 text-amber-900",
    title: "text-amber-900",
    icon: "text-amber-600",
  },
  info: {
    container: "border-blue-200 bg-blue-50 text-blue-900",
    title: "text-blue-900",
    icon: "text-blue-600",
  },
};

const icons: Record<AlertVariant, JSX.Element> = {
  success: (
    <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
    </svg>
  ),
  error: (
    <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" />
    </svg>
  ),
  warning: (
    <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
    </svg>
  ),
  info: (
    <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
    </svg>
  ),
};

export function Alert({ variant, title, children, onClose }: AlertProps) {
  const styles = variantStyles[variant];

  return (
    <div className={`relative rounded-lg border p-4 ${styles.container}`}>
      <div className="flex gap-3">
        <div className={styles.icon}>{icons[variant]}</div>
        <div className="flex-1">
          {title && <p className={`font-semibold ${styles.title}`}>{title}</p>}
          <div className="mt-1 text-sm">{children}</div>
        </div>
        {onClose && (
          <button onClick={onClose} className="text-current opacity-60 hover:opacity-100">
            <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
}
