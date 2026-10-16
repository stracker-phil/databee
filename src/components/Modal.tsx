import { useEffect, type ReactNode } from "react";

interface Props {
  onClose: () => void;
  className?: string;
  labelledBy?: string;
  /** Exposes the sheet as its own window to Stage (src/stage/hooks.ts). */
  stageWindow?: string;
  children: ReactNode;
}

export function Modal({ onClose, className = "", labelledBy, stageWindow, children }: Props) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={`dialog ${className}`} data-stage={stageWindow ? `${stageWindow}:sheet` : "dialog"} data-stage-window={stageWindow} role="dialog" aria-modal="true" aria-labelledby={labelledBy}>
        {children}
      </div>
    </div>
  );
}
