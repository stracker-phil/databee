import { useEffect } from "react";
import type { Cell } from "../api";
import type { CellSource } from "../lib/cells";

/** The rows a grid shows, in its order, for commands on the selected cell's grid. */
export interface GridRows {
  rows: Cell[][];
  names: string[];
}

const grids = new Map<string, GridRows>();

const gridId = (source: CellSource) => `${source.sessionId}/${source.resultId}`;

/** Registers the rows of a mounted grid under its result. */
export function useGridRows(source: CellSource | undefined, grid: GridRows) {
  const id = source && gridId(source);
  useEffect(() => {
    if (!id) return;
    grids.set(id, grid);
    return () => {
      if (grids.get(id) === grid) grids.delete(id);
    };
  }, [id, grid]);
}

export function gridRowsOf(source: CellSource): GridRows | undefined {
  return grids.get(gridId(source));
}
