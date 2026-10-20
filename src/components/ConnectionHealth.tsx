import { AlertTriangle } from "lucide-react";
import { useStore, type LostHealth } from "../state/store";

const clock = (ms: number, seconds = false) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: seconds ? "2-digit" : undefined });

function healthTitle(health: LostHealth): string {
  const lost = `Connection lost since ${clock(health.since)} · ${health.error}`;
  return health.state === "reconnecting" ? `${lost} · reconnecting…` : `${lost} · retries by itself at ${clock(health.retryAt, true)}`;
}

/** Next to the connection name while its server does not answer: a warning that names the cause, and Retry. */
export function ConnectionHealth({ sessionId }: { sessionId: string }) {
  const health = useStore((s) => s.health[sessionId]);
  if (!health) return null;
  const reconnecting = health.state === "reconnecting";
  return (
    <span className="conn-health" role="status" title={healthTitle(health)} data-stage="toolbar:connection-health" data-stage-label="Connection lost">
      <AlertTriangle size="1rem" className="conn-health-icon" aria-label="Connection lost" />
      <button
        type="button"
        className="btn conn-retry"
        data-stage="toolbar:retry"
        disabled={reconnecting}
        onClick={() => void useStore.getState().retryConnection(sessionId)}
      >
        {reconnecting ? "Reconnecting…" : "Retry"}
      </button>
    </span>
  );
}
