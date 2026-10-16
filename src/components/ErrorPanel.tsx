import { AlertTriangle } from "lucide-react";

/** `title` names what failed, e.g. the statement of a script. */
export function ErrorPanel({ title, message, code }: { title?: string; message: string; code: string | null }) {
  return (
    <div className="error-panel" role="alert" data-stage="query:error" data-stage-label="Query error">
      <AlertTriangle size="1.231rem" style={{ flex: "none", marginTop: 1 }} />
      <div>
        {title && <div className="code">{title}</div>}
        {code && <div className="code">Error {code}</div>}
        <div className="msg">{message}</div>
      </div>
    </div>
  );
}
