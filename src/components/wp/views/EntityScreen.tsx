import { useEffect, useRef, useState } from "react";
import { Camera, Columns2, Radio, RefreshCw, Table2 } from "lucide-react";
import { api, type EntityDoc, type EntityViewData } from "../../../api";
import { selectActiveSession, useStore } from "../../../state/store";
import { ErrorPanel } from "../../QueryTab";
import { Loading } from "../ui";
import { useWpData } from "../useWpData";
import { flattenDoc } from "./common";
import { OrderView } from "./OrderView";
import { ActionView, CustomerView, GenericView, ProductView, SessionView } from "./OtherViews";

const LIVE_MS = 3000;

export function docOf(v: EntityViewData): EntityDoc {
  return v.view === "generic" ? v : v.doc;
}

export function EntityScreen({ entity, id, embedded = false }: { entity: string; id: string; embedded?: boolean }) {
  const session = useStore(selectActiveSession);
  const { openTableTab, setMode, navigate, takeSnapshot, setEntityMeta } = useStore.getState();
  const [live, setLive] = useState(false);
  const [changed, setChanged] = useState<Set<string>>(new Set());
  const [updatedAt, setUpdatedAt] = useState<number>(Date.now());
  const previous = useRef<Record<string, string> | null>(null);
  const { data, error, loading, reload } = useWpData((s, d) => api.wpView(s, d, entity, id), [entity, id]);

  useEffect(() => {
    if (!live) return;
    const t = setInterval(reload, LIVE_MS);
    return () => clearInterval(t);
  }, [live, reload]);

  useEffect(() => {
    if (!data) return;
    const d = docOf(data);
    if (!embedded) setEntityMeta({
      key: `${entity}:${id}`,
      parent: data.view === "product" ? "products" : data.view === "customer" ? "users" : undefined,
      title: data.view === "order" || !d.title ? `#${id}` : d.title,
      table: d.table,
      idColumn: d.idColumn,
      id: d.id,
    });
    const now = flattenDoc(d);
    const before = previous.current;
    if (before) {
      const diff = new Set(Object.keys({ ...before, ...now }).filter((k) => before[k] !== now[k]));
      if (diff.size) setChanged(diff);
    }
    previous.current = now;
    setUpdatedAt(Date.now());
  }, [data]);

  useEffect(() => () => (embedded ? undefined : setEntityMeta(null)), [setEntityMeta, embedded]);

  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data) return <Loading />;
  const doc = docOf(data);

  return (
    <>
      <div className="entity-tools">
        <button className={`btn${live ? " btn-live" : ""}`} aria-pressed={live} onClick={() => setLive(!live)} title="Poll every 3 s and highlight changes">
          <Radio size="0.923rem" /> {live ? "Live" : "Live mode"}
        </button>
        {live && <span className="muted small">updated {Math.round((Date.now() - updatedAt) / 1000)}s ago{changed.size ? ` · ${changed.size} changed` : ""}</span>}
        <div className="spacer" />
        <button className="btn" onClick={() => void takeSnapshot(entity, id)} title="Store the current state to diff later">
          <Camera size="0.923rem" /> Snapshot
        </button>
        <button
          className="btn"
          onClick={() =>
            useStore.getState().openDialog({
              type: "prompt",
              title: `Compare ${entity} #${id} with ID`,
              value: "",
              onSubmit: (other) => /^\d+$/.test(other) && navigate({ screen: "compare", entity, a: id, b: other }),
            })
          }
        >
          <Columns2 size="0.923rem" /> Compare…
        </button>
        <button
          className="btn"
          onClick={() => {
            if (!session?.database) return;
            setMode("db");
            openTableTab(session.database, doc.table, "data", `\`${doc.idColumn}\` = ${doc.id}`);
          }}
        >
          <Table2 size="0.923rem" /> Source row
        </button>
        <button className="icon-btn" aria-label="Reload" onClick={reload}>
          <RefreshCw size="1rem" className={loading ? "spin" : undefined} />
        </button>
      </div>
      {data.view === "order" && <OrderView o={data} changed={changed} />}
      {data.view === "customer" && <CustomerView c={data} changed={changed} />}
      {data.view === "product" && <ProductView p={data} changed={changed} />}
      {data.view === "action" && <ActionView a={data} changed={changed} />}
      {data.view === "session" && <SessionView s={data} changed={changed} />}
      {data.view === "generic" && <GenericView doc={data} changed={changed} />}
    </>
  );
}
