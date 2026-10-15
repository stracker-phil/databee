export interface TreemapInput<T> {
  value: number;
  data: T;
}

export interface TreemapRect<T> {
  x: number;
  y: number;
  w: number;
  h: number;
  data: T;
}

/**
 * Squarified treemap (Bruls et al.): lays out values in a w×h box with tiles as close to
 * square as possible. Zero values are dropped; input order doesn't matter.
 */
export function squarify<T>(items: TreemapInput<T>[], w: number, h: number): TreemapRect<T>[] {
  const nodes = items.filter((i) => i.value > 0).sort((a, b) => b.value - a.value);
  const total = nodes.reduce((s, n) => s + n.value, 0);
  if (!total || w <= 0 || h <= 0) return [];
  const scale = (w * h) / total;
  const areas = nodes.map((n) => ({ area: n.value * scale, data: n.data }));
  const out: TreemapRect<T>[] = [];
  let x = 0;
  let y = 0;
  let cw = w;
  let ch = h;

  const worst = (row: number[], side: number) => {
    const sum = row.reduce((a, b) => a + b, 0);
    const max = Math.max(...row);
    const min = Math.min(...row);
    return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min));
  };

  let i = 0;
  while (i < areas.length) {
    const side = Math.min(cw, ch);
    const row = [areas[i].area];
    let j = i + 1;
    while (j < areas.length && worst([...row, areas[j].area], side) <= worst(row, side)) {
      row.push(areas[j].area);
      j++;
    }
    const sum = row.reduce((a, b) => a + b, 0);
    const horizontal = cw >= ch; // lay the row along the shorter side
    const thickness = sum / side;
    let offset = 0;
    for (let k = 0; k < row.length; k++) {
      const len = row[k] / thickness;
      out.push(
        horizontal
          ? { x, y: y + offset, w: thickness, h: len, data: areas[i + k].data }
          : { x: x + offset, y, w: len, h: thickness, data: areas[i + k].data },
      );
      offset += len;
    }
    if (horizontal) {
      x += thickness;
      cw -= thickness;
    } else {
      y += thickness;
      ch -= thickness;
    }
    i = j;
  }
  return out;
}
