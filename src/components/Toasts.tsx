import { useEffect } from "react";
import { AlertTriangle, CheckCircle2, Info, X } from "lucide-react";
import { useStore, type Toast } from "../state/store";

const AUTO_DISMISS_MS = { success: 2500, info: 4000, error: 8000 };

function ToastItem({ toast }: { toast: Toast }) {
  const dismiss = useStore((s) => s.dismissToast);
  useEffect(() => {
    const timer = setTimeout(() => dismiss(toast.id), AUTO_DISMISS_MS[toast.kind]);
    return () => clearTimeout(timer);
  }, [toast, dismiss]);
  const Icon = toast.kind === "error" ? AlertTriangle : toast.kind === "success" ? CheckCircle2 : Info;
  return (
    <div className={`toast ${toast.kind}`} role={toast.kind === "error" ? "alert" : "status"}>
      <Icon size={15} />
      <span className="text">{toast.message}</span>
      <button className="icon-btn" style={{ width: 18, height: 18 }} aria-label="Dismiss" onClick={() => dismiss(toast.id)}>
        <X size={12} />
      </button>
    </div>
  );
}

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  return (
    <div className="toasts">
      {toasts.map((t) => (
        <ToastItem key={t.id} toast={t} />
      ))}
    </div>
  );
}
