import { useState } from "react";
import { Camera, GitCompare, Trash2 } from "lucide-react";
import { api, type DiffEntry } from "../../api";
import { relativeTime } from "../../lib/format";
import { selectActiveSession, useStore } from "../../state/store";
import { ErrorPanel } from "../QueryTab";
import { Card, Loading } from "./ui";
import { useWpData } from "./useWpData";

function DiffTable({ entries }: { entries: DiffEntry[] }) {
  if (entries.length === 0) return <div className="muted">No differences — nothing changed since the snapshot.</div>;
  return (
    <table className="diff">
      <thead>
        <tr><th>Path</th><th>Before</th><th>After</th></tr>
      </thead>
      <tbody>
        {entries.map((e) => (
          <tr key={e.path} className={e.before == null ? "added" : e.after == null ? "removed" : "changed"}>
            <td className="mono">{e.path}</td>
            <td className="mono">{e.before ?? <span className="muted">—</span>}</td>
            <td className="mono">{e.after ?? <span className="muted">—</span>}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function SnapshotsScreen() {
  const session = useStore(selectActiveSession);
  const toast = useStore((s) => s.toast);
  const [pattern, setPattern] = useState("woocommerce\\_ppcp%");
  const [diff, setDiff] = useState<{ id: string; entries: DiffEntry[] } | null>(null);
  const { data, error, reload } = useWpData(api.wpSnapshots, []);
  if (!session?.database) return null;
  const db = session.database;

  const takeOptions = async () => {
    try {
      await api.wpSnapshotTake(session.id, db, { entity: null, id: null, includeRelated: false, optionPattern: pattern, includeActions: false }, `options ${pattern}`);
      reload();
    } catch (e) {
      toast("error", String((e as { message?: string }).message ?? e));
    }
  };

  return (
    <div className="dash">
      <Card title="Watch a scope, act, then diff">
        <p className="muted" style={{ marginTop: 0 }}>
          Snapshot an order (from its page) or a set of options, run a checkout or a cron job, then diff to see every value that changed.
        </p>
        <div style={{ display: "flex", gap: 8 }}>
          <input className="input mono" style={{ maxWidth: 320 }} aria-label="Option pattern" value={pattern} onChange={(e) => setPattern(e.target.value)} />
          <button className="btn" onClick={() => void takeOptions()}>
            <Camera size="0.923rem" /> Snapshot options
          </button>
        </div>
      </Card>
      {error && <ErrorPanel message={error.message} code={error.code} />}
      {!data ? (
        <Loading />
      ) : (
        <Card title="Snapshots" aside={`${data.length} saved`}>
          <div className="rows">
            {data.length === 0 && <div className="muted">No snapshots yet.</div>}
            {data.map((s) => (
              <div key={s.id} className="row-link" role="row">
                <Camera size="1.077rem" />
                <span className="grow-text">{s.label}</span>
                <span className="muted">{s.paths} values · {relativeTime(s.created)}</span>
                <button
                  className="btn"
                  onClick={async () => {
                    try {
                      setDiff({ id: s.id, entries: await api.wpSnapshotDiff(session.id, db, s.id) });
                    } catch (e) {
                      toast("error", String((e as { message?: string }).message ?? e));
                    }
                  }}
                >
                  <GitCompare size="0.923rem" /> Diff now
                </button>
                <button className="icon-btn" aria-label={`Delete snapshot ${s.label}`} onClick={() => void api.wpSnapshotDelete(s.id).then(reload)}>
                  <Trash2 size="1rem" />
                </button>
              </div>
            ))}
          </div>
        </Card>
      )}
      {diff && (
        <Card title="Changes since snapshot" aside={`${diff.entries.length} paths`}>
          <DiffTable entries={diff.entries} />
        </Card>
      )}
    </div>
  );
}

export function CompareScreen({ entity, a, b }: { entity: string; a: string; b: string }) {
  const [onlyDiff, setOnlyDiff] = useState(true);
  const { data, error } = useWpData((s, d) => api.wpCompare(s, d, entity, a, b), [entity, a, b]);
  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data) return <Loading />;
  const rows = onlyDiff ? data.filter((r) => r.a !== r.b) : data;
  return (
    <div className="dash">
      <Card
        title={`${entity} #${a} ↔ #${b}`}
        aside={
          <label className="check">
            <input type="checkbox" checked={onlyDiff} onChange={(e) => setOnlyDiff(e.target.checked)} /> only differences ({data.filter((r) => r.a !== r.b).length})
          </label>
        }
      >
        <table className="diff">
          <thead>
            <tr><th>Path</th><th>#{a}</th><th>#{b}</th></tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.path} className={r.a === r.b ? "" : r.a == null ? "added" : r.b == null ? "removed" : "changed"}>
                <td className="mono">{r.path}</td>
                <td className="mono">{r.a ?? <span className="muted">—</span>}</td>
                <td className="mono">{r.b ?? <span className="muted">—</span>}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
    </div>
  );
}
