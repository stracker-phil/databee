/** "Changes since snapshot…": the snapshots of a row or saved query, newest first; one opens its change marks. */
import { useState } from "react";
import { Trash2 } from "lucide-react";
import { api, type SnapshotInfo } from "../api";
import { formatCount, relativeTime } from "../lib/format";
import { fail, useStore } from "../state/store";
import { Modal } from "./Modal";

interface Props {
  title: string;
  snapshots: SnapshotInfo[];
  onPick: (id: string) => void;
}

export function SnapshotsDialog({ title, snapshots: initial, onPick }: Props) {
  const close = useStore((s) => s.closeDialog);
  const [snapshots, setSnapshots] = useState(initial);
  const remove = async (id: string) => {
    try {
      await api.snapshotDelete(id);
      setSnapshots((list) => list.filter((s) => s.id !== id));
    } catch (e) {
      fail(e, "Could not delete the snapshot");
    }
  };
  return (
    <Modal onClose={close} className="snapshots" labelledBy="snapshots-title">
      <h2 id="snapshots-title">Changes of {title} since</h2>
      {snapshots.length === 0 && <p className="muted">No snapshots left.</p>}
      <ul className="snapshot-list">
        {snapshots.map((s) => (
          <li key={s.id} className="snapshot-item">
            <button
              className="snapshot-pick"
              data-stage={`snapshot:${s.id}`}
              data-stage-label={s.label}
              data-stage-list="snapshots"
              onClick={() => {
                close();
                onPick(s.id);
              }}
            >
              <span className="snapshot-label">{s.label}</span>
              <span className="o3">
                {relativeTime(s.created)} · {formatCount(s.values)} values
              </span>
            </button>
            <button className="icon-btn" aria-label={`Delete snapshot ${s.label}`} data-stage={`snapshot:delete:${s.id}`} onClick={() => void remove(s.id)}>
              <Trash2 size="0.923rem" />
            </button>
          </li>
        ))}
      </ul>
      <div className="actions">
        <button data-stage="dialog:cancel" className="btn" onClick={close}>Close</button>
      </div>
    </Modal>
  );
}
