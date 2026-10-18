import { Maximize2, Minimize2 } from "lucide-react";
import { createContext, useContext } from "react";
import { useBindingLabel } from "../../commands";
import { useStore } from "../../state/store";

/** The pane of an object tab a header is in, while the stack can maximize it. */
export interface PaneControl {
  tabId: string;
  index: number;
  maximized: boolean;
}

export const PaneContext = createContext<PaneControl | null>(null);

/** Maximizes the pane it is in, or restores the panes; absent outside a stack that can maximize. */
export function PaneMaximize() {
  const pane = useContext(PaneContext);
  const keys = useBindingLabel("column.maximize");
  if (!pane) return null;
  const label = pane.maximized ? "Restore panes" : "Maximize pane";
  const Icon = pane.maximized ? Minimize2 : Maximize2;
  const toggle = () => {
    const s = useStore.getState();
    s.setActiveColumn(pane.tabId, pane.index);
    s.toggleMaximized(pane.tabId);
  };
  return (
    <button
      className="icon-btn"
      aria-label={label}
      aria-pressed={pane.maximized}
      title={keys ? `${label} (${keys})` : label}
      data-stage={`pane-maximize:${pane.index}`}
      data-stage-label={label}
      onClick={toggle}
    >
      <Icon size="1rem" />
    </button>
  );
}
