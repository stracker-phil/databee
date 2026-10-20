import { useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, Info, X } from "lucide-react";
import { useStore, type Toast } from "../state/store";

const AUTO_DISMISS_MS = { success: 2500, info: 4000, error: 8000 };

function ToastItem({ toast }: { toast: Toast }) {
  const dismiss = useStore((s) => s.dismissToast);
  useEffect(() => {
    const timer = setTimeout(() => dismiss(toast.id), AUTO_DISMISS_MS[toast.kind]);
    return () => clearTimeout(timer);
  }, [toast, dismiss]);
  const [details, setDetails] = useState(false);
  const Icon = toast.kind === "error" ? AlertTriangle : toast.kind === "success" ? CheckCircle2 : Info;
  const [headline, ...rest] = toast.kind === "error" ? errorLines(toast.message) : [toast.message];
  return (
    <div className={`toast ${toast.kind}`} role={toast.kind === "error" ? "alert" : "status"}>
      <Icon size="1.154rem" />
      <span className="text">
        {toast.kind === "error" && <strong className="toast-title">Something went wrong{toast.count > 1 ? ` (×${toast.count})` : ""}</strong>}
        {toast.kind !== "error" && toast.count > 1 && <span className="muted">×{toast.count} </span>}
        <span>{headline}</span>
        {rest.length > 0 && (
          <>
            {" "}
            <button className="link-btn" onClick={() => setDetails((d) => !d)}>
              {details ? "Hide details" : "Details"}
            </button>
            {details && <pre className="toast-details">{rest.join("\n")}</pre>}
          </>
        )}
      </span>
      <button className="icon-btn" style={{ width: "1.385rem", height: 18 }} aria-label="Dismiss" onClick={() => dismiss(toast.id)}>
        <X size="0.923rem" />
      </button>
    </div>
  );
}

/** First line is the headline; the raw remainder (stack, SQL, codes) hides behind "Details". */
function errorLines(message: string): string[] {
  const clean = message.replace(/^Unexpected error:\s*/, "").trim();
  const [first, ...rest] = clean.split(/\n+/);
  const headline = first.length > 160 ? `${first.slice(0, 157)}…` : first;
  const detail = [first.length > 160 ? first : null, ...rest].filter((l): l is string => !!l);
  return [headline, ...detail];
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
