type AlertVariant = "success" | "error" | "warning" | "info";

interface AlertProps {
  variant: AlertVariant;
  title?: string;
  children: React.ReactNode;
  onClose?: () => void;
}

const variantStyles: Record<AlertVariant, { container: string; title: string; icon: string }> = {
  success: {
    container: "border-2 border-cyan-500/50 bg-cyan-900/20 text-cyan-100 shadow-lg shadow-cyan-500/20",
    title: "text-cyan-300",
    icon: "text-cyan-400",
  },
  error: {
    container: "border-2 border-red-500/50 bg-red-900/20 text-red-100 shadow-lg shadow-red-500/20",
    title: "text-red-300",
    icon: "text-red-400",
  },
  warning: {
    container: "border-2 border-yellow-500/50 bg-yellow-900/20 text-yellow-100 shadow-lg shadow-yellow-500/20",
    title: "text-yellow-300",
    icon: "text-yellow-400",
  },
  info: {
    container: "border-2 border-purple-500/50 bg-purple-900/20 text-purple-100 shadow-lg shadow-purple-500/20",
    title: "text-purple-300",
    icon: "text-purple-400",
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
    <div className={`relative rounded-lg border-2 p-5 shadow-sm ${styles.container}`}>
      <div className="flex gap-4">
        <div className={styles.icon}>{icons[variant]}</div>
        <div className="flex-1">
          {title && <p className={`text-lg font-bold ${styles.title}`}>{title}</p>}
          <div className="mt-2 text-base leading-relaxed">{children}</div>
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
