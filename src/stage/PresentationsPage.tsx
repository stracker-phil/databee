import { useEffect, useState } from "react";
import { Copy, RefreshCw } from "lucide-react";
import { Group, NumberField, Row, Switch } from "../components/settings/controls";
import { copyText } from "../lib/clipboard";
import { relativeTime } from "../lib/format";
import { useApiResult } from "../lib/useApiResult";
import { fail, useStore } from "../state/store";
import { stageApi } from "./api";
import type { StageSettings } from "./types";

/** Port bounds. Rust's `StageSettings::sanitized` is the source of truth: it replaces ports below 1024. */
const PORT_LIMITS = { min: 1024, max: 65_535 } as const;

const STATUS_POLL_MS = 5000;

/** The Stage API's status for the current settings, polled so "Last presenter request" stays current. */
function useStageStatus(s: StageSettings | null) {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => setTick((n) => n + 1), STATUS_POLL_MS);
    return () => clearInterval(timer);
  }, []);
  const key = s ? JSON.stringify([s.enabled, s.port, s.token, tick]) : null;
  return useApiResult(key, stageApi.status, { keepData: true });
}

/** The hook's switch, status, port, token and register command (docs/stage.md). */
export function PresentationsPage() {
  const loaded = useApiResult("stage-settings", stageApi.settings, { onError: (e) => fail(e, "Could not load the presentation settings") });
  const [saved, setStage] = useState<StageSettings | null>(null);
  const s = saved ?? loaded.data;
  const [revealed, setRevealed] = useState(false);
  const toast = useStore((st) => st.toast);
  const { data: status, error: statusError } = useStageStatus(s);
  if (!s) return null;
  const patchStage = (patch: Partial<StageSettings>) =>
    stageApi.updateSettings({ ...s, ...patch }).then(setStage, (e) => fail(e, "Could not save the presentation settings"));
  const newToken = () =>
    stageApi.newToken().then(
      (next) => {
        setStage(next);
        toast("success", "New token; register DataBee with Stage again");
      },
      (e) => fail(e, "Could not replace the token"),
    );
  return (
    <Group>
      <Switch label="Allow presentations" description="Runs a local API on 127.0.0.1; requests from web pages are refused." checked={s.enabled} onChange={(enabled) => void patchStage({ enabled })} />
      <Row label="Status" description={status?.running ? status.url : undefined}>
        {statusError ? (
          <span className="pill tone-orange">{statusError.message}</span>
        ) : status?.error ? (
          <span className="pill tone-orange">{status.error}</span>
        ) : status?.running ? (
          <span className="pill tone-green">Ready</span>
        ) : (
          <span className="badge">Off</span>
        )}
      </Row>
      <NumberField label="Port" description="27184 unless another app needs it. Register again after a change." value={s.port} {...PORT_LIMITS} onCommit={(port) => void patchStage({ port })} />
      <Row label="Token" description="A new token disconnects presenters until DataBee is registered again.">
        <button type="button" className="btn mono" title={revealed ? "Hide" : "Show the token"} onClick={() => setRevealed((r) => !r)}>
          {revealed ? s.token : "••••••••••••"}
        </button>
        <button type="button" className="icon-btn" aria-label="Copy token" title="Copy token" onClick={() => void copyText(s.token, "token")}>
          <Copy size="1rem" />
        </button>
        <button type="button" className="btn" onClick={() => void newToken()}>
          <RefreshCw size="1rem" /> New token
        </button>
      </Row>
      {status && (
        <Row label="Register with Stage" description={<code className="mono small selectable">{status.registerCommand}</code>}>
          <button type="button" className="btn" onClick={() => void copyText(status.registerCommand, "the register command")}>
            <Copy size="1rem" /> Copy
          </button>
        </Row>
      )}
      <Row label="Last presenter request">
        <span className="muted">{status?.lastRequest ? relativeTime(status.lastRequest) : "None yet"}</span>
      </Row>
    </Group>
  );
}
