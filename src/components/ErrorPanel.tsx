import { AlertTriangle } from "lucide-react";

export function ErrorPanel({ message, code }: { message: string; code: string | null }) {
  return (
    <div className="error-panel" role="alert" data-stage="query:error" data-stage-label="Query error">
      <AlertTriangle size="1.231rem" style={{ flex: "none", marginTop: 1 }} />
      <div>
        {code && <div className="code">Error {code}</div>}
        <div className="msg">{message}</div>
      </div>
    </div>
  );
}
