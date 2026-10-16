import { Table2 } from "lucide-react";
import { api } from "../../api";
import { formatCount } from "../../lib/format";
import { useStore } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { Card, Loading } from "./ui";
import { useWpData } from "./useWpData";

/** "Find everywhere": one exact value across options, meta tables, action args and the log. */
export function FindScreen({ value }: { value: string }) {
  const { openEntity, navigate, openRow } = useStore.getState();
  const { data, error } = useWpData([value], (s, d) => api.wpFindValue(s, d, value));
  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data) return <Loading />;
  const total = data.reduce((s, g) => s + g.count, 0);
  return (
    <div className="dash">
      <div className="dash-head">
        <h2>
          <span className="mono">{value}</span>
        </h2>
        <span className="muted">{total === 0 ? "not found anywhere" : `${formatCount(total)} ${total === 1 ? "match" : "matches"} in ${data.length} ${data.length === 1 ? "place" : "places"}`}</span>
      </div>
      {data.map((g) => (
        <Card key={g.table} title={g.source} aside={`${formatCount(g.count)} in ${g.table}${g.count > g.hits.length ? ` · showing ${g.hits.length}` : ""}`}>
          <div className="rows">
            {g.hits.map((h) => (
              <div key={h.rowId} className="row-link" role="row">
                {h.entity && h.entityId ? (
                  <button
                    className="link-btn plain grow-text"
                    onClick={() => (h.entity === "option" ? navigate({ screen: "list", list: "settings" }) : openEntity(h.entity!, h.entityId!))}
                  >
                    {h.label}
                  </button>
                ) : (
                  <span className="grow-text">{h.label}</span>
                )}
                <span className="muted mono small">{h.column}</span>
                <button className="link-btn plain small" onClick={() => openRow(g.table, g.idColumn, h.rowId)} title={`${g.table} where ${g.idColumn} = ${h.rowId}`}>
                  <Table2 size="0.692rem" /> row
                </button>
              </div>
            ))}
          </div>
        </Card>
      ))}
    </div>
  );
}
